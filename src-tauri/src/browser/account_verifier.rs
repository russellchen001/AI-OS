//! BROWSER-B — real provider account-state verification.
//!
//! A verifier answers four things about the AI-OS managed browser session:
//!
//! - does the inspected page really belong to the expected provider;
//! - what HTTPS origin was actually observed inside that page;
//! - is there a non-secret marker showing a signed-in account;
//! - is the account authenticated, or not yet.
//!
//! Rules held here:
//!
//! - the account decision is never made from a URL alone. The origin is read
//!   from inside the live page and must be matched by a provider signed-in
//!   signal read from the same page;
//! - only the AI-OS managed browser is inspected. If AI-OS owns no running
//!   browser for the provider there is no verification, not a failure;
//! - the raw signed-in signal is a display value belonging to the user. It is
//!   reduced to an irreversible marker before it can reach Connections,
//!   Planner, Evidence, Memory or the frontend;
//! - anything unparsed, unexpected or unmatched resolves to not authenticated.

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::authenticated_runtime::{control_port_for, origin_belongs_to_provider};
use super::devtools::{list_targets, parse_evaluated_string, protocol_call, DevToolsTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccountVerification {
    /// A real signed-in account was observed on a real provider origin.
    Authenticated {
        verified_origin: String,
        account_marker: String,
    },
    /// The managed browser is available but no signed-in account was observed.
    NotAuthenticated,
    /// AI-OS owns no running managed browser for this provider.
    NoManagedSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct AccountProbePayload {
    #[serde(default)]
    origin: String,
    #[serde(default)]
    signal: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider adapters
// ---------------------------------------------------------------------------

/// Every expression returns the same JSON shape:
/// `{"origin": <origin read from the live page>, "signal": <account text|null>}`.
///
/// The signal must come from a signed-in-only surface, never from the presence
/// of a login form or from the URL.
const AMAZON_PROBE: &str = r#"(function(){var l=document.querySelector('#nav-link-accountList-nav-line-1');var t=l?l.textContent.trim():'';var s=document.querySelector('#nav-item-signout, a[href*="/gp/flex/sign-out"]');var v=(t&&s)?t:null;return JSON.stringify({origin:location.origin,signal:v});})()"#;

const TAOBAO_PROBE: &str = r#"(function(){var n=document.querySelector('.site-nav-login-info-nick, .site-nav-user .nick, #J_SiteNavLogin .site-nav-user');var t=n?n.textContent.trim():'';var o=document.querySelector('.site-nav-logout, a[href*="logout"]');var v=(t&&o)?t:null;return JSON.stringify({origin:location.origin,signal:v});})()"#;

const JD_PROBE: &str = r#"(function(){var n=document.querySelector('#ttbar-login .nickname, .nickname');var t=n?n.textContent.trim():'';var o=document.querySelector('a[href*="logout"], #ttbar-login .loginout');var v=(t&&o)?t:null;return JSON.stringify({origin:location.origin,signal:v});})()"#;

const PINDUODUO_PROBE: &str = r#"(function(){var n=document.querySelector('.user-info .nickname, ._2rMuHFmp, .personal-info .nickname');var t=n?n.textContent.trim():'';var o=document.querySelector('a[href*="logout"], .logout');var v=(t&&o)?t:null;return JSON.stringify({origin:location.origin,signal:v});})()"#;

fn account_probe_expression(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "amazon-consumer" => Some(AMAZON_PROBE),
        "taobao-consumer" => Some(TAOBAO_PROBE),
        "jd-consumer" => Some(JD_PROBE),
        "pinduoduo-consumer" => Some(PINDUODUO_PROBE),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Pure decision logic
// ---------------------------------------------------------------------------

/// Text that means "you are not signed in" in the provider surfaces above.
const SIGN_IN_PROMPTS: [&str; 10] = [
    "sign in",
    "sign-in",
    "log in",
    "login",
    "登录",
    "登入",
    "請登入",
    "ログイン",
    "anmelden",
    "se connecter",
];

fn signal_is_signed_in(signal: &str) -> bool {
    let normalized = signal.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    !SIGN_IN_PROMPTS
        .iter()
        .any(|prompt| normalized.contains(prompt))
}

/// The observed display value is reduced to an irreversible marker. It proves
/// an account is present and stays stable for the same account, and it carries
/// no name, address, email, cookie or token out of the runtime.
fn account_marker(provider_id: &str, signal: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(provider_id.as_bytes());
    hasher.update(b":");
    hasher.update(signal.trim().to_lowercase().as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("account-{hex}")
}

/// Decide the account state from one page probe payload. Fail-closed.
pub(crate) fn evaluate_account_probe(provider_id: &str, payload: &str) -> AccountVerification {
    let Ok(probe) = serde_json::from_str::<AccountProbePayload>(payload) else {
        return AccountVerification::NotAuthenticated;
    };

    // Provider match uses the origin observed inside the live page.
    if !origin_belongs_to_provider(provider_id, &probe.origin) {
        return AccountVerification::NotAuthenticated;
    }

    let Some(signal) = probe.signal.as_deref() else {
        return AccountVerification::NotAuthenticated;
    };

    if !signal_is_signed_in(signal) {
        return AccountVerification::NotAuthenticated;
    }

    AccountVerification::Authenticated {
        verified_origin: probe.origin.clone(),
        account_marker: account_marker(provider_id, signal),
    }
}

/// Choose which managed page to inspect. Only real provider pages qualify, so
/// a blank tab or a devtools surface can never be probed.
pub(crate) fn select_provider_target<'a>(
    provider_id: &str,
    targets: &'a [DevToolsTarget],
) -> Option<&'a DevToolsTarget> {
    targets.iter().find(|target| {
        target.target_type == "page"
            && !target.web_socket_debugger_url.is_empty()
            && origin_belongs_to_provider(provider_id, &page_origin(&target.url))
    })
}

/// Origin of a target URL, used only to choose where to look — never to decide
/// that an account is authenticated.
fn page_origin(url: &str) -> String {
    let Some(remainder) = url.strip_prefix("https://") else {
        return String::new();
    };
    let authority = remainder.split('/').next().unwrap_or_default();
    format!("https://{authority}")
}

// ---------------------------------------------------------------------------
// Verification against the owned managed browser
// ---------------------------------------------------------------------------

pub(crate) fn verify_managed_account(provider_id: &str) -> Result<AccountVerification, String> {
    let Some(expression) = account_probe_expression(provider_id) else {
        return Err("This Provider has no authenticated browser verifier".to_owned());
    };

    let Some(port) = control_port_for(provider_id) else {
        return Ok(AccountVerification::NoManagedSession);
    };

    let targets = match list_targets(port) {
        Ok(targets) => targets,
        // A browser that cannot be inspected is not an authenticated account.
        Err(_) => return Ok(AccountVerification::NoManagedSession),
    };

    let Some(target) = select_provider_target(provider_id, &targets) else {
        return Ok(AccountVerification::NotAuthenticated);
    };

    let result = protocol_call(
        &target.web_socket_debugger_url,
        1,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": false,
        }),
    );

    let payload = match result.and_then(|value| parse_evaluated_string(&value)) {
        Ok(payload) => payload,
        Err(_) => return Ok(AccountVerification::NotAuthenticated),
    };

    Ok(evaluate_account_probe(provider_id, &payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(target_type: &str, url: &str) -> DevToolsTarget {
        DevToolsTarget {
            id: "AB12".to_owned(),
            target_type: target_type.to_owned(),
            url: url.to_owned(),
            web_socket_debugger_url: "ws://127.0.0.1:51321/devtools/page/AB12".to_owned(),
        }
    }

    #[test]
    fn every_in_scope_browser_provider_has_a_verifier() {
        for provider_id in [
            "amazon-consumer",
            "taobao-consumer",
            "jd-consumer",
            "pinduoduo-consumer",
        ] {
            let expression = account_probe_expression(provider_id)
                .unwrap_or_else(|| panic!("{provider_id} has no verifier"));
            assert!(expression.contains("location.origin"));
            assert!(expression.contains("signal"));
        }
        assert!(account_probe_expression("google-workspace").is_none());
    }

    #[test]
    fn authentication_requires_both_a_provider_origin_and_an_account_signal() {
        let verified = evaluate_account_probe(
            "amazon-consumer",
            r#"{"origin":"https://www.amazon.co.jp","signal":"こんにちは, Russell"}"#,
        );
        match verified {
            AccountVerification::Authenticated {
                verified_origin,
                account_marker,
            } => {
                assert_eq!(verified_origin, "https://www.amazon.co.jp");
                assert!(account_marker.starts_with("account-"));
            }
            other => panic!("expected an authenticated account, got {other:?}"),
        }

        // Right provider origin, no account signal.
        assert_eq!(
            evaluate_account_probe(
                "amazon-consumer",
                r#"{"origin":"https://www.amazon.com","signal":null}"#
            ),
            AccountVerification::NotAuthenticated
        );

        // An account signal on an origin that is not the provider.
        assert_eq!(
            evaluate_account_probe(
                "amazon-consumer",
                r#"{"origin":"https://www.amazon.com.evil.example","signal":"Hello, Russell"}"#
            ),
            AccountVerification::NotAuthenticated
        );

        // Plain HTTP is never verified evidence.
        assert_eq!(
            evaluate_account_probe(
                "amazon-consumer",
                r#"{"origin":"http://www.amazon.com","signal":"Hello, Russell"}"#
            ),
            AccountVerification::NotAuthenticated
        );

        // Unparsable page output is not an account.
        assert_eq!(
            evaluate_account_probe("amazon-consumer", "not json"),
            AccountVerification::NotAuthenticated
        );
    }

    #[test]
    fn a_sign_in_prompt_is_never_an_authenticated_account() {
        for prompt in [
            "Hello, sign in",
            "Sign in",
            "登录",
            "ログイン",
            "Anmelden",
            "   ",
        ] {
            let payload = json!({"origin": "https://www.amazon.com", "signal": prompt}).to_string();
            assert_eq!(
                evaluate_account_probe("amazon-consumer", &payload),
                AccountVerification::NotAuthenticated,
                "{prompt} must not authenticate"
            );
        }
    }

    #[test]
    fn account_marker_is_irreversible_stable_and_carries_no_personal_detail() {
        let first = account_marker("amazon-consumer", "Hello, Russell Chen");
        let second = account_marker("amazon-consumer", "hello, russell chen ");
        let other_account = account_marker("amazon-consumer", "Hello, Someone Else");
        let other_provider = account_marker("taobao-consumer", "Hello, Russell Chen");

        assert_eq!(first, second, "the same account must produce one marker");
        assert_ne!(first, other_account);
        assert_ne!(first, other_provider);
        assert!(first.starts_with("account-"));
        assert_eq!(first.len(), "account-".len() + 12);
        let lowered = first.to_lowercase();
        assert!(!lowered.contains("russell"));
        assert!(!lowered.contains("chen"));
        assert!(!lowered.contains("hello"));
    }

    #[test]
    fn only_real_provider_pages_are_ever_probed() {
        let targets = vec![
            target("page", "about:blank"),
            target("service_worker", "https://www.amazon.com/sw.js"),
            target("page", "https://mail.example.com/inbox"),
            target("page", "https://www.amazon.com.au/gp/css/homepage.html"),
        ];
        let chosen = select_provider_target("amazon-consumer", &targets).unwrap();
        assert_eq!(chosen.url, "https://www.amazon.com.au/gp/css/homepage.html");

        assert!(select_provider_target("taobao-consumer", &targets).is_none());
        assert!(select_provider_target("amazon-consumer", &[]).is_none());

        let no_channel = vec![DevToolsTarget {
            id: "CD34".to_owned(),
            target_type: "page".to_owned(),
            url: "https://www.amazon.com/".to_owned(),
            web_socket_debugger_url: String::new(),
        }];
        assert!(select_provider_target("amazon-consumer", &no_channel).is_none());
    }

    #[test]
    fn amazon_verification_is_region_aware() {
        for origin in [
            "https://www.amazon.com",
            "https://www.amazon.com.au",
            "https://www.amazon.co.jp",
            "https://www.amazon.co.uk",
            "https://www.amazon.de",
        ] {
            let payload = json!({"origin": origin, "signal": "Hello, Russell"}).to_string();
            match evaluate_account_probe("amazon-consumer", &payload) {
                AccountVerification::Authenticated {
                    verified_origin, ..
                } => assert_eq!(verified_origin, origin),
                other => panic!("{origin} should verify, got {other:?}"),
            }
        }
    }
}

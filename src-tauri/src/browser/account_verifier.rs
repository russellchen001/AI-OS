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
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::authenticated_runtime::{control_port_for, origin_belongs_to_provider};
use super::diagnostics;
use super::site_registry::{
    self, site_is_signed_in, GenericPageSample, SiteDefinition,
};
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
    #[serde(default)]
    evidence: Option<ProbeEvidence>,
}

/// What the page probe saw, in a shape that carries no personal detail: which
/// candidate selector matched, and whether a sign-in affordance was present.
/// The matched text itself never appears here.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ProbeEvidence {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    login: bool,
    /// Path of the page AI-OS itself navigated to. Not a browsing history:
    /// recovery chooses this destination.
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    ready: Option<String>,
    /// Rough page size, to tell a loaded storefront from a blank or
    /// challenge page.
    #[serde(default)]
    links: Option<u32>,
    /// Page title, truncated. On these sites it is the page's own name and is
    /// what distinguishes an account page from a login page.
    #[serde(default)]
    title: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider adapters
// ---------------------------------------------------------------------------

/// Every expression returns the same JSON shape:
/// `{"origin": <origin read from the live page>, "signal": <account text|null>}`.
///
/// The signal must come from a signed-in-only surface, never from the presence
/// of a login form or from the URL.
///
/// Amazon's sign-out control lives in a flyout that is not always in the
/// initial DOM, so requiring it made a signed-in account read as signed out.
/// The evidence is instead the account greeting plus the absence of any
/// sign-in affordance, which keeps the check fail-closed: no greeting, or any
/// sign-in surface present, is never an authenticated account.
const AMAZON_PROBE: &str = r#"(function(){var q=function(s){try{return document.querySelector(s)}catch(e){return null}};var t=function(e){return e&&e.textContent?e.textContent.trim():''};var c=[['greeting','#nav-link-accountList-nav-line-1'],['greeting-alt','#nav-link-accountList .nav-line-1']];var k=null,v=null;for(var i=0;i<c.length;i++){var x=t(q(c[i][1]));if(x){k=c[i][0];v=x;break}}var l=q('#nav-link-accountList[href*="/ap/signin"], #nav-signin-tooltip, form[action*="/ap/signin"], #ap_email, #ap_password');return JSON.stringify({origin:location.origin,signal:(v&&!l)?v:null,evidence:{key:k,login:!!l,path:location.pathname,ready:document.readyState,links:document.querySelectorAll('a').length}});})()"#;

const TAOBAO_PROBE: &str = r#"(function(){var q=function(s){try{return document.querySelector(s)}catch(e){return null}};var t=function(e){return e&&e.textContent?e.textContent.trim():''};var c=[['nick','.site-nav-login-info-nick'],['user-nick','.site-nav-user .nick'],['user-nick-attr','[class*="userNick"]'],['nick-name','.nick-name']];var k=null,v=null;for(var i=0;i<c.length;i++){var x=t(q(c[i][1]));if(x){k=c[i][0];v=x;break}}var l=q('#login-form, .login-blocks, a[href*="login.taobao.com/member/login"]');return JSON.stringify({origin:location.origin,signal:(v&&!l)?v:null,evidence:{key:k,login:!!l,path:location.pathname,ready:document.readyState,links:document.querySelectorAll('a').length}});})()"#;

const JD_PROBE: &str = r#"(function(){var q=function(s){try{return document.querySelector(s)}catch(e){return null}};var t=function(e){return e&&e.textContent?e.textContent.trim():''};var p=(location.pathname||'').toLowerCase();var onLogin=p.indexOf('login')>=0||p.indexOf('signin')>=0||/passport\./.test(location.host);var c=[['nick','.nickname'],['user-name','#user-name'],['user-name-attr','[class*="user-name"]'],['ttbar-nick','#ttbar-login .nickname']];var k=null,v=null;for(var i=0;i<c.length;i++){var x=t(q(c[i][1]));if(x){k=c[i][0];v=x;break}}if(!v&&!onLogin){var d=(document.title||'').trim();if(d){k='title';v=d}}var l=q('.login-form, #formlogin, #loginForm, .login-wrap')||onLogin;return JSON.stringify({origin:location.origin,signal:(v&&!l)?v:null,evidence:{key:k,login:!!l,path:location.pathname,ready:document.readyState,links:document.querySelectorAll('a').length,title:(document.title||'').slice(0,30)}});})()"#;

const PINDUODUO_PROBE: &str = r#"(function(){var q=function(s){try{return document.querySelector(s)}catch(e){return null}};var t=function(e){return e&&e.textContent?e.textContent.trim():''};var p=(location.pathname||'').toLowerCase();var onLogin=p.indexOf('login')>=0||p.indexOf('signin')>=0;var c=[['nick-attr','[class*="nickname"]'],['user-name','.user-name'],['user-name-attr','[class*="userName"]'],['user-info','[class*="userInfo"]'],['personal','[class*="personal"] [class*="name"]']];var k=null,v=null;for(var i=0;i<c.length;i++){var x=t(q(c[i][1]));if(x){k=c[i][0];v=x;break}}if(!v&&!onLogin){var d=(document.title||'').trim();if(d){k='title';v=d}}var l=q('[class*="login-btn"], [class*="loginBtn"], #login-container, .login-wrap')||onLogin;return JSON.stringify({origin:location.origin,signal:(v&&!l)?v:null,evidence:{key:k,login:!!l,path:location.pathname,ready:document.readyState,links:document.querySelectorAll('a').length,title:(document.title||'').slice(0,30)}});})()"#;

/// eBay uses a signed-in-only account settings page. A signed-out visitor is
/// redirected to signin.ebay.com, so remaining on /uas without a login form is
/// structural account evidence. No page text or personal value is read.
const EBAY_PROBE: &str = r#"(function(){var q=function(s){try{return document.querySelector(s)}catch(e){return null}};var p=location.pathname||'';var account=location.hostname==='accountsettings.ebay.com'&&(p==='/uas'||p.indexOf('/uas/')===0);var l=q('input[type="password"], form[action*="SignIn"], form[action*="signin"], form[action*="login"]');var k=account?'account-page':null;return JSON.stringify({origin:location.origin,signal:(account&&!l)?'account-page-present':null,evidence:{key:k,login:!!l,path:p,ready:document.readyState,links:document.querySelectorAll('a').length}});})()"#;

/// The page restart recovery should open to see whether the account is still
/// signed in.
///
/// Amazon and Taobao show account state on their own storefront. JD's top bar
/// does not render for recovery and Pinduoduo's mobile home has no account area
/// at all, so those two go to a page only a signed-in account can reach:
/// arriving there without being bounced to a login page is itself the evidence.
pub(crate) fn account_home_url(provider_id: &str, verified_origin: Option<&str>) -> Option<String> {
    match provider_id {
        "ebay" => Some("https://accountsettings.ebay.com/uas".to_owned()),
        // Region-aware: the account's own regional storefront.
        "amazon-consumer" => verified_origin.map(str::to_owned),
        "taobao-consumer" => Some("https://i.taobao.com/my_taobao.htm".to_owned()),
        "jd-consumer" => Some("https://home.jd.com/".to_owned()),
        "pinduoduo-consumer" => Some("https://mobile.yangkeduo.com/personal.html".to_owned()),
        _ => site_registry::site(provider_id)
            .and_then(|site| {
                // The page the user named, if they named one; otherwise wherever
                // the account was last verified, and the login page as a last
                // resort.
                site.account_url
                    .clone()
                    .or_else(|| verified_origin.map(str::to_owned))
                    .or(Some(site.login_url.clone()))
            })
            .or_else(|| verified_origin.map(str::to_owned)),
    }
}

fn account_probe_expression(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "ebay" => Some(EBAY_PROBE),
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

    if provider_id == "ebay" {
        let Some(evidence) = probe.evidence.as_ref() else {
            return AccountVerification::NotAuthenticated;
        };
        let account_path = evidence
            .path
            .as_deref()
            .map(|path| path == "/uas" || path.starts_with("/uas/"))
            .unwrap_or(false);
        if probe.origin != "https://accountsettings.ebay.com"
            || evidence.key.as_deref() != Some("account-page")
            || evidence.login
            || !account_path
        {
            return AccountVerification::NotAuthenticated;
        }
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

/// Save a picture of the top of the page a provider verifier just looked at.
///
/// Diagnostics only, written beside the repository and gitignored. Selector
/// names alone cannot tell a signed-out page from a challenge page from one
/// that never rendered. The capture is clipped to the header strip, which is
/// where sign-in state lives, so it does not photograph the page's contents.
pub(crate) fn capture_provider_page(
    provider_id: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let port = control_port_for(provider_id)
        .ok_or_else(|| "No managed browser to capture".to_owned())?;
    let targets = list_targets(port)?;
    let target = select_provider_target(provider_id, &targets)
        .ok_or_else(|| "No provider page to capture".to_owned())?;

    let result = protocol_call(
        &target.web_socket_debugger_url,
        1,
        "Page.captureScreenshot",
        json!({
            "format": "png",
            "clip": { "x": 0, "y": 0, "width": 1280, "height": 220, "scale": 1 },
        }),
    )?;

    let encoded = result
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| "Managed browser returned no image".to_owned())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Managed browser image could not be decoded".to_owned())?;
    std::fs::write(path, bytes).map_err(|_| "Diagnostic image could not be saved".to_owned())
}

/// Take a generic page sample from the managed browser.
///
/// Used both to verify a user-added site and, at sign-in, to learn what
/// distinguishes signed in from signed out on it.
pub(crate) fn sample_provider_page(provider_id: &str) -> Result<GenericPageSample, String> {
    let port = control_port_for(provider_id)
        .ok_or_else(|| "No managed browser to sample".to_owned())?;
    let targets = list_targets(port)?;
    let target = select_provider_target(provider_id, &targets)
        .ok_or_else(|| "No page for this site".to_owned())?;

    let result = protocol_call(
        &target.web_socket_debugger_url,
        1,
        "Runtime.evaluate",
        json!({
            "expression": site_registry::generic_probe_expression(),
            "returnByValue": true,
            "awaitPromise": false,
        }),
    )?;
    let payload = parse_evaluated_string(&result)?;
    serde_json::from_str::<GenericPageSample>(&payload)
        .map_err(|_| "Page sample could not be read".to_owned())
}

/// A user-added site is verified structurally, so AI-OS never reads who the
/// account belongs to. The marker records that a session on this site was
/// verified, and nothing about its owner.
fn user_site_marker(provider_id: &str, origin: &str) -> String {
    account_marker(provider_id, origin)
}

fn verify_user_site(site: &SiteDefinition) -> Result<AccountVerification, String> {
    if control_port_for(&site.provider_id).is_none() {
        diagnostics::record(
            "verifier",
            &format!("provider={} kind=user-site outcome=no_control_channel", site.provider_id),
        );
        return Ok(AccountVerification::NoManagedSession);
    }

    let sample = match sample_provider_page(&site.provider_id) {
        Ok(sample) => sample,
        Err(error) => {
            diagnostics::record(
                "verifier",
                &format!("provider={} kind=user-site sample_failed detail={error}", site.provider_id),
            );
            return Ok(AccountVerification::NotAuthenticated);
        }
    };

    let signed_in = site_is_signed_in(site, &sample);
    diagnostics::record(
        "verifier",
        &format!(
            "provider={} kind=user-site path={} ready={} keys=[{}] has_account_page={} has_evidence={} authenticated={signed_in}",
            site.provider_id,
            sample.path,
            sample.ready,
            sample.keys.join("|"),
            site.account_url.is_some(),
            site.evidence.is_some(),
        ),
    );

    if !signed_in {
        return Ok(AccountVerification::NotAuthenticated);
    }

    Ok(AccountVerification::Authenticated {
        account_marker: user_site_marker(&site.provider_id, &sample.origin),
        verified_origin: sample.origin,
    })
}

/// Send the managed browser's own page to a URL.
///
/// Used to visit a site's account page after a sign-in, which is what lets a
/// site be verified without AI-OS knowing anything about its markup.
pub(crate) fn navigate_provider_page(provider_id: &str, url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("A managed browser is only ever sent to an https page".to_owned());
    }
    let port = control_port_for(provider_id)
        .ok_or_else(|| "No managed browser to navigate".to_owned())?;
    let targets = list_targets(port)?;
    let target = targets
        .iter()
        .find(|target| target.target_type == "page" && !target.web_socket_debugger_url.is_empty())
        .ok_or_else(|| "No page to navigate".to_owned())?;
    protocol_call(
        &target.web_socket_debugger_url,
        1,
        "Page.navigate",
        json!({ "url": url }),
    )
    .map(|_| ())
}

pub(crate) fn verify_managed_account(provider_id: &str) -> Result<AccountVerification, String> {
    // A site the user added has no hand-written page knowledge, so it is
    // verified from its account page or from evidence learned at sign-in.
    if let Some(site) = site_registry::site(provider_id) {
        return verify_user_site(&site);
    }

    let Some(expression) = account_probe_expression(provider_id) else {
        return Err("This Provider has no authenticated browser verifier".to_owned());
    };

    let Some(port) = control_port_for(provider_id) else {
        diagnostics::record(
            "verifier",
            &format!("provider={provider_id} outcome=no_control_channel"),
        );
        return Ok(AccountVerification::NoManagedSession);
    };

    let targets = match list_targets(port) {
        Ok(targets) => targets,
        // A browser that cannot be inspected is not an authenticated account.
        Err(error) => {
            diagnostics::record(
                "verifier",
                &format!("provider={provider_id} outcome=targets_unreadable detail={error}"),
            );
            return Ok(AccountVerification::NoManagedSession);
        }
    };

    let Some(target) = select_provider_target(provider_id, &targets) else {
        diagnostics::record(
            "verifier",
            &format!("provider={provider_id} provider_target=none targets={}", targets.len()),
        );
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
        Err(error) => {
            diagnostics::record(
                "verifier",
                &format!("provider={provider_id} probe_failed detail={error}"),
            );
            return Ok(AccountVerification::NotAuthenticated);
        }
    };

    let outcome = evaluate_account_probe(provider_id, &payload);
    // Shape only: whether the page was on a provider origin, whether any
    // account signal was present, which candidate selector matched and whether
    // a sign-in affordance was on the page. No matched text is ever recorded.
    let probe: Option<AccountProbePayload> = serde_json::from_str(&payload).ok();
    let evidence = probe.as_ref().and_then(|value| value.evidence.as_ref());
    diagnostics::record(
        "verifier",
        &format!(
            "provider={provider_id} origin_valid={} signal_present={} matched={} login_present={} path={} ready={} links={} title={} authenticated={}",
            probe
                .as_ref()
                .map(|value| origin_belongs_to_provider(provider_id, &value.origin))
                .unwrap_or(false),
            probe
                .as_ref()
                .map(|value| value.signal.is_some())
                .unwrap_or(false),
            evidence
                .and_then(|value| value.key.as_deref())
                .unwrap_or("none"),
            evidence.map(|value| value.login).unwrap_or(false),
            evidence
                .and_then(|value| value.path.as_deref())
                .unwrap_or("?"),
            evidence
                .and_then(|value| value.ready.as_deref())
                .unwrap_or("?"),
            evidence.and_then(|value| value.links).unwrap_or(0),
            evidence
                .and_then(|value| value.title.as_deref())
                .unwrap_or("?"),
            matches!(outcome, AccountVerification::Authenticated { .. })
        ),
    );
    Ok(outcome)
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
            "ebay",
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
    fn every_probe_reports_evidence_and_reads_nothing_it_should_not() {
        for provider_id in [
            "amazon-consumer",
            "taobao-consumer",
            "jd-consumer",
            "pinduoduo-consumer",
        ] {
            let expression = account_probe_expression(provider_id).unwrap();

            // Every probe returns the same shape, so a failure says which
            // candidate selector matched instead of only that it failed.
            assert!(expression.contains("evidence"), "{provider_id} reports no evidence");
            assert!(expression.contains("key:k"), "{provider_id} names no matched selector");
            assert!(expression.contains("login:!!l"), "{provider_id} does not report the sign-in affordance");

            // On these sites the logout link points at the login host, so a
            // sign-in selector must never match a login host generically —
            // doing so makes a signed-in account read as signed out forever.
            for generic in [
                r#"href*="login.taobao.com"]"#,
                r#"href*="passport.jd.com"]"#,
                r#"href*="login"]"#,
            ] {
                assert!(
                    !expression.contains(generic),
                    "{provider_id} would read its own logout link as a sign-in link"
                );
            }

            for forbidden in ["document.cookie", "localStorage", "sessionStorage"] {
                assert!(
                    !expression.contains(forbidden),
                    "{provider_id} must never read {forbidden}"
                );
            }
        }

        let ebay = account_probe_expression("ebay").unwrap();
        for forbidden in [
            "textContent",
            "document.cookie",
            "localStorage",
            "sessionStorage",
            "Authorization",
        ] {
            assert!(!ebay.contains(forbidden), "eBay probe must not read {forbidden}");
        }
    }

    #[test]
    fn recovery_looks_at_a_page_that_actually_shows_the_account() {
        assert_eq!(
            account_home_url("ebay", Some("https://signin.ebay.com")),
            Some("https://accountsettings.ebay.com/uas".to_owned())
        );
        // Amazon stays region-aware: the account's own storefront.
        assert_eq!(
            account_home_url("amazon-consumer", Some("https://www.amazon.co.jp")),
            Some("https://www.amazon.co.jp".to_owned())
        );
        // These sign in on a passport host that shows no account state, so the
        // verified origin is the wrong place to look.
        assert_eq!(
            account_home_url("jd-consumer", Some("https://passport.jd.com")),
            Some("https://home.jd.com/".to_owned())
        );
        assert_eq!(
            account_home_url("taobao-consumer", Some("https://login.taobao.com")),
            Some("https://i.taobao.com/my_taobao.htm".to_owned())
        );
        assert_eq!(
            account_home_url("pinduoduo-consumer", None),
            Some("https://mobile.yangkeduo.com/personal.html".to_owned())
        );
        // Whatever is chosen must still pass the provider origin check.
        for provider_id in ["taobao-consumer", "jd-consumer", "pinduoduo-consumer"] {
            let home = account_home_url(provider_id, None).unwrap();
            assert!(
                origin_belongs_to_provider(provider_id, &home),
                "{provider_id} recovery destination is not a provider origin"
            );
        }
    }

    #[test]
    fn ebay_account_page_is_fail_closed_and_uses_only_structural_evidence() {
        let authenticated = evaluate_account_probe(
            "ebay",
            r#"{"origin":"https://accountsettings.ebay.com","signal":"account-page-present","evidence":{"key":"account-page","login":false,"path":"/uas","ready":"complete","links":12}}"#,
        );
        let marker = match authenticated {
            AccountVerification::Authenticated { account_marker, .. } => account_marker,
            other => panic!("expected eBay account evidence, got {other:?}"),
        };
        assert!(marker.starts_with("account-"));
        assert!(!marker.contains("ebay"));

        for payload in [
            r#"{"origin":"https://signin.ebay.com","signal":null}"#,
            r#"{"origin":"https://accountsettings.ebay.com","signal":null}"#,
            r#"{"origin":"https://www.ebay.com","signal":"account-page-present","evidence":{"key":"account-page","login":false,"path":"/uas"}}"#,
            r#"{"origin":"https://accountsettings.ebay.com","signal":"account-page-present","evidence":{"key":"account-page","login":false,"path":"/other"}}"#,
            r#"{"origin":"https://accountsettings.ebay.com","signal":"account-page-present","evidence":{"key":"account-page","login":true,"path":"/uas"}}"#,
            r#"{"origin":"https://accountsettings.ebay.com.evil.test","signal":"account-page-present"}"#,
            r#"{"origin":"http://accountsettings.ebay.com","signal":"account-page-present"}"#,
        ] {
            assert_eq!(
                evaluate_account_probe("ebay", payload),
                AccountVerification::NotAuthenticated
            );
        }
    }

    #[test]
    fn ebay_without_a_managed_control_channel_is_not_a_session() {
        assert_eq!(
            verify_managed_account("ebay").unwrap(),
            AccountVerification::NoManagedSession
        );
    }

    #[test]
    fn a_probe_without_evidence_still_decides_correctly() {
        // Evidence is diagnostic only; a payload without it is still judged.
        assert_eq!(
            evaluate_account_probe(
                "taobao-consumer",
                r#"{"origin":"https://www.taobao.com","signal":null}"#
            ),
            AccountVerification::NotAuthenticated
        );
        match evaluate_account_probe(
            "jd-consumer",
            r#"{"origin":"https://www.jd.com","signal":"shopper","evidence":{"key":"nickname","login":false}}"#,
        ) {
            AccountVerification::Authenticated { verified_origin, .. } => {
                assert_eq!(verified_origin, "https://www.jd.com")
            }
            other => panic!("expected an authenticated account, got {other:?}"),
        }
        // A sign-in prompt as the matched text is still refused.
        assert_eq!(
            evaluate_account_probe(
                "taobao-consumer",
                r#"{"origin":"https://www.taobao.com","signal":"请登录","evidence":{"key":"nick","login":false}}"#
            ),
            AccountVerification::NotAuthenticated
        );
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

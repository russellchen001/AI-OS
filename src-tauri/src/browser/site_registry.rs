//! User-defined authenticated browser sites.
//!
//! The built-in commerce providers know their own pages. A site the user adds
//! is different: AI-OS has never seen it and cannot ship selectors for it. Two
//! mechanisms cover that, and a site may use either.
//!
//! 1. **An account page.** If the user names a page only a signed-in account
//!    can reach, staying on it is the evidence and being sent to a login page
//!    is the refusal. This needs nothing about the site's markup.
//!
//! 2. **Evidence learned at sign-in.** The managed browser samples a generic
//!    battery of selectors before the user signs in and again after they
//!    confirm they have. What appeared, and what disappeared, is the
//!    discriminator, and it is stored for later runs.
//!
//! Both stay fail-closed. A site with neither an account page nor learned
//! evidence can never report a connected account.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// A site the user added themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SiteDefinition {
    pub provider_id: String,
    pub display_name: String,
    pub login_url: String,
    /// A page only a signed-in account can reach, if the user named one.
    #[serde(default)]
    pub account_url: Option<String>,
    /// Every host this site is allowed to be verified on. Deliberately exact:
    /// deriving a parent domain would need a public suffix list to be safe, and
    /// guessing one wrongly would widen what counts as this site.
    pub hosts: Vec<String>,
    #[serde(default)]
    pub evidence: Option<LearnedEvidence>,
    pub created_at: String,
}

/// What told AI-OS the difference between signed out and signed in on this
/// site. Selector keys only — never page text, never an account name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LearnedEvidence {
    /// Battery keys that appeared only after signing in.
    pub signed_in_keys: Vec<String>,
    /// Battery keys that were there only before signing in.
    pub signed_out_keys: Vec<String>,
    pub verified_origin: String,
    pub learned_at: String,
}

/// Sites the user added, as this process currently knows them.
///
/// Held in memory so the browser layer can answer "is this origin this site"
/// and "how is this site verified" without an app handle, exactly as it does
/// for the built-in providers.
fn sites() -> &'static Mutex<BTreeMap<String, SiteDefinition>> {
    static SITES: OnceLock<Mutex<BTreeMap<String, SiteDefinition>>> = OnceLock::new();
    SITES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) fn set_sites(list: Vec<SiteDefinition>) {
    if let Ok(mut stored) = sites().lock() {
        *stored = list
            .into_iter()
            .map(|site| (site.provider_id.clone(), site))
            .collect();
    }
}

pub(crate) fn site(provider_id: &str) -> Option<SiteDefinition> {
    sites().lock().ok()?.get(provider_id).cloned()
}

pub(crate) fn all_sites() -> Vec<SiteDefinition> {
    sites()
        .lock()
        .map(|stored| stored.values().cloned().collect())
        .unwrap_or_default()
}

/// The page sample used for a site AI-OS has never seen.
///
/// Built from the battery itself so the two can never drift apart. It reports
/// which keys matched — never any page text, and never an account name.
pub(crate) fn generic_probe_expression() -> &'static str {
    static PROBE: OnceLock<String> = OnceLock::new();
    PROBE.get_or_init(|| {
        let entries: Vec<String> = EVIDENCE_BATTERY
            .iter()
            .map(|(key, selector)| {
                format!(
                    "[{},{}]",
                    serde_json::to_string(key).expect("key encodes"),
                    serde_json::to_string(selector).expect("selector encodes")
                )
            })
            .collect();
        format!(
            "(function(){{var q=function(s){{try{{return document.querySelector(s)}}catch(e){{return null}}}};var c=[{}];var m=[];for(var i=0;i<c.length;i++){{if(q(c[i][1]))m.push(c[i][0])}}return JSON.stringify({{origin:location.origin,path:location.pathname,title:(document.title||'').slice(0,30),ready:document.readyState,keys:m}});}})()",
            entries.join(",")
        )
    })
}

/// What a generic page sample returns.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenericPageSample {
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub ready: String,
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Decide a user-added site from one page sample.
///
/// The account page is tried first because it needs nothing about the site's
/// markup; learned evidence is the fallback. A site with neither can never be
/// connected.
pub(crate) fn site_is_signed_in(site: &SiteDefinition, sample: &GenericPageSample) -> bool {
    if !site.hosts.iter().any(|host| {
        sample.origin == format!("https://{host}")
    }) {
        return false;
    }

    if let Some(account_url) = site.account_url.as_deref() {
        if account_page_holds(account_url, &sample.origin, &sample.path, &sample.keys) {
            return true;
        }
    }

    site.evidence
        .as_ref()
        .map(|evidence| evidence_matches(evidence, &sample.keys))
        .unwrap_or(false)
}

/// One generic battery entry: a stable key and the selector it stands for.
pub(crate) const EVIDENCE_BATTERY: [(&str, &str); 14] = [
    // Signed-in-only affordances. A site that offers a way out is a site
    // someone is inside of.
    ("logout-href", r#"a[href*="logout"], a[href*="signout"], a[href*="sign-out"], a[href*="loginout"]"#),
    ("logout-class", r#"[class*="logout"], [class*="signout"], [id*="logout"]"#),
    ("account-nickname", r#"[class*="nickname"], [class*="nickName"]"#),
    ("account-username", r#"[class*="username"], [class*="userName"], [class*="user-name"]"#),
    ("account-avatar", r#"[class*="avatar"], [class*="Avatar"]"#),
    ("account-center", r#"[class*="account"], [class*="member-info"], [class*="userInfo"]"#),
    ("order-link", r#"a[href*="order"], a[href*="/my"], a[href*="member"]"#),
    // Signed-out-only affordances.
    ("login-href", r#"a[href*="login"]:not([href*="logout"]):not([href*="loginout"]), a[href*="signin"], a[href*="sign-in"]"#),
    ("login-class", r#"[class*="login-btn"], [class*="loginBtn"], [class*="signin-btn"]"#),
    ("password-field", r#"input[type="password"]"#),
    ("login-form", r#"form[action*="login"], form[action*="signin"], #login-form, .login-form"#),
    ("register-link", r#"a[href*="register"], a[href*="signup"], a[href*="sign-up"]"#),
    ("qr-login", r#"[class*="qrcode"], [class*="qr-code"], [class*="scan-login"]"#),
    ("captcha", r#"[class*="captcha"], [id*="captcha"]"#),
];

/// Keys that must never be treated as evidence of being signed in, whatever
/// the diff says. A password field appearing is not a sign of an account.
const NEVER_SIGNED_IN: [&str; 5] = [
    "password-field",
    "login-form",
    "login-class",
    "qr-login",
    "captcha",
];

/// Learn the discriminator from what the page looked like before and after the
/// user signed in.
///
/// Only what actually changed is kept. A key present in both states says
/// nothing, and keeping it would make the check pass on a signed-out page.
pub(crate) fn learn_evidence(
    before: &[String],
    after: &[String],
    verified_origin: &str,
    learned_at: &str,
) -> Option<LearnedEvidence> {
    let signed_in_keys: Vec<String> = after
        .iter()
        .filter(|key| !before.contains(key))
        .filter(|key| !NEVER_SIGNED_IN.contains(&key.as_str()))
        .cloned()
        .collect();

    let signed_out_keys: Vec<String> = before
        .iter()
        .filter(|key| !after.contains(key))
        .cloned()
        .collect();

    // Nothing distinguishable appeared, so there is nothing to check later.
    // Reporting that honestly beats storing evidence that always passes.
    if signed_in_keys.is_empty() {
        return None;
    }

    Some(LearnedEvidence {
        signed_in_keys,
        signed_out_keys,
        verified_origin: verified_origin.to_owned(),
        learned_at: learned_at.to_owned(),
    })
}

/// Apply learned evidence to a later page sample.
pub(crate) fn evidence_matches(evidence: &LearnedEvidence, present: &[String]) -> bool {
    let signed_in = evidence
        .signed_in_keys
        .iter()
        .any(|key| present.contains(key));
    let signed_out = evidence
        .signed_out_keys
        .iter()
        .any(|key| present.contains(key));
    signed_in && !signed_out
}

/// Whether a page sample taken on a named account page shows an account.
///
/// The site itself answers this: a signed-out visitor is sent to a login page,
/// so still being on the account page — with no login surface on it — is the
/// evidence, and no knowledge of the site's markup is needed.
pub(crate) fn account_page_holds(
    account_url: &str,
    observed_origin: &str,
    observed_path: &str,
    present: &[String],
) -> bool {
    let Some(expected) = url_origin(account_url) else {
        return false;
    };
    if expected != observed_origin {
        return false;
    }
    if path_looks_like_login(observed_path) {
        return false;
    }
    !present.iter().any(|key| NEVER_SIGNED_IN.contains(&key.as_str()))
}

pub(crate) fn path_looks_like_login(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    ["login", "signin", "sign-in", "logon", "auth"]
        .iter()
        .any(|marker| path.contains(marker))
}

/// Origin of an https URL. Anything that is not https has no origin here, so a
/// site can never be enrolled or verified over plain http.
pub(crate) fn url_origin(url: &str) -> Option<String> {
    let remainder = url.strip_prefix("https://")?;
    let authority = remainder.split('/').next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some(format!("https://{authority}"))
}

pub(crate) fn url_host(url: &str) -> Option<String> {
    let origin = url_origin(url)?;
    origin.strip_prefix("https://").map(str::to_owned)
}

/// A provider id derived from what the user typed, safe to use as a directory
/// name and impossible to confuse with a built-in provider.
pub(crate) fn derive_provider_id(login_url: &str, display_name: &str) -> Option<String> {
    let host = url_host(login_url)?;
    let slug: String = host
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        // The host was unusable, so fall back to the name the user gave.
        let fallback: String = display_name
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .map(|character| character.to_ascii_lowercase())
            .collect();
        if fallback.is_empty() {
            return None;
        }
        return Some(format!("site-{fallback}"));
    }
    Some(format!("site-{slug}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn sample(origin: &str, path: &str, present: &[&str]) -> GenericPageSample {
        GenericPageSample {
            origin: origin.to_owned(),
            path: path.to_owned(),
            title: String::new(),
            ready: "complete".to_owned(),
            keys: keys(present),
        }
    }

    fn site_with(account_url: Option<&str>, evidence: Option<LearnedEvidence>) -> SiteDefinition {
        SiteDefinition {
            provider_id: "site-shop-example-com".to_owned(),
            display_name: "Shop".to_owned(),
            login_url: "https://shop.example.com/login".to_owned(),
            account_url: account_url.map(str::to_owned),
            hosts: vec!["shop.example.com".to_owned()],
            evidence,
            created_at: "now".to_owned(),
        }
    }

    #[test]
    fn the_generic_probe_is_built_from_the_battery_and_reads_nothing_personal() {
        let probe = generic_probe_expression();
        for (key, _) in EVIDENCE_BATTERY {
            assert!(probe.contains(key), "battery key {key} is missing from the probe");
        }
        assert!(probe.contains("location.origin"));
        for forbidden in ["document.cookie", "localStorage", "sessionStorage", "textContent"] {
            assert!(!probe.contains(forbidden), "the generic probe must not read {forbidden}");
        }
    }

    #[test]
    fn a_site_with_no_account_page_and_no_learned_evidence_can_never_connect() {
        let site = site_with(None, None);
        assert!(!site_is_signed_in(
            &site,
            &sample("https://shop.example.com", "/", &["logout-href", "account-nickname"])
        ));
    }

    #[test]
    fn a_site_is_only_ever_verified_on_its_own_hosts() {
        let evidence = LearnedEvidence {
            signed_in_keys: keys(&["logout-href"]),
            signed_out_keys: keys(&["login-href"]),
            verified_origin: "https://shop.example.com".to_owned(),
            learned_at: "now".to_owned(),
        };
        let site = site_with(None, Some(evidence));

        assert!(site_is_signed_in(
            &site,
            &sample("https://shop.example.com", "/", &["logout-href"])
        ));
        // Same markers, somewhere else entirely.
        assert!(!site_is_signed_in(
            &site,
            &sample("https://shop.example.com.evil.test", "/", &["logout-href"])
        ));
        assert!(!site_is_signed_in(
            &site,
            &sample("http://shop.example.com", "/", &["logout-href"])
        ));
    }

    #[test]
    fn an_account_page_answers_even_when_nothing_was_learned() {
        let site = site_with(Some("https://shop.example.com/my"), None);
        assert!(site_is_signed_in(
            &site,
            &sample("https://shop.example.com", "/my", &[])
        ));
        // Sent to the login page instead.
        assert!(!site_is_signed_in(
            &site,
            &sample("https://shop.example.com", "/login", &["password-field"])
        ));
    }

    #[test]
    fn learning_keeps_only_what_signing_in_changed() {
        let before = keys(&["login-href", "password-field", "register-link", "order-link"]);
        let after = keys(&["logout-href", "account-nickname", "order-link"]);

        let evidence = learn_evidence(&before, &after, "https://shop.example", "now").unwrap();

        // Only what appeared counts; order-link was there all along and would
        // have made a signed-out page pass.
        assert_eq!(
            evidence.signed_in_keys,
            keys(&["logout-href", "account-nickname"])
        );
        assert_eq!(
            evidence.signed_out_keys,
            keys(&["login-href", "password-field", "register-link"])
        );
        assert_eq!(evidence.verified_origin, "https://shop.example");
    }

    #[test]
    fn a_login_surface_is_never_learned_as_proof_of_an_account() {
        // A site that shows a password field only after some flow must not have
        // that recorded as evidence of being signed in.
        let before = keys(&["login-href"]);
        let after = keys(&["password-field", "captcha", "qr-login"]);
        assert!(learn_evidence(&before, &after, "https://shop.example", "now").is_none());
    }

    #[test]
    fn learning_refuses_when_nothing_distinguishable_appeared() {
        let same = keys(&["order-link", "account-center"]);
        assert!(learn_evidence(&same, &same, "https://shop.example", "now").is_none());
        assert!(learn_evidence(&same, &[], "https://shop.example", "now").is_none());
    }

    #[test]
    fn learned_evidence_needs_a_signed_in_marker_and_no_signed_out_one() {
        let evidence = LearnedEvidence {
            signed_in_keys: keys(&["logout-href", "account-nickname"]),
            signed_out_keys: keys(&["login-href", "password-field"]),
            verified_origin: "https://shop.example".to_owned(),
            learned_at: "now".to_owned(),
        };

        assert!(evidence_matches(&evidence, &keys(&["logout-href"])));
        assert!(evidence_matches(&evidence, &keys(&["account-nickname", "order-link"])));
        // Signed out again.
        assert!(!evidence_matches(&evidence, &keys(&["login-href"])));
        // Both present: the page is ambiguous, so it is refused.
        assert!(!evidence_matches(&evidence, &keys(&["logout-href", "login-href"])));
        // Nothing recognisable at all.
        assert!(!evidence_matches(&evidence, &[]));
    }

    #[test]
    fn an_account_page_answers_only_while_the_site_keeps_us_there() {
        let none: Vec<String> = Vec::new();
        assert!(account_page_holds(
            "https://home.example.com/",
            "https://home.example.com",
            "/",
            &none
        ));
        // Bounced to a login page.
        assert!(!account_page_holds(
            "https://home.example.com/",
            "https://passport.example.com",
            "/",
            &none
        ));
        assert!(!account_page_holds(
            "https://home.example.com/",
            "https://home.example.com",
            "/login.html",
            &none
        ));
        // Still on the page but it is asking for a password.
        assert!(!account_page_holds(
            "https://home.example.com/",
            "https://home.example.com",
            "/",
            &keys(&["password-field"])
        ));
        // Plain http can never be an account page.
        assert!(!account_page_holds(
            "http://home.example.com/",
            "http://home.example.com",
            "/",
            &none
        ));
    }

    #[test]
    fn provider_ids_are_derived_safely_and_never_collide_with_built_ins() {
        assert_eq!(
            derive_provider_id("https://www.example.com/login", "Example"),
            Some("site-www-example-com".to_owned())
        );
        // A built-in id can never be produced, because every derived id is
        // prefixed and every built-in one is not.
        for built_in in [
            "amazon-consumer",
            "taobao-consumer",
            "jd-consumer",
            "pinduoduo-consumer",
        ] {
            assert_ne!(
                derive_provider_id("https://www.amazon.com/ap/signin", "Amazon").unwrap(),
                built_in
            );
        }
        // The id has to survive being used as a directory name.
        let id = derive_provider_id("https://shop.example.co.uk/sign_in", "Shop").unwrap();
        assert!(id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-'));
        // Not https, so not enrollable.
        assert_eq!(derive_provider_id("http://www.example.com", "Example"), None);
    }

    #[test]
    fn origins_are_https_only_and_reject_credential_tricks() {
        assert_eq!(
            url_origin("https://www.example.com/a/b"),
            Some("https://www.example.com".to_owned())
        );
        assert_eq!(url_origin("http://www.example.com"), None);
        assert_eq!(url_origin("https://user@evil.example"), None);
        assert_eq!(url_origin("https://"), None);
        assert_eq!(url_host("https://shop.example.com/x"), Some("shop.example.com".to_owned()));
    }

    #[test]
    fn login_paths_are_recognised_across_common_spellings() {
        for path in ["/login", "/Sign-In", "/user/signin", "/auth/callback", "/LOGON"] {
            assert!(path_looks_like_login(path), "{path} should look like login");
        }
        for path in ["/", "/personal.html", "/orders", "/my"] {
            assert!(!path_looks_like_login(path), "{path} should not");
        }
    }
}

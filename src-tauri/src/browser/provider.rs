use crate::provider_selection::AuthorizationRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserSessionMetadata {
    pub provider: String,
    pub platform: String,
    pub authenticated: bool,
    pub session_ref: Option<AuthorizationRef>,
    pub last_verified_at: Option<u64>,
    pub state: BrowserAuthenticationState,
    pub verification: Option<BrowserAccountVerification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum BrowserAuthenticationState {
    AuthorizationRequired,
    Authenticated,
    Expired,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserAccountVerification {
    pub account_marker: String,
    pub verified_url_origin: String,
}

impl BrowserSessionMetadata {
    pub(crate) fn verified(
        provider: impl Into<String>,
        platform: impl Into<String>,
        session_ref: AuthorizationRef,
        account_marker: impl Into<String>,
        verified_url_origin: impl Into<String>,
        observed_at: u64,
    ) -> Result<Self, String> {
        let account_marker = account_marker.into();
        let origin = verified_url_origin.into();
        if account_marker.trim().is_empty() || !origin.starts_with("https://") {
            return Err("Browser account verification evidence is incomplete".to_owned());
        }
        Ok(Self {
            provider: provider.into(),
            platform: platform.into(),
            authenticated: true,
            session_ref: Some(session_ref),
            last_verified_at: Some(observed_at),
            state: BrowserAuthenticationState::Authenticated,
            verification: Some(BrowserAccountVerification {
                account_marker,
                verified_url_origin: origin,
            }),
        })
    }

    pub(crate) fn expire(&mut self) {
        self.authenticated = false;
        self.state = BrowserAuthenticationState::Expired;
        self.verification = None;
    }
}

#[derive(Debug, Clone)]
pub struct BrowserRequest {
    pub action: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct BrowserResponse {
    pub provider: String,
    pub output: Value,
}

pub trait BrowserProvider: Send + Sync {
    fn id(&self) -> &str;

    fn execute(&self, request: BrowserRequest) -> Result<BrowserResponse, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticated_session_metadata_serializes_without_browser_secrets() {
        let metadata = BrowserSessionMetadata {
            provider: "authenticated-browser".to_owned(),
            platform: "example-commerce".to_owned(),
            authenticated: true,
            session_ref: Some(AuthorizationRef::opaque("browser-profile:primary")),
            last_verified_at: Some(42),
            state: BrowserAuthenticationState::Authenticated,
            verification: Some(BrowserAccountVerification {
                account_marker: "account-present".to_owned(),
                verified_url_origin: "https://example.com".to_owned(),
            }),
        };
        let value = serde_json::to_value(metadata).unwrap();

        assert_eq!(value["authenticated"], true);
        assert!(value.get("cookie").is_none());
        assert!(value.get("password").is_none());
        assert!(value.get("token").is_none());
    }

    #[test]
    fn public_page_is_not_account_authentication_and_expiry_is_explicit() {
        assert!(BrowserSessionMetadata::verified(
            "browser",
            "shop",
            AuthorizationRef::opaque("profile:primary"),
            "",
            "https://shop.example",
            42
        )
        .is_err());
        let mut session = BrowserSessionMetadata::verified(
            "browser",
            "shop",
            AuthorizationRef::opaque("profile:primary"),
            "signed-in-user",
            "https://shop.example",
            42,
        )
        .unwrap();
        session.expire();
        assert!(!session.authenticated);
        assert_eq!(session.state, BrowserAuthenticationState::Expired);
    }
}

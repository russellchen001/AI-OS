use chrono::{Duration, Utc};
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    config::BrokerConfig,
    contract::{CapabilityState, Environment, EBAY_BASE_SCOPE, EBAY_IDENTITY_SCOPE},
    storage::TokenRecord,
};

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    refresh_token_expires_in: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityResponse {
    user_id: String,
}

pub fn approved_scopes() -> [&'static str; 2] {
    [EBAY_BASE_SCOPE, EBAY_IDENTITY_SCOPE]
}

fn approved_scope_string() -> String {
    approved_scopes().join(" ")
}

pub fn authorization_url(config: &BrokerConfig, state: &str) -> Result<String, String> {
    let mut url = config.ebay_authorization_url.clone();
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.ru_name)
        .append_pair("response_type", "code")
        .append_pair("scope", &approved_scope_string())
        .append_pair("state", state);
    Ok(url.to_string())
}

pub async fn exchange_authorization_code(
    config: &BrokerConfig,
    code: &str,
) -> Result<TokenRecord, String> {
    let response = reqwest::Client::new()
        .post(config.ebay_token_url.clone())
        .basic_auth(&config.client_id, Some(&config.client_secret))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", config.ru_name.as_str()),
        ])
        .send()
        .await
        .map_err(|_| "EBAY_TOKEN_ENDPOINT_UNREACHABLE".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "EBAY_TOKEN_EXCHANGE_HTTP_{}",
            response.status().as_u16()
        ));
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|_| "EBAY_TOKEN_RESPONSE_INVALID".to_owned())?;
    let account_marker = validate_identity(config, &token.access_token).await?;
    Ok(TokenRecord {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: Utc::now() + Duration::seconds(token.expires_in.max(60)),
        refresh_expires_at: token
            .refresh_token_expires_in
            .map(|seconds| Utc::now() + Duration::seconds(seconds.max(60))),
        granted_scopes: approved_scopes().into_iter().map(str::to_owned).collect(),
        identity_validated: true,
        account_marker,
    })
}

pub async fn refresh_if_needed(
    config: &BrokerConfig,
    mut record: TokenRecord,
) -> Result<TokenRecord, String> {
    if record.expires_at <= Utc::now() + Duration::minutes(2) {
        let refresh = record
            .refresh_token
            .clone()
            .ok_or_else(|| "EBAY_LOGIN_REQUIRED".to_owned())?;
        if record
            .refresh_expires_at
            .is_some_and(|expiry| expiry <= Utc::now())
        {
            return Err("EBAY_LOGIN_REQUIRED".to_owned());
        }
        let scope = approved_scope_string();
        let response = reqwest::Client::new()
            .post(config.ebay_token_url.clone())
            .basic_auth(&config.client_id, Some(&config.client_secret))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
                ("scope", scope.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "EBAY_TOKEN_ENDPOINT_UNREACHABLE".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "EBAY_TOKEN_REFRESH_HTTP_{}",
                response.status().as_u16()
            ));
        }
        let token: TokenResponse = response
            .json()
            .await
            .map_err(|_| "EBAY_TOKEN_RESPONSE_INVALID".to_owned())?;
        record.access_token = token.access_token;
        record.expires_at = Utc::now() + Duration::seconds(token.expires_in.max(60));
        if let Some(refresh_token) = token.refresh_token {
            record.refresh_token = Some(refresh_token);
        }
        if let Some(seconds) = token.refresh_token_expires_in {
            record.refresh_expires_at = Some(Utc::now() + Duration::seconds(seconds.max(60)));
        }
    }
    record.account_marker = validate_identity(config, &record.access_token).await?;
    record.identity_validated = true;
    Ok(record)
}

async fn validate_identity(config: &BrokerConfig, token: &str) -> Result<String, String> {
    let url = config
        .ebay_api_base_url
        .join("commerce/identity/v1/user/")
        .map_err(|_| "EBAY_IDENTITY_URL_INVALID".to_owned())?;
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "EBAY_IDENTITY_UNREACHABLE".to_owned())?;
    if !response.status().is_success() {
        return Err(format!("EBAY_IDENTITY_HTTP_{}", response.status().as_u16()));
    }
    let identity: IdentityResponse = response
        .json()
        .await
        .map_err(|_| "EBAY_IDENTITY_RESPONSE_INVALID".to_owned())?;
    if identity.user_id.trim().is_empty() {
        return Err("EBAY_IDENTITY_RESPONSE_INVALID".to_owned());
    }
    Ok(account_marker(config, &identity.user_id))
}

fn account_marker(config: &BrokerConfig, user_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"ai-os:ebay-account-marker:v1\0");
    digest.update(config.client_secret.as_bytes());
    digest.update(b"\0");
    digest.update(user_id.as_bytes());
    format!("ebay-account:{:x}", digest.finalize())
}

pub async fn revoke(config: &BrokerConfig, record: &TokenRecord) -> Result<(), String> {
    let (token, token_type_hint) = record
        .refresh_token
        .as_deref()
        .map(|token| (token, "refresh_token"))
        .unwrap_or((&record.access_token, "access_token"));
    let endpoint = format!(
        "{}/revoke",
        config.ebay_token_url.as_str().trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(endpoint)
        .basic_auth(&config.client_id, Some(&config.client_secret))
        .form(&[("token", token), ("token_type_hint", token_type_hint)])
        .send()
        .await
        .map_err(|_| "EBAY_TOKEN_REVOKE_UNREACHABLE".to_owned())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "EBAY_TOKEN_REVOKE_HTTP_{}",
            response.status().as_u16()
        ))
    }
}

pub fn capability_states(config: &BrokerConfig, authorized: bool) -> Vec<CapabilityState> {
    let now = Utc::now().to_rfc3339();
    [
        ("ebay.browse.search", false, false),
        ("ebay.browse.item.read", false, false),
        ("ebay.cart.read", true, false),
        ("ebay.cart.add", true, false),
        ("ebay.cart.update", true, false),
        ("ebay.cart.remove", true, false),
        ("ebay.checkout.prepare", true, false),
        ("ebay.checkout.confirm", true, true),
        ("ebay.order.list", true, false),
        ("ebay.order.read", true, false),
    ]
    .into_iter()
    .map(|(id, approval_required, confirmation_required)| {
        let approved = !approval_required || config.buy_api_approved;
        CapabilityState {
            capability_id: id.to_owned(),
            available: authorized && approved,
            environment: config.environment.clone(),
            authorization_state: if authorized {
                "GRANTED"
            } else {
                "DISCONNECTED"
            }
            .to_owned(),
            approval_state: if approved { "APPROVED" } else { "REQUIRED" }.to_owned(),
            required_scopes: vec!["buy.api".to_owned()],
            confirmation_required,
            unavailable_reason: if !authorized {
                Some("Authorization required".to_owned())
            } else if !approved {
                Some("Developer Approval Required".to_owned())
            } else {
                None
            },
            last_verified_at: authorized.then_some(now.clone()),
            evidence_reference: authorized
                .then(|| format!("ebay-capability:{}", uuid::Uuid::new_v4().simple())),
        }
    })
    .collect()
}

pub async fn execute_capability(
    config: &BrokerConfig,
    record: &TokenRecord,
    capability_id: &str,
    input: &Value,
) -> Result<(), String> {
    let text = |name: &str| {
        input
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("CAPABILITY_INPUT_REQUIRED_{name}"))
    };
    let (method, path, body) = match capability_id {
        "ebay.browse.search" => (
            Method::GET,
            format!(
                "buy/browse/v1/item_summary/search?q={}",
                url::form_urlencoded::byte_serialize(text("query")?.as_bytes()).collect::<String>()
            ),
            None,
        ),
        "ebay.browse.item.read" => (
            Method::GET,
            format!("buy/browse/v1/item/{}", text("itemId")?),
            None,
        ),
        "ebay.cart.read" | "ebay.checkout.prepare" => (
            Method::GET,
            format!(
                "buy/order/v2/guest_checkout_session/{}",
                text("checkoutSessionId")?
            ),
            None,
        ),
        "ebay.cart.add" => (
            Method::POST,
            "buy/order/v2/guest_checkout_session/initiate".to_owned(),
            Some(input.clone()),
        ),
        "ebay.cart.update" => (
            Method::POST,
            format!(
                "buy/order/v2/guest_checkout_session/{}/update_quantity",
                text("checkoutSessionId")?
            ),
            Some(input.clone()),
        ),
        "ebay.cart.remove" => (
            Method::POST,
            format!(
                "buy/order/v2/guest_checkout_session/{}/remove_line_item",
                text("checkoutSessionId")?
            ),
            Some(input.clone()),
        ),
        "ebay.checkout.confirm" => (
            Method::POST,
            format!(
                "buy/order/v2/guest_checkout_session/{}/place_order",
                text("checkoutSessionId")?
            ),
            Some(input.clone()),
        ),
        "ebay.order.list" => (Method::GET, "buy/order/v2/purchase_order".to_owned(), None),
        "ebay.order.read" => (
            Method::GET,
            format!("buy/order/v2/purchase_order/{}", text("purchaseOrderId")?),
            None,
        ),
        _ => return Err("CAPABILITY_NOT_DECLARED".to_owned()),
    };
    let url = config
        .ebay_api_base_url
        .join(&path)
        .map_err(|_| "EBAY_CAPABILITY_URL_INVALID".to_owned())?;
    let client = reqwest::Client::new();
    let mut request = client
        .request(method, url)
        .bearer_auth(&record.access_token)
        .header("X-EBAY-C-MARKETPLACE-ID", "EBAY_US");
    if let Some(value) = body {
        request = request.json(&value);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "EBAY_CAPABILITY_UNREACHABLE".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "EBAY_CAPABILITY_HTTP_{}",
            response.status().as_u16()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn browse_and_checkout_approval_are_independent() {
        let mut config = test_config();
        config.buy_api_approved = false;
        let states = capability_states(&config, true);
        assert!(
            states
                .iter()
                .find(|state| state.capability_id == "ebay.browse.search")
                .unwrap()
                .available
        );
        let checkout = states
            .iter()
            .find(|state| state.capability_id == "ebay.checkout.confirm")
            .unwrap();
        assert!(!checkout.available);
        assert_eq!(checkout.approval_state, "REQUIRED");
        assert!(checkout.confirmation_required);
    }

    #[test]
    fn authorization_uses_only_approved_identity_scopes_and_runame() {
        let config = test_config();
        let url = url::Url::parse(&authorization_url(&config, "opaque-state").unwrap()).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        let scopes = approved_scope_string();
        assert_eq!(query.get("redirect_uri"), Some(&config.ru_name));
        assert_eq!(query.get("state").map(String::as_str), Some("opaque-state"));
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some(scopes.as_str())
        );
        assert!(!query.get("scope").unwrap().contains("buy.order"));
    }

    #[test]
    fn account_marker_is_stable_and_does_not_expose_raw_identity() {
        let config = test_config();
        let first = account_marker(&config, "raw-ebay-user-id");
        let second = account_marker(&config, "raw-ebay-user-id");
        assert_eq!(first, second);
        assert!(!first.contains("raw-ebay-user-id"));
        assert_eq!(first.len(), "ebay-account:".len() + 64);
    }

    #[tokio::test]
    async fn token_exchange_requires_identity_api_before_authorized_record() {
        let app = Router::new()
            .route(
                "/token",
                post(|| async {
                    Json(serde_json::json!({
                        "access_token": "mock-access",
                        "refresh_token": "mock-refresh",
                        "expires_in": 7200,
                        "refresh_token_expires_in": 86400
                    }))
                }),
            )
            .route(
                "/commerce/identity/v1/user/",
                get(|| async { Json(serde_json::json!({ "userId": "raw-user-id" })) }),
            );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = test_config();
        config.ebay_token_url = url::Url::parse(&format!("http://{address}/token")).unwrap();
        config.ebay_api_base_url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let record = exchange_authorization_code(&config, "single-use-code")
            .await
            .unwrap();
        assert!(record.identity_validated);
        assert!(!record.account_marker.contains("raw-user-id"));
        assert_eq!(record.granted_scopes, approved_scopes());
        assert!(record.refresh_token.is_some());
        assert!(record.refresh_expires_at.is_some());
    }

    #[tokio::test]
    async fn identity_failure_cannot_create_connected_token_record() {
        let app = Router::new()
            .route(
                "/token",
                post(|| async {
                    Json(serde_json::json!({
                        "access_token": "mock-access",
                        "refresh_token": "mock-refresh",
                        "expires_in": 7200
                    }))
                }),
            )
            .route(
                "/commerce/identity/v1/user/",
                get(|| async { StatusCode::UNAUTHORIZED }),
            );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = test_config();
        config.ebay_token_url = url::Url::parse(&format!("http://{address}/token")).unwrap();
        config.ebay_api_base_url = url::Url::parse(&format!("http://{address}/")).unwrap();
        assert_eq!(
            exchange_authorization_code(&config, "single-use-code")
                .await
                .unwrap_err(),
            "EBAY_IDENTITY_HTTP_401"
        );
    }

    #[tokio::test]
    async fn restart_reverification_does_not_blindly_trust_saved_marker() {
        let app = Router::new().route(
            "/commerce/identity/v1/user/",
            get(|| async { StatusCode::UNAUTHORIZED }),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = test_config();
        config.ebay_api_base_url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let record = TokenRecord {
            access_token: "saved-access".to_owned(),
            refresh_token: Some("saved-refresh".to_owned()),
            expires_at: Utc::now() + Duration::hours(1),
            refresh_expires_at: Some(Utc::now() + Duration::days(30)),
            granted_scopes: approved_scopes().into_iter().map(str::to_owned).collect(),
            identity_validated: true,
            account_marker: "saved-marker".to_owned(),
        };
        assert_eq!(
            refresh_if_needed(&config, record).await.unwrap_err(),
            "EBAY_IDENTITY_HTTP_401"
        );
    }

    #[tokio::test]
    async fn expired_access_token_refreshes_then_reverifies_identity() {
        let app = Router::new()
            .route(
                "/token",
                post(|| async {
                    Json(serde_json::json!({
                        "access_token": "refreshed-access",
                        "expires_in": 7200
                    }))
                }),
            )
            .route(
                "/commerce/identity/v1/user/",
                get(|| async { Json(serde_json::json!({ "userId": "raw-user-id" })) }),
            );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = test_config();
        config.ebay_token_url = url::Url::parse(&format!("http://{address}/token")).unwrap();
        config.ebay_api_base_url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let record = TokenRecord {
            access_token: "expired-access".to_owned(),
            refresh_token: Some("saved-refresh".to_owned()),
            expires_at: Utc::now() - Duration::seconds(1),
            refresh_expires_at: Some(Utc::now() + Duration::days(30)),
            granted_scopes: approved_scopes().into_iter().map(str::to_owned).collect(),
            identity_validated: true,
            account_marker: "old-marker".to_owned(),
        };
        let refreshed = refresh_if_needed(&config, record).await.unwrap();
        assert_eq!(refreshed.access_token, "refreshed-access");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("saved-refresh"));
        assert!(refreshed.identity_validated);
        assert_ne!(refreshed.account_marker, "old-marker");
    }

    #[tokio::test]
    async fn disconnect_uses_official_refresh_token_revoke_endpoint() {
        let app = Router::new().route("/token/revoke", post(|| async { StatusCode::OK }));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = test_config();
        config.ebay_token_url = url::Url::parse(&format!("http://{address}/token")).unwrap();
        let record = TokenRecord {
            access_token: "access".to_owned(),
            refresh_token: Some("refresh".to_owned()),
            expires_at: Utc::now() + Duration::hours(1),
            refresh_expires_at: Some(Utc::now() + Duration::days(30)),
            granted_scopes: approved_scopes().into_iter().map(str::to_owned).collect(),
            identity_validated: true,
            account_marker: "marker".to_owned(),
        };
        revoke(&config, &record).await.unwrap();
    }

    #[tokio::test]
    async fn browse_adapter_uses_real_http_method_path_query_and_status() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0u8; 4096];
            let size = stream.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..size]);
            assert!(
                request.starts_with("GET /buy/browse/v1/item_summary/search?q=hard+drive HTTP/1.1")
            );
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer fake-access"));
            stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}").await.unwrap();
        });
        let mut config = test_config();
        config.ebay_api_base_url = url::Url::parse(&format!("http://{address}/")).unwrap();
        let record = TokenRecord {
            access_token: "fake-access".to_owned(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::hours(1),
            refresh_expires_at: None,
            granted_scopes: approved_scopes().into_iter().map(str::to_owned).collect(),
            identity_validated: true,
            account_marker: "marker".to_owned(),
        };
        execute_capability(
            &config,
            &record,
            "ebay.browse.search",
            &serde_json::json!({"query":"hard drive"}),
        )
        .await
        .unwrap();
    }

    fn test_config() -> BrokerConfig {
        BrokerConfig {
            environment: Environment::Sandbox,
            client_id: "fake-client".to_owned(),
            client_secret: "fake-secret".to_owned(),
            ru_name: "fake-runame".to_owned(),
            public_base_url: url::Url::parse("http://localhost:8787/").unwrap(),
            encryption_key: [3u8; 32],
            buy_api_approved: false,
            bind_address: "127.0.0.1:0".parse().unwrap(),
            ebay_api_base_url: url::Url::parse("http://localhost:1/").unwrap(),
            ebay_authorization_url: url::Url::parse(
                "https://auth.sandbox.ebay.com/oauth2/authorize",
            )
            .unwrap(),
            ebay_token_url: url::Url::parse("http://localhost:1/token").unwrap(),
            token_store_path: std::path::PathBuf::from("unused-test-token-store.json"),
        }
    }
}

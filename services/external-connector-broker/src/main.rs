mod config;
mod contract;
mod ebay;
mod security;
mod storage;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use contract::{
    AuthorizationReferenceRequest, BrokerEnvelope, CallbackRequest, ConfigurationRequest,
    Environment, ExecuteRequest,
};
use serde::Deserialize;
use std::sync::Arc;
use storage::{MemorySecureStorage, SecureAuthorizationStorage};

use crate::{
    config::BrokerConfig,
    security::{new_authorization_reference, validate_authorization_reference, TokenCipher},
};

#[derive(Clone)]
struct AppState {
    config: BrokerConfig,
    storage: Arc<dyn SecureAuthorizationStorage>,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<BrokerEnvelope>)>;

#[tokio::main]
async fn main() {
    let config = BrokerConfig::from_environment().unwrap_or_else(|error| {
        eprintln!("Broker configuration error: {error}");
        std::process::exit(2)
    });
    let address = config.bind_address;
    config.oauth_callback_url().unwrap_or_else(|error| {
        eprintln!("Broker callback configuration error: {error}");
        std::process::exit(2)
    });
    let storage = Arc::new(
        MemorySecureStorage::persistent(
            TokenCipher::new(config.encryption_key),
            config.token_store_path.clone(),
        )
        .unwrap_or_else(|error| {
            eprintln!("Broker secure storage error: {error}");
            std::process::exit(2)
        }),
    );
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|_| {
            eprintln!("Broker bind failed");
            std::process::exit(2)
        });
    axum::serve(listener, router(AppState { config, storage }))
        .await
        .unwrap_or_else(|_| {
            eprintln!("Broker server failed");
            std::process::exit(2)
        });
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/connectors/ebay-buy/manifest", get(manifest))
        .route(
            "/connectors/ebay-buy/configuration/validate",
            post(validate_configuration),
        )
        .route(
            "/connectors/ebay-buy/authorization/begin",
            post(begin_authorization),
        )
        .route(
            "/connectors/ebay-buy/authorization/status",
            get(authorization_status),
        )
        .route(
            "/connectors/ebay-buy/authorization/callback",
            get(authorization_callback_get).post(authorization_callback),
        )
        .route(
            "/connectors/ebay-buy/authorization/revoke",
            post(revoke_authorization),
        )
        .route("/connectors/ebay-buy/capabilities", get(capabilities))
        .route(
            "/connectors/ebay-buy/capabilities/execute",
            post(execute_capability),
        )
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<BrokerEnvelope> {
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    envelope.approval_status = "HEALTHY".to_owned();
    Json(envelope)
}

async fn manifest() -> Json<contract::ConnectorManifest> {
    Json(contract::ebay_manifest())
}

async fn validate_configuration(
    State(state): State<AppState>,
    Json(input): Json<ConfigurationRequest>,
) -> Json<BrokerEnvelope> {
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    if input.environment != state.config.environment {
        envelope.error_code = Some("ENVIRONMENT_MISMATCH".to_owned());
        return Json(envelope);
    }
    if let Some(client_id) = input.public_configuration.get("clientId") {
        if client_id != &state.config.client_id {
            envelope.error_code = Some("CLIENT_ID_MISMATCH".to_owned());
            return Json(envelope);
        }
    }
    if let Some(ru_name) = input.public_configuration.get("ruName") {
        if ru_name != &state.config.ru_name {
            envelope.error_code = Some("RUNAME_MISMATCH".to_owned());
            return Json(envelope);
        }
    }
    envelope.approval_status = if state.config.buy_api_approved {
        "APPROVED"
    } else {
        "PARTIAL"
    }
    .to_owned();
    envelope.capabilities = ebay::capability_states(&state.config, false);
    Json(envelope)
}

async fn begin_authorization(
    State(state): State<AppState>,
    Json(input): Json<ConfigurationRequest>,
) -> ApiResult<BrokerEnvelope> {
    if input.environment != state.config.environment {
        return Err(api_error(
            &state,
            StatusCode::BAD_REQUEST,
            "ENVIRONMENT_MISMATCH",
        ));
    }
    let state_value = uuid::Uuid::new_v4().simple().to_string();
    let reference = new_authorization_reference(&state.config.environment);
    state
        .storage
        .create_state(
            state_value.clone(),
            MemorySecureStorage::pending(reference.clone(), state.config.environment.clone()),
        )
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?;
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    envelope.authorization_reference = Some(reference);
    envelope.authorization_url = Some(
        ebay::authorization_url(&state.config, &state_value)
            .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?,
    );
    envelope.approval_status = "WAITING_FOR_USER".to_owned();
    Ok(Json(envelope))
}

async fn authorization_callback(
    State(state): State<AppState>,
    Json(input): Json<CallbackRequest>,
) -> ApiResult<BrokerEnvelope> {
    complete_authorization_callback(state, input).await
}

async fn authorization_callback_get(
    State(state): State<AppState>,
    Query(input): Query<CallbackRequest>,
) -> ApiResult<BrokerEnvelope> {
    complete_authorization_callback(state, input).await
}

async fn complete_authorization_callback(
    state: AppState,
    input: CallbackRequest,
) -> ApiResult<BrokerEnvelope> {
    if input.code.trim().is_empty() {
        return Err(api_error(
            &state,
            StatusCode::BAD_REQUEST,
            "AUTHORIZATION_CODE_REQUIRED",
        ));
    }
    let pending = state
        .storage
        .consume_state(&input.state)
        .map_err(|code| api_error(&state, StatusCode::BAD_REQUEST, &code))?;
    if pending.environment != state.config.environment {
        return Err(api_error(
            &state,
            StatusCode::BAD_REQUEST,
            "ENVIRONMENT_MISMATCH",
        ));
    }
    let tokens = ebay::exchange_authorization_code(&state.config, &input.code)
        .await
        .map_err(|code| api_error(&state, StatusCode::BAD_GATEWAY, &code))?;
    state
        .storage
        .save_tokens(&pending.reference, &tokens)
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?;
    Ok(Json(authorized_envelope(
        &state,
        &pending.reference,
        &tokens,
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceQuery {
    authorization_reference: String,
}

async fn authorization_status(
    State(state): State<AppState>,
    Query(query): Query<ReferenceQuery>,
) -> ApiResult<BrokerEnvelope> {
    validate_authorization_reference(&query.authorization_reference, &state.config.environment)
        .map_err(|code| api_error(&state, StatusCode::BAD_REQUEST, &code))?;
    let Some(tokens) = state
        .storage
        .load_tokens(&query.authorization_reference)
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?
    else {
        return Ok(Json(waiting_envelope(
            &state,
            &query.authorization_reference,
        )));
    };
    let refreshed = ebay::refresh_if_needed(&state.config, tokens)
        .await
        .map_err(|code| api_error(&state, StatusCode::UNAUTHORIZED, &code))?;
    state
        .storage
        .save_tokens(&query.authorization_reference, &refreshed)
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?;
    Ok(Json(authorized_envelope(
        &state,
        &query.authorization_reference,
        &refreshed,
    )))
}

async fn capabilities(
    State(state): State<AppState>,
    Query(query): Query<ReferenceQuery>,
) -> ApiResult<BrokerEnvelope> {
    authorization_status(State(state), Query(query)).await
}

async fn revoke_authorization(
    State(state): State<AppState>,
    Json(input): Json<AuthorizationReferenceRequest>,
) -> ApiResult<BrokerEnvelope> {
    validate_authorization_reference(&input.authorization_reference, &state.config.environment)
        .map_err(|code| api_error(&state, StatusCode::BAD_REQUEST, &code))?;
    let tokens = state
        .storage
        .load_tokens(&input.authorization_reference)
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?;
    let remote_revoke_complete = match tokens.as_ref() {
        Some(tokens) => ebay::revoke(&state.config, tokens).await.is_ok(),
        None => true,
    };
    state
        .storage
        .delete_tokens(&input.authorization_reference)
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?;
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    envelope.approval_status = if remote_revoke_complete {
        "REMOTE_REVOKED_LOCAL_TOKEN_DELETED"
    } else {
        "REMOTE_REVOKE_FAILED_LOCAL_TOKEN_DELETED"
    }
    .to_owned();
    if !remote_revoke_complete {
        envelope.error_code = Some("REMOTE_REVOKE_FAILED_LOCAL_TOKEN_DELETED".to_owned());
    }
    Ok(Json(envelope))
}

async fn execute_capability(
    State(state): State<AppState>,
    Json(input): Json<ExecuteRequest>,
) -> ApiResult<BrokerEnvelope> {
    validate_authorization_reference(&input.authorization_reference, &state.config.environment)
        .map_err(|code| api_error(&state, StatusCode::BAD_REQUEST, &code))?;
    let tokens = state
        .storage
        .load_tokens(&input.authorization_reference)
        .map_err(|code| api_error(&state, StatusCode::INTERNAL_SERVER_ERROR, &code))?
        .ok_or_else(|| api_error(&state, StatusCode::UNAUTHORIZED, "LOGIN_REQUIRED"))?;
    let capability = ebay::capability_states(&state.config, true)
        .into_iter()
        .find(|capability| capability.capability_id == input.capability_id)
        .ok_or_else(|| api_error(&state, StatusCode::BAD_REQUEST, "CAPABILITY_NOT_DECLARED"))?;
    if !capability.available {
        return Err(api_error(
            &state,
            StatusCode::FORBIDDEN,
            if capability.approval_state == "REQUIRED" {
                "DEVELOPER_APPROVAL_REQUIRED"
            } else {
                "CAPABILITY_UNAVAILABLE"
            },
        ));
    }
    if capability.confirmation_required && !input.confirmed {
        return Err(api_error(
            &state,
            StatusCode::PRECONDITION_REQUIRED,
            "USER_CONFIRMATION_REQUIRED",
        ));
    }
    ebay::execute_capability(&state.config, &tokens, &input.capability_id, &input.input)
        .await
        .map_err(|code| api_error(&state, StatusCode::BAD_GATEWAY, &code))?;
    let mut envelope = authorized_envelope(&state, &input.authorization_reference, &tokens);
    envelope.approval_status = "EXECUTION_VALIDATED".to_owned();
    Ok(Json(envelope))
}

fn waiting_envelope(state: &AppState, reference: &str) -> BrokerEnvelope {
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    envelope.authorization_reference = Some(reference.to_owned());
    envelope.approval_status = "WAITING_FOR_USER".to_owned();
    envelope.capabilities = ebay::capability_states(&state.config, false);
    envelope
}

fn authorized_envelope(
    state: &AppState,
    reference: &str,
    tokens: &storage::TokenRecord,
) -> BrokerEnvelope {
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    envelope.authorization_reference = Some(reference.to_owned());
    envelope.granted_scopes = tokens.granted_scopes.clone();
    envelope.approval_status = if state.config.buy_api_approved {
        "APPROVED"
    } else {
        "PARTIAL"
    }
    .to_owned();
    envelope.capabilities = ebay::capability_states(&state.config, tokens.identity_validated);
    envelope.identity_validated = tokens.identity_validated;
    envelope
}

fn api_error(
    state: &AppState,
    status: StatusCode,
    code: &str,
) -> (StatusCode, Json<BrokerEnvelope>) {
    let mut envelope = BrokerEnvelope::empty(state.config.environment.clone());
    envelope.error_code = Some(code.to_owned());
    (status, Json(envelope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn wrong_state_and_reused_code_are_rejected_without_token_output() {
        let app = router(test_state());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/connectors/ebay-buy/authorization/callback")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"state":"wrong","code":"fake"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("accessToken"));
        assert!(!text.contains("refreshToken"));

        let state = test_state();
        state
            .storage
            .create_state(
                "single-use-state".to_owned(),
                MemorySecureStorage::pending(
                    "ebay-buy:sandbox:reference".to_owned(),
                    Environment::Sandbox,
                ),
            )
            .unwrap();
        let app = router(state);
        let callback = || {
            Request::builder()
                .method("POST")
                .uri("/connectors/ebay-buy/authorization/callback")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"state":"single-use-state","code":"one-time-code"}"#,
                ))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(callback()).await.unwrap().status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            app.oneshot(callback()).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    fn test_state() -> AppState {
        let config = BrokerConfig {
            environment: Environment::Sandbox,
            client_id: "fake-client".to_owned(),
            client_secret: "fake-secret".to_owned(),
            ru_name: "fake-runame".to_owned(),
            public_base_url: url::Url::parse("http://localhost:8787/").unwrap(),
            encryption_key: [4u8; 32],
            buy_api_approved: false,
            bind_address: "127.0.0.1:0".parse().unwrap(),
            ebay_api_base_url: url::Url::parse("http://localhost:1/").unwrap(),
            ebay_authorization_url: url::Url::parse(
                "https://auth.sandbox.ebay.com/oauth2/authorize",
            )
            .unwrap(),
            ebay_token_url: url::Url::parse("http://localhost:1/token").unwrap(),
            token_store_path: std::path::PathBuf::from("unused-test-token-store.json"),
        };
        AppState {
            storage: Arc::new(MemorySecureStorage::new(TokenCipher::new(
                config.encryption_key,
            ))),
            config,
        }
    }
}

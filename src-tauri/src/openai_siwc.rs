//! OpenAI's official "Sign in with ChatGPT" for open-source, locally run apps
//! (developers.openai.com/siwc/token-sharing-open-source). The first sign-in
//! registers AI-OS through OpenAI's dynamic-registration flow and returns a
//! client ID issued to this user and host; later sign-ins and refreshes reuse
//! it. Requests then go to the public Responses API with the user's own
//! ChatGPT plan. This replaces the earlier route that borrowed the Codex
//! CLI's client and its private `chatgpt.com/backend-api` endpoints.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub(crate) const ROUTE_KIND: &str = "openai-siwc";
/// The route that borrowed Codex CLI's client; no longer used for requests.
pub(crate) const RETIRED_ROUTE_KIND: &str = "openai-codex";
pub(crate) const AUTHORIZATION_URL: &str = "https://auth.openai.com/api/accounts/authorize";
pub(crate) const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub(crate) const RESOURCE: &str = "https://api.openai.com/v1";
pub(crate) const RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
pub(crate) const MODELS_URL: &str = "https://api.openai.com/v1/models";
pub(crate) const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const PLAN_USAGE_SCOPE: &str = "chatgpt.tokens.use.direct";
const DYNAMIC_CLIENT: &str = "dynamic_agent_client";
const AGENT_NAME: &str = "AI-OS";
const ISSUER: &str = "https://auth.openai.com";
const REGISTRATIONS_FILE: &str = "openai-sign-in.json";

pub(crate) const RECONNECT_MESSAGE: &str =
    "AI-OS now connects ChatGPT through OpenAI's official Sign in with ChatGPT. Sign in to OpenAI again in My AI.";

/// Not secrets: the stable host id and the client id OpenAI issued to each
/// connection (tokens stay in Keychain).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Registrations {
    ext_agent_host_id: Option<String>,
    #[serde(default)]
    clients: BTreeMap<String, String>,
}

fn read_registrations(directory: &Path) -> Registrations {
    std::fs::read_to_string(directory.join(REGISTRATIONS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_registrations(directory: &Path, registrations: &Registrations) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|_| "AI-OS could not save the OpenAI sign-in registration".to_owned())?;
    let path = directory.join(REGISTRATIONS_FILE);
    let temporary = directory.join(format!("{REGISTRATIONS_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(registrations)
        .map_err(|_| "AI-OS could not save the OpenAI sign-in registration".to_owned())?;
    std::fs::write(&temporary, bytes)
        .and_then(|_| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
            }
            std::fs::rename(&temporary, &path)
        })
        .map_err(|_| "AI-OS could not save the OpenAI sign-in registration".to_owned())
}

/// This Mac's stable `ext_agent_host_id`, created once.
pub(crate) fn host_id(directory: &Path) -> Result<String, String> {
    let mut registrations = read_registrations(directory);
    if let Some(host_id) = registrations.ext_agent_host_id.clone() {
        return Ok(host_id);
    }
    let host_id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
    registrations.ext_agent_host_id = Some(host_id.clone());
    write_registrations(directory, &registrations)?;
    Ok(host_id)
}

pub(crate) fn registered_client(directory: &Path, instance_id: &str) -> Option<String> {
    read_registrations(directory)
        .clients
        .get(instance_id)
        .cloned()
}

pub(crate) fn remember_client(
    directory: &Path,
    instance_id: &str,
    client_id: &str,
) -> Result<(), String> {
    let mut registrations = read_registrations(directory);
    registrations
        .clients
        .insert(instance_id.to_owned(), client_id.to_owned());
    write_registrations(directory, &registrations)
}

/// The client to authorize with and the parameters beyond the generic
/// OAuth/PKCE ones. A first sign-in registers (`dynamic_agent_client` plus
/// the app name); a reauthorization uses the issued client and no name.
pub(crate) fn authorization(
    issued_client: Option<&str>,
    host_id: &str,
    nonce: &str,
) -> (String, Vec<(&'static str, String)>) {
    let mut parameters = vec![
        ("ext_agent_host_id", host_id.to_owned()),
        ("resource", RESOURCE.to_owned()),
        ("nonce", nonce.to_owned()),
    ];
    match issued_client {
        Some(client) => (client.to_owned(), parameters),
        None => {
            parameters.push(("agent_name_hint", AGENT_NAME.to_owned()));
            (DYNAMIC_CLIENT.to_owned(), parameters)
        }
    }
}

pub(crate) fn is_registration(client_id: &str) -> bool {
    client_id == DYNAMIC_CLIENT
}

/// The client the code is exchanged with: a registration must return its
/// newly issued client; a reauthorization may omit it but never change it.
pub(crate) fn exchange_client(
    session_client: &str,
    callback_client: Option<&str>,
) -> Result<String, String> {
    let callback_client = callback_client
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if is_registration(session_client) {
        let issued = callback_client
            .ok_or_else(|| "OpenAI sign-in did not return a registered client".to_owned())?;
        if issued.len() > 256
            || !issued
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
        {
            return Err("OpenAI sign-in returned an invalid client".to_owned());
        }
        return Ok(issued.to_owned());
    }
    match callback_client {
        Some(client) if client != session_client => {
            Err("OpenAI sign-in returned a different client; sign in again".to_owned())
        }
        _ => Ok(session_client.to_owned()),
    }
}

fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()
}

/// The verified account behind a sign-in.
#[derive(Debug, PartialEq)]
pub(crate) struct SignedInAccount {
    pub(crate) subject: String,
    pub(crate) email: Option<String>,
}

/// Checks the ID token's issuer, audience (the issued client), expiry and
/// nonce. It arrives straight from OpenAI's token endpoint over TLS, which
/// OpenID Connect accepts in place of a signature check for this flow.
pub(crate) fn validate_id_token(
    id_token: &str,
    client_id: &str,
    nonce: &str,
    now: i64,
) -> Result<SignedInAccount, String> {
    let invalid = || "OpenAI sign-in returned an invalid identity token".to_owned();
    let claims = jwt_claims(id_token).ok_or_else(invalid)?;
    if claims.get("iss").and_then(Value::as_str) != Some(ISSUER) {
        return Err(invalid());
    }
    let audience_matches = match claims.get("aud") {
        Some(Value::String(audience)) => audience == client_id,
        Some(Value::Array(audiences)) => audiences
            .iter()
            .any(|value| value.as_str() == Some(client_id)),
        _ => false,
    };
    if !audience_matches
        || claims.get("nonce").and_then(Value::as_str) != Some(nonce)
        || claims
            .get("exp")
            .and_then(Value::as_i64)
            .is_none_or(|expiry| expiry <= now)
    {
        return Err(invalid());
    }
    let subject = claims
        .get("sub")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or_else(invalid)?
        .to_owned();
    Ok(SignedInAccount {
        subject,
        email: claims
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

/// A valid identity alone does not allow plan usage; the grant must include it.
pub(crate) fn grants_plan_usage(token: &Value) -> bool {
    token
        .get("scope")
        .and_then(Value::as_str)
        .is_some_and(|scope| {
            scope
                .split_whitespace()
                .any(|item| item == PLAN_USAGE_SCOPE)
        })
}

/// Responses API body for plan usage: `store: false`, `stream: true`, and
/// system guidance as `instructions` (system message items are rejected).
pub(crate) fn responses_body(model_id: &str, messages: &[(String, String)]) -> Value {
    let instructions = messages
        .iter()
        .filter(|(role, _)| role == "system")
        .map(|(_, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let input: Vec<Value> = messages
        .iter()
        .filter(|(role, _)| role != "system")
        .map(|(role, content)| serde_json::json!({"role": role, "content": content}))
        .collect();
    let mut body = serde_json::json!({
        "model": model_id,
        "input": input,
        "store": false,
        "stream": true,
    });
    if !instructions.is_empty() {
        body["instructions"] = Value::String(instructions);
    }
    body
}

/// A `response.failed` stream event's reason, in words the user can act on.
pub(crate) fn stream_failure(event: &Value) -> Option<String> {
    if event.get("type").and_then(Value::as_str) != Some("response.failed") {
        return None;
    }
    let code = event
        .pointer("/response/error/code")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Some(match code {
        "subscription_sharing_usage_limit_exceeded" => {
            "Usage limit reached. Review your plan or this app's limit in ChatGPT settings (chatgpt.com/settings/usage).".to_owned()
        }
        "subscription_sharing_usage_unavailable" => {
            "ChatGPT plan usage is not available for this account right now.".to_owned()
        }
        other => format!("OpenAI did not complete the response ({other})."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_token(claims: Value) -> String {
        format!(
            "e30.{}.signature",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    #[test]
    fn host_id_is_created_once_and_clients_are_kept_per_connection() {
        let directory = tempfile::tempdir().unwrap();
        let first = host_id(directory.path()).unwrap();
        assert!(first.starts_with("urn:uuid:"));
        assert_eq!(host_id(directory.path()).unwrap(), first);
        assert_eq!(registered_client(directory.path(), "openai-default"), None);
        remember_client(directory.path(), "openai-default", "oaiapp_123").unwrap();
        remember_client(directory.path(), "openai-work", "oaiapp_456").unwrap();
        assert_eq!(
            registered_client(directory.path(), "openai-default").as_deref(),
            Some("oaiapp_123")
        );
        assert_eq!(host_id(directory.path()).unwrap(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(directory.path().join(REGISTRATIONS_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn first_sign_in_registers_and_later_ones_reuse_the_issued_client() {
        let (client, parameters) = authorization(None, "urn:uuid:host", "n1");
        assert_eq!(client, "dynamic_agent_client");
        assert!(parameters.contains(&("agent_name_hint", "AI-OS".to_owned())));
        assert!(parameters.contains(&("resource", "https://api.openai.com/v1".to_owned())));
        assert!(parameters.contains(&("ext_agent_host_id", "urn:uuid:host".to_owned())));
        let (client, parameters) = authorization(Some("oaiapp_123"), "urn:uuid:host", "n2");
        assert_eq!(client, "oaiapp_123");
        assert!(!parameters.iter().any(|(key, _)| *key == "agent_name_hint"));
    }

    #[test]
    fn exchange_uses_the_issued_client_and_rejects_a_changed_one() {
        assert_eq!(
            exchange_client("dynamic_agent_client", Some("oaiapp_new")).unwrap(),
            "oaiapp_new"
        );
        assert!(exchange_client("dynamic_agent_client", None).is_err());
        assert!(exchange_client("dynamic_agent_client", Some("bad client")).is_err());
        assert_eq!(exchange_client("oaiapp_123", None).unwrap(), "oaiapp_123");
        assert_eq!(
            exchange_client("oaiapp_123", Some("oaiapp_123")).unwrap(),
            "oaiapp_123"
        );
        assert!(exchange_client("oaiapp_123", Some("oaiapp_other")).is_err());
    }

    #[test]
    fn id_token_must_match_issuer_client_nonce_and_be_unexpired() {
        let good = serde_json::json!({"iss": ISSUER, "aud": "oaiapp_1", "nonce": "n", "exp": 2000, "sub": "user-1", "email": "a@b.c"});
        assert_eq!(
            validate_id_token(&id_token(good.clone()), "oaiapp_1", "n", 1000).unwrap(),
            SignedInAccount {
                subject: "user-1".into(),
                email: Some("a@b.c".into())
            }
        );
        let mut array_audience = good.clone();
        array_audience["aud"] = serde_json::json!(["other", "oaiapp_1"]);
        assert!(validate_id_token(&id_token(array_audience), "oaiapp_1", "n", 1000).is_ok());
        for (key, value) in [
            ("iss", serde_json::json!("https://evil")),
            ("aud", serde_json::json!("oaiapp_2")),
            ("nonce", serde_json::json!("x")),
            ("exp", serde_json::json!(999)),
        ] {
            let mut claims = good.clone();
            claims[key] = value;
            assert!(
                validate_id_token(&id_token(claims), "oaiapp_1", "n", 1000).is_err(),
                "{key}"
            );
        }
        assert!(validate_id_token("not-a-jwt", "oaiapp_1", "n", 1000).is_err());
    }

    #[test]
    fn plan_usage_requires_the_granted_scope() {
        assert!(grants_plan_usage(
            &serde_json::json!({"scope": "chatgpt.tokens.use.direct email offline_access openid profile resource.invoke"})
        ));
        assert!(!grants_plan_usage(
            &serde_json::json!({"scope": "openid profile email"})
        ));
        assert!(!grants_plan_usage(&serde_json::json!({})));
    }

    #[test]
    fn responses_body_moves_system_text_to_instructions() {
        let body = responses_body(
            "gpt-6.1-sol",
            &[
                ("system".into(), "Be brief.".into()),
                ("user".into(), "Hi".into()),
                ("assistant".into(), "Hello".into()),
                ("user".into(), "Again".into()),
            ],
        );
        assert_eq!(body["instructions"], "Be brief.");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 3);
        assert!(input.iter().all(|item| item["role"] != "system"));
        assert!(responses_body("m", &[("user".into(), "Hi".into())])
            .get("instructions")
            .is_none());
    }

    #[test]
    fn plan_limit_failures_are_explained() {
        let event = serde_json::json!({"type": "response.failed", "response": {"error": {"code": "subscription_sharing_usage_limit_exceeded"}}});
        assert!(stream_failure(&event)
            .unwrap()
            .contains("chatgpt.com/settings/usage"));
        assert!(stream_failure(
            &serde_json::json!({"type": "response.output_text.delta", "delta": "x"})
        )
        .is_none());
    }
}

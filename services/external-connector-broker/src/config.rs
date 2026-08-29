use base64::Engine;
use std::{env, net::SocketAddr};
use url::Url;

use crate::contract::Environment;

#[derive(Clone)]
pub struct BrokerConfig {
    pub environment: Environment,
    pub client_id: String,
    pub client_secret: String,
    pub ru_name: String,
    pub public_base_url: Url,
    pub encryption_key: [u8; 32],
    pub buy_api_approved: bool,
    pub bind_address: SocketAddr,
    pub ebay_api_base_url: Url,
    pub ebay_authorization_url: Url,
    pub ebay_token_url: Url,
}

impl BrokerConfig {
    pub fn from_environment() -> Result<Self, String> {
        let environment = match required("EBAY_ENVIRONMENT")?.as_str() {
            "SANDBOX" => Environment::Sandbox,
            "PRODUCTION" => Environment::Production,
            _ => return Err("EBAY_ENVIRONMENT must be SANDBOX or PRODUCTION".to_owned()),
        };
        let public_base_url = Url::parse(&required("PUBLIC_BROKER_BASE_URL")?)
            .map_err(|_| "PUBLIC_BROKER_BASE_URL is invalid".to_owned())?;
        validate_public_url(&public_base_url, &environment)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(required("BROKER_ENCRYPTION_KEY")?)
            .map_err(|_| "BROKER_ENCRYPTION_KEY must be base64".to_owned())?;
        let encryption_key: [u8; 32] = decoded
            .try_into()
            .map_err(|_| "BROKER_ENCRYPTION_KEY must decode to exactly 32 bytes".to_owned())?;
        let bind_address = env::var("BROKER_BIND_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1:8787".to_owned())
            .parse()
            .map_err(|_| "BROKER_BIND_ADDRESS is invalid".to_owned())?;
        let (api, authorize, token) = match environment {
            Environment::Sandbox => (
                "https://api.sandbox.ebay.com/",
                "https://auth.sandbox.ebay.com/oauth2/authorize",
                "https://api.sandbox.ebay.com/identity/v1/oauth2/token",
            ),
            Environment::Production => (
                "https://api.ebay.com/",
                "https://auth.ebay.com/oauth2/authorize",
                "https://api.ebay.com/identity/v1/oauth2/token",
            ),
            Environment::Development => {
                return Err("Broker eBay environment cannot be DEVELOPMENT".to_owned())
            }
        };
        Ok(Self {
            environment,
            client_id: required("EBAY_CLIENT_ID")?,
            client_secret: required("EBAY_CLIENT_SECRET")?,
            ru_name: required("EBAY_RUNAME")?,
            public_base_url,
            encryption_key,
            buy_api_approved: env::var("EBAY_BUY_API_APPROVED").is_ok_and(|value| value == "true"),
            bind_address,
            ebay_api_base_url: Url::parse(api).unwrap(),
            ebay_authorization_url: Url::parse(authorize).unwrap(),
            ebay_token_url: Url::parse(token).unwrap(),
        })
    }
}

fn required(name: &str) -> Result<String, String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn validate_public_url(url: &Url, environment: &Environment) -> Result<(), String> {
    let localhost = matches!(url.host_str(), Some("localhost" | "127.0.0.1"));
    if url.scheme() == "https"
        || (matches!(environment, Environment::Sandbox) && url.scheme() == "http" && localhost)
    {
        return Ok(());
    }
    Err("PUBLIC_BROKER_BASE_URL must use HTTPS; Sandbox allows localhost HTTP".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_rejects_http_and_sandbox_allows_localhost_only() {
        assert!(validate_public_url(
            &Url::parse("http://broker.example").unwrap(),
            &Environment::Production
        )
        .is_err());
        assert!(validate_public_url(
            &Url::parse("http://localhost:8787").unwrap(),
            &Environment::Sandbox
        )
        .is_ok());
        assert!(validate_public_url(
            &Url::parse("http://broker.example").unwrap(),
            &Environment::Sandbox
        )
        .is_err());
    }
}

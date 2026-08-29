use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const CONNECTOR_ID: &str = "ebay-buy";
pub const CONTRACT_VERSION: &str = "1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Environment {
    Sandbox,
    Production,
    Development,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicConfigurationField {
    pub name: String,
    pub label: String,
    pub required: bool,
    pub allowed_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDefinition {
    pub capability_id: String,
    pub required_scopes: Vec<String>,
    pub confirmation_required: bool,
    pub approval_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorManifest {
    pub connector_id: String,
    pub display_name: String,
    pub connector_version: String,
    pub interface_kind: String,
    pub environments: Vec<Environment>,
    pub public_configuration_schema: Vec<PublicConfigurationField>,
    pub broker_contract_version: String,
    pub authorization_kind: String,
    pub authorization_start_mode: String,
    pub official_authorization_hosts: Vec<String>,
    pub callback_strategy: String,
    pub capability_definitions: Vec<CapabilityDefinition>,
    pub required_scopes: Vec<String>,
    pub approval_requirements: Vec<String>,
    pub confirmation_policy: String,
    pub disconnect_policy: String,
    pub health_check_policy: String,
    pub error_mapping: BTreeMap<String, String>,
    pub sensitive_field_denylist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityState {
    pub capability_id: String,
    pub available: bool,
    pub environment: Environment,
    pub authorization_state: String,
    pub approval_state: String,
    pub required_scopes: Vec<String>,
    pub confirmation_required: bool,
    pub unavailable_reason: Option<String>,
    pub last_verified_at: Option<String>,
    pub evidence_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrokerEnvelope {
    pub connector_id: String,
    pub contract_version: String,
    pub environment: Environment,
    pub authorization_reference: Option<String>,
    pub authorization_url: Option<String>,
    pub granted_scopes: Vec<String>,
    pub approval_status: String,
    pub capabilities: Vec<CapabilityState>,
    pub identity_validated: bool,
    pub error_code: Option<String>,
}

impl BrokerEnvelope {
    pub fn empty(environment: Environment) -> Self {
        Self {
            connector_id: CONNECTOR_ID.to_owned(),
            contract_version: CONTRACT_VERSION.to_owned(),
            environment,
            authorization_reference: None,
            authorization_url: None,
            granted_scopes: vec![],
            approval_status: "NOT_VERIFIED".to_owned(),
            capabilities: vec![],
            identity_validated: false,
            error_code: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationRequest {
    pub public_configuration: BTreeMap<String, String>,
    pub environment: Environment,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallbackRequest {
    pub state: String,
    pub code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorizationReferenceRequest {
    pub authorization_reference: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecuteRequest {
    pub authorization_reference: String,
    pub capability_id: String,
    pub confirmed: bool,
    #[serde(default)]
    pub input: Value,
}

pub fn ebay_manifest() -> ConnectorManifest {
    let capability = |id: &str, approval: bool, confirmation: bool| CapabilityDefinition {
        capability_id: id.to_owned(),
        required_scopes: vec!["buy.api".to_owned()],
        confirmation_required: confirmation,
        approval_required: approval,
    };
    ConnectorManifest {
        connector_id: CONNECTOR_ID.to_owned(),
        display_name: "eBay".to_owned(),
        connector_version: "1.0.0".to_owned(),
        interface_kind: "BACKEND_BROKER".to_owned(),
        environments: vec![Environment::Sandbox, Environment::Production],
        public_configuration_schema: vec![
            PublicConfigurationField {
                name: "environment".to_owned(),
                label: "Environment".to_owned(),
                required: true,
                allowed_values: vec!["SANDBOX".to_owned(), "PRODUCTION".to_owned()],
            },
            PublicConfigurationField {
                name: "clientId".to_owned(),
                label: "App ID / Client ID".to_owned(),
                required: true,
                allowed_values: vec![],
            },
            PublicConfigurationField {
                name: "ruName".to_owned(),
                label: "RuName".to_owned(),
                required: true,
                allowed_values: vec![],
            },
        ],
        broker_contract_version: CONTRACT_VERSION.to_owned(),
        authorization_kind: "OAUTH_VIA_BROKER".to_owned(),
        authorization_start_mode: "BROKER".to_owned(),
        official_authorization_hosts: vec![
            "auth.ebay.com".to_owned(),
            "auth.sandbox.ebay.com".to_owned(),
        ],
        callback_strategy: "EBAY_RUNAME".to_owned(),
        capability_definitions: vec![
            capability("ebay.browse.search", false, false),
            capability("ebay.browse.item.read", false, false),
            capability("ebay.cart.read", true, false),
            capability("ebay.cart.add", true, false),
            capability("ebay.cart.update", true, false),
            capability("ebay.cart.remove", true, false),
            capability("ebay.checkout.prepare", true, false),
            capability("ebay.checkout.confirm", true, true),
            capability("ebay.order.list", true, false),
            capability("ebay.order.read", true, false),
        ],
        required_scopes: vec!["buy.api".to_owned()],
        approval_requirements: vec!["Production Buy API approval".to_owned()],
        confirmation_policy: "MANIFEST_CAPABILITY".to_owned(),
        disconnect_policy: "BROKER_REVOKE_THEN_LOCAL".to_owned(),
        health_check_policy: "IDENTITY_REQUIRED".to_owned(),
        error_mapping: BTreeMap::new(),
        sensitive_field_denylist: [
            "clientsecret",
            "certid",
            "accesstoken",
            "refreshtoken",
            "password",
            "cookie",
            "sessiontoken",
            "apisecret",
            "privatekey",
            "authorizationcode",
            "token",
            "secret",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

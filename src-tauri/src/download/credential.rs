use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadCredentialType {
    UsernamePassword,
    Token,
    Cookie,
    ApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadCredential {
    pub id: String,

    pub provider: String,

    pub credential_type: DownloadCredentialType,

    #[serde(default)]
    pub data: Value,
}

pub trait DownloadCredentialStore: Send + Sync {
    fn get(&self, credential_id: &str) -> Result<DownloadCredential, String>;
}

pub struct ProviderDownloadCredentialStore;

impl ProviderDownloadCredentialStore {
    fn load_provider_instance(
        &self,
        credential_id: &str,
    ) -> Result<crate::providers::ProviderInstance, String> {
        let instances =
            crate::providers::list_provider_instances().map_err(|error| error.to_string())?;

        instances
            .into_iter()
            .find(|instance| instance.id == credential_id)
            .ok_or_else(|| format!("provider credential not found: {}", credential_id))
    }
}

impl DownloadCredentialStore for ProviderDownloadCredentialStore {
    fn get(&self, credential_id: &str) -> Result<DownloadCredential, String> {
        let instance = self.load_provider_instance(credential_id)?;

        let credential_type = match instance.credential.kind {
            crate::providers::ProviderCredentialKind::OAuth => DownloadCredentialType::Token,

            crate::providers::ProviderCredentialKind::ApiKey => DownloadCredentialType::ApiKey,

            crate::providers::ProviderCredentialKind::Local => DownloadCredentialType::Token,
        };

        Ok(DownloadCredential {
            id: instance.id,
            provider: instance.provider_id,
            credential_type,
            data: serde_json::json!({
                "keychainAccount":
                    instance.credential.keychain_account,
                "refreshable":
                    instance.credential.refreshable,
            }),
        })
    }
}

pub struct EmptyDownloadCredentialStore;

impl DownloadCredentialStore for EmptyDownloadCredentialStore {
    fn get(&self, credential_id: &str) -> Result<DownloadCredential, String> {
        Err(format!("download credential not found: {}", credential_id))
    }
}

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::{contract::Environment, security::TokenCipher};

#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    pub reference: String,
    pub environment: Environment,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub refresh_expires_at: Option<DateTime<Utc>>,
    pub granted_scopes: Vec<String>,
    pub identity_validated: bool,
    pub account_marker: String,
}

pub trait SecureAuthorizationStorage: Send + Sync {
    fn create_state(&self, state: String, pending: PendingAuthorization) -> Result<(), String>;
    fn consume_state(&self, state: &str) -> Result<PendingAuthorization, String>;
    fn save_tokens(&self, reference: &str, record: &TokenRecord) -> Result<(), String>;
    fn load_tokens(&self, reference: &str) -> Result<Option<TokenRecord>, String>;
    fn delete_tokens(&self, reference: &str) -> Result<bool, String>;
}

#[derive(Clone)]
pub struct MemorySecureStorage {
    states: Arc<RwLock<HashMap<String, PendingAuthorization>>>,
    encrypted_tokens: Arc<RwLock<HashMap<String, String>>>,
    cipher: TokenCipher,
    token_store_path: Option<Arc<PathBuf>>,
}

impl MemorySecureStorage {
    pub fn new(cipher: TokenCipher) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            encrypted_tokens: Arc::new(RwLock::new(HashMap::new())),
            cipher,
            token_store_path: None,
        }
    }

    pub fn persistent(cipher: TokenCipher, path: PathBuf) -> Result<Self, String> {
        let encrypted_tokens = if path.exists() {
            let bytes = std::fs::read(&path).map_err(|_| "TOKEN_STORAGE_UNAVAILABLE".to_owned())?;
            serde_json::from_slice(&bytes).map_err(|_| "TOKEN_STORAGE_CORRUPT".to_owned())?
        } else {
            HashMap::new()
        };
        Ok(Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            encrypted_tokens: Arc::new(RwLock::new(encrypted_tokens)),
            cipher,
            token_store_path: Some(Arc::new(path)),
        })
    }

    fn persist(&self, tokens: &HashMap<String, String>) -> Result<(), String> {
        let Some(path) = self.token_store_path.as_deref() else {
            return Ok(());
        };
        let parent = path
            .parent()
            .ok_or_else(|| "TOKEN_STORAGE_UNAVAILABLE".to_owned())?;
        std::fs::create_dir_all(parent).map_err(|_| "TOKEN_STORAGE_UNAVAILABLE".to_owned())?;
        let temporary = path.with_extension("tmp");
        let bytes = serde_json::to_vec(tokens).map_err(|_| "TOKEN_ENCODING_FAILED".to_owned())?;
        std::fs::write(&temporary, bytes).map_err(|_| "TOKEN_STORAGE_UNAVAILABLE".to_owned())?;
        set_private_permissions(&temporary)?;
        std::fs::rename(&temporary, path).map_err(|_| "TOKEN_STORAGE_UNAVAILABLE".to_owned())?;
        set_private_permissions(path)
    }
    pub fn pending(reference: String, environment: Environment) -> PendingAuthorization {
        PendingAuthorization {
            reference,
            environment,
            expires_at: Utc::now() + Duration::minutes(10),
        }
    }
}

impl SecureAuthorizationStorage for MemorySecureStorage {
    fn create_state(&self, state: String, pending: PendingAuthorization) -> Result<(), String> {
        let mut states = self
            .states
            .write()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?;
        if states.insert(state, pending).is_some() {
            return Err("OAUTH_STATE_COLLISION".to_owned());
        }
        Ok(())
    }

    fn consume_state(&self, state: &str) -> Result<PendingAuthorization, String> {
        let pending = self
            .states
            .write()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?
            .remove(state)
            .ok_or_else(|| "OAUTH_STATE_INVALID_OR_USED".to_owned())?;
        if pending.expires_at <= Utc::now() {
            return Err("OAUTH_STATE_EXPIRED".to_owned());
        }
        Ok(pending)
    }

    fn save_tokens(&self, reference: &str, record: &TokenRecord) -> Result<(), String> {
        let encoded = serde_json::to_vec(record).map_err(|_| "TOKEN_ENCODING_FAILED".to_owned())?;
        let sealed = self.cipher.encrypt(&encoded)?;
        let mut tokens = self
            .encrypted_tokens
            .write()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?;
        tokens.insert(reference.to_owned(), sealed);
        self.persist(&tokens)
    }

    fn load_tokens(&self, reference: &str) -> Result<Option<TokenRecord>, String> {
        let sealed = self
            .encrypted_tokens
            .read()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?
            .get(reference)
            .cloned();
        sealed
            .map(|value| {
                self.cipher.decrypt(&value).and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|_| "TOKEN_STORAGE_CORRUPT".to_owned())
                })
            })
            .transpose()
    }

    fn delete_tokens(&self, reference: &str) -> Result<bool, String> {
        let mut tokens = self
            .encrypted_tokens
            .write()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?;
        let removed = tokens.remove(reference).is_some();
        self.persist(&tokens)?;
        Ok(removed)
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "TOKEN_STORAGE_UNAVAILABLE".to_owned())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_state_is_single_use_and_tokens_are_deleted() {
        let storage = MemorySecureStorage::new(TokenCipher::new([9u8; 32]));
        storage
            .create_state(
                "state".to_owned(),
                MemorySecureStorage::pending("reference".to_owned(), Environment::Sandbox),
            )
            .unwrap();
        assert!(storage.consume_state("state").is_ok());
        assert!(storage.consume_state("state").is_err());
        let record = TokenRecord {
            access_token: "fake".to_owned(),
            refresh_token: Some("fake-refresh".to_owned()),
            expires_at: Utc::now() + Duration::hours(2),
            refresh_expires_at: Some(Utc::now() + Duration::days(30)),
            granted_scopes: vec!["identity.readonly".to_owned()],
            identity_validated: true,
            account_marker: "marker".to_owned(),
        };
        storage.save_tokens("reference", &record).unwrap();
        assert!(storage.load_tokens("reference").unwrap().is_some());
        assert!(storage.delete_tokens("reference").unwrap());
        assert!(storage.load_tokens("reference").unwrap().is_none());
    }

    #[test]
    fn expired_oauth_state_is_consumed_and_fails_closed() {
        let storage = MemorySecureStorage::new(TokenCipher::new([6u8; 32]));
        storage
            .create_state(
                "expired-state".to_owned(),
                PendingAuthorization {
                    reference: "reference".to_owned(),
                    environment: Environment::Sandbox,
                    expires_at: Utc::now() - Duration::seconds(1),
                },
            )
            .unwrap();
        assert_eq!(
            storage.consume_state("expired-state").unwrap_err(),
            "OAUTH_STATE_EXPIRED"
        );
        assert_eq!(
            storage.consume_state("expired-state").unwrap_err(),
            "OAUTH_STATE_INVALID_OR_USED"
        );
    }

    #[test]
    fn encrypted_tokens_survive_restart_without_plaintext_on_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tokens.json");
        let record = TokenRecord {
            access_token: "sensitive-access".to_owned(),
            refresh_token: Some("sensitive-refresh".to_owned()),
            expires_at: Utc::now() + Duration::hours(2),
            refresh_expires_at: None,
            granted_scopes: vec!["identity.readonly".to_owned()],
            identity_validated: true,
            account_marker: "marker".to_owned(),
        };
        MemorySecureStorage::persistent(TokenCipher::new([8u8; 32]), path.clone())
            .unwrap()
            .save_tokens("reference", &record)
            .unwrap();
        let disk = std::fs::read_to_string(&path).unwrap();
        assert!(!disk.contains("sensitive-access"));
        assert!(!disk.contains("sensitive-refresh"));
        let restarted = MemorySecureStorage::persistent(TokenCipher::new([8u8; 32]), path).unwrap();
        assert_eq!(
            restarted
                .load_tokens("reference")
                .unwrap()
                .unwrap()
                .account_marker,
            "marker"
        );
    }
}

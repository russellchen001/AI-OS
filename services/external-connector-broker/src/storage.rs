use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
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
    pub granted_scopes: Vec<String>,
    pub identity_validated: bool,
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
}

impl MemorySecureStorage {
    pub fn new(cipher: TokenCipher) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            encrypted_tokens: Arc::new(RwLock::new(HashMap::new())),
            cipher,
        }
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
        self.encrypted_tokens
            .write()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?
            .insert(reference.to_owned(), sealed);
        Ok(())
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
        Ok(self
            .encrypted_tokens
            .write()
            .map_err(|_| "STORAGE_UNAVAILABLE".to_owned())?
            .remove(reference)
            .is_some())
    }
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
            granted_scopes: vec!["buy.api".to_owned()],
            identity_validated: true,
        };
        storage.save_tokens("reference", &record).unwrap();
        assert!(storage.load_tokens("reference").unwrap().is_some());
        assert!(storage.delete_tokens("reference").unwrap());
        assert!(storage.load_tokens("reference").unwrap().is_none());
    }
}

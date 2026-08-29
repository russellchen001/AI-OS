use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::Engine;

use crate::contract::{Environment, CONNECTOR_ID};

#[derive(Clone)]
pub struct TokenCipher {
    cipher: Aes256Gcm,
}

impl TokenCipher {
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new((&key).into()),
        }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String, String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| "TOKEN_ENCRYPTION_FAILED".to_owned())?;
        let mut sealed = nonce.to_vec();
        sealed.extend(ciphertext);
        Ok(base64::engine::general_purpose::STANDARD.encode(sealed))
    }

    pub fn decrypt(&self, sealed: &str) -> Result<Vec<u8>, String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(sealed)
            .map_err(|_| "TOKEN_STORAGE_CORRUPT".to_owned())?;
        if bytes.len() <= 12 {
            return Err("TOKEN_STORAGE_CORRUPT".to_owned());
        }
        let (nonce, ciphertext) = bytes.split_at(12);
        self.cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| "TOKEN_DECRYPTION_FAILED".to_owned())
    }
}

pub fn reference_prefix(environment: &Environment) -> &'static str {
    match environment {
        Environment::Sandbox => "ebay-buy:sandbox:",
        Environment::Production => "ebay-buy:production:",
        Environment::Development => "ebay-buy:development:",
    }
}

pub fn new_authorization_reference(environment: &Environment) -> String {
    format!(
        "{}{}",
        reference_prefix(environment),
        uuid::Uuid::new_v4().simple()
    )
}

pub fn validate_authorization_reference(
    reference: &str,
    environment: &Environment,
) -> Result<(), String> {
    if !reference.starts_with(reference_prefix(environment))
        || reference.len() <= reference_prefix(environment).len()
    {
        return Err("AUTHORIZATION_REFERENCE_SCOPE_MISMATCH".to_owned());
    }
    if !reference.starts_with(CONNECTOR_ID) {
        return Err("CONNECTOR_ID_MISMATCH".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_encrypted_and_environment_references_are_isolated() {
        let cipher = TokenCipher::new([7u8; 32]);
        let sealed = cipher.encrypt(b"fake-access-token").unwrap();
        assert!(!sealed.contains("fake-access-token"));
        assert_eq!(cipher.decrypt(&sealed).unwrap(), b"fake-access-token");
        let sandbox = new_authorization_reference(&Environment::Sandbox);
        assert!(validate_authorization_reference(&sandbox, &Environment::Sandbox).is_ok());
        assert!(validate_authorization_reference(&sandbox, &Environment::Production).is_err());
    }
}

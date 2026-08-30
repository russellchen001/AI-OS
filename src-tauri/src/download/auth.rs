use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const THUNDER_AUTH_FILE: &str = "thunder-auth.json";
const THUNDER_KEYCHAIN_SERVICE: &str = "com.ai-os.download.thunder";
const THUNDER_KEYCHAIN_ACCOUNT: &str = "managed-login";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThunderAuthMode {
    Unset,
    Manual,
    Managed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThunderAuthSettings {
    pub mode: ThunderAuthMode,
    pub has_managed_credential: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetThunderAuthModeInput {
    pub mode: ThunderAuthMode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetThunderManagedCredentialInput {
    pub secret: String,
}

fn auth_file() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Thunder auth configuration directory unavailable.".to_string())?
        .join("AI OS");

    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;

    Ok(dir.join(THUNDER_AUTH_FILE))
}

fn load_mode() -> Result<ThunderAuthMode, String> {
    let path = auth_file()?;

    if !path.exists() {
        return Ok(ThunderAuthMode::Unset);
    }

    let data = fs::read_to_string(path).map_err(|error| error.to_string())?;

    serde_json::from_str(&data).map_err(|error| error.to_string())
}

fn save_mode(mode: &ThunderAuthMode) -> Result<(), String> {
    let path = auth_file()?;

    let data = serde_json::to_string_pretty(mode).map_err(|error| error.to_string())?;

    fs::write(path, data).map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn managed_credential_exists() -> Result<bool, String> {
    crate::keychain_trace::record(
        "READ",
        "thunder_credential_status",
        THUNDER_KEYCHAIN_SERVICE,
        THUNDER_KEYCHAIN_ACCOUNT,
        "MISS",
    );
    match security_framework::passwords::get_generic_password(
        THUNDER_KEYCHAIN_SERVICE,
        THUNDER_KEYCHAIN_ACCOUNT,
    ) {
        Ok(secret) => Ok(!secret.is_empty()),

        Err(error) if error.code() == -25300 => Ok(false),

        Err(_) => Err("Thunder managed credential status is unavailable.".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
fn managed_credential_exists() -> Result<bool, String> {
    Ok(false)
}

#[cfg(target_os = "macos")]
fn store_managed_credential(secret: &str) -> Result<(), String> {
    crate::keychain_trace::record(
        "WRITE_UPSERT",
        "thunder_credential_set",
        THUNDER_KEYCHAIN_SERVICE,
        THUNDER_KEYCHAIN_ACCOUNT,
        "UPDATE",
    );
    security_framework::passwords::set_generic_password(
        THUNDER_KEYCHAIN_SERVICE,
        THUNDER_KEYCHAIN_ACCOUNT,
        secret.as_bytes(),
    )
    .map_err(|_| "macOS Keychain could not store the Thunder credential.".to_string())
}

#[cfg(not(target_os = "macos"))]
fn store_managed_credential(_secret: &str) -> Result<(), String> {
    Err("Thunder managed credentials are not supported on this platform.".to_string())
}

#[cfg(target_os = "macos")]
fn remove_managed_credential() -> Result<(), String> {
    crate::keychain_trace::record(
        "DELETE",
        "thunder_credential_delete",
        THUNDER_KEYCHAIN_SERVICE,
        THUNDER_KEYCHAIN_ACCOUNT,
        "INVALIDATE",
    );
    match security_framework::passwords::delete_generic_password(
        THUNDER_KEYCHAIN_SERVICE,
        THUNDER_KEYCHAIN_ACCOUNT,
    ) {
        Ok(()) => Ok(()),

        Err(error) if error.code() == -25300 => Ok(()),

        Err(_) => Err("macOS Keychain could not remove the Thunder credential.".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
fn remove_managed_credential() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn get_thunder_auth_settings() -> Result<ThunderAuthSettings, String> {
    Ok(ThunderAuthSettings {
        mode: load_mode()?,
        has_managed_credential: managed_credential_exists()?,
    })
}

#[tauri::command]
pub fn set_thunder_auth_mode(
    input: SetThunderAuthModeInput,
) -> Result<ThunderAuthSettings, String> {
    if input.mode == ThunderAuthMode::Manual {
        remove_managed_credential()?;
    }

    save_mode(&input.mode)?;

    get_thunder_auth_settings()
}

#[tauri::command]
pub fn set_thunder_managed_credential(
    input: SetThunderManagedCredentialInput,
) -> Result<ThunderAuthSettings, String> {
    if input.secret.trim().is_empty() {
        return Err("Thunder credential must not be empty.".to_string());
    }

    store_managed_credential(input.secret.trim())?;

    save_mode(&ThunderAuthMode::Managed)?;

    get_thunder_auth_settings()
}

#[tauri::command]
pub fn delete_thunder_managed_credential() -> Result<ThunderAuthSettings, String> {
    remove_managed_credential()?;

    get_thunder_auth_settings()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn auth_mode_serialization_is_stable() {
        assert_eq!(
            serde_json::to_string(&ThunderAuthMode::Manual).unwrap(),
            "\"manual\""
        );

        assert_eq!(
            serde_json::to_string(&ThunderAuthMode::Managed).unwrap(),
            "\"managed\""
        );
    }
}

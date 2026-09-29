const KEYCHAIN_SERVICE: &str = "com.ai-os.openclaw-gateway";
const MANAGED_ACCOUNT: &str = "managed-local";

pub(crate) const MANAGED_TOKEN_REF: &str = "keychain://com.ai-os.openclaw-gateway/managed-local";

#[cfg(target_os = "macos")]
pub(crate) fn store_managed_gateway_token(token: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        MANAGED_ACCOUNT,
        token.as_bytes(),
    )
    .map_err(|_| "AI-OS could not store the OpenClaw token in Keychain.".to_string())
}

#[cfg(target_os = "macos")]
fn read_managed_gateway_token() -> Result<String, String> {
    let value =
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, MANAGED_ACCOUNT)
            .map_err(|_| "AI-OS could not read the OpenClaw token from Keychain.".to_string())?;

    String::from_utf8(value).map_err(|_| "The OpenClaw Keychain token is invalid.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn store_managed_gateway_token(_token: &str) -> Result<(), String> {
    Err("Managed OpenClaw Keychain storage currently requires macOS.".to_string())
}

#[cfg(not(target_os = "macos"))]
fn read_managed_gateway_token() -> Result<String, String> {
    Err("Managed OpenClaw Keychain storage currently requires macOS.".to_string())
}

pub(crate) fn resolve_gateway_token(value: &str) -> Result<String, String> {
    if value == MANAGED_TOKEN_REF {
        read_managed_gateway_token()
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_gateway_tokens_pass_through_unchanged() {
        assert_eq!(
            resolve_gateway_token("existing-token").unwrap(),
            "existing-token"
        );
        assert_ne!(MANAGED_TOKEN_REF, "existing-token");
    }
}

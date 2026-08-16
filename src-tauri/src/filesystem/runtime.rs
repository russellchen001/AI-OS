use serde_json::Value;

use super::registry::resolve_filesystem_provider;


pub fn execute_filesystem_capability(
    capability: &str,
    input: Value,
) -> Result<Value, String> {

    let provider =
        resolve_filesystem_provider(
            "openclaw-filesystem"
        )
        .ok_or_else(|| {
            "filesystem provider unavailable".to_string()
        })?;


    let result =
        provider.execute(
            super::provider::FileRequest {
                capability: capability.to_string(),
                input,
            }
        )?;


    Ok(result.output)
}

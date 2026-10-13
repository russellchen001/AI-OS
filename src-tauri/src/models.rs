use serde_json::Value;

// The local-model skill is called from Runtime's synchronous operation worker.
// All oMLX management operations share the same authenticated provider adapter
// used by the UI. Never access a second local model server.
pub(crate) fn execute_local_model_capability(
    capability: &str,
    input: &Value,
    user_confirmed: bool,
) -> Result<Value, String> {
    if matches!(capability, "models.pull" | "models.delete") && !user_confirmed {
        return Err("User confirmation is required to manage models.".to_owned());
    }
    tauri::async_runtime::block_on(async move {
        match capability {
            "models.list" => serde_json::to_value(crate::providers::list_omlx_admin_models().await?)
                .map_err(|error| format!("Could not serialize oMLX models: {error}")),
            "models.show" => {
                let model = required_model_input(input)?;
                let details = crate::providers::show_omlx_model(model).await?;
                Ok(serde_json::json!({ "details": details }))
            }
            "models.pull" => {
                let model = required_model_input(input)?;
                let message = crate::providers::pull_omlx_model(model.clone()).await?;
                Ok(serde_json::json!({ "model": model, "message": message }))
            }
            "models.delete" => {
                let model = required_model_input(input)?;
                let message = crate::providers::delete_omlx_model(model.clone()).await?;
                Ok(serde_json::json!({ "model": model, "message": message }))
            }
            _ => Err(format!("Unsupported local model capability: {}", capability.trim())),
        }
    })
}

fn required_model_input(input: &Value) -> Result<String, String> {
    input.get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Model name is required.".to_owned())
}

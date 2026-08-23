use serde::{Deserialize, Serialize};

use serde_json::Value;

use std::{
    process::{Command, Stdio},
    time::Duration,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub families: Option<Vec<String>>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    pub model: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaPullProgress {
    pub status: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagModel {
    name: Option<String>,
    model: Option<String>,
    size: Option<u64>,
    digest: Option<String>,

    #[serde(rename = "modified_at")]
    modified_at: Option<String>,

    details: Option<OllamaTagDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagDetails {
    format: Option<String>,
    family: Option<String>,
    families: Option<Vec<String>>,

    #[serde(rename = "parameter_size")]
    parameter_size: Option<String>,

    #[serde(rename = "quantization_level")]
    quantization_level: Option<String>,
}

fn ollama_binary() -> String {
    let candidates = [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
        "ollama",
    ];

    for candidate in candidates {
        let status = Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if status.map(|value| value.success()).unwrap_or(false) {
            return candidate.to_string();
        }
    }

    "ollama".to_string()
}

fn run_ollama(arguments: &[&str]) -> Result<String, String> {
    let binary = ollama_binary();

    let output = Command::new(&binary)
        .args(arguments)
        .output()
        .map_err(|error| format!("Unable to run Ollama command '{}': {}", binary, error))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        return Err(if stderr.is_empty() {
            format!("Ollama command failed with status {}", output.status)
        } else {
            stderr
        });
    }

    Ok(if stdout.is_empty() { stderr } else { stdout })
}

fn ollama_api(method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
    let url = format!("http://127.0.0.1:11434{}", path);

    let mut command = Command::new("/usr/bin/curl");

    command
        .arg("--silent")
        .arg("--show-error")
        .arg("--fail")
        .arg("--max-time")
        .arg(Duration::from_secs(3600).as_secs().to_string())
        .arg("-X")
        .arg(method)
        .arg(&url);

    if let Some(json) = body {
        command
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("--data")
            .arg(json);
    }

    let output = command
        .output()
        .map_err(|error| format!("Unable to connect to Ollama: {}", error))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        return Err(if stderr.is_empty() {
            "Ollama API request failed.".to_string()
        } else {
            stderr
        });
    }

    Ok(stdout)
}

#[tauri::command]
pub fn list_ollama_models() -> Result<Vec<OllamaModel>, String> {
    let response = ollama_api("GET", "/api/tags", None)?;

    let parsed: OllamaTagsResponse = serde_json::from_str(&response)
        .map_err(|error| format!("Unable to parse Ollama model list: {}", error))?;

    let models = parsed
        .models
        .into_iter()
        .map(|model| {
            let details = model.details.map(|details| OllamaModelDetails {
                format: details.format,

                family: details.family,

                families: details.families,

                parameter_size: details.parameter_size,

                quantization_level: details.quantization_level,
            });

            OllamaModel {
                name: model
                    .name
                    .clone()
                    .or_else(|| model.model.clone())
                    .unwrap_or_else(|| "Unknown model".to_string()),

                model: model.model.or(model.name).unwrap_or_default(),

                size: model.size.unwrap_or(0),

                digest: model.digest.unwrap_or_default(),

                modified_at: model.modified_at.unwrap_or_default(),

                details,
            }
        })
        .collect();

    Ok(models)
}

#[tauri::command]
pub fn pull_ollama_model(model: String) -> Result<OllamaPullProgress, String> {
    let model = model.trim();

    if model.is_empty() {
        return Err("Model name is required.".to_string());
    }

    let body = serde_json::json!({
        "name": model,
        "stream": false,
    })
    .to_string();

    let response = ollama_api("POST", "/api/pull", Some(&body))?;

    let parsed: Value = serde_json::from_str(&response)
        .map_err(|error| format!("Unable to parse Ollama download response: {}", error))?;

    Ok(OllamaPullProgress {
        status: parsed
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("success")
            .to_string(),

        digest: parsed
            .get("digest")
            .and_then(Value::as_str)
            .map(str::to_string),

        total: parsed.get("total").and_then(Value::as_u64),

        completed: parsed.get("completed").and_then(Value::as_u64),
    })
}

#[tauri::command]
pub fn delete_ollama_model(model: String) -> Result<String, String> {
    let model = model.trim();

    if model.is_empty() {
        return Err("Model name is required.".to_string());
    }

    let body = serde_json::json!({
        "name": model,
    })
    .to_string();

    ollama_api("DELETE", "/api/delete", Some(&body))?;

    Ok(format!("Model {} was deleted.", model))
}

#[tauri::command]
pub fn run_ollama_model(model: String, prompt: String) -> Result<String, String> {
    let model = model.trim();

    let prompt = prompt.trim();

    if model.is_empty() {
        return Err("Model name is required.".to_string());
    }

    if prompt.is_empty() {
        return Err("Prompt is required.".to_string());
    }

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    })
    .to_string();

    let response = ollama_api("POST", "/api/generate", Some(&body))?;

    let parsed: Value = serde_json::from_str(&response)
        .map_err(|error| format!("Unable to parse Ollama response: {}", error))?;

    parsed
        .get("response")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Ollama returned no response.".to_string())
}

#[tauri::command]
pub fn show_ollama_model(model: String) -> Result<String, String> {
    let model = model.trim();

    if model.is_empty() {
        return Err("Model name is required.".to_string());
    }

    let body = serde_json::json!({
        "name": model,
        "verbose": true,
    })
    .to_string();

    match ollama_api("POST", "/api/show", Some(&body)) {
        Ok(response) => {
            let parsed: Result<Value, _> = serde_json::from_str(&response);

            match parsed {
                Ok(value) => {
                    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
                }

                Err(_) => Ok(response),
            }
        }

        Err(_) => run_ollama(&["show", model]),
    }
}

fn ollama_manifest_path(model: &str) -> Result<std::path::PathBuf, String> {
    let (name, tag) = model
        .rsplit_once(':')
        .filter(|(_, tag)| !tag.contains('/'))
        .unwrap_or((model, "latest"));
    let mut parts = name.split('/').collect::<Vec<_>>();
    if parts
        .iter()
        .any(|part| part.is_empty() || matches!(*part, "." | ".."))
    {
        return Err("Ollama model name is invalid".to_owned());
    }
    let registry = if parts.len() > 2 {
        parts.remove(0).to_owned()
    } else {
        "registry.ollama.ai".to_owned()
    };
    if parts.len() == 1 {
        parts.insert(0, "library");
    }
    let models_dir = std::env::var_os("OLLAMA_MODELS")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join(".ollama/models"))
        })
        .ok_or_else(|| "Ollama model folder is unavailable".to_owned())?;
    Ok(parts
        .into_iter()
        .fold(models_dir.join("manifests").join(registry), |path, part| {
            path.join(part)
        })
        .join(tag))
}

#[tauri::command]
pub fn show_ollama_model_in_finder(model: String) -> Result<(), String> {
    let manifest_path = ollama_manifest_path(model.trim())?;
    if !manifest_path.is_file() {
        return Err("Ollama model manifest was not found".to_owned());
    }
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|_| "Ollama model manifest could not be read".to_owned())?,
    )
    .map_err(|_| "Ollama model manifest is invalid".to_owned())?;
    let digest = manifest
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|layers| {
            layers
                .iter()
                .max_by_key(|layer| layer.get("size").and_then(Value::as_u64).unwrap_or(0))
        })
        .and_then(|layer| layer.get("digest"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Ollama model weight file was not found".to_owned())?;
    let blob_path = manifest_path
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "manifests"))
        .and_then(std::path::Path::parent)
        .ok_or_else(|| "Ollama model folder is unavailable".to_owned())?
        .join("blobs")
        .join(digest.replace(':', "-"));
    if !blob_path.is_file() {
        return Err("Ollama model weight file no longer exists".to_owned());
    }
    Command::new("/usr/bin/open")
        .arg("-R")
        .arg(blob_path)
        .status()
        .map_err(|_| "AI-OS could not open this model in Finder".to_owned())?
        .success()
        .then_some(())
        .ok_or_else(|| "Finder could not reveal this Ollama model".to_owned())
}

pub(crate) fn execute_local_model_capability(
    capability: &str,
    input: &Value,
    user_confirmed: bool,
) -> Result<Value, String> {
    match capability {
        "models.list" => serde_json::to_value(list_ollama_models()?)
            .map_err(|error| format!("Unable to serialize Ollama model list: {error}")),

        "models.show" => {
            let model = required_model_input(input)?;
            let details = show_ollama_model(model)?;
            Ok(serde_json::json!({
                "details": details,
            }))
        }

        "models.pull" => {
            if !user_confirmed {
                return Err("User confirmation is required to download a model.".to_owned());
            }

            let model = required_model_input(input)?;
            let progress = pull_ollama_model(model.clone())?;

            Ok(serde_json::json!({
                "model": model,
                "progress": progress,
            }))
        }

        "models.delete" => {
            if !user_confirmed {
                return Err("User confirmation is required to delete a model.".to_owned());
            }

            let model = required_model_input(input)?;
            let message = delete_ollama_model(model.clone())?;

            Ok(serde_json::json!({
                "model": model,
                "message": message,
            }))
        }

        _ => Err(format!(
            "Unsupported local model capability: {}",
            capability.trim()
        )),
    }
}

fn required_model_input(input: &Value) -> Result<String, String> {
    input
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Model name is required.".to_owned())
}

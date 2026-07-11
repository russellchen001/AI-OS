use serde::{
    Deserialize,
    Serialize,
};

use serde_json::Value;

use std::{
    process::{
        Command,
        Stdio,
    },
    time::Duration,
};

#[derive(
    Debug,
    Clone,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub families:
        Option<Vec<String>>,
    pub parameter_size:
        Option<String>,
    pub quantization_level:
        Option<String>,
}

#[derive(
    Debug,
    Clone,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    pub model: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
    pub details:
        Option<OllamaModelDetails>,
}

#[derive(
    Debug,
    Clone,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OllamaPullProgress {
    pub status: String,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub digest: Option<String>,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub total: Option<u64>,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub completed: Option<u64>,
}

#[derive(
    Debug,
    Deserialize,
)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(
    Debug,
    Deserialize,
)]
struct OllamaTagModel {
    name: Option<String>,
    model: Option<String>,
    size: Option<u64>,
    digest: Option<String>,

    #[serde(
        rename = "modified_at"
    )]
    modified_at: Option<String>,

    details:
        Option<OllamaTagDetails>,
}

#[derive(
    Debug,
    Deserialize,
)]
struct OllamaTagDetails {
    format: Option<String>,
    family: Option<String>,
    families:
        Option<Vec<String>>,

    #[serde(
        rename = "parameter_size"
    )]
    parameter_size:
        Option<String>,

    #[serde(
        rename = "quantization_level"
    )]
    quantization_level:
        Option<String>,
}

fn ollama_binary()
    -> String
{
    let candidates = [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
        "ollama",
    ];

    for candidate in candidates {
        let status =
            Command::new(candidate)
                .arg("--version")
                .stdout(
                    Stdio::null(),
                )
                .stderr(
                    Stdio::null(),
                )
                .status();

        if status
            .map(
                |value| {
                    value.success()
                },
            )
            .unwrap_or(false)
        {
            return candidate
                .to_string();
        }
    }

    "ollama".to_string()
}

fn run_ollama(
    arguments: &[&str],
) -> Result<String, String> {
    let binary =
        ollama_binary();

    let output =
        Command::new(&binary)
            .args(arguments)
            .output()
            .map_err(|error| {
                format!(
                    "Unable to run Ollama command '{}': {}",
                    binary,
                    error
                )
            })?;

    let stdout =
        String::from_utf8_lossy(
            &output.stdout,
        )
        .trim()
        .to_string();

    let stderr =
        String::from_utf8_lossy(
            &output.stderr,
        )
        .trim()
        .to_string();

    if !output.status.success() {
        return Err(
            if stderr.is_empty() {
                format!(
                    "Ollama command failed with status {}",
                    output.status
                )
            } else {
                stderr
            },
        );
    }

    Ok(
        if stdout.is_empty() {
            stderr
        } else {
            stdout
        },
    )
}

fn ollama_api(
    method: &str,
    path: &str,
    body:
        Option<&str>,
) -> Result<String, String> {
    let url = format!(
        "http://127.0.0.1:11434{}",
        path
    );

    let mut command =
        Command::new(
            "/usr/bin/curl",
        );

    command
        .arg("--silent")
        .arg("--show-error")
        .arg("--fail")
        .arg("--max-time")
        .arg(
            Duration::from_secs(3600)
                .as_secs()
                .to_string(),
        )
        .arg("-X")
        .arg(method)
        .arg(&url);

    if let Some(json) = body {
        command
            .arg("-H")
            .arg(
                "Content-Type: application/json",
            )
            .arg("--data")
            .arg(json);
    }

    let output =
        command.output()
            .map_err(|error| {
                format!(
                    "Unable to connect to Ollama: {}",
                    error
                )
            })?;

    let stdout =
        String::from_utf8_lossy(
            &output.stdout,
        )
        .to_string();

    let stderr =
        String::from_utf8_lossy(
            &output.stderr,
        )
        .trim()
        .to_string();

    if !output.status.success() {
        return Err(
            if stderr.is_empty() {
                "Ollama API request failed."
                    .to_string()
            } else {
                stderr
            },
        );
    }

    Ok(stdout)
}

#[tauri::command]
pub fn list_ollama_models()
    -> Result<
        Vec<OllamaModel>,
        String,
    >
{
    let response =
        ollama_api(
            "GET",
            "/api/tags",
            None,
        )?;

    let parsed:
        OllamaTagsResponse =
        serde_json::from_str(
            &response,
        )
        .map_err(|error| {
            format!(
                "Unable to parse Ollama model list: {}",
                error
            )
        })?;

    let models =
        parsed
            .models
            .into_iter()
            .map(
                |model| {
                    let details =
                        model.details.map(
                            |details| {
                                OllamaModelDetails {
                                    format:
                                        details.format,

                                    family:
                                        details.family,

                                    families:
                                        details.families,

                                    parameter_size:
                                        details
                                            .parameter_size,

                                    quantization_level:
                                        details
                                            .quantization_level,
                                }
                            },
                        );

                    OllamaModel {
                        name:
                            model.name
                                .clone()
                                .or_else(
                                    || {
                                        model
                                            .model
                                            .clone()
                                    },
                                )
                                .unwrap_or_else(
                                    || {
                                        "Unknown model"
                                            .to_string()
                                    },
                                ),

                        model:
                            model.model
                                .or(
                                    model.name,
                                )
                                .unwrap_or_default(),

                        size:
                            model.size
                                .unwrap_or(0),

                        digest:
                            model.digest
                                .unwrap_or_default(),

                        modified_at:
                            model.modified_at
                                .unwrap_or_default(),

                        details,
                    }
                },
            )
            .collect();

    Ok(models)
}

#[tauri::command]
pub fn pull_ollama_model(
    model: String,
) -> Result<
    OllamaPullProgress,
    String,
> {
    let model =
        model.trim();

    if model.is_empty() {
        return Err(
            "Model name is required."
                .to_string(),
        );
    }

    let body =
        serde_json::json!({
            "name": model,
            "stream": false,
        })
        .to_string();

    let response =
        ollama_api(
            "POST",
            "/api/pull",
            Some(&body),
        )?;

    let parsed:
        Value =
        serde_json::from_str(
            &response,
        )
        .map_err(|error| {
            format!(
                "Unable to parse Ollama download response: {}",
                error
            )
        })?;

    Ok(
        OllamaPullProgress {
            status:
                parsed
                    .get("status")
                    .and_then(
                        Value::as_str,
                    )
                    .unwrap_or(
                        "success",
                    )
                    .to_string(),

            digest:
                parsed
                    .get("digest")
                    .and_then(
                        Value::as_str,
                    )
                    .map(
                        str::to_string,
                    ),

            total:
                parsed
                    .get("total")
                    .and_then(
                        Value::as_u64,
                    ),

            completed:
                parsed
                    .get("completed")
                    .and_then(
                        Value::as_u64,
                    ),
        },
    )
}

#[tauri::command]
pub fn delete_ollama_model(
    model: String,
) -> Result<String, String> {
    let model =
        model.trim();

    if model.is_empty() {
        return Err(
            "Model name is required."
                .to_string(),
        );
    }

    let body =
        serde_json::json!({
            "name": model,
        })
        .to_string();

    ollama_api(
        "DELETE",
        "/api/delete",
        Some(&body),
    )?;

    Ok(format!(
        "Model {} was deleted.",
        model
    ))
}

#[tauri::command]
pub fn run_ollama_model(
    model: String,
    prompt: String,
) -> Result<String, String> {
    let model =
        model.trim();

    let prompt =
        prompt.trim();

    if model.is_empty() {
        return Err(
            "Model name is required."
                .to_string(),
        );
    }

    if prompt.is_empty() {
        return Err(
            "Prompt is required."
                .to_string(),
        );
    }

    let body =
        serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        })
        .to_string();

    let response =
        ollama_api(
            "POST",
            "/api/generate",
            Some(&body),
        )?;

    let parsed:
        Value =
        serde_json::from_str(
            &response,
        )
        .map_err(|error| {
            format!(
                "Unable to parse Ollama response: {}",
                error
            )
        })?;

    parsed
        .get("response")
        .and_then(
            Value::as_str,
        )
        .map(
            str::to_string,
        )
        .ok_or_else(|| {
            "Ollama returned no response."
                .to_string()
        })
}

#[tauri::command]
pub fn show_ollama_model(
    model: String,
) -> Result<String, String> {
    let model =
        model.trim();

    if model.is_empty() {
        return Err(
            "Model name is required."
                .to_string(),
        );
    }

    let body =
        serde_json::json!({
            "name": model,
            "verbose": true,
        })
        .to_string();

    match ollama_api(
        "POST",
        "/api/show",
        Some(&body),
    ) {
        Ok(response) => {
            let parsed:
                Result<Value, _> =
                serde_json::from_str(
                    &response,
                );

            match parsed {
                Ok(value) => {
                    serde_json::to_string_pretty(
                        &value,
                    )
                    .map_err(
                        |error| {
                            error.to_string()
                        },
                    )
                }

                Err(_) => Ok(response),
            }
        }

        Err(_) => {
            run_ollama(
                &[
                    "show",
                    model,
                ],
            )
        }
    }
}
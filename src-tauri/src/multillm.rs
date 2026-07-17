use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

static REQUESTS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

fn requests() -> &'static Mutex<HashMap<String, CancellationToken>> {
    REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiLlmMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMultiLlmRequest {
    pub operation_id: String,
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<MultiLlmMessage>,
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamEvent {
    operation_id: String,
    provider_id: String,
    text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishedEvent {
    operation_id: String,
    provider_id: String,
    cancelled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEvent {
    operation_id: String,
    provider_id: String,
    message: String,
}

fn endpoint(base_url: &str, provider_id: &str) -> String {
    let base = base_url.trim_end_matches('/');

    if provider_id == "ollama" {
        format!("{base}/api/chat")
    } else if provider_id == "claude" || base.contains("anthropic.com") {
        format!("{base}/messages")
    } else {
        format!("{base}/chat/completions")
    }
}

fn anthropic_body(request: &StartMultiLlmRequest) -> Value {
    let system = request
        .messages
        .iter()
        .filter(|message| message.role == "system")
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let messages = request
        .messages
        .iter()
        .filter(|message| message.role != "system")
        .map(|message| {
            json!({
                "role": message.role,
                "content": message.content,
            })
        })
        .collect::<Vec<_>>();

    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "stream": true,
    });

    if !system.is_empty() {
        body["system"] = Value::String(system);
    }

    body
}

fn openai_body(request: &StartMultiLlmRequest) -> Value {
    json!({
        "model": request.model,
        "messages": request.messages,
        "stream": true,
        "max_tokens": request.max_tokens,
    })
}

fn extract_text(value: &Value, anthropic: bool, ollama: bool) -> Option<String> {
    if ollama {
        value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .map(str::to_string)
    } else if anthropic {
        value
            .get("delta")
            .and_then(|delta| delta.get("text"))
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}

fn emit_error(app: &AppHandle, request: &StartMultiLlmRequest, message: impl Into<String>) {
    let _ = app.emit(
        "multillm://error",
        ErrorEvent {
            operation_id: request.operation_id.clone(),
            provider_id: request.provider_id.clone(),
            message: message.into(),
        },
    );
}

#[tauri::command]
pub async fn start_multillm_stream(
    app: AppHandle,
    request: StartMultiLlmRequest,
) -> Result<(), String> {
    if request.operation_id.trim().is_empty() {
        return Err("Operation ID is required.".to_string());
    }

    if request.provider_id != "ollama" && request.api_key.trim().is_empty() {
        return Err("API key is required.".to_string());
    }

    let token = CancellationToken::new();

    requests()
        .lock()
        .map_err(|_| "Unable to access MultiLLM request state.".to_string())?
        .insert(request.operation_id.clone(), token.clone());

    let anthropic = request.provider_id == "claude" || request.base_url.contains("anthropic.com");

    let ollama = request.provider_id == "ollama";

    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| error.to_string())?;

    let mut builder = client
        .post(endpoint(&request.base_url, &request.provider_id))
        .header(reqwest::header::CONTENT_TYPE, "application/json");

    builder = if anthropic {
        builder
            .header("x-api-key", &request.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&anthropic_body(&request))
    } else {
        builder
            .bearer_auth(&request.api_key)
            .json(&openai_body(&request))
    };

    let response = tokio::select! {
        _ = token.cancelled() => {
            requests()
                .lock()
                .ok()
                .map(|mut map| {
                    map.remove(&request.operation_id);
                });

            let _ = app.emit(
                "multillm://done",
                FinishedEvent {
                    operation_id:
                        request.operation_id,
                    provider_id:
                        request.provider_id,
                    cancelled: true,
                },
            );

            return Ok(());
        }
        result = builder.send() => {
            result.map_err(|error| error.to_string())?
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let details = response.text().await.unwrap_or_default();

        requests().lock().ok().map(|mut map| {
            map.remove(&request.operation_id);
        });

        let message = format!(
            "HTTP {}: {}",
            status,
            details.chars().take(500).collect::<String>(),
        );

        emit_error(&app, &request, &message);

        return Err(message);
    }

    let mut stream = response.bytes_stream();

    let mut buffer = String::new();

    let mut cancelled = false;

    loop {
        let next = tokio::select! {
            _ = token.cancelled() => {
                cancelled = true;
                None
            }
            item = stream.next() => item
        };

        let Some(item) = next else {
            break;
        };

        let bytes = match item {
            Ok(value) => value,
            Err(error) => {
                emit_error(&app, &request, error.to_string());
                break;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(position) = buffer.find('\n') {
            let line = buffer[..position].trim().to_string();

            buffer.drain(..=position);

            let data = if ollama {
                line.as_str()
            } else {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };

                data.trim()
            };

            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let Ok(value) = serde_json::from_str::<Value>(data) else {
                continue;
            };

            if let Some(text) = extract_text(&value, anthropic, ollama) {
                let _ = app.emit(
                    "multillm://chunk",
                    StreamEvent {
                        operation_id: request.operation_id.clone(),
                        provider_id: request.provider_id.clone(),
                        text,
                    },
                );
            }
        }
    }

    requests().lock().ok().map(|mut map| {
        map.remove(&request.operation_id);
    });

    let _ = app.emit(
        "multillm://done",
        FinishedEvent {
            operation_id: request.operation_id,
            provider_id: request.provider_id,
            cancelled,
        },
    );

    Ok(())
}

#[tauri::command]
pub fn cancel_multillm_stream(operation_id: String) -> Result<(), String> {
    let token = requests()
        .lock()
        .map_err(|_| "Unable to access MultiLLM request state.".to_string())?
        .get(&operation_id)
        .cloned();

    if let Some(token) = token {
        token.cancel();
    }

    Ok(())
}

use serde_json::{json, Value};

use super::provider::{DownloadRequest, DownloadTask};

const ARIA2_RPC_URL: &str = "http://127.0.0.1:6800/jsonrpc";

pub struct Aria2Client;

impl Aria2Client {
    async fn call(method: &str, params: Value) -> Result<Value, String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": "ai-os",
            "method": method,
            "params": params
        });

        let response = reqwest::Client::new()
            .post(ARIA2_RPC_URL)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let body: Value = response.json().await.map_err(|e| e.to_string())?;

        if let Some(error) = body.get("error") {
            return Err(error.to_string());
        }

        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn add_uri(request: DownloadRequest) -> Result<DownloadTask, String> {
        let result = Self::call(
            "aria2.addUri",
            json!([
                [
                    request.source
                ],
                {
                    "dir": request.destination
                }
            ]),
        )
        .await?;

        Ok(DownloadTask {
            task_id: result.as_str().unwrap_or("").to_string(),

            provider: "aria2".to_string(),

            state: "active".to_string(),

            progress: None,

            metadata: json!({}),

            output: result,
        })
    }

    pub async fn tell_status(task_id: String) -> Result<DownloadTask, String> {
        let result = Self::call("aria2.tellStatus", json!([task_id])).await?;

        Ok(DownloadTask {
            task_id,

            provider: "aria2".to_string(),

            state: result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),

            progress: None,

            metadata: result.clone(),

            output: result,
        })
    }

    pub async fn pause(task_id: String) -> Result<DownloadTask, String> {
        Self::call("aria2.pause", json!([task_id])).await?;

        Self::tell_status(task_id).await
    }

    pub async fn resume(task_id: String) -> Result<DownloadTask, String> {
        Self::call("aria2.unpause", json!([task_id])).await?;

        Self::tell_status(task_id).await
    }

    pub async fn cancel(task_id: String) -> Result<DownloadTask, String> {
        let result = Self::call("aria2.remove", json!([task_id])).await?;

        Ok(DownloadTask {
            task_id: result.as_str().unwrap_or("").to_string(),

            provider: "aria2".to_string(),

            state: "removed".to_string(),

            progress: None,

            metadata: json!({}),

            output: result,
        })
    }

    pub async fn list_active() -> Result<Vec<DownloadTask>, String> {
        let result = Self::call("aria2.tellActive", json!([])).await?;

        let mut tasks = Vec::new();

        if let Some(items) = result.as_array() {
            for item in items {
                tasks.push(DownloadTask {
                    task_id: item
                        .get("gid")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),

                    provider: "aria2".to_string(),

                    state: item
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),

                    progress: item
                        .get("completedLength")
                        .and_then(Value::as_str)
                        .and_then(|v| v.parse::<u64>().ok()),

                    metadata: item.clone(),

                    output: item.clone(),
                });
            }
        }

        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, path::Path, time::Duration};

    #[tokio::test]
    #[ignore = "requires the P15 verification aria2 daemon and HTTP fixture"]
    async fn add_uri_downloads_into_requested_destination() {
        let source = env::var("AI_OS_ARIA2_TEST_URL").expect("test URL missing");
        let destination =
            env::var("AI_OS_ARIA2_TEST_DESTINATION").expect("test destination missing");
        let expected_file = Path::new(&destination).join("payload.txt");

        let task = Aria2Client::add_uri(DownloadRequest {
            source,
            destination,
            provider: Some("aria2".to_string()),
            credential_id: None,
            options: json!({}),
        })
        .await
        .expect("aria2.addUri failed");

        for _ in 0..100 {
            let status = Aria2Client::tell_status(task.task_id.clone())
                .await
                .expect("aria2.tellStatus failed");
            if status.state == "complete" {
                assert_eq!(
                    std::fs::read_to_string(&expected_file).unwrap(),
                    "p15 destination\n"
                );
                return;
            }
            assert_ne!(
                status.state, "error",
                "aria2 download failed: {}",
                status.output
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        panic!("aria2 download did not complete in time");
    }
}

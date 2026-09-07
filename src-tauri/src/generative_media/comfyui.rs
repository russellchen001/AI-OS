use reqwest::{blocking::Client, Url};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComfyUiApiHealth {
    pub comfyui_version: Option<String>,
    pub object_info_reachable: bool,
}

impl ComfyUiApiHealth {
    pub(crate) fn is_healthy(&self) -> bool {
        self.comfyui_version.is_some() && self.object_info_reachable
    }
}

pub(crate) fn probe_comfyui_api(endpoint: &str) -> Result<ComfyUiApiHealth, String> {
    let mut base =
        Url::parse(endpoint).map_err(|error| format!("Invalid ComfyUI endpoint: {error}"))?;

    if !matches!(base.scheme(), "http" | "https") {
        return Err("ComfyUI endpoint must use http or https.".to_owned());
    }

    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("Failed to create ComfyUI HTTP client: {error}"))?;

    let get = |path: &str| -> Option<Value> {
        let response = client.get(base.join(path).ok()?).send().ok()?;
        response.status().is_success().then_some(())?;
        response.json().ok()
    };

    let comfyui_version = get("system_stats").and_then(|value| {
        value["system"]["comfyui_version"]
            .as_str()
            .map(str::to_owned)
    });

    let object_info_reachable = get("object_info").is_some_and(|value| value.is_object());

    Ok(ComfyUiApiHealth {
        comfyui_version,
        object_info_reachable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_http_endpoint_is_rejected() {
        assert!(probe_comfyui_api("file:///tmp/comfyui").is_err());
    }
}

#[cfg(test)]
mod http_behavior_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn mock_comfyui(system: &'static str, objects: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        thread::spawn(move || {
            for body in [system, objects] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        format!("http://{address}")
    }

    #[test]
    fn real_comfyui_shape_is_healthy() {
        let endpoint = mock_comfyui(
            r#"{"system":{"comfyui_version":"0.34.5"}}"#,
            r#"{"CheckpointLoaderSimple":{"input":{}}}"#,
        );

        assert!(probe_comfyui_api(&endpoint).unwrap().is_healthy());
    }

    #[test]
    fn ordinary_json_service_is_not_comfyui() {
        let endpoint = mock_comfyui(r#"{"status":"ok"}"#, r#"{"status":"ok"}"#);

        assert!(!probe_comfyui_api(&endpoint).unwrap().is_healthy());
    }

    #[test]
    #[ignore = "requires AI_OS_COMFYUI_TEST_ENDPOINT"]
    fn live_comfyui_endpoint_is_healthy_when_requested() {
        let endpoint = std::env::var("AI_OS_COMFYUI_TEST_ENDPOINT")
            .expect("AI_OS_COMFYUI_TEST_ENDPOINT must be set for the live test");

        assert!(probe_comfyui_api(&endpoint).unwrap().is_healthy());
    }
}

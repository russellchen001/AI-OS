//! Loopback DevTools transport for the AI-OS managed browser.
//!
//! This module is pure plumbing. It knows how to reach the managed browser's
//! own DevTools endpoint over the loopback interface and how to parse what
//! comes back. It makes no decision about accounts, authentication or
//! connection state, and it never accepts a non-loopback endpoint.

use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

pub(crate) const CONTROL_CHANNEL_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PROTOCOL_MESSAGES: usize = 64;

/// A page/target reported by the managed browser.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct DevToolsTarget {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: String,
}

// ---------------------------------------------------------------------------
// Loopback guards
// ---------------------------------------------------------------------------

fn loopback_port(authority: &str) -> Option<u16> {
    if authority.contains('@') {
        return None;
    }
    let (host, port) = authority.rsplit_once(':')?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return None;
    }
    port.parse::<u16>().ok().filter(|port| *port != 0)
}

/// Accepts only `ws://<loopback>:<port>/...` and returns the port.
pub(crate) fn loopback_websocket_port(ws_url: &str) -> Option<u16> {
    let remainder = ws_url.strip_prefix("ws://")?;
    let authority = remainder.split('/').next()?;
    loopback_port(authority)
}

// ---------------------------------------------------------------------------
// HTTP on the loopback control channel
// ---------------------------------------------------------------------------

/// Raw HTTP GET against the managed browser's DevTools HTTP endpoint.
/// The response is returned verbatim, headers included.
pub(crate) fn http_get(port: u16, path: &str) -> Result<String, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, CONTROL_CHANNEL_TIMEOUT)
        .map_err(|_| "Managed browser control channel is not reachable".to_owned())?;
    let _ = stream.set_read_timeout(Some(CONTROL_CHANNEL_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONTROL_CHANNEL_TIMEOUT));
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Managed browser control channel refused the request".to_owned())?;
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => response.extend_from_slice(&buffer[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && !response.is_empty() =>
            {
                // Chromium may leave the DevTools HTTP connection open after
                // the complete response has arrived. Once bytes were received,
                // a later read timeout is not a failed request.
                break;
            }
            Err(_) => {
                return Err("Managed browser control channel returned no response".to_owned());
            }
        }
    }

    if response.is_empty() {
        return Err("Managed browser control channel returned no response".to_owned());
    }

    String::from_utf8(response)
        .map_err(|_| "Managed browser control channel returned invalid text".to_owned())
}

/// Extract the JSON document from a raw HTTP response without depending on a
/// particular transfer encoding.
pub(crate) fn extract_json_payload(response: &str) -> Option<&str> {
    let body = response.split_once("\r\n\r\n").map(|(_, body)| body)?;
    let start = body.find(['[', '{'])?;
    let opening = body.as_bytes()[start];
    let closing = if opening == b'[' { ']' } else { '}' };
    let end = body.rfind(closing)?;
    if end < start {
        return None;
    }
    Some(&body[start..=end])
}

pub(crate) fn parse_targets(response: &str) -> Result<Vec<DevToolsTarget>, String> {
    let payload = extract_json_payload(response)
        .ok_or_else(|| "Managed browser target list is unreadable".to_owned())?;
    serde_json::from_str::<Vec<DevToolsTarget>>(payload)
        .map_err(|_| "Managed browser target list is invalid".to_owned())
}

pub(crate) fn parse_browser_websocket_url(response: &str) -> Result<String, String> {
    let payload = extract_json_payload(response)
        .ok_or_else(|| "Managed browser version document is unreadable".to_owned())?;
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| "Managed browser version document is invalid".to_owned())?;
    value
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Managed browser exposes no control channel".to_owned())
}

pub(crate) fn list_targets(port: u16) -> Result<Vec<DevToolsTarget>, String> {
    parse_targets(&http_get(port, "/json/list")?)
}

pub(crate) fn browser_websocket_url(port: u16) -> Result<String, String> {
    parse_browser_websocket_url(&http_get(port, "/json/version")?)
}

// ---------------------------------------------------------------------------
// DevTools protocol
// ---------------------------------------------------------------------------

pub(crate) fn parse_protocol_response(message: &Value) -> Result<Value, String> {
    if let Some(error) = message.get("error") {
        let detail = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown protocol error");
        return Err(format!("Managed browser rejected the request: {detail}"));
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// Read the string value out of a `Runtime.evaluate` result.
pub(crate) fn parse_evaluated_string(result: &Value) -> Result<String, String> {
    if result.get("exceptionDetails").is_some() {
        return Err("Managed browser page probe raised an exception".to_owned());
    }
    let inner = result
        .get("result")
        .ok_or_else(|| "Managed browser page probe returned nothing".to_owned())?;
    match inner.get("type").and_then(Value::as_str) {
        Some("string") => inner
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "Managed browser page probe returned no value".to_owned()),
        _ => Err("Managed browser page probe returned an unexpected value".to_owned()),
    }
}

/// Issue one DevTools protocol call over a loopback WebSocket.
pub(crate) fn protocol_call(
    ws_url: &str,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let port = loopback_websocket_port(ws_url)
        .ok_or_else(|| "Managed browser control channel is not loopback".to_owned())?;

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let stream = TcpStream::connect_timeout(&address, CONTROL_CHANNEL_TIMEOUT)
        .map_err(|_| "Managed browser control channel is not reachable".to_owned())?;
    let _ = stream.set_read_timeout(Some(CONTROL_CHANNEL_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONTROL_CHANNEL_TIMEOUT));

    let request = ws_url
        .into_client_request()
        .map_err(|_| "Managed browser control channel address is invalid".to_owned())?;
    let (mut socket, _response) = tungstenite::client::client(request, stream)
        .map_err(|_| "Managed browser control channel handshake failed".to_owned())?;

    let payload = serde_json::to_string(&json!({
        "id": id,
        "method": method,
        "params": params,
    }))
    .map_err(|_| "Managed browser request could not be encoded".to_owned())?;

    socket
        .send(Message::text(payload))
        .map_err(|_| "Managed browser did not accept the request".to_owned())?;

    let mut outcome = Err("Managed browser did not answer the request".to_owned());
    for _ in 0..MAX_PROTOCOL_MESSAGES {
        let Ok(message) = socket.read() else { break };
        let Ok(text) = message.into_text() else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        // Protocol events carry no id; only the matching reply is ours.
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            outcome = parse_protocol_response(&value);
            break;
        }
    }

    let _ = socket.close(None);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_control_channels_are_accepted() {
        assert_eq!(
            loopback_websocket_port("ws://127.0.0.1:51321/devtools/page/AB12"),
            Some(51321)
        );
        assert_eq!(
            loopback_websocket_port("ws://localhost:51321/devtools/browser/CD34"),
            Some(51321)
        );
        assert_eq!(
            loopback_websocket_port("ws://10.1.2.3:51321/devtools/page/AB12"),
            None
        );
        assert_eq!(
            loopback_websocket_port("ws://127.0.0.1@evil.example:80/devtools/page/AB12"),
            None
        );
        assert_eq!(
            loopback_websocket_port("wss://127.0.0.1:51321/devtools"),
            None
        );
        assert_eq!(loopback_websocket_port("ws://127.0.0.1:0/devtools"), None);
    }

    #[test]
    fn devtools_documents_are_parsed_out_of_raw_http_responses() {
        let list = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: application/json\r\n\r\n",
            "[{\"id\":\"AB12\",\"type\":\"page\",\"url\":\"https://www.amazon.co.jp/\",",
            "\"webSocketDebuggerUrl\":\"ws://127.0.0.1:51321/devtools/page/AB12\"}]"
        );
        let targets = parse_targets(list).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "AB12");
        assert_eq!(targets[0].target_type, "page");
        assert_eq!(targets[0].url, "https://www.amazon.co.jp/");

        let version = concat!(
            "HTTP/1.1 200 OK\r\n\r\n",
            "{\"Browser\":\"Chrome/140\",",
            "\"webSocketDebuggerUrl\":\"ws://127.0.0.1:51321/devtools/browser/CD34\"}"
        );
        assert_eq!(
            parse_browser_websocket_url(version).unwrap(),
            "ws://127.0.0.1:51321/devtools/browser/CD34"
        );

        assert!(parse_targets("HTTP/1.1 500 Internal Server Error\r\n\r\n").is_err());
        assert!(parse_browser_websocket_url("HTTP/1.1 200 OK\r\n\r\n{}").is_err());
    }

    #[test]
    fn protocol_errors_and_exceptions_are_never_treated_as_results() {
        let error = json!({"id": 1, "error": {"code": -32000, "message": "Target closed"}});
        assert!(parse_protocol_response(&error).is_err());

        let success = json!({"id": 1, "result": {"result": {"type": "string", "value": "{}"}}});
        let result = parse_protocol_response(&success).unwrap();
        assert_eq!(parse_evaluated_string(&result).unwrap(), "{}");

        let exception = json!({
            "result": {"type": "object"},
            "exceptionDetails": {"text": "Uncaught"}
        });
        assert!(parse_evaluated_string(&exception).is_err());

        let wrong_type = json!({"result": {"type": "undefined"}});
        assert!(parse_evaluated_string(&wrong_type).is_err());
    }
}

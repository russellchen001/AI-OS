use chrono::Utc;

use serde::{
    Deserialize,
    Serialize,
};

use serde_json::{
    json,
    Value,
};

use std::{
    fs,
    net::TcpStream,
    path::PathBuf,
    time::Duration,
};

use tungstenite::{
    connect,
    stream::MaybeTlsStream,
    Error as WebSocketError,
    Message,
    WebSocket,
};

use url::Url;

const CONFIG_FILE_NAME: &str =
    "openclaw-servers.json";

const PROTOCOL_VERSION: u64 = 4;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
struct StoredOpenClawServer {
    id: String,
    name: String,
    server_url: String,
    gateway_token: String,
    enabled: bool,
    active: bool,
    auto_connect: bool,

    #[serde(default)]
    connection_state: String,

    #[serde(default)]
    connection_message: String,

    #[serde(default)]
    last_checked_at: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawServer {
    pub id: String,
    pub name: String,
    pub server_url: String,

    /*
     * 永远不把真实 Token 返回前端。
     */
    pub gateway_token: String,
    pub has_gateway_token: bool,

    pub enabled: bool,
    pub active: bool,
    pub auto_connect: bool,

    pub connection_state: String,
    pub connection_message: String,
    pub last_checked_at: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawServerInput {
    pub name: String,
    pub server_url: String,
    pub gateway_token: String,
    pub enabled: bool,
    pub auto_connect: bool,
}

#[derive(
    Debug,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawActionResult {
    pub success: bool,
    pub message: String,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub server: Option<OpenClawServer>,
}

#[derive(
    Debug,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawConnectionResult {
    pub success: bool,
    pub state: String,
    pub message: String,
    pub checked_at: String,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub server: Option<OpenClawServer>,
}

#[derive(
    Debug,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawRemoteStatus {
    pub connected: bool,
    pub server_id: String,
    pub server_name: String,
    pub server_url: String,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub gateway_status: Option<String>,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub version: Option<String>,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub raw_response: Option<String>,
}

struct GatewayHandshake {
    success: bool,
    state: String,
    message: String,
    version: Option<String>,
    payload: Option<Value>,
}

fn action_success(
    message: impl Into<String>,
    server: Option<OpenClawServer>,
) -> OpenClawActionResult {
    OpenClawActionResult {
        success: true,
        message: message.into(),
        server,
    }
}

fn action_failure(
    message: impl Into<String>,
) -> OpenClawActionResult {
    OpenClawActionResult {
        success: false,
        message: message.into(),
        server: None,
    }
}

fn config_directory()
    -> Result<PathBuf, String>
{
    let home =
        dirs::home_dir()
            .ok_or_else(|| {
                "Unable to determine the home directory."
                    .to_string()
            })?;

    Ok(
        home
            .join(".ai-os")
            .join("config"),
    )
}

fn config_file()
    -> Result<PathBuf, String>
{
    Ok(
        config_directory()?
            .join(CONFIG_FILE_NAME),
    )
}

fn read_servers()
    -> Result<
        Vec<StoredOpenClawServer>,
        String,
    >
{
    let path =
        config_file()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents =
        fs::read_to_string(
            &path,
        )
        .map_err(|error| {
            format!(
                "Unable to read OpenClaw configuration {}: {}",
                path.display(),
                error,
            )
        })?;

    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(
        &contents,
    )
    .map_err(|error| {
        format!(
            "Unable to parse OpenClaw configuration: {}",
            error,
        )
    })
}

fn write_servers(
    servers:
        &[StoredOpenClawServer],
) -> Result<(), String> {
    let directory =
        config_directory()?;

    fs::create_dir_all(
        &directory,
    )
    .map_err(|error| {
        format!(
            "Unable to create OpenClaw configuration directory: {}",
            error,
        )
    })?;

    let path =
        config_file()?;

    let contents =
        serde_json::to_string_pretty(
            servers,
        )
        .map_err(|error| {
            format!(
                "Unable to serialize OpenClaw configuration: {}",
                error,
            )
        })?;

    fs::write(
        &path,
        contents,
    )
    .map_err(|error| {
        format!(
            "Unable to save OpenClaw configuration {}: {}",
            path.display(),
            error,
        )
    })
}

fn public_server(
    stored: &StoredOpenClawServer,
) -> OpenClawServer {
    OpenClawServer {
        id:
            stored.id.clone(),

        name:
            stored.name.clone(),

        server_url:
            stored.server_url.clone(),

        /*
         * 列表只返回掩码。
         */
        gateway_token:
            if stored
                .gateway_token
                .is_empty()
            {
                String::new()
            } else {
                "••••••••••••••••"
                    .to_string()
            },

        has_gateway_token:
            !stored
                .gateway_token
                .trim()
                .is_empty(),

        enabled:
            stored.enabled,

        active:
            stored.active,

        auto_connect:
            stored.auto_connect,

        connection_state:
            if stored
                .connection_state
                .is_empty()
            {
                "unknown".to_string()
            } else {
                stored
                    .connection_state
                    .clone()
            },

        connection_message:
            stored
                .connection_message
                .clone(),

        last_checked_at:
            stored
                .last_checked_at
                .clone(),
    }
}

fn normalize_url(
    value: &str,
) -> Result<String, String> {
    let trimmed =
        value.trim().trim_end_matches('/');

    if trimmed.is_empty() {
        return Err(
            "Server URL is required."
                .to_string(),
        );
    }

    let parsed =
        Url::parse(trimmed)
            .map_err(|error| {
                format!(
                    "Invalid Server URL: {}",
                    error,
                )
            })?;

    match parsed.scheme() {
        "http" | "https" |
        "ws" | "wss" => {}

        _ => {
            return Err(
                "Server URL must use HTTP, HTTPS, WS or WSS."
                    .to_string(),
            );
        }
    }

    if parsed.host_str().is_none() {
        return Err(
            "Server URL must contain a host."
                .to_string(),
        );
    }

    Ok(trimmed.to_string())
}

fn websocket_url(
    server_url: &str,
) -> Result<String, String> {
    let normalized =
        normalize_url(server_url)?;

    let mut parsed =
        Url::parse(&normalized)
            .map_err(|error| {
                error.to_string()
            })?;

    match parsed.scheme() {
        "http" => {
            parsed
                .set_scheme("ws")
                .map_err(|_| {
                    "Unable to convert HTTP URL to WebSocket URL."
                        .to_string()
                })?;
        }

        "https" => {
            parsed
                .set_scheme("wss")
                .map_err(|_| {
                    "Unable to convert HTTPS URL to WebSocket URL."
                        .to_string()
                })?;
        }

        "ws" | "wss" => {}

        _ => {
            return Err(
                "Unsupported OpenClaw URL scheme."
                    .to_string(),
            );
        }
    }

    /*
     * OpenClaw Gateway 使用根 WebSocket 地址。
     */
    if parsed.path().is_empty() {
        parsed.set_path("/");
    }

    Ok(parsed.to_string())
}

fn validate_input(
    input:
        &OpenClawServerInput,
    require_token: bool,
) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err(
            "Server name is required."
                .to_string(),
        );
    }

    normalize_url(
        &input.server_url,
    )?;

    if require_token &&
        input
            .gateway_token
            .trim()
            .is_empty()
    {
        return Err(
            "Gateway Token is required."
                .to_string(),
        );
    }

    Ok(())
}

fn generate_id(
    name: &str,
) -> String {
    let slug =
        name
            .trim()
            .to_lowercase()
            .chars()
            .map(|character| {
                if character
                    .is_ascii_alphanumeric()
                {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(
                |part| {
                    !part.is_empty()
                },
            )
            .collect::<Vec<_>>()
            .join("-");

    format!(
        "{}-{}",
        if slug.is_empty() {
            "openclaw"
        } else {
            &slug
        },
        Utc::now()
            .timestamp_millis(),
    )
}

fn configure_socket_timeout(
    socket:
        &mut WebSocket<
            MaybeTlsStream<TcpStream>,
        >,
) {
    let duration =
        Some(
            Duration::from_secs(10),
        );

    match socket.get_mut() {
        MaybeTlsStream::Plain(
            stream,
        ) => {
            let _ =
                stream
                    .set_read_timeout(
                        duration,
                    );

            let _ =
                stream
                    .set_write_timeout(
                        duration,
                    );
        }

        MaybeTlsStream::NativeTls(
            stream,
        ) => {
            let tcp =
                stream.get_mut();

            let _ =
                tcp.set_read_timeout(
                    duration,
                );

            let _ =
                tcp.set_write_timeout(
                    duration,
                );
        }

        _ => {}
    }
}

fn read_json_message(
    socket:
        &mut WebSocket<
            MaybeTlsStream<TcpStream>,
        >,
) -> Result<Value, String> {
    loop {
        let message =
            socket
                .read()
                .map_err(
                    websocket_error,
                )?;

        match message {
            Message::Text(text) => {
                return serde_json::from_str(
                    text.as_ref(),
                )
                .map_err(|error| {
                    format!(
                        "OpenClaw returned invalid JSON: {}",
                        error,
                    )
                });
            }

            Message::Binary(bytes) => {
                return serde_json::from_slice(
                    &bytes,
                )
                .map_err(|error| {
                    format!(
                        "OpenClaw returned invalid JSON: {}",
                        error,
                    )
                });
            }

            Message::Ping(payload) => {
                socket
                    .send(
                        Message::Pong(
                            payload,
                        ),
                    )
                    .map_err(
                        websocket_error,
                    )?;
            }

            Message::Close(frame) => {
                return Err(format!(
                    "OpenClaw closed the connection: {:?}",
                    frame,
                ));
            }

            _ => {}
        }
    }
}

fn websocket_error(
    error: WebSocketError,
) -> String {
    match error {
        WebSocketError::Io(
            io_error,
        ) => {
            format!(
                "Unable to reach OpenClaw Gateway: {}",
                io_error,
            )
        }

        WebSocketError::Http(
            response,
        ) => {
            format!(
                "OpenClaw rejected the WebSocket request with HTTP status {}.",
                response.status(),
            )
        }

        other => {
            format!(
                "OpenClaw WebSocket error: {}",
                other,
            )
        }
    }
}

fn classify_gateway_error(
    message: &str,
    code: Option<&str>,
) -> String {
    let combined =
        format!(
            "{} {}",
            message,
            code.unwrap_or_default(),
        )
        .to_lowercase();

    if combined.contains(
        "unauthorized",
    ) || combined.contains(
        "invalid token",
    ) || combined.contains(
        "token missing",
    ) || combined.contains(
        "auth",
    ) && combined.contains(
        "token",
    ) {
        return "unauthorized"
            .to_string();
    }

    if combined.contains(
        "device",
    ) || combined.contains(
        "pairing",
    ) || combined.contains(
        "nonce",
    ) || combined.contains(
        "signature",
    ) {
        return "error".to_string();
    }

    "error".to_string()
}

fn gateway_handshake(
    server_url: &str,
    gateway_token: &str,
) -> GatewayHandshake {
    let ws_url =
        match websocket_url(
            server_url,
        ) {
            Ok(value) => value,

            Err(error) => {
                return GatewayHandshake {
                    success: false,
                    state:
                        "error"
                            .to_string(),
                    message: error,
                    version: None,
                    payload: None,
                };
            }
        };

    let (
        mut socket,
        _response,
    ) = match connect(
        ws_url.as_str(),
    ) {
        Ok(value) => value,

        Err(error) => {
            return GatewayHandshake {
                success: false,
                state:
                    "unreachable"
                        .to_string(),

                message:
                    websocket_error(
                        error,
                    ),

                version: None,
                payload: None,
            };
        }
    };

    configure_socket_timeout(
        &mut socket,
    );

    /*
     * Gateway 首帧应是 connect.challenge。
     */
    let challenge =
        match read_json_message(
            &mut socket,
        ) {
            Ok(value) => value,

            Err(error) => {
                let _ =
                    socket.close(None);

                return GatewayHandshake {
                    success: false,
                    state:
                        "unreachable"
                            .to_string(),
                    message: error,
                    version: None,
                    payload: None,
                };
            }
        };

    let challenge_event =
        challenge
            .get("event")
            .and_then(
                Value::as_str,
            );

    if challenge_event !=
        Some(
            "connect.challenge",
        )
    {
        let _ =
            socket.close(None);

        return GatewayHandshake {
            success: false,
            state:
                "error".to_string(),

            message:
                "The server responded, but it did not provide an OpenClaw connect challenge."
                    .to_string(),

            version: None,
            payload:
                Some(challenge),
        };
    }

    let request_id =
        format!(
            "ai-os-{}",
            Utc::now()
                .timestamp_millis(),
        );

    /*
     * 远程 Gateway 可能进一步要求设备身份或配对。
     * 此基础客户端先使用共享 Gateway Token 请求只读权限。
     */
    let connect_request =
        json!({
            "type": "req",
            "id": request_id,
            "method": "connect",
            "params": {
                "minProtocol": PROTOCOL_VERSION,
                "maxProtocol": PROTOCOL_VERSION,

                "client": {
                    "id": "ai-os",
                    "version": env!(
                        "CARGO_PKG_VERSION"
                    ),
                    "platform": std::env::consts::OS,
                    "mode": "operator"
                },

                "role": "operator",

                "scopes": [
                    "operator.read"
                ],

                "caps": [],
                "commands": [],
                "permissions": {},

                "auth": {
                    "token": gateway_token
                },

                "locale": "en-US",

                "userAgent": format!(
                    "ai-os/{}",
                    env!(
                        "CARGO_PKG_VERSION"
                    )
                )
            }
        });

    if let Err(error) =
        socket.send(
            Message::Text(
                connect_request
                    .to_string()
                    .into(),
            ),
        )
    {
        let _ =
            socket.close(None);

        return GatewayHandshake {
            success: false,
            state:
                "unreachable"
                    .to_string(),

            message:
                websocket_error(
                    error,
                ),

            version: None,
            payload: None,
        };
    }

    let response =
        match read_json_message(
            &mut socket,
        ) {
            Ok(value) => value,

            Err(error) => {
                let _ =
                    socket.close(None);

                return GatewayHandshake {
                    success: false,
                    state:
                        "unreachable"
                            .to_string(),
                    message: error,
                    version: None,
                    payload: None,
                };
            }
        };

    let _ =
        socket.close(None);

    let ok =
        response
            .get("ok")
            .and_then(
                Value::as_bool,
            )
            .unwrap_or(false);

    if ok {
        let payload =
            response
                .get("payload")
                .cloned();

        let version =
            payload
                .as_ref()
                .and_then(
                    |value| {
                        value
                            .get("server")
                    },
                )
                .and_then(
                    |server| {
                        server.get(
                            "version",
                        )
                    },
                )
                .and_then(
                    Value::as_str,
                )
                .map(
                    str::to_string,
                );

        return GatewayHandshake {
            success: true,
            state:
                "connected"
                    .to_string(),

            message:
                match &version {
                    Some(version) => {
                        format!(
                            "Connected to OpenClaw Gateway {}.",
                            version,
                        )
                    }

                    None => {
                        "Connected to OpenClaw Gateway."
                            .to_string()
                    }
                },

            version,
            payload,
        };
    }

    let error_value =
        response.get("error");

    let error_message =
        error_value
            .and_then(
                |value| {
                    value.get(
                        "message",
                    )
                },
            )
            .and_then(
                Value::as_str,
            )
            .unwrap_or(
                "OpenClaw rejected the connection.",
            );

    let error_code =
        error_value
            .and_then(
                |value| {
                    value.get(
                        "code",
                    )
                },
            )
            .and_then(
                Value::as_str,
            )
            .or_else(|| {
                error_value
                    .and_then(
                        |value| {
                            value.get(
                                "details",
                            )
                        },
                    )
                    .and_then(
                        |details| {
                            details.get(
                                "code",
                            )
                        },
                    )
                    .and_then(
                        Value::as_str,
                    )
            });

    let state =
        classify_gateway_error(
            error_message,
            error_code,
        );

    let friendly_message =
        if error_message
            .to_lowercase()
            .contains("device")
            || error_message
                .to_lowercase()
                .contains(
                    "pair",
                )
        {
            format!(
                "{} The remote Gateway requires device identity or pairing.",
                error_message,
            )
        } else {
            error_message.to_string()
        };

    GatewayHandshake {
        success: false,
        state,
        message:
            friendly_message,
        version: None,
        payload:
            Some(response),
    }
}

fn update_connection_result(
    servers:
        &mut [StoredOpenClawServer],
    index: usize,
    handshake:
        &GatewayHandshake,
) -> OpenClawConnectionResult {
    let checked_at =
        Utc::now().to_rfc3339();

    servers[index]
        .connection_state =
        handshake.state.clone();

    servers[index]
        .connection_message =
        handshake.message.clone();

    servers[index]
        .last_checked_at =
        Some(
            checked_at.clone(),
        );

    OpenClawConnectionResult {
        success:
            handshake.success,

        state:
            handshake.state.clone(),

        message:
            handshake.message.clone(),

        checked_at,

        server:
            Some(
                public_server(
                    &servers[index],
                ),
            ),
    }
}

#[tauri::command]
pub fn list_openclaw_servers()
    -> Result<
        Vec<OpenClawServer>,
        String,
    >
{
    Ok(
        read_servers()?
            .iter()
            .map(public_server)
            .collect(),
    )
}

#[tauri::command]
pub fn save_openclaw_server(
    server: OpenClawServerInput,
) -> OpenClawActionResult {
    if let Err(error) =
        validate_input(
            &server,
            true,
        )
    {
        return action_failure(error);
    }

    let mut servers =
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let normalized_url =
        match normalize_url(
            &server.server_url,
        ) {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let is_first =
        servers.is_empty();

    let stored =
        StoredOpenClawServer {
            id:
                generate_id(
                    &server.name,
                ),

            name:
                server
                    .name
                    .trim()
                    .to_string(),

            server_url:
                normalized_url,

            gateway_token:
                server
                    .gateway_token
                    .trim()
                    .to_string(),

            enabled:
                server.enabled,

            active:
                is_first,

            auto_connect:
                server.auto_connect,

            connection_state:
                "unknown"
                    .to_string(),

            connection_message:
                "Connection has not been tested."
                    .to_string(),

            last_checked_at:
                None,
        };

    servers.push(
        stored.clone(),
    );

    if let Err(error) =
        write_servers(
            &servers,
        )
    {
        return action_failure(error);
    }

    action_success(
        format!(
            "OpenClaw server {} was added.",
            stored.name,
        ),
        Some(
            public_server(
                &stored,
            ),
        ),
    )
}

#[tauri::command]
pub fn update_openclaw_server(
    id: String,
    server: OpenClawServerInput,
) -> OpenClawActionResult {
    if let Err(error) =
        validate_input(
            &server,
            false,
        )
    {
        return action_failure(error);
    }

    let mut servers =
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let Some(index) =
        servers.iter().position(
            |item| item.id == id,
        )
    else {
        return action_failure(
            "OpenClaw server was not found.",
        );
    };

    let normalized_url =
        match normalize_url(
            &server.server_url,
        ) {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let old_token =
        servers[index]
            .gateway_token
            .clone();

    servers[index].name =
        server
            .name
            .trim()
            .to_string();

    servers[index]
        .server_url =
        normalized_url;

    servers[index]
        .gateway_token =
        if server
            .gateway_token
            .trim()
            .is_empty()
        {
            old_token
        } else {
            server
                .gateway_token
                .trim()
                .to_string()
        };

    servers[index].enabled =
        server.enabled;

    servers[index]
        .auto_connect =
        server.auto_connect;

    servers[index]
        .connection_state =
        "unknown".to_string();

    servers[index]
        .connection_message =
        "Connection must be tested again."
            .to_string();

    servers[index]
        .last_checked_at =
        None;

    let updated =
        servers[index].clone();

    if let Err(error) =
        write_servers(
            &servers,
        )
    {
        return action_failure(error);
    }

    action_success(
        format!(
            "OpenClaw server {} was updated.",
            updated.name,
        ),
        Some(
            public_server(
                &updated,
            ),
        ),
    )
}

#[tauri::command]
pub fn delete_openclaw_server(
    id: String,
) -> OpenClawActionResult {
    let mut servers =
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let Some(index) =
        servers.iter().position(
            |server| {
                server.id == id
            },
        )
    else {
        return action_failure(
            "OpenClaw server was not found.",
        );
    };

    let removed =
        servers.remove(index);

    if removed.active {
        if let Some(first) =
            servers
                .iter_mut()
                .find(
                    |server| {
                        server.enabled
                    },
                )
        {
            first.active = true;
        }
    }

    if let Err(error) =
        write_servers(
            &servers,
        )
    {
        return action_failure(error);
    }

    action_success(
        format!(
            "OpenClaw server {} was deleted.",
            removed.name,
        ),
        None,
    )
}

#[tauri::command]
pub fn toggle_openclaw_server(
    id: String,
    enabled: bool,
) -> OpenClawActionResult {
    let mut servers =
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let Some(index) =
        servers.iter().position(
            |server| {
                server.id == id
            },
        )
    else {
        return action_failure(
            "OpenClaw server was not found.",
        );
    };

    servers[index].enabled =
        enabled;

    if !enabled &&
        servers[index].active
    {
        servers[index].active =
            false;

        if let Some(other) =
            servers
                .iter_mut()
                .find(
                    |server| {
                        server.enabled
                    },
                )
        {
            other.active = true;
        }
    }

    let updated =
        servers[index].clone();

    if let Err(error) =
        write_servers(
            &servers,
        )
    {
        return action_failure(error);
    }

    action_success(
        format!(
            "{} was {}.",
            updated.name,
            if enabled {
                "enabled"
            } else {
                "disabled"
            },
        ),
        Some(
            public_server(
                &updated,
            ),
        ),
    )
}

#[tauri::command]
pub fn set_active_openclaw_server(
    id: String,
) -> OpenClawActionResult {
    let mut servers =
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return action_failure(
                    error,
                );
            }
        };

    let Some(target_index) =
        servers.iter().position(
            |server| {
                server.id == id
            },
        )
    else {
        return action_failure(
            "OpenClaw server was not found.",
        );
    };

    if !servers[target_index]
        .enabled
    {
        return action_failure(
            "A disabled server cannot be made active.",
        );
    }

    for server in
        &mut servers
    {
        server.active =
            false;
    }

    servers[target_index]
        .active =
        true;

    let updated =
        servers[target_index]
            .clone();

    if let Err(error) =
        write_servers(
            &servers,
        )
    {
        return action_failure(error);
    }

    action_success(
        format!(
            "{} is now the active OpenClaw server.",
            updated.name,
        ),
        Some(
            public_server(
                &updated,
            ),
        ),
    )
}

#[tauri::command]
pub fn test_openclaw_connection(
    id: String,
) -> OpenClawConnectionResult {
    let mut servers =
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return OpenClawConnectionResult {
                    success: false,
                    state:
                        "error"
                            .to_string(),
                    message: error,
                    checked_at:
                        Utc::now()
                            .to_rfc3339(),
                    server: None,
                };
            }
        };

    let Some(index) =
        servers.iter().position(
            |server| {
                server.id == id
            },
        )
    else {
        return OpenClawConnectionResult {
            success: false,
            state:
                "error".to_string(),
            message:
                "OpenClaw server was not found."
                    .to_string(),
            checked_at:
                Utc::now()
                    .to_rfc3339(),
            server: None,
        };
    };

    let handshake =
        gateway_handshake(
            &servers[index]
                .server_url,
            &servers[index]
                .gateway_token,
        );

    let result =
        update_connection_result(
            &mut servers,
            index,
            &handshake,
        );

    let _ =
        write_servers(
            &servers,
        );

    result
}

#[tauri::command]
pub fn test_openclaw_connection_input(
    server: OpenClawServerInput,
) -> OpenClawConnectionResult {
    let checked_at =
        Utc::now().to_rfc3339();

    if let Err(error) =
        validate_input(
            &server,
            true,
        )
    {
        return OpenClawConnectionResult {
            success: false,
            state:
                "error".to_string(),
            message: error,
            checked_at,
            server: None,
        };
    }

    let handshake =
        gateway_handshake(
            &server.server_url,
            &server.gateway_token,
        );

    OpenClawConnectionResult {
        success:
            handshake.success,

        state:
            handshake.state,

        message:
            handshake.message,

        checked_at,

        server: None,
    }
}

#[tauri::command]
pub fn get_active_openclaw_status()
    -> Result<
        OpenClawRemoteStatus,
        String,
    >
{
    let servers =
        read_servers()?;

    let active =
        servers
            .iter()
            .find(
                |server| {
                    server.active &&
                    server.enabled
                },
            )
            .ok_or_else(|| {
                "No active OpenClaw server is configured."
                    .to_string()
            })?;

    let handshake =
        gateway_handshake(
            &active.server_url,
            &active.gateway_token,
        );

    Ok(OpenClawRemoteStatus {
        connected:
            handshake.success,

        server_id:
            active.id.clone(),

        server_name:
            active.name.clone(),

        server_url:
            active
                .server_url
                .clone(),

        gateway_status:
            Some(
                handshake
                    .state
                    .clone(),
            ),

        version:
            handshake.version,

        raw_response:
            handshake
                .payload
                .and_then(
                    |payload| {
                        serde_json::
                            to_string_pretty(
                                &payload,
                            )
                            .ok()
                    },
                ),
    })
}
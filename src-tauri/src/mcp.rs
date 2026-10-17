use chrono::Utc;

use serde::{Deserialize, Serialize};

use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub transport: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default)]
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInput {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub transport: String,

    pub command: Option<String>,

    #[serde(default)]
    pub args: Vec<String>,

    pub url: Option<String>,

    #[serde(default)]
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpActionResult {
    pub success: bool,
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<McpServer>,
}

fn success(message: impl Into<String>, server: Option<McpServer>) -> McpActionResult {
    McpActionResult {
        success: true,
        message: message.into(),
        server,
    }
}

fn failure(message: impl Into<String>) -> McpActionResult {
    McpActionResult {
        success: false,
        message: message.into(),
        server: None,
    }
}

fn config_directory() -> Result<PathBuf, String> {
    let home =
        dirs::home_dir().ok_or_else(|| "Unable to determine the home directory.".to_string())?;

    Ok(home.join(".ai-os").join("config"))
}

fn config_file() -> Result<PathBuf, String> {
    Ok(config_directory()?.join("mcp-servers.json"))
}

fn default_servers() -> Vec<McpServer> {
    vec![
        McpServer {
            id: "filesystem".to_string(),

            name: "Filesystem".to_string(),

            description: "Read and manage approved local files.".to_string(),

            enabled: false,

            transport: "stdio".to_string(),

            command: Some("npx".to_string()),

            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "~".to_string(),
            ],

            url: None,

            environment: HashMap::new(),
        },
        McpServer {
            id: "github".to_string(),

            name: "GitHub".to_string(),

            description: "Access repositories, issues and pull requests.".to_string(),

            enabled: false,

            transport: "stdio".to_string(),

            command: Some("npx".to_string()),

            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],

            url: None,

            environment: HashMap::from([(
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                String::new(),
            )]),
        },
    ]
}

fn read_servers() -> Result<Vec<McpServer>, String> {
    let path = config_file()?;

    if !path.exists() {
        let defaults = default_servers();

        write_servers(&defaults)?;

        return Ok(defaults);
    }

    let contents = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Unable to read MCP configuration {}: {}",
            path.display(),
            error
        )
    })?;

    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(&contents)
        .map_err(|error| format!("Unable to parse MCP configuration: {}", error))
}

fn write_servers(servers: &[McpServer]) -> Result<(), String> {
    let directory = config_directory()?;

    fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create MCP configuration directory: {}", error))?;

    let path = config_file()?;

    let contents = serde_json::to_string_pretty(servers)
        .map_err(|error| format!("Unable to serialize MCP configuration: {}", error))?;

    fs::write(&path, contents).map_err(|error| {
        format!(
            "Unable to save MCP configuration {}: {}",
            path.display(),
            error
        )
    })
}

fn normalize_transport(value: &str) -> Result<String, String> {
    match value.trim().to_lowercase().as_str() {
        "stdio" => Ok("stdio".to_string()),

        "http" => Ok("http".to_string()),

        "sse" => Ok("sse".to_string()),

        _ => Err("MCP transport must be stdio, http or sse.".to_string()),
    }
}

fn validate_input(input: &McpServerInput) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("MCP server name is required.".to_string());
    }

    let transport = normalize_transport(&input.transport)?;

    if transport == "stdio" {
        if input
            .command
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            return Err("A command is required for stdio MCP servers.".to_string());
        }
    } else if input.url.as_deref().unwrap_or_default().trim().is_empty() {
        return Err("A URL is required for HTTP and SSE MCP servers.".to_string());
    }

    Ok(())
}

fn make_id(name: &str) -> String {
    let normalized = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();

    let compact = normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    format!(
        "{}-{}",
        if compact.is_empty() {
            "server"
        } else {
            &compact
        },
        Utc::now().timestamp_millis(),
    )
}

fn server_from_input(id: String, input: McpServerInput) -> Result<McpServer, String> {
    validate_input(&input)?;

    let transport = normalize_transport(&input.transport)?;

    Ok(McpServer {
        id,

        name: input.name.trim().to_string(),

        description: input.description.trim().to_string(),

        enabled: input.enabled,

        command: if transport == "stdio" {
            input.command.map(|value| value.trim().to_string())
        } else {
            None
        },

        args: if transport == "stdio" {
            input
                .args
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect()
        } else {
            Vec::new()
        },

        url: if transport == "stdio" {
            None
        } else {
            input.url.map(|value| value.trim().to_string())
        },

        environment: input.environment,

        transport,
    })
}

#[tauri::command]
pub fn list_mcp_servers() -> Result<Vec<McpServer>, String> {
    read_servers()
}

#[tauri::command]
pub fn save_mcp_server(server: McpServerInput) -> McpActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return failure(error);
        }
    };

    let id = make_id(&server.name);

    let next_server = match server_from_input(id, server) {
        Ok(value) => value,

        Err(error) => {
            return failure(error);
        }
    };

    servers.push(next_server.clone());

    if let Err(error) = write_servers(&servers) {
        return failure(error);
    }

    success(
        format!("MCP server {} was added.", next_server.name),
        Some(next_server),
    )
}

#[tauri::command]
pub fn update_mcp_server(id: String, server: McpServerInput) -> McpActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return failure(error);
        }
    };

    let Some(index) = servers.iter().position(|current| current.id == id) else {
        return failure(format!("MCP server was not found: {}", id));
    };

    let next_server = match server_from_input(id, server) {
        Ok(value) => value,

        Err(error) => {
            return failure(error);
        }
    };

    servers[index] = next_server.clone();

    if let Err(error) = write_servers(&servers) {
        return failure(error);
    }

    success(
        format!("MCP server {} was updated.", next_server.name),
        Some(next_server),
    )
}

#[tauri::command]
pub fn toggle_mcp_server(id: String, enabled: bool) -> McpActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return failure(error);
        }
    };

    let Some(server) = servers.iter_mut().find(|server| server.id == id) else {
        return failure(format!("MCP server was not found: {}", id));
    };

    server.enabled = enabled;

    let updated = server.clone();

    if let Err(error) = write_servers(&servers) {
        return failure(error);
    }

    success(
        format!(
            "{} was {}.",
            updated.name,
            if enabled { "enabled" } else { "disabled" },
        ),
        Some(updated),
    )
}

#[tauri::command]
pub fn delete_mcp_server(id: String) -> McpActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return failure(error);
        }
    };

    let Some(index) = servers.iter().position(|server| server.id == id) else {
        return failure(format!("MCP server was not found: {}", id));
    };

    let removed = servers.remove(index);

    if let Err(error) = write_servers(&servers) {
        return failure(error);
    }

    success(
        format!("MCP server {} was deleted.", removed.name),
        Some(removed),
    )
}

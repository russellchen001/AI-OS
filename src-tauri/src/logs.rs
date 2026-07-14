use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    pub source: Option<String>,
    pub level: Option<String>,
    pub limit: usize,
}

fn home_directory() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Unable to determine the home directory.".to_string())
}

fn log_directory() -> Result<PathBuf, String> {
    Ok(home_directory()?.join(".ai-os").join("logs"))
}

fn source_file_name(source: &str) -> Option<&'static str> {
    match source {
        "AI OS" => Some("ai-os.log"),

        "OpenClaw" => Some("openclaw.log"),

        "Ollama" => Some("ollama.log"),

        "Docker" => Some("docker.log"),

        "Open WebUI" => Some("open-webui.log"),

        "Cherry Studio" => Some("cherry-studio.log"),

        _ => None,
    }
}

fn file_source_name(file_name: &str) -> &'static str {
    match file_name {
        "openclaw.log" => "OpenClaw",

        "ollama.log" => "Ollama",

        "docker.log" => "Docker",

        "open-webui.log" => "Open WebUI",

        "cherry-studio.log" => "Cherry Studio",

        _ => "AI OS",
    }
}

fn detect_level(message: &str) -> &'static str {
    let normalized = message.to_lowercase();

    if normalized.contains("error")
        || normalized.contains("failed")
        || normalized.contains("fatal")
        || normalized.contains("panic")
    {
        return "error";
    }

    if normalized.contains("warning") || normalized.contains("warn") {
        return "warning";
    }

    if normalized.contains("debug") || normalized.contains("trace") {
        return "debug";
    }

    "info"
}

fn read_file_lines(path: &Path, source: &str) -> Vec<LogEntry> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(Utc::now);

    contents
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let message = line.trim();

            if message.is_empty() {
                return None;
            }

            Some(LogEntry {
                id: format!("{}:{}:{}", source, modified.timestamp(), index,),

                timestamp: modified.to_rfc3339(),

                level: detect_level(message).to_string(),

                source: source.to_string(),

                message: message.to_string(),
            })
        })
        .collect()
}

fn read_known_files(selected_source: Option<&str>) -> Result<Vec<LogEntry>, String> {
    let directory = log_directory()?;

    fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create log directory: {}", error))?;

    let mut result = Vec::new();

    if let Some(source) = selected_source {
        if source != "All" {
            if let Some(file_name) = source_file_name(source) {
                result.extend(read_file_lines(&directory.join(file_name), source));
            }

            return Ok(result);
        }
    }

    let files = [
        "ai-os.log",
        "openclaw.log",
        "ollama.log",
        "docker.log",
        "open-webui.log",
        "cherry-studio.log",
    ];

    for file_name in files {
        result.extend(read_file_lines(
            &directory.join(file_name),
            file_source_name(file_name),
        ));
    }

    Ok(result)
}

fn command_output(command: &str, arguments: &[&str], source: &str) -> Vec<LogEntry> {
    let Ok(output) = Command::new(command).args(arguments).output() else {
        return Vec::new();
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    let stderr = String::from_utf8_lossy(&output.stderr);

    let timestamp = Utc::now().to_rfc3339();

    stdout
        .lines()
        .chain(stderr.lines())
        .enumerate()
        .filter_map(|(index, line)| {
            let message = line.trim();

            if message.is_empty() {
                return None;
            }

            Some(LogEntry {
                id: format!(
                    "{}:{}:{}",
                    source,
                    Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                    index,
                ),

                timestamp: timestamp.clone(),

                level: detect_level(message).to_string(),

                source: source.to_string(),

                message: message.to_string(),
            })
        })
        .collect()
}

fn live_service_logs(selected_source: Option<&str>) -> Vec<LogEntry> {
    let mut result = Vec::new();

    let include = |source: &str| {
        selected_source
            .map(|selected| selected == "All" || selected == source)
            .unwrap_or(true)
    };

    if include("Ollama") {
        result.extend(command_output(
            "/bin/launchctl",
            &["print", "gui/501/com.ollama.ollama"],
            "Ollama",
        ));
    }

    if include("Docker") {
        result.extend(command_output(
            "/usr/local/bin/docker",
            &["ps", "--format", "{{.Names}}\t{{.Status}}\t{{.Image}}"],
            "Docker",
        ));
    }

    if include("OpenClaw") {
        result.extend(
            command_output("/bin/launchctl", &["list"], "OpenClaw")
                .into_iter()
                .filter(|entry| entry.message.to_lowercase().contains("openclaw")),
        );
    }

    result
}

#[tauri::command]
pub fn get_logs(query: LogQuery) -> Result<Vec<LogEntry>, String> {
    let source = query.source.as_deref();

    let mut entries = read_known_files(source)?;

    entries.extend(live_service_logs(source));

    if let Some(level) = query.level.as_deref() {
        if level != "All" {
            entries.retain(|entry| entry.level == level);
        }
    }

    entries.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));

    let limit = query.limit.max(1);

    if entries.len() > limit {
        entries = entries.split_off(entries.len() - limit);
    }

    Ok(entries)
}

#[tauri::command]
pub fn clear_logs(source: String) -> Result<String, String> {
    let directory = log_directory()?;

    fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create log directory: {}", error))?;

    if source == "All" {
        let files = [
            "ai-os.log",
            "openclaw.log",
            "ollama.log",
            "docker.log",
            "open-webui.log",
            "cherry-studio.log",
        ];

        for file_name in files {
            let path = directory.join(file_name);

            if path.exists() {
                fs::write(path, "").map_err(|error| format!("Unable to clear logs: {}", error))?;
            }
        }

        return Ok("All AI OS log files were cleared.".to_string());
    }

    let file_name =
        source_file_name(&source).ok_or_else(|| format!("Unknown log source: {}", source))?;

    let path = directory.join(file_name);

    fs::write(&path, "").map_err(|error| format!("Unable to clear {} logs: {}", source, error))?;

    Ok(format!("{} logs were cleared.", source))
}

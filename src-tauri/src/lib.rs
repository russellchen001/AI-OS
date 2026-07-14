mod backup;
mod health;
mod logs;
mod mcp;
mod models;
mod openclaw;
mod stop;

use std::{process::Command, thread, time::Duration};

const DOCKER_CLI: &str = "/Applications/Docker.app/Contents/Resources/bin/docker";

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name,)
}

fn run_shell(command: &str) -> Result<String, String> {
    let output = Command::new("/bin/sh")
        .args(["-c", command])
        .output()
        .map_err(|error| error.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(stdout)
    } else if !stderr.is_empty() {
        Err(stderr)
    } else if !stdout.is_empty() {
        Err(stdout)
    } else {
        Err(format!("Command failed with status: {}", output.status,))
    }
}

fn spawn_shell(command: &str) -> Result<(), String> {
    Command::new("/bin/sh")
        .args(["-c", command])
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn open_application(name: &str) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .args(["-a", name])
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to open {}: {}", name, error,))
}

fn open_url(url: &str) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to open {}: {}", url, error,))
}

fn docker_is_ready() -> bool {
    Command::new(DOCKER_CLI)
        .arg("info")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn ensure_docker_ready() -> Result<(), String> {
    if docker_is_ready() {
        return Ok(());
    }

    open_application("Docker")?;

    for _ in 0..45 {
        thread::sleep(Duration::from_secs(1));

        if docker_is_ready() {
            return Ok(());
        }
    }

    Err("Docker Desktop did not become ready within 45 seconds.".to_string())
}

fn find_open_webui_container() -> Result<Option<String>, String> {
    let output = Command::new(DOCKER_CLI)
        .args(["ps", "-a", "--format", "{{.Names}}|{{.Image}}"])
        .output()
        .map_err(|error| format!("Failed to list Docker containers: {}", error,))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(if stderr.is_empty() {
            "Docker daemon is not available.".to_string()
        } else {
            stderr
        });
    }

    let containers = String::from_utf8_lossy(&output.stdout);

    let container_name = containers.lines().find_map(|line| {
        let lower = line.to_lowercase();

        let matches = lower.contains("open-webui")
            || lower.contains("open_webui")
            || lower.contains("openwebui")
            || lower.contains("ghcr.io/open-webui/open-webui");

        if !matches {
            return None;
        }

        line.split('|')
            .next()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    });

    Ok(container_name)
}

fn start_open_webui_container() -> Result<String, String> {
    ensure_docker_ready()?;

    let container_name = find_open_webui_container()?.ok_or_else(|| {
        "Open WebUI container was not found. Run `docker ps -a` to verify the container."
            .to_string()
    })?;

    let output = Command::new(DOCKER_CLI)
        .args(["start", &container_name])
        .output()
        .map_err(|error| {
            format!(
                "Failed to start Open WebUI container {}: {}",
                container_name, error,
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(format!(
            "✅ Open WebUI started successfully: {}\nOpen http://localhost:3000",
            container_name,
        ))
    } else {
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("Exit status: {}", output.status,)
        };

        Err(format!(
            "Failed to start Open WebUI container {}: {}",
            container_name, details,
        ))
    }
}

fn stop_open_webui_container() -> Result<String, String> {
    ensure_docker_ready()?;

    let container_name = find_open_webui_container()?
        .ok_or_else(|| "Open WebUI container was not found.".to_string())?;

    let output = Command::new(DOCKER_CLI)
        .args(["stop", &container_name])
        .output()
        .map_err(|error| {
            format!(
                "Failed to stop Open WebUI container {}: {}",
                container_name, error,
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(format!(
            "🛑 Open WebUI stopped successfully: {}",
            container_name,
        ))
    } else {
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("Exit status: {}", output.status,)
        };

        Err(format!(
            "Failed to stop Open WebUI container {}: {}",
            container_name, details,
        ))
    }
}

#[tauri::command]
fn start_all() -> String {
    let _ = open_application("Docker");

    let _ = spawn_shell(
        r#"
if command -v brew >/dev/null 2>&1; then
  brew services start ollama >/dev/null 2>&1 || true
elif [ -x /opt/homebrew/bin/brew ]; then
  /opt/homebrew/bin/brew services start ollama >/dev/null 2>&1 || true
fi

pgrep -f "ollama serve" >/dev/null 2>&1 ||
  nohup ollama serve >/tmp/ollama.log 2>&1 &
"#,
    );

    let _ = spawn_shell(
        r#"
launchctl bootstrap gui/$(id -u) \
  "$HOME/Library/LaunchAgents/ai.openclaw.gateway.plist" \
  2>/dev/null || true

launchctl kickstart -k \
  gui/$(id -u)/ai.openclaw.gateway \
  2>/dev/null || true
"#,
    );

    let _ = spawn_shell(
        r#"
DOCKER="/Applications/Docker.app/Contents/Resources/bin/docker"

for i in $(seq 1 45); do
  if "$DOCKER" info >/dev/null 2>&1; then
    break
  fi

  sleep 1
done

CONTAINER=$(
  "$DOCKER" ps -a \
    --format '{{.Names}}|{{.Image}}' \
    2>/dev/null |
  awk -F'|' '
    BEGIN { IGNORECASE=1 }
    /open-webui|open_webui|openwebui|ghcr.io\/open-webui\/open-webui/ {
      print $1
      exit
    }
  '
)

if [ -n "$CONTAINER" ]; then
  "$DOCKER" start "$CONTAINER" >/dev/null 2>&1 || true
fi
"#,
    );

    let _ = open_application("Cherry Studio");

    "🚀 Start All command sent. Docker and Open WebUI may need up to 45 seconds.".to_string()
}

#[tauri::command]
fn start_service(service: String) -> Result<String, String> {
    match service.as_str() {
        "OpenClaw" => {
            spawn_shell(
                r#"
launchctl bootstrap gui/$(id -u) \
  "$HOME/Library/LaunchAgents/ai.openclaw.gateway.plist" \
  2>/dev/null || true

launchctl kickstart -k \
  gui/$(id -u)/ai.openclaw.gateway
"#,
            )?;

            Ok("🚀 OpenClaw start command sent.".to_string())
        }

        "Ollama" => {
            spawn_shell(
                r#"
if command -v brew >/dev/null 2>&1; then
  brew services start ollama >/dev/null 2>&1 || true
elif [ -x /opt/homebrew/bin/brew ]; then
  /opt/homebrew/bin/brew services start ollama >/dev/null 2>&1 || true
fi

pgrep -f "ollama serve" >/dev/null 2>&1 ||
  nohup ollama serve >/tmp/ollama.log 2>&1 &
"#,
            )?;

            Ok("🚀 Ollama start command sent.".to_string())
        }

        "Docker" => {
            if docker_is_ready() {
                return Ok("✅ Docker Desktop is already running.".to_string());
            }

            open_application("Docker")?;

            Ok("🚀 Docker Desktop is starting. Please allow up to 45 seconds.".to_string())
        }

        "Open WebUI" => start_open_webui_container(),

        "Cherry Studio" => {
            open_application("Cherry Studio")?;

            Ok("🚀 Cherry Studio start command sent.".to_string())
        }

        _ => Err(format!("Unknown service: {}", service,)),
    }
}

#[tauri::command]
fn stop_service(service: String) -> Result<String, String> {
    match service.as_str() {
        "OpenClaw" => {
            spawn_shell(
                r#"
launchctl bootout \
  gui/$(id -u)/ai.openclaw.gateway \
  2>/dev/null || true

launchctl remove ai.openclaw.gateway \
  2>/dev/null || true

pkill -f "openclaw" \
  2>/dev/null || true
"#,
            )?;

            Ok("🛑 OpenClaw stop command sent.".to_string())
        }

        "Ollama" => {
            spawn_shell(
                r#"
if command -v brew >/dev/null 2>&1; then
  brew services stop ollama >/dev/null 2>&1 || true
elif [ -x /opt/homebrew/bin/brew ]; then
  /opt/homebrew/bin/brew services stop ollama >/dev/null 2>&1 || true
fi

pkill -f "ollama serve" \
  2>/dev/null || true
"#,
            )?;

            Ok("🛑 Ollama stop command sent.".to_string())
        }

        "Docker" => {
            if !docker_is_ready() {
                return Ok("✅ Docker Desktop is already stopped.".to_string());
            }

            let output = Command::new(DOCKER_CLI)
                .args(["desktop", "stop", "--force"])
                .output()
                .map_err(|error| format!("Failed to run Docker stop command: {}", error,))?;

            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            if output.status.success() {
                Ok("🛑 Docker Desktop stopped successfully.".to_string())
            } else {
                let details = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!("Exit status: {}", output.status,)
                };

                Err(format!("Docker stop command failed: {}", details,))
            }
        }

        "Open WebUI" => stop_open_webui_container(),

        "Cherry Studio" => {
            spawn_shell(
                r#"
osascript -e \
  'tell application "Cherry Studio" to quit' \
  2>/dev/null ||
pkill -f "Cherry Studio" \
  2>/dev/null || true
"#,
            )?;

            Ok("🛑 Cherry Studio stop command sent.".to_string())
        }

        _ => Err(format!("Unknown service: {}", service,)),
    }
}

#[tauri::command]
fn open_service(
    service: String,
    openclaw_url: Option<String>,
    ollama_url: Option<String>,
    open_web_ui_url: Option<String>,
) -> Result<String, String> {
    match service.as_str() {
        "OpenClaw" => {
            let url = openclaw_url
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "http://localhost:18789".to_string());

            open_url(&url)?;

            Ok(format!("↗ Opened OpenClaw: {}", url,))
        }

        "Ollama" => {
            let url = ollama_url
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "http://localhost:11434".to_string());

            open_url(&url)?;

            Ok(format!("↗ Opened Ollama: {}", url,))
        }

        "Docker" => {
            open_application("Docker")?;

            Ok("↗ Opened Docker Desktop.".to_string())
        }

        "Open WebUI" => {
            let url = open_web_ui_url
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "http://localhost:3000".to_string());

            open_url(&url)?;

            Ok(format!("↗ Opened Open WebUI: {}", url,))
        }

        "Cherry Studio" => {
            open_application("Cherry Studio")?;

            Ok("↗ Opened Cherry Studio.".to_string())
        }

        _ => Err(format!("Unknown service: {}", service,)),
    }
}

#[tauri::command]
fn system_metrics() -> Result<String, String> {
    let script = r#"
CPU=$(top -l 1 -n 0 | awk '/CPU usage/ {
  user=$3
  sys=$5
  gsub("%","",user)
  gsub("%","",sys)
  printf "%.1f", user + sys
}')

PAGE_SIZE=$(pagesize)

ACTIVE=$(vm_stat | awk '/Pages active/ {
  gsub("\\.","",$3)
  print $3
}')

WIRED=$(vm_stat | awk '/Pages wired down/ {
  gsub("\\.","",$4)
  print $4
}')

COMPRESSED=$(vm_stat | awk '/Pages occupied by compressor/ {
  gsub("\\.","",$5)
  print $5
}')

MEM_TOTAL_BYTES=$(sysctl -n hw.memsize)

MEM_USED_BYTES=$(( \
  (${ACTIVE:-0} + ${WIRED:-0} + ${COMPRESSED:-0}) \
  * PAGE_SIZE \
))

MEM_USED_GB=$(awk "BEGIN {
  printf \"%.2f\", $MEM_USED_BYTES / 1073741824
}")

MEM_TOTAL_GB=$(awk "BEGIN {
  printf \"%.2f\", $MEM_TOTAL_BYTES / 1073741824
}")

DISK_LINE=$(df -k / | tail -1)

DISK_TOTAL_KB=$(echo "$DISK_LINE" | awk '{print $2}')
DISK_USED_KB=$(echo "$DISK_LINE" | awk '{print $3}')

DISK_TOTAL_GB=$(awk "BEGIN {
  printf \"%.2f\", $DISK_TOTAL_KB / 1048576
}")

DISK_USED_GB=$(awk "BEGIN {
  printf \"%.2f\", $DISK_USED_KB / 1048576
}")

echo "$CPU|$MEM_USED_GB|$MEM_TOTAL_GB|$DISK_USED_GB|$DISK_TOTAL_GB"
"#;

    run_shell(script)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_all,
            start_service,
            stop_service,
            open_service,
            system_metrics,
            health::health_check,
            stop::stop_all,
            backup::create_backup,
            backup::cancel_backup,
            backup::restore_backup,
            backup::list_backups,
            backup::reveal_backup,
            backup::delete_backup,
            logs::get_logs,
            logs::clear_logs,
            models::list_ollama_models,
            models::pull_ollama_model,
            models::delete_ollama_model,
            models::run_ollama_model,
            models::show_ollama_model,
            mcp::list_mcp_servers,
            mcp::save_mcp_server,
            mcp::update_mcp_server,
            mcp::toggle_mcp_server,
            mcp::delete_mcp_server,
            openclaw::list_openclaw_servers,
            openclaw::save_openclaw_server,
            openclaw::update_openclaw_server,
            openclaw::delete_openclaw_server,
            openclaw::duplicate_openclaw_server,
            openclaw::toggle_openclaw_server,
            openclaw::set_active_openclaw_server,
            openclaw::test_openclaw_connection,
            openclaw::test_openclaw_connection_input,
            openclaw::test_all_openclaw_servers,
            openclaw::get_active_openclaw_status,
            openclaw::get_openclaw_dashboard_summary,
            openclaw::get_openclaw_runtime_config,
            openclaw::export_openclaw_servers,
            openclaw::import_openclaw_servers,
            openclaw::invoke_active_openclaw_gateway,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

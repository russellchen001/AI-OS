mod backup;
mod health;
mod logs;
mod mcp;
mod models;
mod multillm;
mod openclaw;
mod runtime;
mod stop;

use std::process::Command;

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
        .manage(runtime::executor::RuntimeExecutionState::default())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_all,
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
            multillm::start_multillm_stream,
            multillm::cancel_multillm_stream,
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
            runtime::list_runtimes,
            runtime::get_runtime_statuses,
            runtime::ipc::start_runtime_operation,
            runtime::ipc::get_runtime_operation,
            runtime::ipc::cancel_runtime_operation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

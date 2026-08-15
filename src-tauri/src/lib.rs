mod backup;
mod claude_code;
mod conversations;
mod health;
mod logs;
mod mcp;
mod mcp_runtime;
mod memory;
mod memory_service;
mod models;
mod multillm;
mod openclaw;
pub mod planner;
mod providers;
mod runtime;
mod task_execution;
pub mod task_plan_orchestration;

use std::process::Command;
use tauri::Manager;

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
    let runtime_state = runtime::executor::RuntimeExecutionState::default();
    let task_runtime_state = runtime_state.clone();

    tauri::Builder::default()
        .manage(runtime_state)
        .setup(move |app| {
            let emitter = std::sync::Arc::new(runtime::executor::TauriEventEmitter::new(
                app.handle().clone(),
            ));
            app.manage(task_execution::build_task_execution_state(
                task_runtime_state.clone(),
                emitter,
            ));
            Ok(())
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            system_metrics,
            conversations::list_native_conversations,
            conversations::save_native_conversation,
            conversations::delete_native_conversation,
            conversations::import_native_conversations,
            health::health_check,
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
            mcp_runtime::list_mcp_tools,
            mcp_runtime::call_mcp_tool,
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
            runtime::skills::registry::list_skills,
            runtime::skills::registry::get_skill,
            runtime::get_runtime_statuses,
            runtime::ipc::start_runtime_operation,
            runtime::bulk::start_runtime_bulk_operation,
            runtime::ipc::get_runtime_operation,
            runtime::ipc::cancel_runtime_operation,
            task_execution::submit_chat_task,
            task_execution::start_chat_task_execution,
            task_execution::complete_chat_task_execution,
            task_execution::fail_chat_task_execution,
            task_execution::execute_chat_work_task,
            providers::list_provider_instances,
            providers::save_provider_instance,
            providers::remove_provider_instance,
            providers::list_provider_adapters,
            providers::get_provider_adapter,
            providers::set_provider_credential,
            providers::get_provider_credential_status,
            providers::delete_provider_credential,
            providers::discover_provider_models,
            providers::test_provider_connection,
            providers::begin_provider_oauth,
            providers::begin_grok_device_auth,
            providers::complete_grok_device_auth,
            providers::cancel_grok_device_auth,
            providers::begin_kimi_device_auth,
            providers::complete_kimi_device_auth,
            providers::cancel_kimi_device_auth,
            providers::complete_provider_oauth,
            providers::cancel_provider_oauth,
            providers::refresh_provider_oauth,
            providers::generate_provider_response,
            providers::execute_ai_center,
            providers::execute_ai_center_stream,
            providers::resolve_ai_center_route,
            providers::start_provider_response_stream,
            providers::cancel_provider_response_stream,
            claude_code::get_claude_code_status,
            claude_code::generate_claude_code_response,
            claude_code::cancel_claude_code_request,
            memory::save_memory,
            memory::list_memory,
            memory::delete_memory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

pub mod task_engine;

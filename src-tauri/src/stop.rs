use std::process::Command;

fn run_shell(command: &str) -> (bool, String) {
    match Command::new("/bin/sh")
        .args(["-c", command])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            let details = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };

            (output.status.success(), details)
        }
        Err(error) => (false, error.to_string()),
    }
}

#[tauri::command]
pub fn stop_all() -> String {
    let (openclaw_ok, _) = run_shell(
        "launchctl bootout gui/$(id -u)/ai.openclaw.gateway 2>/dev/null \
         || launchctl remove ai.openclaw.gateway 2>/dev/null",
    );

    let (ollama_ok, _) = run_shell(
        "/opt/homebrew/bin/brew services stop ollama 2>/dev/null \
         || pkill -f 'ollama serve'",
    );

    let (cherry_ok, _) = run_shell(
        "osascript -e 'tell application \"Cherry Studio\" to quit' 2>/dev/null \
         || pkill -f 'Cherry Studio'",
    );

    let (docker_ok, docker_output) = run_shell(
        "\"/Applications/Docker.app/Contents/Resources/bin/docker\" \
         desktop stop --force",
    );

    format!(
        "🛑 Stop All completed\n\n\
OpenClaw: {}\n\
Ollama: {}\n\
Cherry Studio: {}\n\
Docker: {}\n\n\
Docker command output:\n{}",
        if openclaw_ok { "Stopped" } else { "Failed / already stopped" },
        if ollama_ok { "Stopped" } else { "Failed / already stopped" },
        if cherry_ok { "Stopped" } else { "Failed / already stopped" },
        if docker_ok { "Command succeeded" } else { "Command failed" },
        if docker_output.is_empty() {
            "(no output)"
        } else {
            &docker_output
        },
    )
}
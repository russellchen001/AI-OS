use std::process::Command;

pub fn check_service(name: &str) -> bool {
    match name {
        "openclaw" => {
            Command::new("launchctl")
                .args(["list"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("ai.openclaw.gateway"))
                .unwrap_or(false)
        }
        "docker" => {
            Command::new("docker")
                .arg("info")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        "ollama" => {
            Command::new("curl")
                .args(["-s", "http://127.0.0.1:11434/api/tags"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        _ => false,
    }
}
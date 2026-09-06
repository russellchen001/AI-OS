//! Direct system power requests.
//!
//! Computer Control may request normal macOS sleep, restart, or shutdown.
//! It does not escalate when the operating system refuses the request.
//!
//! Power never uses privilege escalation, low-level forced power commands,
//! process termination, GUI clicking, or keyboard simulation.
//!
//! Runtime separately enforces current-execution confirmation. Persistent
//! Trusted Automation approval cannot authorize these operations.

use crate::system::SystemError;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PowerAction {
    Sleep,
    Restart,
    Shutdown,
}

impl PowerAction {
    fn capability(self) -> &'static str {
        match self {
            Self::Sleep => "system.power.sleep",
            Self::Restart => "system.power.restart",
            Self::Shutdown => "system.power.shutdown",
        }
    }

    fn script(self) -> &'static str {
        match self {
            Self::Sleep => r#"tell application "System Events" to sleep"#,
            Self::Restart => r#"tell application "System Events" to restart"#,
            Self::Shutdown => r#"tell application "System Events" to shut down"#,
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Restart => "restart",
            Self::Shutdown => "shutdown",
        }
    }
}

fn validate_input(input: &Value) -> Result<(), SystemError> {
    let object = input.as_object().ok_or_else(|| {
        SystemError::invalid("system power capabilities require an empty input object")
    })?;

    if !object.is_empty() {
        return Err(SystemError::invalid(
            "system power capabilities do not accept parameters",
        ));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn execute(action: PowerAction, input: &Value) -> Result<Value, SystemError> {
    validate_input(input)?;

    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(action.script())
        .output()
        .map_err(|error| {
            SystemError::execution(format!(
                "macOS {} request could not start: {error}",
                action.operation()
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

        return Err(SystemError::execution(if stderr.is_empty() {
            format!("macOS refused the {} request", action.operation())
        } else {
            format!("macOS refused the {} request: {stderr}", action.operation())
        }));
    }

    Ok(json!({
        "capability": action.capability(),
        "operationResult": {
            "status": "requested",
            "operation": action.operation(),
            "provider": "macos-system-events"
        }
    }))
}

pub(crate) fn sleep(input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        return execute(PowerAction::Sleep, input);
    }

    #[cfg(not(target_os = "macos"))]
    {
        validate_input(input)?;

        Err(SystemError::execution(
            "system.power.sleep has no adapter on this operating system in v1.0",
        ))
    }
}

pub(crate) fn restart(input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        return execute(PowerAction::Restart, input);
    }

    #[cfg(not(target_os = "macos"))]
    {
        validate_input(input)?;

        Err(SystemError::execution(
            "system.power.restart has no adapter on this operating system in v1.0",
        ))
    }
}

pub(crate) fn shutdown(input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        return execute(PowerAction::Shutdown, input);
    }

    #[cfg(not(target_os = "macos"))]
    {
        validate_input(input)?;

        Err(SystemError::execution(
            "system.power.shutdown has no adapter on this operating system in v1.0",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_capabilities_accept_only_empty_object_input() {
        assert!(validate_input(&json!({})).is_ok());

        for rejected in [
            json!(null),
            json!([]),
            json!("now"),
            json!({"force": true}),
            json!({"delay": 0}),
            json!({"reason": "test"}),
        ] {
            let error = validate_input(&rejected).unwrap_err();
            assert!(error.invalid_request);
        }
    }

    #[test]
    fn power_actions_have_exact_capability_and_platform_script() {
        assert_eq!(PowerAction::Sleep.capability(), "system.power.sleep");
        assert_eq!(PowerAction::Restart.capability(), "system.power.restart");
        assert_eq!(PowerAction::Shutdown.capability(), "system.power.shutdown");

        assert_eq!(
            PowerAction::Sleep.script(),
            r#"tell application "System Events" to sleep"#
        );
        assert_eq!(
            PowerAction::Restart.script(),
            r#"tell application "System Events" to restart"#
        );
        assert_eq!(
            PowerAction::Shutdown.script(),
            r#"tell application "System Events" to shut down"#
        );
    }

    #[test]
    fn power_scripts_are_only_normal_system_events_requests() {
        for action in [
            PowerAction::Sleep,
            PowerAction::Restart,
            PowerAction::Shutdown,
        ] {
            let script = action.script();

            assert!(script.starts_with(r#"tell application "System Events" to "#));

            assert!(!script.contains("keystroke"));
            assert!(!script.contains("do shell script"));
            assert!(!script.contains("with administrator privileges"));
        }
    }

    // Deliberately no real destructive E2E. A successful restart or shutdown
    // would terminate the test runner and could interrupt the person's work.
}

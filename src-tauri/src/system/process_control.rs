//! Destructive process control.
//!
//! Process observation stays in `inspect.rs`. Termination lives separately so
//! read-only machine inspection can never accidentally grow a destructive side
//! effect.
//!
//! A PID alone is not an identity. Operating systems reuse process identifiers,
//! so the caller must supply the `startedAtUnixSeconds` value previously read
//! from `system.process.info`. AI-OS refreshes the PID immediately before
//! signalling it and fails closed if the process birth time changed.
//!
//! v1 sends only a normal termination request (`SIGTERM` where supported).
//! There is deliberately no force flag and no SIGKILL fallback.

use crate::system::SystemError;
use serde_json::{json, Map, Value};
use std::{thread, time::Duration};
use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

const EXIT_OBSERVATION_ATTEMPTS: usize = 50;
const EXIT_OBSERVATION_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminationTarget {
    pid: u32,
    expected_start_time_unix_seconds: u64,
}

fn parse_target(input: &Value) -> Result<TerminationTarget, SystemError> {
    let object = input.as_object().ok_or_else(|| {
        SystemError::invalid(
            "system.process.terminate requires an object with pid and expectedStartTimeUnixSeconds",
        )
    })?;

    validate_exact_keys(object)?;

    let pid = object
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            SystemError::invalid(
                "system.process.terminate requires pid as an unsigned 32-bit integer",
            )
        })?;

    if pid <= 1 {
        return Err(SystemError::invalid(
            "system.process.terminate refuses special system process identifiers 0 and 1",
        ));
    }

    if pid == std::process::id() {
        return Err(SystemError::invalid(
            "system.process.terminate refuses to terminate the AI-OS process executing the request",
        ));
    }

    let expected_start_time_unix_seconds = object
        .get("expectedStartTimeUnixSeconds")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            SystemError::invalid(
                "system.process.terminate requires expectedStartTimeUnixSeconds as a positive integer",
            )
        })?;

    Ok(TerminationTarget {
        pid,
        expected_start_time_unix_seconds,
    })
}

fn validate_exact_keys(object: &Map<String, Value>) -> Result<(), SystemError> {
    if object.len() != 2
        || !object.contains_key("pid")
        || !object.contains_key("expectedStartTimeUnixSeconds")
    {
        return Err(SystemError::invalid(
            "system.process.terminate accepts only pid and expectedStartTimeUnixSeconds; force, signal and delay controls are not supported",
        ));
    }

    Ok(())
}

fn refresh_one(system: &mut System, pid: Pid) {
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
}

fn is_terminal_process_state(process: &sysinfo::Process) -> bool {
    matches!(
        format!("{:?}", process.status()).as_str(),
        "Zombie" | "Dead"
    )
}

fn observe_original_process(
    system: &mut System,
    pid: Pid,
    expected_start_time: u64,
) -> Option<&'static str> {
    refresh_one(system, pid);

    match system.process(pid) {
        None => Some("absent"),
        Some(process) if process.start_time() != expected_start_time => Some("pidReused"),
        Some(process) if is_terminal_process_state(process) => Some("terminalState"),
        Some(_) => None,
    }
}

pub(crate) fn terminate_process(input: &Value) -> Result<Value, SystemError> {
    let target = parse_target(input)?;
    let pid = Pid::from_u32(target.pid);

    let mut system = System::new();
    refresh_one(&mut system, pid);

    let process = system.process(pid).ok_or_else(|| {
        SystemError::invalid(format!(
            "no process is currently running with pid {}",
            target.pid
        ))
    })?;

    let observed_start_time = process.start_time();

    if observed_start_time != target.expected_start_time_unix_seconds {
        return Err(SystemError::invalid(format!(
            "pid {} no longer identifies the process that was confirmed; expected start time {}, observed {}",
            target.pid,
            target.expected_start_time_unix_seconds,
            observed_start_time
        )));
    }

    match process.kill_with(Signal::Term) {
        None => {
            return Err(SystemError::execution(
                "normal process termination is not supported on this operating system",
            ));
        }
        Some(false) => {
            return Err(SystemError::execution(format!(
                "the operating system refused to send a normal termination request to pid {}",
                target.pid
            )));
        }
        Some(true) => {}
    }

    for _ in 0..EXIT_OBSERVATION_ATTEMPTS {
        if let Some(observation) =
            observe_original_process(&mut system, pid, target.expected_start_time_unix_seconds)
        {
            return Ok(json!({
                "capability": "system.process.terminate",
                "operationResult": {
                    "status": "terminated",
                    "pid": target.pid,
                    "startedAtUnixSeconds": target.expected_start_time_unix_seconds,
                    "signal": "term",
                    "observation": observation
                },
                "warnings": []
            }));
        }

        thread::sleep(EXIT_OBSERVATION_INTERVAL);
    }

    Err(SystemError::execution(format!(
        "pid {} remained active after the normal termination request; AI-OS did not escalate to a force kill",
        target.pid
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn termination_request_requires_exact_process_identity() {
        assert_eq!(
            parse_target(&json!({
                "pid": 1234,
                "expectedStartTimeUnixSeconds": 5678
            }))
            .unwrap(),
            TerminationTarget {
                pid: 1234,
                expected_start_time_unix_seconds: 5678,
            }
        );

        for rejected in [
            json!(null),
            json!({}),
            json!({"pid":1234}),
            json!({"expectedStartTimeUnixSeconds":5678}),
            json!({
                "pid":1234,
                "expectedStartTimeUnixSeconds":0
            }),
            json!({
                "pid":1234.5,
                "expectedStartTimeUnixSeconds":5678
            }),
            json!({
                "pid":"1234",
                "expectedStartTimeUnixSeconds":5678
            }),
            json!({
                "pid":1234,
                "expectedStartTimeUnixSeconds":"5678"
            }),
            json!({
                "pid":1234,
                "expectedStartTimeUnixSeconds":5678,
                "force":true
            }),
            json!({
                "pid":1234,
                "expectedStartTimeUnixSeconds":5678,
                "signal":"kill"
            }),
            json!({
                "pid":1234,
                "expectedStartTimeUnixSeconds":5678,
                "delay":1
            }),
        ] {
            assert!(parse_target(&rejected).unwrap_err().invalid_request);
        }
    }

    #[test]
    fn special_and_self_process_targets_are_rejected_before_signalling() {
        for pid in [0, 1, std::process::id()] {
            assert!(
                parse_target(&json!({
                    "pid":pid,
                    "expectedStartTimeUnixSeconds":1
                }))
                .unwrap_err()
                .invalid_request
            );
        }
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "spawns and terminates only this test's disposable /bin/sleep child"]
    fn disposable_child_is_identity_checked_terminated_and_observed_real_e2e() {
        let mut child = Command::new("/bin/sleep")
            .arg("20")
            .spawn()
            .expect("spawn disposable sleep child");

        let pid_u32 = child.id();
        let pid = Pid::from_u32(pid_u32);
        let mut system = System::new();

        let started_at = (0..40)
            .find_map(|_| {
                refresh_one(&mut system, pid);
                let observed = system.process(pid).map(|process| process.start_time());

                if observed.is_none() {
                    thread::sleep(Duration::from_millis(25));
                }

                observed
            })
            .expect("disposable child became observable");

        let wrong_identity = terminate_process(&json!({
            "pid": pid_u32,
            "expectedStartTimeUnixSeconds": started_at.saturating_add(1)
        }))
        .unwrap_err();

        assert!(
            wrong_identity.invalid_request,
            "stale identity must fail before signalling"
        );

        assert!(
            child.try_wait().unwrap().is_none(),
            "identity mismatch must leave the child running"
        );

        let result = terminate_process(&json!({
            "pid": pid_u32,
            "expectedStartTimeUnixSeconds": started_at
        }))
        .expect("terminate disposable child");

        assert_eq!(result["operationResult"]["status"], json!("terminated"));
        assert_eq!(result["operationResult"]["pid"], json!(pid_u32));
        assert_eq!(
            result["operationResult"]["startedAtUnixSeconds"],
            json!(started_at)
        );
        assert_eq!(result["operationResult"]["signal"], json!("term"));

        let observation = result["operationResult"]["observation"].as_str().unwrap();

        assert!(
            matches!(observation, "absent" | "terminalState" | "pidReused"),
            "termination requires observation that the original process is gone"
        );

        let status = child.wait().expect("reap disposable child");

        assert!(
            !status.success(),
            "sleep should have ended because SIGTERM interrupted it"
        );
    }
}

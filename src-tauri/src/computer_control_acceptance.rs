//! Computer Control v1 final cross-capability acceptance.
//!
//! This module is test-only and adds no product capability.
//!
//! The final workflow deliberately includes only capabilities admitted to the
//! current v1 product scope. Native notification delivery was deferred after
//! real acceptance exposed platform-administration requirements outside that
//! scope.

use crate::runtime::openclaw_execution::{
    OpenClawExecutionAdapter, OpenClawExecutionRequest, OpenClawExecutionResult,
};
use crate::runtime::openclaw_gateway_adapter::OpenClawGatewayExecutionAdapter;
use crate::runtime::openclaw_permission::{
    ConfiguredCapabilityPermissionGate, PermissionEnforcingOpenClawExecutionAdapter,
};
use serde_json::{json, Value};
use std::sync::Arc;

fn runtime_adapter() -> PermissionEnforcingOpenClawExecutionAdapter {
    PermissionEnforcingOpenClawExecutionAdapter::new(
        Arc::new(ConfiguredCapabilityPermissionGate::new(Vec::<String>::new())),
        Arc::new(OpenClawGatewayExecutionAdapter),
    )
}

fn execute(
    adapter: &PermissionEnforcingOpenClawExecutionAdapter,
    execution_id: &str,
    capability: &str,
    input: Value,
    user_confirmed: bool,
) -> Result<OpenClawExecutionResult, crate::runtime::openclaw_execution::OpenClawExecutionError> {
    let request = OpenClawExecutionRequest::new(execution_id, capability, input)?
        .with_user_confirmation(user_confirmed);

    adapter.execute(&request, &mut |_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::openclaw_execution::OpenClawExecutionErrorKind;
    use std::{process::Command, thread, time::Duration};

    fn confirmed(
        adapter: &PermissionEnforcingOpenClawExecutionAdapter,
        id: &str,
        capability: &str,
        input: Value,
    ) -> OpenClawExecutionResult {
        execute(adapter, id, capability, input, true).unwrap_or_else(|error| {
            panic!(
                "{capability} failed through final Runtime path: {}",
                error.message
            )
        })
    }

    fn assert_capability(result: &OpenClawExecutionResult, capability: &str) {
        assert_eq!(
            result.output["capability"],
            json!(capability),
            "{capability} returned a different capability envelope"
        );
    }

    #[test]
    fn final_cross_capability_workflow_real_e2e() {
        if !cfg!(target_os = "macos") {
            return;
        }

        let adapter = runtime_adapter();

        let storage = confirmed(&adapter, "cc-e2-storage", "system.storage", json!({}));
        assert_capability(&storage, "system.storage");

        let applications = confirmed(
            &adapter,
            "cc-e2-app-running",
            "system.app.running",
            json!({}),
        );
        assert_capability(&applications, "system.app.running");

        let audio = confirmed(
            &adapter,
            "cc-e2-audio",
            "system.audio.volume.get",
            json!({}),
        );
        assert_capability(&audio, "system.audio.volume.get");

        let power_error = execute(
            &adapter,
            "cc-e2-unconfirmed-power",
            "system.power.shutdown",
            json!({}),
            false,
        )
        .unwrap_err();

        assert_eq!(
            power_error.kind,
            OpenClawExecutionErrorKind::PermissionRequired
        );

        let mut child = Command::new("/bin/sleep")
            .arg("20")
            .spawn()
            .expect("spawn disposable workflow process");

        let pid = child.id();

        let process_info = (0..40)
            .find_map(|attempt| {
                match execute(
                    &adapter,
                    &format!("cc-e2-process-info-{attempt}"),
                    "system.process.info",
                    json!({"pid": pid}),
                    true,
                ) {
                    Ok(result) => Some(result),
                    Err(_) => {
                        thread::sleep(Duration::from_millis(25));
                        None
                    }
                }
            })
            .expect("disposable workflow process became observable");

        assert_capability(&process_info, "system.process.info");

        let started_at = process_info.output["operationResult"]["startedAtUnixSeconds"]
            .as_u64()
            .expect("process info carries stable start identity");

        let termination_input = json!({
            "pid": pid,
            "expectedStartTimeUnixSeconds": started_at
        });

        let unconfirmed = execute(
            &adapter,
            "cc-e2-process-terminate-unconfirmed",
            "system.process.terminate",
            termination_input.clone(),
            false,
        )
        .unwrap_err();

        assert_eq!(
            unconfirmed.kind,
            OpenClawExecutionErrorKind::PermissionRequired
        );

        assert!(
            child.try_wait().unwrap().is_none(),
            "unconfirmed termination must leave the test child alive"
        );

        let terminated = confirmed(
            &adapter,
            "cc-e2-process-terminate-confirmed",
            "system.process.terminate",
            termination_input,
        );

        assert_capability(&terminated, "system.process.terminate");

        assert_eq!(
            terminated.output["operationResult"]["status"],
            json!("terminated")
        );

        let status = child.wait().expect("reap disposable workflow child");

        assert!(
            !status.success(),
            "the disposable child should have ended through normal termination"
        );
    }
}

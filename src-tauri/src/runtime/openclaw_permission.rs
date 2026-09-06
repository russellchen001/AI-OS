use super::openclaw_execution::{
    OpenClawExecutionAdapter, OpenClawExecutionError, OpenClawExecutionErrorKind,
    OpenClawExecutionProgress, OpenClawExecutionRequest, OpenClawExecutionResult,
};
use std::sync::Arc;
use std::{collections::HashSet, iter::IntoIterator};

/// What a person can authorise once, in the moment, for a single run.
///
/// A capability that is not on this list is DENIED no matter what else in the
/// build supports it. That is how twelve capabilities -- implemented, routed
/// through the resolver, covered by the gate -- turned out to be uncallable in
/// production: everything about them was right except that nobody could say
/// yes to them. It is the sixth time in this work that a working adapter has
/// been reachable from nowhere, and the first five were each found by hand.
///
/// `every_capability_the_resolver_can_route_is_one_a_person_can_authorise` is
/// what makes a seventh impossible: declaring a capability the resolver can
/// route now fails the suite until a person can also permit it.
pub(crate) const CONFIRMABLE_CAPABILITIES: &[&str] = &[
    "filesystem.scan",
    "filesystem.read",
    "filesystem.write",
    "filesystem.move",
    "download.start",
    "document.read",
    "document.create",
    "document.edit",
    "document.convert",
    // Work done to a PDF itself, in Rust, on any machine. Every one of these
    // writes a file, so every one of them is something a person says yes to.
    "document.merge",
    "document.split",
    "document.rotate",
    "document.encrypt",
    "document.decrypt",
    "document.annotate",
    "document.fill",
    "document.redact",
    "document.stamp",
    "document.replace",
    "spreadsheet.read",
    "spreadsheet.create",
    "spreadsheet.edit",
    "spreadsheet.convert",
    "presentation.read",
    "presentation.create",
    "presentation.edit",
    "presentation.export",
    "presentation.convert",
    // Computer Control. Reading what the machine is doing is still something a
    // person says yes to: it is their machine, and a process list says what
    // they are running.
    "system.storage",
    "system.cpu",
    "system.memory",
    "system.network",
    "system.process.list",
    "system.process.info",
    // Process termination is destructive and additionally appears in
    // ALWAYS_CONFIRM_CAPABILITIES below.
    "system.process.terminate",
    // Starting and stopping an application is doing something on a person's
    // machine, so it is something they say yes to -- and so is asking what
    // they have installed.
    "system.app.list",
    "system.app.running",
    "system.app.launch",
    "system.app.quit",
    // Clipboard contents may contain private material, and writing changes system
    // state, so both operations require the same one-time user confirmation.
    "system.clipboard.read",
    "system.clipboard.write",
    // System output audio state is explicit machine state. Reads expose it
    // and writes alter it, so all C2 operations remain confirmable.
    "system.audio.volume.get",
    "system.audio.volume.set",
    "system.audio.mute.get",
    "system.audio.mute.set",
    // Notifications change visible system state, but unlike Power they may
    // be intentionally enabled for Trusted Automation.
    "system.notification.send",
    // D2 may read public permission state or hand the person to the correct
    // System Settings pane. It never grants or edits TCC authorization.
    "system.permission.list",
    "system.permission.open_settings",
    "system.power.sleep",
    "system.power.restart",
    "system.power.shutdown",
];

const APPROVAL_REQUIRED_MESSAGE: &str = "OpenClaw action requires explicit approval.";
/// These operations can suspend or terminate the person's current session.
/// Persistent Trusted Automation approval therefore never substitutes for
/// confirmation attached to the current execution request.
const ALWAYS_CONFIRM_CAPABILITIES: &[&str] = &[
    "system.process.terminate",
    "system.power.sleep",
    "system.power.restart",
    "system.power.shutdown",
];

const PERMISSION_DENIED_MESSAGE: &str = "OpenClaw action is not permitted.";
const PERMISSION_UNDETERMINED_MESSAGE: &str = "OpenClaw permission could not be determined.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenClawPermissionDecision {
    Allowed,
    RequiresApproval,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenClawPermissionCheckError {
    Unavailable,
}

pub(crate) trait OpenClawPermissionGate: Send + Sync {
    fn authorize(
        &self,
        request: &OpenClawExecutionRequest,
    ) -> Result<OpenClawPermissionDecision, OpenClawPermissionCheckError>;
}

pub(crate) struct ConfiguredCapabilityPermissionGate {
    allowed_capabilities: HashSet<String>,
}

impl ConfiguredCapabilityPermissionGate {
    pub(crate) fn new(allowed_capabilities: impl IntoIterator<Item = String>) -> Self {
        Self {
            allowed_capabilities: allowed_capabilities
                .into_iter()
                .map(|capability| capability.trim().to_owned())
                .filter(|capability| !capability.is_empty())
                .collect(),
        }
    }
}

impl OpenClawPermissionGate for ConfiguredCapabilityPermissionGate {
    fn authorize(
        &self,
        request: &OpenClawExecutionRequest,
    ) -> Result<OpenClawPermissionDecision, OpenClawPermissionCheckError> {
        let action = request.action.as_str();

        if ALWAYS_CONFIRM_CAPABILITIES.contains(&action) {
            return Ok(if request.user_confirmed {
                OpenClawPermissionDecision::Allowed
            } else {
                OpenClawPermissionDecision::RequiresApproval
            });
        }

        Ok(
            if self.allowed_capabilities.contains(action)
                || (CONFIRMABLE_CAPABILITIES.contains(&action)
                    && request.user_confirmed)
            {
                OpenClawPermissionDecision::Allowed
            } else {
                OpenClawPermissionDecision::Denied
            },
        )
    }
}

pub(crate) struct PermissionEnforcingOpenClawExecutionAdapter {
    gate: Arc<dyn OpenClawPermissionGate>,
    inner: Arc<dyn OpenClawExecutionAdapter>,
}

impl PermissionEnforcingOpenClawExecutionAdapter {
    pub(crate) fn new(
        gate: Arc<dyn OpenClawPermissionGate>,
        inner: Arc<dyn OpenClawExecutionAdapter>,
    ) -> Self {
        Self { gate, inner }
    }
}

impl OpenClawExecutionAdapter for PermissionEnforcingOpenClawExecutionAdapter {
    fn execute(
        &self,
        request: &OpenClawExecutionRequest,
        report: &mut dyn FnMut(OpenClawExecutionProgress),
    ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
        match self.gate.authorize(request) {
            Ok(OpenClawPermissionDecision::Allowed) => self.inner.execute(request, report),
            Ok(OpenClawPermissionDecision::RequiresApproval) => Err(permission_error(
                OpenClawExecutionErrorKind::PermissionRequired,
                APPROVAL_REQUIRED_MESSAGE,
            )),
            Ok(OpenClawPermissionDecision::Denied) => Err(permission_error(
                OpenClawExecutionErrorKind::PermissionDenied,
                PERMISSION_DENIED_MESSAGE,
            )),
            Err(OpenClawPermissionCheckError::Unavailable) => Err(permission_error(
                OpenClawExecutionErrorKind::PermissionDenied,
                PERMISSION_UNDETERMINED_MESSAGE,
            )),
        }
    }
}

fn permission_error(
    kind: OpenClawExecutionErrorKind,
    message: &'static str,
) -> OpenClawExecutionError {
    OpenClawExecutionError::new(kind, message, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    /// A capability the resolver can route must be one a person can allow.
    ///
    /// Everything else about a capability can be right -- an adapter that
    /// works, a route that reaches it, a gate step that proves it -- and it
    /// can still be dead, because the permission gate denies whatever is not
    /// named here. Twelve were, and none of the existing tests could see it:
    /// they exercise the adapters and the routing, and the answer to "may
    /// this run at all" is asked somewhere else entirely.
    ///
    /// The check is one-directional on purpose. This list also carries
    /// capabilities the Office resolver knows nothing about -- filesystem and
    /// download work -- and requiring the two to match exactly would be a
    /// different, false claim.
    #[test]
    fn every_capability_the_resolver_can_route_is_one_a_person_can_authorise() {
        let mut declared: Vec<&'static str> = crate::document::resolver::office_candidates()
            .iter()
            .flat_map(|candidate| candidate.capabilities.iter().copied())
            .chain(crate::system::CAPABILITIES.iter().copied())
            .collect();

        declared.sort_unstable();
        declared.dedup();

        let unreachable: Vec<&str> = declared
            .into_iter()
            .filter(|capability| !CONFIRMABLE_CAPABILITIES.contains(capability))
            .collect();

        assert!(
            unreachable.is_empty(),
            "these capabilities can be routed and cannot be permitted, so nothing \
             can call them: {unreachable:?}. Add them to CONFIRMABLE_CAPABILITIES, \
             or stop declaring them."
        );
    }

    type PermissionOutcome = Result<OpenClawPermissionDecision, OpenClawPermissionCheckError>;

    struct RecordingGate {
        outcome: PermissionOutcome,
        requests: Mutex<Vec<OpenClawExecutionRequest>>,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    impl OpenClawPermissionGate for RecordingGate {
        fn authorize(
            &self,
            request: &OpenClawExecutionRequest,
        ) -> Result<OpenClawPermissionDecision, OpenClawPermissionCheckError> {
            self.order.lock().unwrap().push("permission");
            self.requests.lock().unwrap().push(request.clone());
            self.outcome
        }
    }

    struct RecordingAdapter {
        outcome: Result<OpenClawExecutionResult, OpenClawExecutionError>,
        progress: Vec<OpenClawExecutionProgress>,
        requests: Mutex<Vec<OpenClawExecutionRequest>>,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    impl OpenClawExecutionAdapter for RecordingAdapter {
        fn execute(
            &self,
            request: &OpenClawExecutionRequest,
            report: &mut dyn FnMut(OpenClawExecutionProgress),
        ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
            self.order.lock().unwrap().push("execution");
            self.requests.lock().unwrap().push(request.clone());
            for update in &self.progress {
                report(update.clone());
            }
            self.outcome.clone()
        }
    }

    fn request(input: serde_json::Value) -> OpenClawExecutionRequest {
        OpenClawExecutionRequest::new("execution-123", "filesystem.scan", input).unwrap()
    }

    fn progress(phase: &str, completed_units: u32) -> OpenClawExecutionProgress {
        OpenClawExecutionProgress {
            phase: phase.to_owned(),
            completed_units: Some(completed_units),
            total_units: Some(2),
            message: format!("{phase} update"),
        }
    }

    fn harness(
        permission: PermissionOutcome,
        outcome: Result<OpenClawExecutionResult, OpenClawExecutionError>,
        progress: Vec<OpenClawExecutionProgress>,
    ) -> (
        PermissionEnforcingOpenClawExecutionAdapter,
        Arc<RecordingGate>,
        Arc<RecordingAdapter>,
        Arc<Mutex<Vec<&'static str>>>,
    ) {
        let order = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(RecordingGate {
            outcome: permission,
            requests: Mutex::new(Vec::new()),
            order: Arc::clone(&order),
        });
        let inner = Arc::new(RecordingAdapter {
            outcome,
            progress,
            requests: Mutex::new(Vec::new()),
            order: Arc::clone(&order),
        });
        let adapter = PermissionEnforcingOpenClawExecutionAdapter::new(gate.clone(), inner.clone());

        (adapter, gate, inner, order)
    }

    fn success() -> OpenClawExecutionResult {
        OpenClawExecutionResult {
            output: json!({"files": 2}),
            summary: Some("Scanned two files.".to_owned()),
        }
    }

    #[test]
    fn allowed_permission_invokes_wrapped_adapter() {
        let request = request(json!({"path": "/safe/example"}));
        let (adapter, gate, inner, _) = harness(
            Ok(OpenClawPermissionDecision::Allowed),
            Ok(success()),
            Vec::new(),
        );

        adapter.execute(&request, &mut |_| {}).unwrap();

        assert_eq!(*gate.requests.lock().unwrap(), vec![request.clone()]);
        assert_eq!(*inner.requests.lock().unwrap(), vec![request]);
    }

    #[test]
    fn allowed_permission_preserves_progress_and_result() {
        let expected_result = success();
        let expected_progress = vec![progress("started", 0), progress("completed", 2)];
        let (adapter, _, _, _) = harness(
            Ok(OpenClawPermissionDecision::Allowed),
            Ok(expected_result.clone()),
            expected_progress.clone(),
        );
        let mut actual_progress = Vec::new();

        let actual_result = adapter
            .execute(&request(json!({})), &mut |update| {
                actual_progress.push(update)
            })
            .unwrap();

        assert_eq!(actual_progress, expected_progress);
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn allowed_permission_preserves_inner_error() {
        let expected = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            "Gateway is unavailable.",
            true,
        );
        let (adapter, _, _, _) = harness(
            Ok(OpenClawPermissionDecision::Allowed),
            Err(expected.clone()),
            Vec::new(),
        );

        let actual = adapter
            .execute(&request(json!({})), &mut |_| {})
            .unwrap_err();

        assert_eq!(actual, expected);
    }

    fn assert_rejected(
        permission: PermissionOutcome,
        expected_kind: OpenClawExecutionErrorKind,
        expected_message: &str,
    ) {
        let (adapter, _, inner, order) = harness(permission, Ok(success()), Vec::new());
        let mut progress = Vec::new();

        let error = adapter
            .execute(&request(json!({})), &mut |update| progress.push(update))
            .unwrap_err();

        assert_eq!(inner.requests.lock().unwrap().len(), 0);
        assert!(progress.is_empty());
        assert_eq!(error.kind, expected_kind);
        assert!(!error.retryable);
        assert!(error.message.ends_with(expected_message));
        assert_eq!(*order.lock().unwrap(), vec!["permission"]);
    }

    #[test]
    fn approval_required_prevents_execution() {
        assert_rejected(
            Ok(OpenClawPermissionDecision::RequiresApproval),
            OpenClawExecutionErrorKind::PermissionRequired,
            APPROVAL_REQUIRED_MESSAGE,
        );
    }

    #[test]
    fn denied_permission_prevents_execution() {
        assert_rejected(
            Ok(OpenClawPermissionDecision::Denied),
            OpenClawExecutionErrorKind::PermissionDenied,
            PERMISSION_DENIED_MESSAGE,
        );
    }

    #[test]
    fn permission_check_failure_fails_closed() {
        assert_rejected(
            Err(OpenClawPermissionCheckError::Unavailable),
            OpenClawExecutionErrorKind::PermissionDenied,
            PERMISSION_UNDETERMINED_MESSAGE,
        );
    }

    #[test]
    fn permission_gate_runs_before_wrapped_adapter() {
        let (adapter, _, _, order) = harness(
            Ok(OpenClawPermissionDecision::Allowed),
            Ok(success()),
            Vec::new(),
        );

        adapter.execute(&request(json!({})), &mut |_| {}).unwrap();

        assert_eq!(*order.lock().unwrap(), vec!["permission", "execution"]);
    }

    #[test]
    fn permission_rejection_does_not_include_request_credentials() {
        let credential = "credential-like-secret";
        let (adapter, _, _, _) = harness(
            Ok(OpenClawPermissionDecision::Denied),
            Ok(success()),
            Vec::new(),
        );

        let error = adapter
            .execute(&request(json!({"credential": credential})), &mut |_| {})
            .unwrap_err();

        assert!(!error.to_string().contains(credential));
        assert!(!format!("{error:?}").contains(credential));
    }

    struct ActionCheckingGate {
        expected_action: &'static str,
        received: Mutex<Vec<OpenClawExecutionRequest>>,
    }

    impl OpenClawPermissionGate for ActionCheckingGate {
        fn authorize(
            &self,
            request: &OpenClawExecutionRequest,
        ) -> Result<OpenClawPermissionDecision, OpenClawPermissionCheckError> {
            assert_eq!(request.action.as_str(), self.expected_action);
            self.received.lock().unwrap().push(request.clone());
            Ok(OpenClawPermissionDecision::Allowed)
        }
    }

    #[test]
    fn permission_gate_can_distinguish_action_without_mutating_request() {
        let original = request(json!({"path": "/safe/example"}));
        let gate = Arc::new(ActionCheckingGate {
            expected_action: "filesystem.scan",
            received: Mutex::new(Vec::new()),
        });
        let order = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(RecordingAdapter {
            outcome: Ok(success()),
            progress: Vec::new(),
            requests: Mutex::new(Vec::new()),
            order,
        });
        let adapter = PermissionEnforcingOpenClawExecutionAdapter::new(gate.clone(), inner.clone());

        adapter.execute(&original, &mut |_| {}).unwrap();

        assert_eq!(*gate.received.lock().unwrap(), vec![original.clone()]);
        assert_eq!(*inner.requests.lock().unwrap(), vec![original]);
    }

    #[test]
    fn all_non_allowed_outcomes_are_non_retryable() {
        let cases = [
            Ok(OpenClawPermissionDecision::RequiresApproval),
            Ok(OpenClawPermissionDecision::Denied),
            Err(OpenClawPermissionCheckError::Unavailable),
        ];

        for permission in cases {
            let (adapter, _, _, _) = harness(permission, Ok(success()), Vec::new());
            let error = adapter
                .execute(&request(json!({})), &mut |_| {})
                .unwrap_err();
            assert!(!error.retryable);
        }
    }

    #[test]
    fn power_capabilities_require_current_confirmation_even_when_trusted() {
        for action in [
            "system.power.sleep",
            "system.power.restart",
            "system.power.shutdown",
        ] {
            let gate =
                ConfiguredCapabilityPermissionGate::new([action.to_owned()]);

            let unconfirmed =
                OpenClawExecutionRequest::new(
                    "execution-power-unconfirmed",
                    action,
                    json!({}),
                )
                .unwrap();

            assert_eq!(
                gate.authorize(&unconfirmed).unwrap(),
                OpenClawPermissionDecision::RequiresApproval,
                "{action} must not inherit Trusted Automation approval"
            );

            assert_eq!(
                gate.authorize(
                    &unconfirmed.with_user_confirmation(true)
                )
                .unwrap(),
                OpenClawPermissionDecision::Allowed,
                "{action} should run after current user confirmation"
            );
        }
    }

    #[test]
    fn ordinary_trusted_automation_behavior_is_unchanged() {
        let gate =
            ConfiguredCapabilityPermissionGate::new([
                "filesystem.scan".to_owned()
            ]);

        assert_eq!(
            gate.authorize(&request(json!({}))).unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn configured_capability_is_allowed() {
        let gate = ConfiguredCapabilityPermissionGate::new(["filesystem.scan".to_owned()]);

        assert_eq!(
            gate.authorize(&request(json!({}))).unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn unconfigured_or_empty_allowlist_denies() {
        let empty = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let configured = ConfiguredCapabilityPermissionGate::new(["filesystem.read".to_owned()]);

        assert_eq!(
            empty.authorize(&request(json!({}))).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            configured.authorize(&request(json!({}))).unwrap(),
            OpenClawPermissionDecision::Denied
        );
    }

    #[test]
    fn one_time_user_confirmation_allows_scan_while_unconfirmed_move_is_denied() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let confirmed_scan = request(json!({"path": "/safe/example"})).with_user_confirmation(true);
        let unconfirmed_move = OpenClawExecutionRequest::new(
            "execution-123",
            "filesystem.move",
            json!({"path": "/safe/example"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&confirmed_scan).unwrap(),
            OpenClawPermissionDecision::Allowed
        );
        assert_eq!(
            gate.authorize(&unconfirmed_move).unwrap(),
            OpenClawPermissionDecision::Denied
        );
    }

    #[test]
    fn one_time_user_confirmation_allows_read_while_unconfirmed_move_is_denied() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let confirmed_read = OpenClawExecutionRequest::new(
            "execution-123",
            "filesystem.read",
            json!({"path": "/safe/example.txt"}),
        )
        .unwrap()
        .with_user_confirmation(true);
        let unconfirmed_move = OpenClawExecutionRequest::new(
            "execution-123",
            "filesystem.move",
            json!({"path": "/safe/example.txt"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&confirmed_read).unwrap(),
            OpenClawPermissionDecision::Allowed
        );
        assert_eq!(
            gate.authorize(&unconfirmed_move).unwrap(),
            OpenClawPermissionDecision::Denied
        );
    }

    #[test]
    fn document_read_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-document-read",
            "document.read",
            json!({"path": "/safe/example.docx"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn document_create_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-document-create",
            "document.create",
            json!({
                "path": "/safe/example.docx",
                "content": "Document body"
            }),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn document_convert_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-document-convert",
            "document.convert",
            json!({
                "source": "/safe/source.doc",
                "destination": "/safe/result.docx"
            }),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn document_edit_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-document-edit",
            "document.edit",
            json!({
                "source": "/safe/source.docx",
                "destination": "/safe/result.docx"
            }),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn spreadsheet_read_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-spreadsheet-read",
            "spreadsheet.read",
            json!({"path": "/safe/workbook.xlsx"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn spreadsheet_create_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-spreadsheet-create",
            "spreadsheet.create",
            json!({
                "path": "/safe/workbook.xlsx",
                "content": "Name\tValue\nAlpha\t42"
            }),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn spreadsheet_edit_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-spreadsheet-edit",
            "spreadsheet.edit",
            json!({
                "source": "/safe/workbook.xlsx",
                "destination": "/safe/result.xlsx",
                "operations": [{"type":"clear_cell","sheet":"Sheet1","row":2,"column":1}]
            }),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn presentation_read_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-presentation-read",
            "presentation.read",
            json!({"path": "/safe/presentation.pptx"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn presentation_create_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-presentation-create",
            "presentation.create",
            json!({
                "path": "/safe/presentation.key",
                "title": "Presentation title",
                "body": "Presentation body"
            }),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn presentation_edit_and_export_require_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        for action in ["presentation.edit", "presentation.export"] {
            let request = OpenClawExecutionRequest::new(
                format!("execution-{action}"),
                action,
                json!({"source":"/safe/input.pptx","destination":"/safe/output"}),
            )
            .unwrap();
            assert_eq!(
                gate.authorize(&request).unwrap(),
                OpenClawPermissionDecision::Denied
            );
            assert_eq!(
                gate.authorize(&request.with_user_confirmation(true))
                    .unwrap(),
                OpenClawPermissionDecision::Allowed
            );
        }
    }

    #[test]
    fn one_time_user_confirmation_allows_write_while_unconfirmed_move_is_denied() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let confirmed_write = OpenClawExecutionRequest::new(
            "execution-123",
            "filesystem.write",
            json!({"path": "/safe/new.txt", "content": "hello", "overwrite": false}),
        )
        .unwrap()
        .with_user_confirmation(true);
        let unconfirmed_move = OpenClawExecutionRequest::new(
            "execution-123",
            "filesystem.move",
            json!({"source": "/safe/a.txt", "destination": "/safe/b.txt"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&confirmed_write).unwrap(),
            OpenClawPermissionDecision::Allowed
        );
        assert_eq!(
            gate.authorize(&unconfirmed_move).unwrap(),
            OpenClawPermissionDecision::Denied
        );
    }

    #[test]
    fn one_time_user_confirmation_allows_filesystem_move() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let confirmed_move = OpenClawExecutionRequest::new(
            "execution-123",
            "filesystem.move",
            json!({"source": "/safe/a.txt", "destination": "/safe/b.txt"}),
        )
        .unwrap()
        .with_user_confirmation(true);

        assert_eq!(
            gate.authorize(&confirmed_move).unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn download_start_requires_and_accepts_one_time_user_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());
        let request = OpenClawExecutionRequest::new(
            "execution-123",
            "download.start",
            json!({"source": "https://example.com/file.zip", "destination": "/safe"}),
        )
        .unwrap();

        assert_eq!(
            gate.authorize(&request).unwrap(),
            OpenClawPermissionDecision::Denied
        );
        assert_eq!(
            gate.authorize(&request.with_user_confirmation(true))
                .unwrap(),
            OpenClawPermissionDecision::Allowed
        );
    }

    #[test]
    fn unconfirmed_filesystem_scan_remains_denied_without_trusted_automation() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());

        assert_eq!(
            gate.authorize(&request(json!({"path": "/safe/example"})))
                .unwrap(),
            OpenClawPermissionDecision::Denied
        );
    }

    #[test]
    fn configured_capability_matching_is_exact() {
        for configured in [
            "Filesystem.Scan",
            "filesystem",
            "filesystem.scan.extra",
            "*",
        ] {
            let gate = ConfiguredCapabilityPermissionGate::new([configured.to_owned()]);
            assert_eq!(
                gate.authorize(&request(json!({}))).unwrap(),
                OpenClawPermissionDecision::Denied
            );
        }
    }

    #[test]
    fn process_termination_always_requires_current_confirmation() {
        let input = json!({
            "pid": 1234,
            "expectedStartTimeUnixSeconds": 5678
        });

        let unconfirmed = OpenClawExecutionRequest::new(
            "execution-process-terminate-unconfirmed",
            "system.process.terminate",
            input.clone(),
        )
        .unwrap();

        let confirmed = OpenClawExecutionRequest::new(
            "execution-process-terminate-confirmed",
            "system.process.terminate",
            input,
        )
        .unwrap()
        .with_user_confirmation(true);

        let empty =
            ConfiguredCapabilityPermissionGate::new(Vec::new());

        let trusted = ConfiguredCapabilityPermissionGate::new([
            "system.process.terminate".to_owned()
        ]);

        for gate in [&empty, &trusted] {
            assert_eq!(
                gate.authorize(&unconfirmed).unwrap(),
                OpenClawPermissionDecision::RequiresApproval,
                "Trusted Automation must never substitute for current confirmation"
            );

            assert_eq!(
                gate.authorize(&confirmed).unwrap(),
                OpenClawPermissionDecision::Allowed
            );
        }
    }

    #[test]
    fn permission_decision_does_not_depend_on_input_payload() {
        let gate = ConfiguredCapabilityPermissionGate::new(["filesystem.scan".to_owned()]);

        for input in [
            json!({}),
            json!({"credential": "secret"}),
            json!({"attemptedOverride": "*"}),
        ] {
            assert_eq!(
                gate.authorize(&request(input)).unwrap(),
                OpenClawPermissionDecision::Allowed
            );
        }
    }
}

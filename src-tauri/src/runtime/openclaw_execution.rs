use serde_json::Value;
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OpenClawExecutionId(String);

impl OpenClawExecutionId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, OpenClawExecutionError> {
        let value = value.into().trim().to_owned();

        if value.is_empty() {
            return Err(OpenClawExecutionError::invalid_request(
                "execution identifier must not be empty",
            ));
        }

        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpenClawExecutionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OpenClawActionId(String);

impl OpenClawActionId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, OpenClawExecutionError> {
        let value = value.into().trim().to_owned();

        if value.is_empty() {
            return Err(OpenClawExecutionError::invalid_request(
                "action identifier must not be empty",
            ));
        }

        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpenClawActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenClawExecutionRequest {
    pub execution_id: OpenClawExecutionId,
    pub action: OpenClawActionId,
    pub input: Value,
}

impl OpenClawExecutionRequest {
    pub(crate) fn new(
        execution_id: impl Into<String>,
        action: impl Into<String>,
        input: Value,
    ) -> Result<Self, OpenClawExecutionError> {
        let execution_id = OpenClawExecutionId::new(execution_id)?;
        let action = OpenClawActionId::new(action)?;

        Ok(Self {
            execution_id,
            action,
            input,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenClawExecutionProgress {
    pub phase: String,
    pub completed_units: Option<u32>,
    pub total_units: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenClawExecutionResult {
    pub output: Value,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenClawExecutionErrorKind {
    InvalidRequest,
    PermissionRequired,
    PermissionDenied,
    AuthenticationRequired,
    PairingRequired,
    ConnectionUnavailable,
    ProtocolFailure,
    ExecutionRejected,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenClawExecutionError {
    pub kind: OpenClawExecutionErrorKind,
    pub message: String,
    pub retryable: bool,
}

impl OpenClawExecutionError {
    pub(crate) fn new(
        kind: OpenClawExecutionErrorKind,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable,
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(OpenClawExecutionErrorKind::InvalidRequest, message, false)
    }
}

impl fmt::Display for OpenClawExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl Error for OpenClawExecutionError {}

pub(crate) trait OpenClawExecutionAdapter: Send + Sync {
    fn execute(
        &self,
        request: &OpenClawExecutionRequest,
        report: &mut dyn FnMut(OpenClawExecutionProgress),
    ) -> Result<OpenClawExecutionResult, OpenClawExecutionError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    struct RecordingAdapter {
        received: Mutex<Vec<OpenClawExecutionRequest>>,
        outcome: Result<OpenClawExecutionResult, OpenClawExecutionError>,
        progress: Vec<OpenClawExecutionProgress>,
    }

    impl OpenClawExecutionAdapter for RecordingAdapter {
        fn execute(
            &self,
            request: &OpenClawExecutionRequest,
            report: &mut dyn FnMut(OpenClawExecutionProgress),
        ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
            self.received.lock().unwrap().push(request.clone());

            for progress in &self.progress {
                report(progress.clone());
            }

            self.outcome.clone()
        }
    }

    fn request() -> OpenClawExecutionRequest {
        OpenClawExecutionRequest::new(
            "execution-001",
            "filesystem.scan",
            json!({"path": "/safe/example"}),
        )
        .unwrap()
    }

    #[test]
    fn valid_request_preserves_identifier_action_and_input() {
        let request = request();

        assert_eq!(request.execution_id.as_str(), "execution-001");
        assert_eq!(request.action.as_str(), "filesystem.scan");
        assert_eq!(request.input, json!({"path": "/safe/example"}));
    }

    #[test]
    fn rejects_empty_execution_identifier() {
        let error = OpenClawExecutionRequest::new("", "test.execute", json!({})).unwrap_err();

        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(!error.retryable);
    }

    #[test]
    fn rejects_blank_action_identifier() {
        let error = OpenClawExecutionRequest::new("execution-001", "   ", json!({})).unwrap_err();

        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(!error.retryable);
    }

    #[test]
    fn action_identifier_rejects_whitespace_only_value() {
        let error = OpenClawActionId::new(" \t\n ").unwrap_err();

        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(!error.retryable);
    }

    #[test]
    fn successful_adapter_execution_reports_ordered_progress_and_result() {
        let request = request();
        let result = OpenClawExecutionResult {
            output: json!({"files": 3}),
            summary: Some("Scanned three files.".to_owned()),
        };
        let adapter = RecordingAdapter {
            received: Mutex::new(Vec::new()),
            outcome: Ok(result.clone()),
            progress: vec![
                OpenClawExecutionProgress {
                    phase: "discovering".to_owned(),
                    completed_units: Some(1),
                    total_units: Some(3),
                    message: "Discovered first file.".to_owned(),
                },
                OpenClawExecutionProgress {
                    phase: "completed".to_owned(),
                    completed_units: Some(3),
                    total_units: Some(3),
                    message: "Scan complete.".to_owned(),
                },
            ],
        };
        let mut progress = Vec::new();

        let actual = adapter
            .execute(&request, &mut |update| progress.push(update))
            .unwrap();

        assert_eq!(*adapter.received.lock().unwrap(), vec![request]);
        assert_eq!(progress, adapter.progress);
        assert_eq!(actual, result);
        assert_eq!(actual.output, json!({"files": 3}));
    }

    #[test]
    fn failed_adapter_execution_preserves_typed_error() {
        let expected = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "Agent execution failed.",
            true,
        );
        let adapter = RecordingAdapter {
            received: Mutex::new(Vec::new()),
            outcome: Err(expected.clone()),
            progress: Vec::new(),
        };

        let error = adapter
            .execute(&request(), &mut |_| {})
            .expect_err("execution must fail");

        assert_eq!(error, expected);
        assert_eq!(error.kind, OpenClawExecutionErrorKind::ExecutionFailed);
        assert!(error.retryable);
    }

    #[test]
    fn representative_error_categories_are_distinguishable() {
        let categories = [
            OpenClawExecutionErrorKind::PermissionRequired,
            OpenClawExecutionErrorKind::PermissionDenied,
            OpenClawExecutionErrorKind::AuthenticationRequired,
            OpenClawExecutionErrorKind::PairingRequired,
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            OpenClawExecutionErrorKind::ProtocolFailure,
            OpenClawExecutionErrorKind::ExecutionRejected,
            OpenClawExecutionErrorKind::ExecutionFailed,
        ];

        for (index, left) in categories.iter().enumerate() {
            for right in categories.iter().skip(index + 1) {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn display_and_debug_do_not_include_unrelated_request_credentials() {
        let credential = "gateway-token-secret";
        let request = OpenClawExecutionRequest::new(
            "execution-001",
            "test.execute",
            json!({"credential": credential}),
        )
        .unwrap();
        let error = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::AuthenticationRequired,
            "OpenClaw authentication is required.",
            false,
        );

        let display = error.to_string();

        assert!(display.contains("AuthenticationRequired"));
        assert!(!display.contains(credential));
        assert!(!format!("{error:?}").contains(credential));
        assert_eq!(request.input["credential"], credential);
    }

    #[test]
    fn adapter_contract_requires_no_runtime_or_gateway_infrastructure() {
        fn execute_with(
            adapter: &dyn OpenClawExecutionAdapter,
            request: &OpenClawExecutionRequest,
        ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
            adapter.execute(request, &mut |_| {})
        }

        let expected = OpenClawExecutionResult {
            output: json!({"ok": true}),
            summary: None,
        };
        let adapter = RecordingAdapter {
            received: Mutex::new(Vec::new()),
            outcome: Ok(expected.clone()),
            progress: Vec::new(),
        };

        assert_eq!(execute_with(&adapter, &request()).unwrap(), expected);
    }
}

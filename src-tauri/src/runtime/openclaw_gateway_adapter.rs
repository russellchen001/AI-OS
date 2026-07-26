use super::openclaw_execution::{
    OpenClawExecutionAdapter, OpenClawExecutionError, OpenClawExecutionErrorKind,
    OpenClawExecutionProgress, OpenClawExecutionRequest, OpenClawExecutionResult,
};
use crate::openclaw::{
    invoke_active_gateway_method, ActiveGatewayFailureKind, ActiveGatewayMethodFailure,
};
use serde_json::Value;

pub(crate) struct OpenClawGatewayExecutionAdapter;

impl OpenClawExecutionAdapter for OpenClawGatewayExecutionAdapter {
    fn execute(
        &self,
        request: &OpenClawExecutionRequest,
        report: &mut dyn FnMut(OpenClawExecutionProgress),
    ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
        execute_with_invoker(&ProductionGatewayMethodInvoker, request, report)
    }
}

trait GatewayMethodInvoker: Send + Sync {
    fn invoke(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ActiveGatewayMethodFailure>;
}

struct ProductionGatewayMethodInvoker;

impl GatewayMethodInvoker for ProductionGatewayMethodInvoker {
    fn invoke(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ActiveGatewayMethodFailure> {
        invoke_active_gateway_method(method, params).map(|result| result.payload)
    }
}

fn execute_with_invoker(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
    _report: &mut dyn FnMut(OpenClawExecutionProgress),
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    invoker
        .invoke(request.action.as_str(), Some(request.input.clone()))
        .map(|payload| OpenClawExecutionResult {
            output: payload,
            summary: None,
        })
        .map_err(map_gateway_failure)
}

fn map_gateway_failure(failure: ActiveGatewayMethodFailure) -> OpenClawExecutionError {
    let (kind, retryable) = match failure.kind {
        ActiveGatewayFailureKind::Unauthorized => {
            (OpenClawExecutionErrorKind::AuthenticationRequired, false)
        }
        ActiveGatewayFailureKind::PairingRequired => {
            (OpenClawExecutionErrorKind::PairingRequired, false)
        }
        ActiveGatewayFailureKind::Unreachable => {
            (OpenClawExecutionErrorKind::ConnectionUnavailable, true)
        }
        ActiveGatewayFailureKind::Protocol => (OpenClawExecutionErrorKind::ProtocolFailure, false),
        ActiveGatewayFailureKind::NoActiveServer => {
            (OpenClawExecutionErrorKind::ConnectionUnavailable, false)
        }
        ActiveGatewayFailureKind::Unknown => (OpenClawExecutionErrorKind::ExecutionFailed, false),
    };

    OpenClawExecutionError::new(kind, failure.message, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::openclaw_execution::OpenClawExecutionRequest;
    use serde_json::json;
    use std::sync::Mutex;

    struct RecordingInvoker {
        calls: Mutex<Vec<(String, Option<Value>)>>,
        outcome: Result<Value, ActiveGatewayMethodFailure>,
    }

    impl GatewayMethodInvoker for RecordingInvoker {
        fn invoke(
            &self,
            method: &str,
            params: Option<Value>,
        ) -> Result<Value, ActiveGatewayMethodFailure> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            self.outcome.clone()
        }
    }

    fn request(input: Value) -> OpenClawExecutionRequest {
        OpenClawExecutionRequest::new("runtime-execution-123", "filesystem.scan", input).unwrap()
    }

    #[test]
    fn maps_action_and_input_without_transmitting_execution_id() {
        let input = json!({"path": "/safe/example"});
        let request = request(input.clone());
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({"files": 3})),
        };

        execute_with_invoker(&invoker, &request, &mut |_| {}).unwrap();

        assert_eq!(
            *invoker.calls.lock().unwrap(),
            vec![("filesystem.scan".to_owned(), Some(input.clone()))]
        );
        assert_eq!(request.input, input);
        assert!(
            !format!("{:?}", invoker.calls.lock().unwrap()).contains(request.execution_id.as_str())
        );
    }

    #[test]
    fn maps_successful_payload_to_execution_result() {
        let payload = json!({"files": ["a", "b"]});
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(payload.clone()),
        };

        let result = execute_with_invoker(&invoker, &request(json!({})), &mut |_| {}).unwrap();

        assert_eq!(result.output, payload);
        assert_eq!(result.summary, None);
    }

    #[test]
    fn production_adapter_emits_no_progress() {
        struct NoNetworkInvoker;

        impl GatewayMethodInvoker for NoNetworkInvoker {
            fn invoke(
                &self,
                _method: &str,
                _params: Option<Value>,
            ) -> Result<Value, ActiveGatewayMethodFailure> {
                Ok(json!({"ok": true}))
            }
        }

        let request = request(json!({}));
        let mut progress = Vec::new();
        let result = execute_with_invoker(&NoNetworkInvoker, &request, &mut |update| {
            progress.push(update)
        });

        assert!(result.is_ok());
        assert!(progress.is_empty());
    }

    fn assert_failure_mapping(
        gateway_kind: ActiveGatewayFailureKind,
        expected_kind: OpenClawExecutionErrorKind,
        expected_retryable: bool,
    ) {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Err(ActiveGatewayMethodFailure {
                kind: gateway_kind,
                message: "Safe Gateway failure.".to_owned(),
            }),
        };

        let error = execute_with_invoker(&invoker, &request(json!({})), &mut |_| {}).unwrap_err();

        assert_eq!(error.kind, expected_kind);
        assert_eq!(error.retryable, expected_retryable);
        assert_eq!(error.message, "Safe Gateway failure.");
    }

    #[test]
    fn maps_unauthorized_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Unauthorized,
            OpenClawExecutionErrorKind::AuthenticationRequired,
            false,
        );
    }

    #[test]
    fn maps_pairing_required_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::PairingRequired,
            OpenClawExecutionErrorKind::PairingRequired,
            false,
        );
    }

    #[test]
    fn maps_unreachable_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Unreachable,
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            true,
        );
    }

    #[test]
    fn maps_protocol_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Protocol,
            OpenClawExecutionErrorKind::ProtocolFailure,
            false,
        );
    }

    #[test]
    fn maps_no_active_server_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::NoActiveServer,
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            false,
        );
    }

    #[test]
    fn maps_unknown_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Unknown,
            OpenClawExecutionErrorKind::ExecutionFailed,
            false,
        );
    }

    #[test]
    fn runtime_error_does_not_include_unrelated_request_credential() {
        let credential = "request-credential-secret";
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Err(ActiveGatewayMethodFailure {
                kind: ActiveGatewayFailureKind::Protocol,
                message: "Gateway protocol failed.".to_owned(),
            }),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(json!({"credential": credential})),
            &mut |_| {},
        )
        .unwrap_err();

        assert!(!error.to_string().contains(credential));
        assert!(!format!("{error:?}").contains(credential));
    }
}

use super::{
    agent_execution::{
        AgentExecutionError, AgentExecutionErrorKind, AgentExecutionProgress,
        AgentExecutionRequest, AgentExecutionResult,
    },
    capability_permission::{
        CapabilityPermissionDecision, ConfiguredCapabilityPermissionGate,
        COMPUTER_USE_ALWAYS_CONFIRM_CAPABILITIES, GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
    },
    executor::{
        execute_generative_media_runtime_task, execute_local_model_runtime_task,
        execute_mcp_runtime_task, execute_runtime_task, OperationEventEmitter,
        RuntimeExecutionState, RuntimeTaskExecutionRequest,
    },
    models::{NormalizedRuntimeError, RuntimeErrorCode},
    openclaw_execution::OpenClawExecutionAdapter,
    openclaw_gateway_adapter::OpenClawGatewayExecutionAdapter,
    skill_invocation::{
        SkillBackend, SkillInvocationContext, SkillInvocationError, SkillInvocationErrorKind,
        SkillInvocationGateway, SkillInvocationRequest, SkillInvocationResult,
    },
    skills,
    trusted_automation::load_trusted_automation_settings,
};
use crate::computer_use::ComputerUseSkillBackend;
use crate::openclaw::{
    invoke_active_gateway_method, ActiveGatewayFailureKind, ActiveGatewayMethodFailure,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

const OPENCLAW_EXECUTION_AGENT_ID: &str = "ai-os-files";
const AGENT_WAIT_ATTEMPTS: usize = 35;

/// Agent-facing Skill transport. It translates one Agent protocol into the
/// generic SkillInvocationGateway contract; it never owns permission policy or
/// invokes a concrete backend directly.
pub(crate) trait AgentSkillTransportAdapter: Send + Sync {
    fn execute(
        &self,
        request: &AgentExecutionRequest,
        context: &SkillInvocationContext,
        gateway: &dyn SkillInvocationGateway,
        report: &mut dyn FnMut(AgentExecutionProgress),
    ) -> Result<AgentExecutionResult, AgentExecutionError>;
}

/// Runtime-owned gateway. Agent-authored input is admitted only after both the
/// exposure boundary and the Runtime permission decision succeed.
pub(crate) struct RuntimeSkillInvocationGateway {
    permission_gate: ConfiguredCapabilityPermissionGate,
    backend: Arc<dyn SkillBackend>,
}

impl RuntimeSkillInvocationGateway {
    pub(crate) fn production(
        runtime: RuntimeExecutionState,
        emitter: Arc<dyn OperationEventEmitter>,
    ) -> Self {
        let allowed = load_trusted_automation_settings()
            .map(|settings| settings.allowed_capabilities())
            .unwrap_or_default();

        Self {
            permission_gate: ConfiguredCapabilityPermissionGate::new(allowed),
            backend: Arc::new(RuntimeSkillBackend::new(runtime, emitter)),
        }
    }

    #[cfg(test)]
    fn with_backend(
        allowed_capabilities: impl IntoIterator<Item = String>,
        backend: Arc<dyn SkillBackend>,
    ) -> Self {
        Self {
            permission_gate: ConfiguredCapabilityPermissionGate::new(allowed_capabilities),
            backend,
        }
    }
}

impl SkillInvocationGateway for RuntimeSkillInvocationGateway {
    fn invoke(
        &self,
        context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
    ) -> Result<SkillInvocationResult, SkillInvocationError> {
        if !context
            .exposed_capabilities
            .iter()
            .any(|capability| capability == &request.capability)
        {
            return Err(SkillInvocationError::new(
                SkillInvocationErrorKind::CapabilityNotExposed,
                "Agent requested a Skill capability that Runtime did not expose.",
                false,
            ));
        }

        skills::resolver::resolve(&request.capability).ok_or_else(|| {
            SkillInvocationError::new(
                SkillInvocationErrorKind::SkillNotFound,
                "Agent requested an unregistered Skill capability.",
                false,
            )
        })?;

        let confirmable = [request.capability.as_str()];
        let always_confirm = GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES
            .contains(&request.capability.as_str())
            || COMPUTER_USE_ALWAYS_CONFIRM_CAPABILITIES.contains(&request.capability.as_str())
            || matches!(
                request.capability.as_str(),
                "system.process.terminate"
                    | "system.power.sleep"
                    | "system.power.restart"
                    | "system.power.shutdown"
            );
        let always_confirmable = always_confirm.then_some(request.capability.as_str());
        let always_confirmable = always_confirmable.as_slice();

        match self.permission_gate.authorize_with_policy(
            &request.capability,
            context.user_confirmed,
            &confirmable,
            always_confirmable,
        ) {
            CapabilityPermissionDecision::Allowed => self.backend.invoke(context, request),
            CapabilityPermissionDecision::RequiresApproval => Err(SkillInvocationError::new(
                SkillInvocationErrorKind::PermissionRequired,
                "Skill invocation requires current user confirmation.",
                false,
            )),
            CapabilityPermissionDecision::Denied => Err(SkillInvocationError::new(
                SkillInvocationErrorKind::PermissionDenied,
                "Runtime denied the Skill invocation.",
                false,
            )),
        }
    }
}

struct RuntimeSkillBackend {
    runtime: RuntimeExecutionState,
    emitter: Arc<dyn OperationEventEmitter>,
    openclaw: Arc<dyn OpenClawExecutionAdapter>,
    computer_use: Arc<ComputerUseSkillBackend>,
}

impl RuntimeSkillBackend {
    fn new(runtime: RuntimeExecutionState, emitter: Arc<dyn OperationEventEmitter>) -> Self {
        Self {
            runtime,
            emitter,
            openclaw: Arc::new(OpenClawGatewayExecutionAdapter),
            computer_use: Arc::new(ComputerUseSkillBackend::production()),
        }
    }
}

impl SkillBackend for RuntimeSkillBackend {
    fn invoke(
        &self,
        context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
    ) -> Result<SkillInvocationResult, SkillInvocationError> {
        let skill = skills::resolver::resolve(&request.capability).ok_or_else(|| {
            SkillInvocationError::new(
                SkillInvocationErrorKind::SkillNotFound,
                "Skill backend could not resolve the capability.",
                false,
            )
        })?;
        let runtime_request = RuntimeTaskExecutionRequest {
            operation_id: format!(
                "skill:{}:{}",
                context.agent_execution_id, request.invocation_id
            ),
            plan_id: context.plan_id.clone(),
            step_id: request.invocation_id.clone(),
            capability: request.capability.clone(),
            input: request.input.clone(),
            user_confirmed: context.user_confirmed,
        };

        let result = match skill.executor.kind.as_str() {
            "computer-use" if skill.executor.handler == "computer-use" => {
                return self.computer_use.invoke(context, request);
            }
            "openclaw" => execute_runtime_task(
                self.runtime.manager(),
                self.runtime.scheduler(),
                Arc::clone(&self.emitter),
                runtime_request,
                Arc::clone(&self.openclaw),
            ),
            "mcp" => execute_mcp_runtime_task(
                self.runtime.manager(),
                self.runtime.scheduler(),
                Arc::clone(&self.emitter),
                runtime_request,
            ),
            "local" if skill.executor.handler == "ollama" => execute_local_model_runtime_task(
                self.runtime.manager(),
                self.runtime.scheduler(),
                Arc::clone(&self.emitter),
                runtime_request,
            ),
            "media" if skill.executor.handler == "generative-media" => {
                execute_generative_media_runtime_task(
                    self.runtime.manager(),
                    self.runtime.scheduler(),
                    Arc::clone(&self.emitter),
                    runtime_request,
                    Arc::new(
                        crate::generative_media::registry::MediaProviderRegistry::production(),
                    ),
                )
            }
            _ => {
                return Err(SkillInvocationError::new(
                    SkillInvocationErrorKind::BackendUnavailable,
                    "No backend is registered for the Skill capability.",
                    false,
                ));
            }
        }
        .map_err(map_runtime_error)?;

        Ok(SkillInvocationResult {
            invocation_id: request.invocation_id.clone(),
            capability: request.capability.clone(),
            backend: skill.executor.kind,
            provider: Some(skill.executor.handler),
            output: result.output.unwrap_or(Value::Null),
        })
    }
}

fn map_runtime_error(error: NormalizedRuntimeError) -> SkillInvocationError {
    let kind = match error.code {
        RuntimeErrorCode::InvalidRequest => SkillInvocationErrorKind::InvalidRequest,
        RuntimeErrorCode::PermissionDenied => SkillInvocationErrorKind::PermissionDenied,
        RuntimeErrorCode::RuntimeNotFound => SkillInvocationErrorKind::BackendUnavailable,
        _ => SkillInvocationErrorKind::ExecutionFailed,
    };
    SkillInvocationError::new(kind, error.message, error.retryable)
}

trait OpenClawAgentMethodInvoker: Send + Sync {
    fn invoke(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ActiveGatewayMethodFailure>;
}

struct ProductionOpenClawAgentMethodInvoker;

impl OpenClawAgentMethodInvoker for ProductionOpenClawAgentMethodInvoker {
    fn invoke(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ActiveGatewayMethodFailure> {
        invoke_active_gateway_method(method, params).map(|result| result.payload)
    }
}

pub(crate) struct OpenClawAgentSkillTransport {
    invoker: Arc<dyn OpenClawAgentMethodInvoker>,
}

impl OpenClawAgentSkillTransport {
    pub(crate) fn production() -> Self {
        Self {
            invoker: Arc::new(ProductionOpenClawAgentMethodInvoker),
        }
    }

    #[cfg(test)]
    fn with_invoker(invoker: Arc<dyn OpenClawAgentMethodInvoker>) -> Self {
        Self { invoker }
    }
}

impl AgentSkillTransportAdapter for OpenClawAgentSkillTransport {
    fn execute(
        &self,
        request: &AgentExecutionRequest,
        context: &SkillInvocationContext,
        gateway: &dyn SkillInvocationGateway,
        report: &mut dyn FnMut(AgentExecutionProgress),
    ) -> Result<AgentExecutionResult, AgentExecutionError> {
        let session_key = format!(
            "agent:{OPENCLAW_EXECUTION_AGENT_ID}:ai-os-skill-{}",
            request.execution_id
        );
        let exposure = json!({
            "goal": request.goal,
            "skills": request.allowed_capabilities,
            "requestedSkill": request.context.get("requestedSkill"),
            "protocol": {
                "type": "skill.invoke",
                "requiredFields": ["invocationId", "capability", "input"]
            }
        });
        let first_message = format!(
            "You are the selected execution Agent. AI-OS Runtime exposes only the capabilities in this JSON: {}. Evaluate every exposed capability that could match the task before declaring no viable path. To use a capability, do not call a similarly named OpenClaw tool and do not perform it directly. Return only one compact JSON object. If any exposed capability can advance the task, return {{\"type\":\"skill.invoke\",\"invocationId\":\"agent-authored-id\",\"capability\":\"one exposed capability\",\"input\":{{}}}}. Only if every exposed capability has been evaluated and none can advance the task may you return {{\"type\":\"execution.unavailable\",\"reason\":\"brief reason\",\"evaluatedCapabilities\":[\"every exposed capability exactly once\"]}}. Do not report execution.unavailable for permission denial, authentication, pairing, connection failures, provider unavailability, temporary errors, protocol errors, ordinary execution failures, or because one capability is inapplicable. Do not include confirmation, permission, backend, provider, or authority fields.",
            exposure
        );

        report(AgentExecutionProgress {
            phase: "skill-request".to_owned(),
            message: "Waiting for the Agent to request an exposed Skill.".to_owned(),
        });
        run_agent(
            self.invoker.as_ref(),
            &session_key,
            &first_message,
            &format!("{}:request", request.execution_id),
        )?;
        let mut decision_history = history(self.invoker.as_ref(), &session_key)?;
        let mut attempted_capabilities = Vec::new();
        let mut prior_failure: Option<AgentExecutionError> = None;

        let result = loop {
            let agent_request = match parse_skill_decision(&decision_history)? {
                AgentSkillDecision::Unavailable(value) => {
                    let no_viable =
                        validated_no_viable_execution_path(&value, &request.allowed_capabilities)?;
                    return Err(prior_failure.unwrap_or(no_viable));
                }
                AgentSkillDecision::Invoke(agent_request) => agent_request,
            };

            if attempted_capabilities.contains(&agent_request.capability) {
                return Err(invalid_skill_protocol());
            }
            attempted_capabilities.push(agent_request.capability.clone());

            report(AgentExecutionProgress {
                phase: "skill-invocation".to_owned(),
                message: "Runtime admitted the Agent Skill request.".to_owned(),
            });
            match gateway.invoke(context, &agent_request) {
                Ok(result) => break result,
                Err(error) => {
                    let failure = map_skill_error(error);
                    let remaining = request
                        .allowed_capabilities
                        .iter()
                        .filter(|capability| !attempted_capabilities.contains(capability))
                        .cloned()
                        .collect::<Vec<_>>();
                    if remaining.is_empty()
                        || !matches!(
                            failure.kind,
                            AgentExecutionErrorKind::ConnectionUnavailable
                                | AgentExecutionErrorKind::ExecutionFailed
                        )
                    {
                        return Err(failure);
                    }
                    if prior_failure.is_none() {
                        prior_failure = Some(failure.clone());
                    }
                    let failure_message = format!(
                        "The attempted AI-OS Skill failed. This failure is not evidence that no viable execution path exists. Evaluate the remaining exposed capabilities and return only one compact JSON object. Request one remaining capability with skill.invoke if it can advance the task. Do not retry an attempted capability. Do not return execution.unavailable because of this failure. Remaining capabilities: {}. Normalized failure: {}",
                        json!(remaining),
                        json!({
                            "type": "skill.failure",
                            "capability": agent_request.capability,
                            "kind": format!("{:?}", failure.kind),
                            "retryable": failure.retryable,
                        })
                    );
                    run_agent(
                        self.invoker.as_ref(),
                        &session_key,
                        &failure_message,
                        &format!(
                            "{}:failure:{}",
                            request.execution_id,
                            attempted_capabilities.len()
                        ),
                    )?;
                    decision_history = history(self.invoker.as_ref(), &session_key)?;
                }
            }
        };

        let result_message = format!(
            "AI-OS Runtime completed your Skill request successfully. Treat this JSON as the normalized Skill result. Return only one compact completion JSON object and do not call tools. A successful exposed Skill is a viable path, so execution.unavailable is not valid after this result: {}",
            json!({
                "type": "skill.result",
                "invocationId": result.invocation_id,
                "capability": result.capability,
                "output": result.output,
            })
        );
        run_agent(
            self.invoker.as_ref(),
            &session_key,
            &result_message,
            &format!("{}:result", request.execution_id),
        )?;
        let completion_history = history(self.invoker.as_ref(), &session_key)?;
        let completion = latest_assistant_text(&completion_history).ok_or_else(|| {
            AgentExecutionError::new(
                AgentExecutionErrorKind::ExecutionFailed,
                "OpenClaw completed without a final Agent response.",
                false,
            )
        })?;

        reject_no_viable_completion(&completion)?;

        Ok(AgentExecutionResult {
            session_id: Some(session_key),
            output: json!({
                "skillInvocation": {
                    "invocationId": result.invocation_id,
                    "capability": result.capability,
                    "backend": result.backend,
                    "provider": result.provider,
                    "output": result.output,
                },
                "agentCompletion": completion,
            }),
        })
    }
}

fn run_agent(
    invoker: &dyn OpenClawAgentMethodInvoker,
    session_key: &str,
    message: &str,
    idempotency_key: &str,
) -> Result<(), AgentExecutionError> {
    let accepted = invoker
        .invoke(
            "agent",
            Some(json!({
                "message": message,
                "agentId": OPENCLAW_EXECUTION_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": idempotency_key,
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AgentExecutionError::new(
                AgentExecutionErrorKind::ExecutionFailed,
                "OpenClaw did not return an Agent run identifier.",
                false,
            )
        })?;

    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;
        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => return Ok(()),
            Some("timeout") => continue,
            Some("error") => {
                return Err(AgentExecutionError::new(
                    AgentExecutionErrorKind::ExecutionFailed,
                    terminal
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("OpenClaw Agent execution failed."),
                    false,
                ));
            }
            _ => {
                return Err(AgentExecutionError::new(
                    AgentExecutionErrorKind::ExecutionFailed,
                    "OpenClaw returned an invalid Agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(AgentExecutionError::new(
        AgentExecutionErrorKind::ExecutionFailed,
        "OpenClaw Agent execution timed out.",
        true,
    ))
}

fn history(
    invoker: &dyn OpenClawAgentMethodInvoker,
    session_key: &str,
) -> Result<Value, AgentExecutionError> {
    invoker
        .invoke(
            "chat.history",
            Some(json!({"sessionKey": session_key, "limit": 10})),
        )
        .map_err(map_gateway_failure)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentSkillRequestWire {
    invocation_id: String,
    capability: String,
    input: Value,
}

#[derive(Debug)]
enum AgentSkillDecision {
    Invoke(SkillInvocationRequest),
    Unavailable(Value),
}

fn parse_skill_decision(history: &Value) -> Result<AgentSkillDecision, AgentExecutionError> {
    let text = latest_assistant_text(history).ok_or_else(|| {
        AgentExecutionError::new(
            AgentExecutionErrorKind::ExecutionFailed,
            "OpenClaw Agent did not return a Skill request.",
            false,
        )
    })?;

    let value = parse_agent_json(&text)?;

    match value.get("type").and_then(Value::as_str) {
        Some("execution.unavailable") => Ok(AgentSkillDecision::Unavailable(value)),
        Some("skill.invoke") => {
            let wire: AgentSkillRequestWire =
                serde_json::from_value(value).map_err(|_| invalid_skill_protocol())?;

            SkillInvocationRequest::new(wire.invocation_id, wire.capability, wire.input)
                .map(AgentSkillDecision::Invoke)
                .map_err(|_| invalid_skill_protocol())
        }
        _ => Err(invalid_skill_protocol()),
    }
}

fn parse_agent_json(text: &str) -> Result<Value, AgentExecutionError> {
    let start = text.find('{').ok_or_else(invalid_skill_protocol)?;
    let end = text.rfind('}').ok_or_else(invalid_skill_protocol)?;

    serde_json::from_str(&text[start..=end]).map_err(|_| invalid_skill_protocol())
}

fn no_viable_execution_path(value: &Value) -> AgentExecutionError {
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("No exposed AI-OS capability provides a viable execution path.");

    AgentExecutionError::new(
        AgentExecutionErrorKind::NoViableExecutionPath,
        reason,
        false,
    )
}

fn validated_no_viable_execution_path(
    value: &Value,
    exposed_capabilities: &[String],
) -> Result<AgentExecutionError, AgentExecutionError> {
    let evaluated = value
        .get("evaluatedCapabilities")
        .and_then(Value::as_array)
        .ok_or_else(invalid_skill_protocol)?;
    let mut normalized = Vec::with_capacity(evaluated.len());
    for capability in evaluated {
        let capability = capability
            .as_str()
            .map(str::trim)
            .filter(|capability| !capability.is_empty())
            .ok_or_else(invalid_skill_protocol)?;
        if normalized.iter().any(|seen| seen == capability)
            || !exposed_capabilities
                .iter()
                .any(|exposed| exposed == capability)
        {
            return Err(invalid_skill_protocol());
        }
        normalized.push(capability.to_owned());
    }
    if normalized.len() != exposed_capabilities.len()
        || exposed_capabilities
            .iter()
            .any(|capability| !normalized.contains(capability))
    {
        return Err(invalid_skill_protocol());
    }

    Ok(no_viable_execution_path(value))
}

fn reject_no_viable_completion(text: &str) -> Result<(), AgentExecutionError> {
    let Some(start) = text.find('{') else {
        return Ok(());
    };
    let Some(end) = text.rfind('}') else {
        return Ok(());
    };

    let Ok(value) = serde_json::from_str::<Value>(&text[start..=end]) else {
        return Ok(());
    };

    if value.get("type").and_then(Value::as_str) == Some("execution.unavailable") {
        return Err(AgentExecutionError::new(
            AgentExecutionErrorKind::ExecutionFailed,
            "OpenClaw reported no viable path after a Skill completed successfully.",
            false,
        ));
    }
    if value.get("type").and_then(Value::as_str) == Some("skill.invoke") {
        return Err(invalid_skill_protocol());
    }

    Ok(())
}

fn latest_assistant_text(history: &Value) -> Option<String> {
    history
        .get("messages")?
        .as_array()?
        .iter()
        .rev()
        .find_map(|entry| {
            let message = entry.get("message").unwrap_or(entry);
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return None;
            }
            match message.get("content")? {
                Value::String(text) => Some(text.clone()),
                Value::Array(items) => {
                    let text = items
                        .iter()
                        .filter_map(|item| item.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n");
                    (!text.trim().is_empty()).then_some(text)
                }
                _ => None,
            }
        })
}

fn invalid_skill_protocol() -> AgentExecutionError {
    AgentExecutionError::new(
        AgentExecutionErrorKind::ExecutionFailed,
        "OpenClaw Agent returned an invalid Skill transport request.",
        false,
    )
}

fn map_skill_error(error: SkillInvocationError) -> AgentExecutionError {
    let kind = match error.kind {
        SkillInvocationErrorKind::InvalidRequest => AgentExecutionErrorKind::InvalidRequest,
        SkillInvocationErrorKind::PermissionRequired
        | SkillInvocationErrorKind::PermissionDenied
        | SkillInvocationErrorKind::CapabilityNotExposed => {
            AgentExecutionErrorKind::PermissionDenied
        }
        SkillInvocationErrorKind::BackendUnavailable => {
            AgentExecutionErrorKind::ConnectionUnavailable
        }
        SkillInvocationErrorKind::SkillNotFound | SkillInvocationErrorKind::ExecutionFailed => {
            AgentExecutionErrorKind::ExecutionFailed
        }
    };
    AgentExecutionError::new(kind, error.message, error.retryable)
}

fn map_gateway_failure(failure: ActiveGatewayMethodFailure) -> AgentExecutionError {
    let (kind, retryable) = match failure.kind {
        ActiveGatewayFailureKind::Unauthorized => {
            (AgentExecutionErrorKind::AuthenticationRequired, false)
        }
        ActiveGatewayFailureKind::PairingRequired => {
            (AgentExecutionErrorKind::PairingRequired, false)
        }
        ActiveGatewayFailureKind::Unreachable => {
            (AgentExecutionErrorKind::ConnectionUnavailable, true)
        }
        ActiveGatewayFailureKind::NoActiveServer => {
            (AgentExecutionErrorKind::ConnectionUnavailable, false)
        }
        ActiveGatewayFailureKind::Protocol | ActiveGatewayFailureKind::Unknown => {
            (AgentExecutionErrorKind::ExecutionFailed, false)
        }
    };
    AgentExecutionError::new(kind, failure.message, retryable)
}

#[cfg(test)]
mod mano_fallback_contract_tests {
    use super::*;

    fn assistant_history(content: &str) -> Value {
        json!({
            "messages": [{
                "message": {
                    "role": "assistant",
                    "content": content
                }
            }]
        })
    }

    #[test]
    fn execution_unavailable_requires_every_exposed_capability_to_be_evaluated() {
        let history = assistant_history(
            r#"{"type":"execution.unavailable","reason":"No exposed capability can complete the task."}"#,
        );

        let AgentSkillDecision::Unavailable(value) = parse_skill_decision(&history).unwrap() else {
            panic!("expected unavailable decision");
        };
        let error = validated_no_viable_execution_path(&value, &["filesystem.scan".to_owned()])
            .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ExecutionFailed);
    }

    #[test]
    fn all_exposed_capabilities_evaluated_allows_no_viable_execution_path() {
        let history = assistant_history(
            r#"{"type":"execution.unavailable","reason":"No exposed capability can complete the task.","evaluatedCapabilities":["filesystem.scan","filesystem.read"]}"#,
        );
        let AgentSkillDecision::Unavailable(value) = parse_skill_decision(&history).unwrap() else {
            panic!("expected unavailable decision");
        };

        let error = validated_no_viable_execution_path(
            &value,
            &["filesystem.scan".to_owned(), "filesystem.read".to_owned()],
        )
        .unwrap();

        assert_eq!(error.kind, AgentExecutionErrorKind::NoViableExecutionPath);
        assert!(!error.retryable);
    }

    #[test]
    fn ordinary_invalid_protocol_does_not_become_mano_fallback_signal() {
        let history =
            assistant_history(r#"{"type":"unexpected.response","reason":"bad protocol"}"#);

        let error = parse_skill_decision(&history).unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ExecutionFailed);
    }

    #[test]
    fn successful_skill_completion_cannot_become_no_viable_path() {
        let error = reject_no_viable_completion(
            r#"{"type":"execution.unavailable","reason":"The attempted Skill cannot finish the task."}"#,
        )
        .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ExecutionFailed);
    }

    #[test]
    fn normal_completion_is_not_a_fallback_signal() {
        reject_no_viable_completion(r#"{"type":"execution.complete","summary":"done"}"#).unwrap();
    }

    #[test]
    fn legacy_plain_text_completion_remains_accepted() {
        reject_no_viable_completion("Task completed successfully.").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};

    struct ScriptedInvoker {
        calls: Mutex<Vec<(String, Option<Value>)>>,
        outcomes: Mutex<VecDeque<Result<Value, ActiveGatewayMethodFailure>>>,
    }

    impl OpenClawAgentMethodInvoker for ScriptedInvoker {
        fn invoke(
            &self,
            method: &str,
            params: Option<Value>,
        ) -> Result<Value, ActiveGatewayMethodFailure> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            self.outcomes.lock().unwrap().pop_front().unwrap()
        }
    }

    struct RecordingBackend {
        calls: Mutex<Vec<(SkillInvocationContext, SkillInvocationRequest)>>,
    }

    impl SkillBackend for RecordingBackend {
        fn invoke(
            &self,
            context: &SkillInvocationContext,
            request: &SkillInvocationRequest,
        ) -> Result<SkillInvocationResult, SkillInvocationError> {
            self.calls
                .lock()
                .unwrap()
                .push((context.clone(), request.clone()));
            Ok(SkillInvocationResult {
                invocation_id: request.invocation_id.clone(),
                capability: request.capability.clone(),
                backend: "existing-backend".to_owned(),
                provider: Some("test-provider".to_owned()),
                output: json!({"entries": ["fixture.txt"]}),
            })
        }
    }

    struct FirstCapabilityFailsBackend {
        calls: Mutex<Vec<String>>,
    }

    impl SkillBackend for FirstCapabilityFailsBackend {
        fn invoke(
            &self,
            _context: &SkillInvocationContext,
            request: &SkillInvocationRequest,
        ) -> Result<SkillInvocationResult, SkillInvocationError> {
            self.calls.lock().unwrap().push(request.capability.clone());
            if request.capability == "filesystem.scan" {
                return Err(SkillInvocationError::new(
                    SkillInvocationErrorKind::ExecutionFailed,
                    "the first candidate failed transiently",
                    true,
                ));
            }
            Ok(SkillInvocationResult {
                invocation_id: request.invocation_id.clone(),
                capability: request.capability.clone(),
                backend: "existing-backend".to_owned(),
                provider: Some("second-provider".to_owned()),
                output: json!({"content": "fixture"}),
            })
        }
    }

    fn execution_request() -> AgentExecutionRequest {
        AgentExecutionRequest::new(
            "execution-1",
            "task-1",
            "plan-1",
            "step-1",
            super::super::agent_execution::AgentId::new("openclaw").unwrap(),
            "Inspect the safe fixture folder",
            vec!["filesystem.scan".to_owned()],
            json!({"requestedSkill": {
                "capability": "filesystem.scan",
                "input": {"path": "/safe/fixture"}
            }}),
            context(true),
        )
        .unwrap()
    }

    fn context(confirmed: bool) -> SkillInvocationContext {
        SkillInvocationContext::new(
            "task-1",
            "plan-1",
            "execution-1",
            "openclaw",
            vec!["filesystem.scan".to_owned()],
            confirmed,
        )
        .unwrap()
    }

    fn multi_capability_request() -> AgentExecutionRequest {
        AgentExecutionRequest::new(
            "execution-1",
            "task-1",
            "plan-1",
            "step-1",
            super::super::agent_execution::AgentId::new("openclaw").unwrap(),
            "Inspect then read the safe fixture",
            vec!["filesystem.scan".to_owned(), "filesystem.read".to_owned()],
            json!({}),
            multi_capability_context(),
        )
        .unwrap()
    }

    fn multi_capability_context() -> SkillInvocationContext {
        SkillInvocationContext::new(
            "task-1",
            "plan-1",
            "execution-1",
            "openclaw",
            vec!["filesystem.scan".to_owned(), "filesystem.read".to_owned()],
            true,
        )
        .unwrap()
    }

    fn computer_use_context(confirmed: bool, exposed: Vec<String>) -> SkillInvocationContext {
        SkillInvocationContext::new(
            "task-1",
            "plan-1",
            "execution-1",
            "openclaw",
            exposed,
            confirmed,
        )
        .unwrap()
    }

    fn computer_use_input() -> Value {
        json!({
            "task": "Interact with the fixture application",
            "goal": "Complete the bounded visual fixture",
            "allowedApplications": ["Fixture App"],
            "maxSteps": 4,
            "maxDurationMs": 2000
        })
    }

    #[test]
    fn openclaw_transport_round_trips_agent_request_gateway_result_and_completion() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"skill.invoke\",\"invocationId\":\"scan-1\",\"capability\":\"filesystem.scan\",\"input\":{\"path\":\"/safe/fixture\"}}"
                }]})),
                Ok(json!({"runId": "result-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"complete\",\"summary\":\"fixture inspected\"}"
                }]})),
            ])),
        });
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());
        let transport = OpenClawAgentSkillTransport::with_invoker(invoker.clone());

        let result = transport
            .execute(&execution_request(), &context(true), &gateway, &mut |_| {})
            .unwrap();

        assert_eq!(
            result.output["skillInvocation"]["capability"],
            "filesystem.scan"
        );
        assert_eq!(backend.calls.lock().unwrap().len(), 1);
        let calls = invoker.calls.lock().unwrap();
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec![
                "agent",
                "agent.wait",
                "chat.history",
                "agent",
                "agent.wait",
                "chat.history"
            ]
        );
        let exposure = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(exposure.contains("filesystem.scan"));
        assert!(!exposure.contains("userConfirmed"));
        assert!(!exposure.contains("permissionDecision"));
    }

    #[test]
    fn available_capability_prevents_premature_no_viable_execution_path() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"execution.unavailable\",\"reason\":\"premature\"}"
                }]})),
            ])),
        });
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());

        let error = OpenClawAgentSkillTransport::with_invoker(invoker)
            .execute(&execution_request(), &context(true), &gateway, &mut |_| {})
            .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ExecutionFailed);
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn one_capability_failure_continues_to_another_available_capability() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"skill.invoke\",\"invocationId\":\"scan-1\",\"capability\":\"filesystem.scan\",\"input\":{\"path\":\"/safe\"}}"
                }]})),
                Ok(json!({"runId": "recovery-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"skill.invoke\",\"invocationId\":\"read-1\",\"capability\":\"filesystem.read\",\"input\":{\"path\":\"/safe/fixture.txt\"}}"
                }]})),
                Ok(json!({"runId": "result-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"complete\",\"summary\":\"fixture read\"}"
                }]})),
            ])),
        });
        let backend = Arc::new(FirstCapabilityFailsBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());

        let result = OpenClawAgentSkillTransport::with_invoker(invoker)
            .execute(
                &multi_capability_request(),
                &multi_capability_context(),
                &gateway,
                &mut |_| {},
            )
            .unwrap();

        assert_eq!(
            backend.calls.lock().unwrap().as_slice(),
            ["filesystem.scan", "filesystem.read"]
        );
        assert_eq!(
            result.output["skillInvocation"]["capability"],
            "filesystem.read"
        );
    }

    #[test]
    fn failed_capability_cannot_be_reclassified_as_no_viable_path() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"skill.invoke\",\"invocationId\":\"scan-1\",\"capability\":\"filesystem.scan\",\"input\":{\"path\":\"/safe\"}}"
                }]})),
                Ok(json!({"runId": "recovery-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"execution.unavailable\",\"reason\":\"the first candidate failed\",\"evaluatedCapabilities\":[\"filesystem.scan\",\"filesystem.read\"]}"
                }]})),
            ])),
        });
        let backend = Arc::new(FirstCapabilityFailsBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());

        let error = OpenClawAgentSkillTransport::with_invoker(invoker)
            .execute(
                &multi_capability_request(),
                &multi_capability_context(),
                &gateway,
                &mut |_| {},
            )
            .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ExecutionFailed);
        assert!(error.retryable);
        assert_eq!(
            backend.calls.lock().unwrap().as_slice(),
            ["filesystem.scan"]
        );
    }

    #[test]
    fn only_all_evaluated_legal_candidates_may_return_no_viable_path() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"execution.unavailable\",\"reason\":\"both are inapplicable\",\"evaluatedCapabilities\":[\"filesystem.scan\",\"filesystem.read\"]}"
                }]})),
            ])),
        });
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());

        let error = OpenClawAgentSkillTransport::with_invoker(invoker)
            .execute(
                &multi_capability_request(),
                &multi_capability_context(),
                &gateway,
                &mut |_| {},
            )
            .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::NoViableExecutionPath);
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn successful_skill_cannot_be_reclassified_as_no_viable_path() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"skill.invoke\",\"invocationId\":\"scan-1\",\"capability\":\"filesystem.scan\",\"input\":{\"path\":\"/safe\"}}"
                }]})),
                Ok(json!({"runId": "result-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"execution.unavailable\",\"reason\":\"wrongly reclassified\",\"evaluatedCapabilities\":[\"filesystem.scan\"]}"
                }]})),
            ])),
        });
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());

        let error = OpenClawAgentSkillTransport::with_invoker(invoker)
            .execute(&execution_request(), &context(true), &gateway, &mut |_| {})
            .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ExecutionFailed);
        assert_eq!(backend.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn mp0_selected_agent_crosses_transport_gateway_backend_registry_and_provider() {
        use crate::computer_use::{
            provider::mock::MockComputerUseProvider, registry::ComputerUseProviderRegistry,
            ComputerUseSkillBackend,
        };

        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "request-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": serde_json::to_string(&json!({
                        "type": "skill.invoke",
                        "invocationId": "computer-use-1",
                        "capability": "computer.use.execute",
                        "input": computer_use_input()
                    })).unwrap()
                }]})),
                Ok(json!({"runId": "result-run"})),
                Ok(json!({"status": "ok"})),
                Ok(json!({"messages": [{
                    "role": "assistant",
                    "content": "{\"type\":\"complete\",\"summary\":\"fixture complete\"}"
                }]})),
            ])),
        });
        let provider = Arc::new(MockComputerUseProvider::ready("diagnostic-only"));
        let backend = Arc::new(ComputerUseSkillBackend::with_registry(
            ComputerUseProviderRegistry::with_selected_provider(
                provider.identity.clone(),
                provider.clone(),
            ),
        ));
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend);
        let transport = OpenClawAgentSkillTransport::with_invoker(invoker.clone());
        let context = computer_use_context(true, vec!["computer.use.execute".to_owned()]);
        let request = AgentExecutionRequest::new(
            "execution-1",
            "task-1",
            "plan-1",
            "step-1",
            super::super::agent_execution::AgentId::new("openclaw").unwrap(),
            "Complete one bounded GUI fixture",
            vec!["computer.use.execute".to_owned()],
            json!({"requestedSkill": {
                "capability": "computer.use.execute",
                "input": computer_use_input()
            }}),
            context.clone(),
        )
        .unwrap();

        assert_eq!(provider.request_count(), 0);
        let result = transport
            .execute(&request, &context, &gateway, &mut |_| {})
            .unwrap();

        assert_eq!(provider.request_count(), 1);
        assert_eq!(
            result.output["skillInvocation"]["capability"],
            "computer.use.execute"
        );
        assert_eq!(
            result.output["skillInvocation"]["provider"],
            "mock-computer-use"
        );
        assert_eq!(invoker.calls.lock().unwrap()[0].0, "agent");
    }

    #[test]
    fn mp0_gateway_blocks_unconfirmed_unexposed_and_trusted_bypass() {
        use crate::computer_use::{
            provider::mock::MockComputerUseProvider, registry::ComputerUseProviderRegistry,
            ComputerUseSkillBackend,
        };

        for (allowed, context, expected) in [
            (
                Vec::new(),
                computer_use_context(false, vec!["computer.use.execute".to_owned()]),
                SkillInvocationErrorKind::PermissionRequired,
            ),
            (
                Vec::new(),
                computer_use_context(true, vec!["browser.search".to_owned()]),
                SkillInvocationErrorKind::CapabilityNotExposed,
            ),
            (
                vec!["computer.use.execute".to_owned()],
                computer_use_context(false, vec!["computer.use.execute".to_owned()]),
                SkillInvocationErrorKind::PermissionRequired,
            ),
        ] {
            let provider = Arc::new(MockComputerUseProvider::ready("test"));
            let backend = Arc::new(ComputerUseSkillBackend::with_registry(
                ComputerUseProviderRegistry::with_selected_provider(
                    provider.identity.clone(),
                    provider.clone(),
                ),
            ));
            let gateway = RuntimeSkillInvocationGateway::with_backend(allowed, backend);
            let request = SkillInvocationRequest::new(
                "computer-use-1",
                "computer.use.execute",
                computer_use_input(),
            )
            .unwrap();

            let error = gateway.invoke(&context, &request).unwrap_err();

            assert_eq!(error.kind, expected);
            assert_eq!(provider.request_count(), 0);
        }
    }

    #[test]
    fn permission_denial_happens_before_backend_invocation() {
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());
        let request = SkillInvocationRequest::new(
            "scan-1",
            "filesystem.scan",
            json!({"path": "/safe/fixture"}),
        )
        .unwrap();

        let error = gateway.invoke(&context(false), &request).unwrap_err();

        assert_eq!(error.kind, SkillInvocationErrorKind::PermissionDenied);
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn non_exposed_capability_is_rejected_before_backend_invocation() {
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());
        let request = SkillInvocationRequest::new(
            "read-1",
            "filesystem.read",
            json!({"path": "/safe/fixture.txt"}),
        )
        .unwrap();

        let error = gateway.invoke(&context(true), &request).unwrap_err();

        assert_eq!(error.kind, SkillInvocationErrorKind::CapabilityNotExposed);
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn transport_error_mapping_never_bypasses_runtime_gateway() {
        let invoker = Arc::new(ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![Err(ActiveGatewayMethodFailure {
                kind: ActiveGatewayFailureKind::Unreachable,
                message: "gateway unavailable".to_owned(),
            })])),
        });
        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let gateway = RuntimeSkillInvocationGateway::with_backend(Vec::new(), backend.clone());

        let error = OpenClawAgentSkillTransport::with_invoker(invoker)
            .execute(&execution_request(), &context(true), &gateway, &mut |_| {})
            .unwrap_err();

        assert_eq!(error.kind, AgentExecutionErrorKind::ConnectionUnavailable);
        assert!(error.retryable);
        assert!(backend.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn exact_openclaw_version_is_absent_from_transport_protocol() {
        let exposure = json!({
            "skills": execution_request().allowed_capabilities,
            "protocol": "skill.invoke"
        });
        assert!(exposure.get("version").is_none());
    }

    #[test]
    #[ignore = "requires an active, paired OpenClaw gateway"]
    fn real_openclaw_agent_skill_round_trip() {
        struct NoopEmitter;
        impl OperationEventEmitter for NoopEmitter {
            fn emit(
                &self,
                _snapshot: super::super::models::RuntimeOperationSnapshot,
            ) -> Result<(), ()> {
                Ok(())
            }
        }

        let fixture = tempfile::tempdir().unwrap();
        std::fs::write(fixture.path().join("fixture.txt"), "safe read-only fixture").unwrap();
        let path = fixture.path().to_string_lossy().to_string();
        let execution_id = format!("real-openclaw-ar1c-{}", uuid::Uuid::new_v4());
        let request = AgentExecutionRequest::new(
            execution_id.clone(),
            "real-task",
            "real-plan",
            "real-step",
            super::super::agent_execution::AgentId::new("openclaw").unwrap(),
            "Inspect the exposed read-only folder Skill and complete.",
            vec!["filesystem.scan".to_owned()],
            json!({"requestedSkill": {
                "capability": "filesystem.scan",
                "input": {"path": path.clone()}
            }}),
            SkillInvocationContext::new(
                "real-task",
                "real-plan",
                execution_id.clone(),
                "openclaw",
                vec!["filesystem.scan".to_owned()],
                true,
            )
            .unwrap(),
        )
        .unwrap();
        let context = SkillInvocationContext::new(
            "real-task",
            "real-plan",
            execution_id,
            "openclaw",
            vec!["filesystem.scan".to_owned()],
            true,
        )
        .unwrap();
        let gateway = RuntimeSkillInvocationGateway::with_backend(
            Vec::new(),
            Arc::new(RuntimeSkillBackend::new(
                RuntimeExecutionState::default(),
                Arc::new(NoopEmitter),
            )),
        );

        let result = OpenClawAgentSkillTransport::production()
            .execute(&request, &context, &gateway, &mut |_| {})
            .unwrap();

        assert_eq!(
            result.output["skillInvocation"]["capability"],
            "filesystem.scan"
        );
        assert!(result.output["skillInvocation"]["output"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry == "fixture.txt"));
        assert!(!result.output["agentCompletion"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .is_empty());
    }
}

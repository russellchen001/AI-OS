use super::{
    domain::{
        MediaError, MediaErrorCode, MediaProgress, MediaProviderSource, MediaRequest, MediaResult,
        QualityLoopPolicy,
    },
    provider::MediaProvider,
};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_AUTO_CORRECTION_ATTEMPTS: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum QualityVerdict {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum QualityIssue {
    RequestMismatch,
    ReferenceMismatch,
    Composition,
    ObviousFailure,
    Artifact,
    TemporalCoherence,
    ProviderFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationQualityAssessment {
    pub request_satisfaction: QualityVerdict,
    pub reference_adherence: QualityVerdict,
    pub composition: QualityVerdict,
    pub temporal_coherence: QualityVerdict,
    pub issues: Vec<QualityIssue>,
    pub retry_useful: bool,
    pub confidence_percent: u8,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CorrectionStrategy {
    pub reason: String,
    pub prompt_clarification: Option<String>,
    pub negative_constraint: Option<String>,
    pub parameter_adjustment: Option<String>,
    pub workflow_profile_adjustment: Option<String>,
    pub reference_weight_adjustment: Option<String>,
    pub regenerate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QualityAttemptRecord {
    attempt: u8,
    assessment: GenerationQualityAssessment,
    correction: Option<CorrectionStrategy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QualityLoopRecord {
    max_correction_attempts: u8,
    attempts: Vec<QualityAttemptRecord>,
    provider_id: String,
    provider_instance_id: Option<String>,
    exhausted: bool,
}

pub(crate) fn execute_quality_loop(
    provider: &dyn MediaProvider,
    request: &MediaRequest,
    report: &mut dyn FnMut(MediaProgress),
    cancelled: &dyn Fn() -> bool,
) -> Result<MediaResult, MediaError> {
    let identity = provider.metadata();
    let corrections_allowed = permitted_corrections(
        &request.options.quality_loop,
        identity.source,
        identity.supports_cost_estimation,
    );
    let mut current = request.clone();
    let mut records = Vec::new();

    for attempt in 0..=corrections_allowed {
        if cancelled() {
            return Err(media_error(
                MediaErrorCode::Cancelled,
                "Generative Media quality loop was cancelled.",
                false,
            ));
        }

        let outcome = provider.execute(&current, report);
        if let Ok(result) = &outcome {
            if result.provider_id != identity.provider_id
                || result.provider_instance_id != identity.provider_instance_id
            {
                return Err(media_error(
                    MediaErrorCode::ProviderError,
                    "Quality loop rejected an unexpected Provider identity.",
                    false,
                ));
            }
        }

        let assessment = assess_generation(&current, &outcome);
        if assessment.request_satisfaction == QualityVerdict::Pass {
            let mut result = outcome.expect("a passing assessment requires a result");
            records.push(QualityAttemptRecord {
                attempt,
                assessment,
                correction: None,
            });
            attach_record(
                &mut result,
                QualityLoopRecord {
                    max_correction_attempts: corrections_allowed,
                    attempts: records,
                    provider_id: identity.provider_id.clone(),
                    provider_instance_id: identity.provider_instance_id.clone(),
                    exhausted: false,
                },
            )?;
            return Ok(result);
        }

        let can_retry = attempt < corrections_allowed && assessment.retry_useful;
        if !can_retry {
            return Err(exhausted_error(outcome.err(), &assessment, attempt));
        }

        let correction = correction_for(&assessment);
        records.push(QualityAttemptRecord {
            attempt,
            assessment,
            correction: Some(correction.clone()),
        });
        apply_correction(&mut current, &correction);
        report(MediaProgress {
            phase: "quality-correction".to_owned(),
            completed_units: Some(u64::from(attempt + 1)),
            total_units: Some(u64::from(corrections_allowed)),
            message: correction.reason,
        });
    }

    unreachable!("bounded quality loop always returns within its fixed attempt range")
}

fn permitted_corrections(
    policy: &QualityLoopPolicy,
    source: MediaProviderSource,
    provider_can_estimate_cost: bool,
) -> u8 {
    if !policy.enabled || !policy.allow_retry {
        return 0;
    }
    if source == MediaProviderSource::Cloud
        && (!policy.allow_cloud_retry
            || !provider_can_estimate_cost
            || policy.max_additional_cost_micros.unwrap_or(0) == 0)
    {
        return 0;
    }
    policy
        .max_correction_attempts
        .min(MAX_AUTO_CORRECTION_ATTEMPTS)
}

fn assess_generation(
    request: &MediaRequest,
    outcome: &Result<MediaResult, MediaError>,
) -> GenerationQualityAssessment {
    if let Ok(result) = outcome {
        let signals = result.metadata.get("qualitySignals");
        let mut issues = Vec::new();
        if result.outputs.is_empty() || signal(signals, "obviousFailure") {
            issues.push(QualityIssue::ObviousFailure);
        }
        if signal(signals, "promptMismatch") {
            issues.push(QualityIssue::RequestMismatch);
        }
        if signal(signals, "referenceMismatch") {
            issues.push(QualityIssue::ReferenceMismatch);
        }
        if signal(signals, "compositionIssue") {
            issues.push(QualityIssue::Composition);
        }
        if signal(signals, "artifactDetected") {
            issues.push(QualityIssue::Artifact);
        }
        if signal(signals, "temporalCoherenceIssue") {
            issues.push(QualityIssue::TemporalCoherence);
        }
        let passed = issues.is_empty();
        return GenerationQualityAssessment {
            request_satisfaction: if passed {
                QualityVerdict::Pass
            } else {
                QualityVerdict::Fail
            },
            reference_adherence: if request.normalized_references.is_empty() {
                QualityVerdict::Unknown
            } else if issues.contains(&QualityIssue::ReferenceMismatch) {
                QualityVerdict::Fail
            } else {
                QualityVerdict::Pass
            },
            composition: if issues.contains(&QualityIssue::Composition) {
                QualityVerdict::Fail
            } else if passed {
                QualityVerdict::Pass
            } else {
                QualityVerdict::Unknown
            },
            temporal_coherence: if issues.contains(&QualityIssue::TemporalCoherence) {
                QualityVerdict::Fail
            } else {
                QualityVerdict::Unknown
            },
            retry_useful: !passed,
            confidence_percent: if passed { 80 } else { 90 },
            reason: if passed {
                "Generation returned usable output without deterministic failure signals."
                    .to_owned()
            } else {
                "Generation returned deterministic quality failure signals.".to_owned()
            },
            issues,
        };
    }

    let error = outcome.as_ref().expect_err("checked failure");
    let retry_useful = error.retryable
        && !matches!(
            error.code,
            MediaErrorCode::Authentication
                | MediaErrorCode::BudgetExceeded
                | MediaErrorCode::PolicyRejected
                | MediaErrorCode::Cancelled
                | MediaErrorCode::InvalidRequest
        );
    GenerationQualityAssessment {
        request_satisfaction: QualityVerdict::Fail,
        reference_adherence: QualityVerdict::Unknown,
        composition: QualityVerdict::Unknown,
        temporal_coherence: QualityVerdict::Unknown,
        issues: vec![QualityIssue::ProviderFailure],
        retry_useful,
        confidence_percent: 100,
        reason: "Provider returned a normalized generation failure.".to_owned(),
    }
}

fn signal(signals: Option<&serde_json::Value>, key: &str) -> bool {
    signals
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn correction_for(assessment: &GenerationQualityAssessment) -> CorrectionStrategy {
    CorrectionStrategy {
        reason: format!("Bounded correction: {}", assessment.reason),
        prompt_clarification: assessment
            .issues
            .contains(&QualityIssue::RequestMismatch)
            .then(|| "Follow every explicit request constraint exactly.".to_owned()),
        negative_constraint: (assessment.issues.contains(&QualityIssue::Artifact)
            || assessment.issues.contains(&QualityIssue::ObviousFailure))
        .then(|| "Avoid visible artifacts and incomplete output.".to_owned()),
        parameter_adjustment: Some("regenerate-once".to_owned()),
        workflow_profile_adjustment: None,
        reference_weight_adjustment: assessment
            .issues
            .contains(&QualityIssue::ReferenceMismatch)
            .then(|| "increase-reference-adherence-within-selected-provider".to_owned()),
        regenerate: true,
    }
}

fn apply_correction(request: &mut MediaRequest, correction: &CorrectionStrategy) {
    if let Some(clarification) = &correction.prompt_clarification {
        request.intent.constraints.push(clarification.clone());
    }
    if let Some(negative) = &correction.negative_constraint {
        request
            .intent
            .details
            .negative_constraints
            .push(negative.clone());
    }
}

fn attach_record(result: &mut MediaResult, record: QualityLoopRecord) -> Result<(), MediaError> {
    let record = serde_json::to_value(record).map_err(|_| {
        media_error(
            MediaErrorCode::ProviderError,
            "Quality-loop metadata could not be serialized.",
            false,
        )
    })?;
    match result.metadata.as_object_mut() {
        Some(metadata) => {
            metadata.insert("qualityLoop".to_owned(), record);
        }
        None => {
            result.metadata = serde_json::json!({
                "providerMetadata": result.metadata,
                "qualityLoop": record,
            });
        }
    }
    Ok(())
}

fn exhausted_error(
    provider_error: Option<MediaError>,
    assessment: &GenerationQualityAssessment,
    final_attempt: u8,
) -> MediaError {
    if let Some(error) = provider_error {
        if final_attempt == 0 {
            return MediaError {
                retryable: false,
                ..error
            };
        }
        return MediaError {
            retryable: false,
            message: format!(
                "Generative Media quality loop exhausted after {} attempt(s): {}",
                final_attempt + 1,
                error.message
            ),
            ..error
        };
    }
    media_error(
        MediaErrorCode::ProviderError,
        &format!(
            "Generative Media quality loop exhausted after {} attempt(s): {}",
            final_attempt + 1,
            assessment.reason
        ),
        false,
    )
}

fn media_error(code: MediaErrorCode, message: &str, retryable: bool) -> MediaError {
    MediaError {
        code,
        message: message.to_owned(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generative_media::{
            domain::{
                CreativeIntent, LawfulContentCompatibility, MediaCapability, MediaExecutionTarget,
                MediaGenerationOptions, MediaKind, MediaOutput, MediaProviderSelection,
                MediaRouteMode, ReferenceSpec,
            },
            provider::{LocalMediaReadiness, MediaProviderMetadata},
        },
        provider_selection::{AuthorizationKind, AuthorizationState, ProviderInterfaceKind},
    };
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    struct SequenceProvider {
        metadata: MediaProviderMetadata,
        outcomes: Mutex<VecDeque<Result<MediaResult, MediaError>>>,
        requests: Mutex<Vec<MediaRequest>>,
    }

    impl MediaProvider for SequenceProvider {
        fn metadata(&self) -> &MediaProviderMetadata {
            &self.metadata
        }

        fn execute(
            &self,
            request: &MediaRequest,
            _report: &mut dyn FnMut(MediaProgress),
        ) -> Result<MediaResult, MediaError> {
            self.requests.lock().unwrap().push(request.clone());
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .expect("fixture outcome")
        }
    }

    fn metadata(source: MediaProviderSource, cost: bool) -> MediaProviderMetadata {
        MediaProviderMetadata {
            provider_id: "fixed-provider".to_owned(),
            provider_instance_id: Some("fixed-account".to_owned()),
            source,
            interface_kind: ProviderInterfaceKind::NativeStructured,
            authorization_kind: AuthorizationKind::None,
            authorization_state: AuthorizationState::Connected,
            authorization_ref: None,
            available: true,
            local_readiness: (source == MediaProviderSource::Local)
                .then_some(LocalMediaReadiness::Ready),
            priority: 1,
            capabilities: vec![MediaCapability::TextToImage],
            lawful_content_compatibility: LawfulContentCompatibility::Unknown,
            recommended_cloud: false,
            supports_progress: true,
            supports_cancellation: true,
            supports_cost_estimation: cost,
        }
    }

    fn request() -> MediaRequest {
        MediaRequest {
            capability: MediaCapability::TextToImage,
            intent: CreativeIntent {
                request: "Create a clean blue square".to_owned(),
                media_kind: MediaKind::Image,
                references: Vec::new(),
                constraints: Vec::new(),
                preferences: Vec::new(),
                details: Default::default(),
            },
            route_mode: MediaRouteMode::Manual,
            execution_target: MediaExecutionTarget::Local,
            manual_provider: Some(MediaProviderSelection {
                provider_id: "fixed-provider".to_owned(),
                provider_instance_id: Some("fixed-account".to_owned()),
            }),
            options: MediaGenerationOptions::default(),
            normalized_references: Vec::new(),
        }
    }

    fn success(signals: serde_json::Value) -> MediaResult {
        MediaResult {
            provider_id: "fixed-provider".to_owned(),
            provider_instance_id: Some("fixed-account".to_owned()),
            outputs: vec![MediaOutput {
                id: "output".to_owned(),
                kind: MediaKind::Image,
                mime_type: "image/png".to_owned(),
                handle: "asset://output".to_owned(),
            }],
            metadata: serde_json::json!({"qualitySignals": signals}),
        }
    }

    fn provider(
        source: MediaProviderSource,
        cost: bool,
        outcomes: Vec<Result<MediaResult, MediaError>>,
    ) -> SequenceProvider {
        SequenceProvider {
            metadata: metadata(source, cost),
            outcomes: Mutex::new(outcomes.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn successful_generation_records_assessment_without_retry() {
        let provider = provider(
            MediaProviderSource::Local,
            false,
            vec![Ok(success(serde_json::json!({})))],
        );
        let result = execute_quality_loop(&provider, &request(), &mut |_| {}, &|| false).unwrap();
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
        assert_eq!(
            result.metadata["qualityLoop"]["attempts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            result.metadata["qualityLoop"]["attempts"][0]["assessment"]["requestSatisfaction"],
            "pass"
        );
    }

    #[test]
    fn poor_result_gets_one_normalized_correction_then_succeeds() {
        let provider = provider(
            MediaProviderSource::Local,
            false,
            vec![
                Ok(success(serde_json::json!({
                    "promptMismatch": true,
                    "artifactDetected": true
                }))),
                Ok(success(serde_json::json!({}))),
            ],
        );
        let result = execute_quality_loop(&provider, &request(), &mut |_| {}, &|| false).unwrap();
        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].manual_provider, requests[1].manual_provider,
            "correction cannot change Provider or account"
        );
        assert!(requests[1]
            .intent
            .constraints
            .iter()
            .any(|value| value.contains("explicit request")));
        assert_eq!(
            result.metadata["qualityLoop"]["attempts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn retry_count_is_hard_capped_and_exhaustion_is_normalized() {
        let failure = || {
            Err(media_error(
                MediaErrorCode::ProviderUnavailable,
                "transient fixture",
                true,
            ))
        };
        let provider = provider(
            MediaProviderSource::Local,
            false,
            vec![failure(), failure()],
        );
        let mut request = request();
        request.options.quality_loop.max_correction_attempts = u8::MAX;
        let error = execute_quality_loop(&provider, &request, &mut |_| {}, &|| false).unwrap_err();
        assert_eq!(provider.requests.lock().unwrap().len(), 2);
        assert!(!error.retryable);
        assert!(error.message.contains("exhausted after 2 attempt(s)"));
    }

    #[test]
    fn cancellation_stops_before_first_or_corrective_attempt() {
        let never_called = provider(
            MediaProviderSource::Local,
            false,
            vec![Ok(success(serde_json::json!({})))],
        );
        let error =
            execute_quality_loop(&never_called, &request(), &mut |_| {}, &|| true).unwrap_err();
        assert_eq!(error.code, MediaErrorCode::Cancelled);
        assert!(never_called.requests.lock().unwrap().is_empty());

        let before_retry = provider(
            MediaProviderSource::Local,
            false,
            vec![Ok(success(serde_json::json!({"obviousFailure": true})))],
        );
        let checks = AtomicUsize::new(0);
        let error = execute_quality_loop(&before_retry, &request(), &mut |_| {}, &|| {
            checks.fetch_add(1, Ordering::SeqCst) > 0
        })
        .unwrap_err();
        assert_eq!(error.code, MediaErrorCode::Cancelled);
        assert_eq!(before_retry.requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn cloud_retry_requires_explicit_authorization_cost_bound_and_estimator() {
        let retryable = || {
            Err(media_error(
                MediaErrorCode::ProviderUnavailable,
                "cloud transient",
                true,
            ))
        };
        let cloud = provider(MediaProviderSource::Cloud, true, vec![retryable()]);
        let error = execute_quality_loop(&cloud, &request(), &mut |_| {}, &|| false).unwrap_err();
        assert!(!error.retryable);
        assert_eq!(cloud.requests.lock().unwrap().len(), 1);

        let no_estimator = provider(MediaProviderSource::Cloud, false, vec![retryable()]);
        let mut no_estimator_request = request();
        no_estimator_request.options.quality_loop.allow_cloud_retry = true;
        no_estimator_request
            .options
            .quality_loop
            .max_additional_cost_micros = Some(1_000);
        execute_quality_loop(&no_estimator, &no_estimator_request, &mut |_| {}, &|| false)
            .unwrap_err();
        assert_eq!(no_estimator.requests.lock().unwrap().len(), 1);

        let authorized = provider(
            MediaProviderSource::Cloud,
            true,
            vec![retryable(), Ok(success(serde_json::json!({})))],
        );
        let mut authorized_request = request();
        authorized_request.execution_target = MediaExecutionTarget::Cloud;
        authorized_request.options.quality_loop.allow_cloud_retry = true;
        authorized_request
            .options
            .quality_loop
            .max_additional_cost_micros = Some(1_000);
        execute_quality_loop(&authorized, &authorized_request, &mut |_| {}, &|| false).unwrap();
        let requests = authorized.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].manual_provider, requests[1].manual_provider);
    }

    #[test]
    fn assessment_normalizes_reference_composition_and_temporal_signals() {
        let mut request = request();
        request.normalized_references.push(ReferenceSpec {
            source_reference_id: "reference".to_owned(),
            kind: MediaKind::Video,
            analyzer_provider_id: "local-vlm".to_owned(),
            analyzer_provider_instance_id: Some("local".to_owned()),
            summary: "moving subject".to_owned(),
            constraints: Vec::new(),
            temporal_events: vec!["subject turns".to_owned()],
            provenance_sha256: "a".repeat(64),
        });
        let outcome = Ok(success(serde_json::json!({
            "referenceMismatch": true,
            "compositionIssue": true,
            "temporalCoherenceIssue": true
        })));
        let assessment = assess_generation(&request, &outcome);
        assert_eq!(assessment.reference_adherence, QualityVerdict::Fail);
        assert_eq!(assessment.composition, QualityVerdict::Fail);
        assert_eq!(assessment.temporal_coherence, QualityVerdict::Fail);
        assert!(assessment.retry_useful);
    }

    #[test]
    fn policy_and_auth_denials_never_retry() {
        for code in [
            MediaErrorCode::BudgetExceeded,
            MediaErrorCode::PolicyRejected,
            MediaErrorCode::Authentication,
        ] {
            let provider = provider(
                MediaProviderSource::Local,
                false,
                vec![Err(media_error(code, "denied", true))],
            );
            execute_quality_loop(&provider, &request(), &mut |_| {}, &|| false).unwrap_err();
            assert_eq!(provider.requests.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn unexpected_provider_or_account_identity_stops_immediately() {
        let mut wrong = success(serde_json::json!({}));
        wrong.provider_instance_id = Some("other-account".to_owned());
        let provider = provider(MediaProviderSource::Local, false, vec![Ok(wrong)]);
        let error =
            execute_quality_loop(&provider, &request(), &mut |_| {}, &|| false).unwrap_err();
        assert_eq!(error.code, MediaErrorCode::ProviderError);
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "runs one real local ComfyUI generation after a deterministic transient failure"]
    fn live_local_bounded_correction_reaches_real_comfyui_once() {
        use crate::generative_media::comfyui_provider::{
            resolve_local_asset_handle, ComfyUiLocalProvider, COMFYUI_LOCAL_PROVIDER_ID,
        };

        struct TransientThenReal<'a> {
            real: &'a ComfyUiLocalProvider,
            calls: AtomicUsize,
        }

        impl MediaProvider for TransientThenReal<'_> {
            fn metadata(&self) -> &MediaProviderMetadata {
                self.real.metadata()
            }

            fn execute(
                &self,
                request: &MediaRequest,
                report: &mut dyn FnMut(MediaProgress),
            ) -> Result<MediaResult, MediaError> {
                if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(media_error(
                        MediaErrorCode::ProviderUnavailable,
                        "deterministic transient smoke failure",
                        true,
                    ));
                }
                self.real.execute(request, report)
            }
        }

        let real = ComfyUiLocalProvider::discover_ready().expect("Ready local ComfyUI");
        let provider = TransientThenReal {
            real: &real,
            calls: AtomicUsize::new(0),
        };
        let mut request = request();
        request.manual_provider = Some(MediaProviderSelection {
            provider_id: COMFYUI_LOCAL_PROVIDER_ID.to_owned(),
            provider_instance_id: real.metadata().provider_instance_id.clone(),
        });
        request.intent.request =
            "a small blue square centered on a plain white background".to_owned();
        let result = execute_quality_loop(&provider, &request, &mut |_| {}, &|| false)
            .expect("bounded correction should reach real ComfyUI");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
        assert_eq!(result.provider_id, COMFYUI_LOCAL_PROVIDER_ID);
        assert_eq!(
            result.metadata["qualityLoop"]["attempts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let output = result.outputs.first().expect("real generated output");
        let path = resolve_local_asset_handle(&output.handle).expect("managed output path");
        assert!(path.is_file());
        std::fs::remove_file(path).expect("remove smoke-owned output");
        eprintln!(
            "LIVE_GM6_CORRECTION provider={} attempts=2",
            result.provider_id
        );
    }
}

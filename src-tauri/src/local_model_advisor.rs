use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
};
use sysinfo::{Disks, System};

const ADVISOR_SOURCE: &str = "llmfit/cli-json-v1";
const FALLBACK_SOURCE: &str = "ai-os/native-hardware";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AdvisorAvailability {
    Ready,
    Unavailable,
    Incompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ModelFitLevel {
    Fit,
    Marginal,
    NotFit,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdvisorRuntimeStatus {
    pub availability: AdvisorAvailability,
    pub source: String,
    pub diagnostic_version: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachineProfile {
    pub platform: String,
    pub architecture: String,
    pub cpu: String,
    pub cpu_cores: usize,
    pub gpu: Option<String>,
    pub accelerator: Option<String>,
    pub total_memory_gb: f64,
    pub available_memory_gb: f64,
    pub unified_memory: bool,
    pub available_storage_gb: Option<f64>,
    pub source: String,
    pub evidence: Vec<String>,
    pub advisor: AdvisorRuntimeStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelReference {
    pub provider_id: String,
    pub model_id: String,
    #[serde(default)]
    pub parameter_size: Option<String>,
    #[serde(default)]
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelFitAssessment {
    pub provider_id: String,
    pub requested_model_id: String,
    pub resolved_model_id: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub estimated_memory_gb: Option<f64>,
    pub fit: ModelFitLevel,
    pub fit_label: String,
    pub recommended_context: Option<u64>,
    pub expected_tokens_per_second: Option<f64>,
    pub provider_compatible: Option<bool>,
    pub confidence: String,
    pub evidence: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelRecommendationRequest {
    #[serde(default)]
    pub objective: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub model_family: Option<String>,
    #[serde(default)]
    pub preferred_local_provider: Option<String>,
    #[serde(default = "default_recommendation_limit")]
    pub limit: usize,
}

fn default_recommendation_limit() -> usize {
    5
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelRecommendation {
    pub rank: usize,
    pub model_id: String,
    pub model_family: Option<String>,
    pub preferred_quantization: Option<String>,
    pub recommended_context: Option<u64>,
    pub estimated_memory_gb: Option<f64>,
    pub expected_tokens_per_second: Option<f64>,
    pub fit: ModelFitLevel,
    pub provider_compatibility: Vec<String>,
    pub score: Option<f64>,
    pub confidence: String,
    pub evidence: Vec<String>,
    pub acquisition_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelRecommendationReport {
    pub machine: MachineProfile,
    pub recommendations: Vec<LocalModelRecommendation>,
    pub preferred: Option<LocalModelRecommendation>,
    pub acquisition_requires_confirmation: bool,
    pub routing_authority: String,
    pub source: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledModelAssessmentReport {
    pub machine: MachineProfile,
    pub assessments: Vec<ModelFitAssessment>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LocalModelRouteEvidence {
    pub fit: ModelFitLevel,
    pub score: Option<f64>,
}

fn route_evidence_cache() -> &'static Mutex<HashMap<(String, String), LocalModelRouteEvidence>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), LocalModelRouteEvidence>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn cached_route_evidence() -> HashMap<(String, String), LocalModelRouteEvidence> {
    route_evidence_cache()
        .lock()
        .map(|cache| cache.clone())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct LlmfitCli {
    executable: PathBuf,
    diagnostic_version: Option<String>,
}

impl LlmfitCli {
    fn discover() -> Result<Self, AdvisorRuntimeStatus> {
        for candidate in llmfit_candidates() {
            let output = Command::new(&candidate)
                .arg("--help")
                .env_remove("LOCALMAXXING_API_KEY")
                .env_remove("GITHUB_TOKEN")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            let Ok(output) = output else {
                continue;
            };
            let help = String::from_utf8_lossy(&output.stdout);
            if !output.status.success()
                || !["system", "fit", "recommend", "info"]
                    .iter()
                    .all(|capability| help.contains(capability))
            {
                continue;
            }
            let diagnostic_version = Command::new(&candidate)
                .arg("--version")
                .output()
                .ok()
                .filter(|version| version.status.success())
                .map(|version| String::from_utf8_lossy(&version.stdout).trim().to_owned())
                .filter(|version| !version.is_empty());
            return Ok(Self {
                executable: candidate,
                diagnostic_version,
            });
        }

        Err(AdvisorRuntimeStatus {
            availability: AdvisorAvailability::Unavailable,
            source: ADVISOR_SOURCE.to_owned(),
            diagnostic_version: None,
            reason: Some(
                "llmfit executable with the required JSON commands was not found".to_owned(),
            ),
        })
    }

    fn status(&self) -> AdvisorRuntimeStatus {
        AdvisorRuntimeStatus {
            availability: AdvisorAvailability::Ready,
            source: ADVISOR_SOURCE.to_owned(),
            diagnostic_version: self.diagnostic_version.clone(),
            reason: None,
        }
    }

    fn json(&self, arguments: &[String]) -> Result<Value, String> {
        if !matches!(
            arguments.first().map(String::as_str),
            Some("system" | "fit" | "recommend" | "info" | "plan")
        ) {
            return Err("AI-OS permits only read-only llmfit advisor commands".to_owned());
        }
        let output = Command::new(&self.executable)
            .arg("--no-dashboard")
            .args(arguments)
            .env_remove("LOCALMAXXING_API_KEY")
            .env_remove("GITHUB_TOKEN")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|_| "llmfit could not be started".to_owned())?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(if error.is_empty() {
                "llmfit returned an unsuccessful status".to_owned()
            } else {
                error
            });
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_| "llmfit returned an incompatible JSON response".to_owned())
    }
}

fn llmfit_candidates() -> Vec<PathBuf> {
    if let Some(candidate) = std::env::var_os("AI_OS_LLMFIT_EXECUTABLE") {
        return vec![PathBuf::from(candidate)];
    }
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/llmfit"),
        PathBuf::from("/usr/local/bin/llmfit"),
        PathBuf::from("llmfit"),
    ];
    candidates.dedup();
    candidates
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

fn available_storage_gb() -> Option<f64> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .find(|disk| disk.mount_point() == Path::new("/"))
        .map(|disk| gib(disk.available_space()))
}

fn native_machine_profile(advisor: AdvisorRuntimeStatus) -> MachineProfile {
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_cpu_all();
    let cpu = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_owned())
        .filter(|cpu| !cpu.is_empty())
        .unwrap_or_else(|| "Unknown CPU".to_owned());
    let total_memory_gb = gib(system.total_memory());
    let available_memory_gb = gib(system.available_memory());

    MachineProfile {
        platform: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        cpu,
        cpu_cores: system.cpus().len(),
        gpu: None,
        accelerator: None,
        total_memory_gb,
        available_memory_gb,
        unified_memory: false,
        available_storage_gb: available_storage_gb(),
        source: FALLBACK_SOURCE.to_owned(),
        evidence: vec!["hardware values measured locally by AI-OS".to_owned()],
        advisor,
    }
}

fn f64_field(value: &Value, name: &str) -> Option<f64> {
    value.get(name).and_then(Value::as_f64)
}

fn u64_field(value: &Value, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn parse_machine(value: &Value, status: AdvisorRuntimeStatus) -> Result<MachineProfile, String> {
    let system = value
        .get("system")
        .ok_or_else(|| "llmfit system response is missing hardware evidence".to_owned())?;
    let total_memory_gb = f64_field(system, "total_ram_gb")
        .ok_or_else(|| "llmfit system response is missing total memory".to_owned())?;
    let available_memory_gb = f64_field(system, "available_ram_gb").unwrap_or(total_memory_gb);
    let cpu =
        string_field(system, &["cpu_name", "cpu"]).unwrap_or_else(|| "Unknown CPU".to_owned());
    let gpu = string_field(system, &["gpu_name"]);
    let accelerator = string_field(system, &["backend", "gpu_backend"]);
    let unified_memory = system
        .get("unified_memory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut evidence = vec![format!(
        "llmfit measured {:.1} GB total and {:.1} GB currently available memory",
        total_memory_gb, available_memory_gb
    )];
    if let Some(accelerator) = accelerator.as_deref() {
        evidence.push(format!("llmfit detected {accelerator} acceleration"));
    }

    Ok(MachineProfile {
        platform: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        cpu,
        cpu_cores: u64_field(system, &["cpu_cores"]).unwrap_or(0) as usize,
        gpu,
        accelerator,
        total_memory_gb,
        available_memory_gb,
        unified_memory,
        available_storage_gb: available_storage_gb(),
        source: ADVISOR_SOURCE.to_owned(),
        evidence,
        advisor: status,
    })
}

fn machine_profile_with(cli: Result<LlmfitCli, AdvisorRuntimeStatus>) -> MachineProfile {
    match cli {
        Ok(cli) => match cli.json(&["system".to_owned(), "--json".to_owned()]) {
            Ok(value) => parse_machine(&value, cli.status()).unwrap_or_else(|reason| {
                native_machine_profile(AdvisorRuntimeStatus {
                    availability: AdvisorAvailability::Incompatible,
                    source: ADVISOR_SOURCE.to_owned(),
                    diagnostic_version: cli.diagnostic_version,
                    reason: Some(reason),
                })
            }),
            Err(reason) => native_machine_profile(AdvisorRuntimeStatus {
                availability: AdvisorAvailability::Incompatible,
                source: ADVISOR_SOURCE.to_owned(),
                diagnostic_version: cli.diagnostic_version,
                reason: Some(reason),
            }),
        },
        Err(status) => native_machine_profile(status),
    }
}

#[tauri::command]
pub(crate) fn get_local_model_advisor_status() -> AdvisorRuntimeStatus {
    LlmfitCli::discover()
        .map(|cli| cli.status())
        .unwrap_or_else(|status| status)
}

#[tauri::command]
pub(crate) fn get_local_machine_profile() -> MachineProfile {
    machine_profile_with(LlmfitCli::discover())
}

fn fit_level(value: Option<&str>) -> ModelFitLevel {
    match value.unwrap_or_default().to_ascii_lowercase().as_str() {
        "perfect" | "good" | "fit" => ModelFitLevel::Fit,
        "marginal" => ModelFitLevel::Marginal,
        "tootight" | "too tight" | "too_tight" | "notfit" | "not fit" | "not_fit" => {
            ModelFitLevel::NotFit
        }
        _ => ModelFitLevel::Unknown,
    }
}

fn provider_compatible(provider_id: &str, model: &Value) -> Option<bool> {
    let provider = provider_id.trim().to_ascii_lowercase();
    let runtime = string_field(model, &["runtime"])
        .unwrap_or_default()
        .to_ascii_lowercase();
    match provider.as_str() {
        "ollama" => Some(
            model
                .get("ollama_name")
                .is_some_and(|value| !value.is_null())
                || runtime.contains("llama"),
        ),
        "omlx" => Some(
            runtime.contains("mlx")
                || string_field(model, &["best_quant"])
                    .is_some_and(|quant| quant.to_ascii_lowercase().contains("mlx")),
        ),
        _ => None,
    }
}

fn model_evidence(model: &Value) -> Vec<String> {
    let mut evidence = model
        .get("notes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(8)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if let Some(method) = model
        .pointer("/estimate_basis/method")
        .and_then(Value::as_str)
    {
        evidence.push(format!("estimate method: {method}"));
    }
    evidence
}

fn unknown_assessment(reference: &LocalModelReference, reason: String) -> ModelFitAssessment {
    ModelFitAssessment {
        provider_id: reference.provider_id.clone(),
        requested_model_id: reference.model_id.clone(),
        resolved_model_id: None,
        parameter_size: reference.parameter_size.clone(),
        quantization: reference.quantization.clone(),
        estimated_memory_gb: None,
        fit: ModelFitLevel::Unknown,
        fit_label: "Unknown".to_owned(),
        recommended_context: None,
        expected_tokens_per_second: None,
        provider_compatible: None,
        confidence: "unsupported".to_owned(),
        evidence: vec![reason],
        source: ADVISOR_SOURCE.to_owned(),
    }
}

fn assessment_from_model(reference: &LocalModelReference, model: &Value) -> ModelFitAssessment {
    let upstream_fit = string_field(model, &["fit_level", "fit_label"]);
    let fit = fit_level(upstream_fit.as_deref());
    ModelFitAssessment {
        provider_id: reference.provider_id.clone(),
        requested_model_id: reference.model_id.clone(),
        resolved_model_id: string_field(model, &["name"]),
        parameter_size: string_field(model, &["parameter_count"])
            .or_else(|| reference.parameter_size.clone()),
        quantization: string_field(model, &["best_quant"])
            .or_else(|| reference.quantization.clone()),
        estimated_memory_gb: f64_field(model, "memory_required_gb")
            .or_else(|| f64_field(model, "total_memory_gb")),
        fit,
        fit_label: upstream_fit.unwrap_or_else(|| "Unknown".to_owned()),
        recommended_context: u64_field(model, &["usable_context", "effective_context_length"]),
        expected_tokens_per_second: f64_field(model, "estimated_tps"),
        provider_compatible: provider_compatible(&reference.provider_id, model),
        confidence: string_field(model, &["estimate_confidence"])
            .unwrap_or_else(|| "unknown".to_owned()),
        evidence: model_evidence(model),
        source: ADVISOR_SOURCE.to_owned(),
    }
}

fn assess_with(cli: &LlmfitCli, reference: &LocalModelReference) -> ModelFitAssessment {
    let arguments = vec![
        "info".to_owned(),
        reference.model_id.clone(),
        "--json".to_owned(),
    ];
    match cli.json(&arguments) {
        Ok(value) => value
            .get("models")
            .and_then(Value::as_array)
            .and_then(|models| models.first())
            .map(|model| assessment_from_model(reference, model))
            .unwrap_or_else(|| {
                unknown_assessment(reference, "llmfit returned no matching model".to_owned())
            }),
        Err(reason) => unknown_assessment(reference, reason),
    }
}

#[tauri::command]
pub(crate) fn assess_local_model(model: LocalModelReference) -> ModelFitAssessment {
    let assessment = match LlmfitCli::discover() {
        Ok(cli) => assess_with(&cli, &model),
        Err(status) => unknown_assessment(
            &model,
            status
                .reason
                .unwrap_or_else(|| "llmfit is unavailable".to_owned()),
        ),
    };
    remember_route_evidence(&assessment, None);
    assessment
}

fn remember_route_evidence(assessment: &ModelFitAssessment, score: Option<f64>) {
    if let Ok(mut cache) = route_evidence_cache().lock() {
        cache.insert(
            (
                assessment.provider_id.clone(),
                assessment.requested_model_id.clone(),
            ),
            LocalModelRouteEvidence {
                fit: assessment.fit.clone(),
                score,
            },
        );
    }
}

#[tauri::command]
pub(crate) fn assess_installed_local_models(
    models: Vec<LocalModelReference>,
) -> InstalledModelAssessmentReport {
    let cli = LlmfitCli::discover();
    let machine = machine_profile_with(cli.clone());
    let assessments = models
        .iter()
        .map(|model| match cli.as_ref() {
            Ok(cli) => assess_with(cli, model),
            Err(status) => unknown_assessment(
                model,
                status
                    .reason
                    .clone()
                    .unwrap_or_else(|| "llmfit is unavailable".to_owned()),
            ),
        })
        .collect::<Vec<_>>();
    for assessment in &assessments {
        remember_route_evidence(assessment, None);
    }
    InstalledModelAssessmentReport {
        machine,
        assessments,
        source: ADVISOR_SOURCE.to_owned(),
    }
}

fn use_case(request: &LocalModelRecommendationRequest) -> Option<&'static str> {
    let value = request
        .capability
        .as_deref()
        .or(request.objective.as_deref())?
        .to_ascii_lowercase();
    if value.contains("code") || value.contains("program") {
        Some("coding")
    } else if value.contains("reason") || value.contains("plan") {
        Some("reasoning")
    } else if value.contains("embed") {
        Some("embedding")
    } else if value.contains("image") || value.contains("vision") {
        Some("multimodal")
    } else if value.contains("chat") {
        Some("chat")
    } else {
        Some("general")
    }
}

fn provider_compatibility(model: &Value) -> Vec<String> {
    let mut providers = Vec::new();
    if model
        .get("ollama_name")
        .is_some_and(|value| !value.is_null())
    {
        providers.push("ollama".to_owned());
    }
    let runtime = string_field(model, &["runtime"])
        .unwrap_or_default()
        .to_ascii_lowercase();
    if runtime.contains("mlx") {
        providers.push("omlx".to_owned());
    }
    if runtime.contains("llama") && !providers.iter().any(|value| value == "ollama") {
        providers.push("ollama".to_owned());
    }
    providers
}

fn recommendation_from_model(rank: usize, model: &Value) -> LocalModelRecommendation {
    let model_id = string_field(model, &["name"]).unwrap_or_else(|| "Unknown model".to_owned());
    LocalModelRecommendation {
        rank,
        model_id: model_id.clone(),
        model_family: string_field(model, &["provider"]),
        preferred_quantization: string_field(model, &["best_quant"]),
        recommended_context: u64_field(model, &["usable_context", "effective_context_length"]),
        estimated_memory_gb: f64_field(model, "memory_required_gb")
            .or_else(|| f64_field(model, "total_memory_gb")),
        expected_tokens_per_second: f64_field(model, "estimated_tps"),
        fit: fit_level(string_field(model, &["fit_level", "fit_label"]).as_deref()),
        provider_compatibility: provider_compatibility(model),
        score: f64_field(model, "score"),
        confidence: string_field(model, &["estimate_confidence"])
            .unwrap_or_else(|| "unknown".to_owned()),
        evidence: model_evidence(model),
        acquisition_model_id: string_field(model, &["ollama_name"]).or(Some(model_id)),
    }
}

#[tauri::command]
pub(crate) fn recommend_local_models(
    request: LocalModelRecommendationRequest,
) -> LocalModelRecommendationReport {
    let cli = LlmfitCli::discover();
    let machine = machine_profile_with(cli.clone());
    let mut warning = None;
    let mut recommendations = Vec::new();
    if let Ok(cli) = cli {
        let limit = request.limit.clamp(1, 20);
        let mut arguments = vec![
            "recommend".to_owned(),
            "--json".to_owned(),
            "--limit".to_owned(),
            limit.to_string(),
        ];
        if let Some(use_case) = use_case(&request) {
            arguments.extend(["--use-case".to_owned(), use_case.to_owned()]);
        }
        match request.preferred_local_provider.as_deref() {
            Some("omlx") => arguments.extend(["--runtime".to_owned(), "mlx".to_owned()]),
            Some("ollama") => {
                arguments.extend(["--force-runtime".to_owned(), "llamacpp".to_owned()])
            }
            _ => {}
        }
        if request
            .capability
            .as_deref()
            .is_some_and(|capability| capability.contains("tool"))
        {
            arguments.extend(["--capability".to_owned(), "tool_use".to_owned()]);
        }
        match cli.json(&arguments) {
            Ok(value) => {
                let family = request.model_family.as_deref().map(str::to_ascii_lowercase);
                recommendations = value
                    .get("models")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|model| {
                        family.as_ref().is_none_or(|family| {
                            ["name", "provider"]
                                .iter()
                                .filter_map(|field| model.get(*field).and_then(Value::as_str))
                                .any(|value| value.to_ascii_lowercase().contains(family))
                        })
                    })
                    .enumerate()
                    .map(|(index, model)| recommendation_from_model(index + 1, model))
                    .collect();
            }
            Err(reason) => warning = Some(reason),
        }
    } else {
        warning = machine.advisor.reason.clone();
    }
    let preferred = recommendations.first().cloned();
    LocalModelRecommendationReport {
        machine,
        recommendations,
        preferred,
        acquisition_requires_confirmation: true,
        routing_authority: "ai-os".to_owned(),
        source: ADVISOR_SOURCE.to_owned(),
        warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(fit: &str, runtime: &str, memory: f64, context: u64) -> Value {
        serde_json::json!({
            "name": "example/Qwen-Coder-7B",
            "provider": "example",
            "parameter_count": "7B",
            "best_quant": "Q4_K_M",
            "memory_required_gb": memory,
            "fit_level": fit,
            "usable_context": context,
            "estimated_tps": 24.5,
            "estimate_confidence": "estimated",
            "runtime": runtime,
            "ollama_name": "qwen-coder:7b",
            "score": 88.0,
            "notes": ["fixture evidence"]
        })
    }

    fn reference(provider: &str, model_id: &str) -> LocalModelReference {
        LocalModelReference {
            provider_id: provider.to_owned(),
            model_id: model_id.to_owned(),
            parameter_size: None,
            quantization: None,
        }
    }

    #[test]
    fn installed_ollama_model_is_assessed_instead_of_assumed_suitable() {
        let assessment = assessment_from_model(
            &reference("ollama", "qwen-coder:7b"),
            &model("Good", "llama.cpp", 5.4, 32768),
        );

        assert_eq!(assessment.fit, ModelFitLevel::Fit);
        assert_eq!(assessment.quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(assessment.recommended_context, Some(32768));
        assert_eq!(assessment.estimated_memory_gb, Some(5.4));
        assert_eq!(assessment.expected_tokens_per_second, Some(24.5));
        assert_eq!(assessment.provider_compatible, Some(true));
    }

    #[test]
    fn installed_omlx_model_is_assessed_with_mlx_compatibility() {
        let assessment = assessment_from_model(
            &reference("omlx", "mlx-community/Qwen-Coder-7B-4bit"),
            &model("Good", "MLX", 5.4, 32768),
        );

        assert_eq!(assessment.fit, ModelFitLevel::Fit);
        assert_eq!(assessment.provider_compatible, Some(true));
        assert_eq!(assessment.quantization.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn oversized_and_marginal_models_remain_distinct() {
        let oversized = assessment_from_model(
            &reference("ollama", "large"),
            &model("TooTight", "llama.cpp", 80.0, 1024),
        );
        let marginal = assessment_from_model(
            &reference("omlx", "medium"),
            &model("Marginal", "MLX", 14.0, 8192),
        );

        assert_eq!(oversized.fit, ModelFitLevel::NotFit);
        assert_eq!(marginal.fit, ModelFitLevel::Marginal);
    }

    #[test]
    fn unknown_model_fails_safe_without_invented_metadata() {
        let assessment = unknown_assessment(&reference("omlx", "unknown"), "not found".to_owned());

        assert_eq!(assessment.fit, ModelFitLevel::Unknown);
        assert_eq!(assessment.provider_compatible, None);
        assert_eq!(assessment.recommended_context, None);
        assert_eq!(assessment.expected_tokens_per_second, None);
    }

    #[test]
    fn recommendation_never_owns_routing_or_acquisition() {
        let recommendation = recommendation_from_model(1, &model("Good", "MLX", 5.4, 32768));

        assert_eq!(recommendation.rank, 1);
        assert!(recommendation
            .provider_compatibility
            .contains(&"omlx".to_owned()));
        assert_eq!(recommendation.fit, ModelFitLevel::Fit);
    }

    #[test]
    fn objective_is_reduced_to_a_category_and_not_forwarded_as_task_text() {
        let request = LocalModelRecommendationRequest {
            objective: Some("private source code from the user".to_owned()),
            capability: Some("code.generate".to_owned()),
            model_family: None,
            preferred_local_provider: None,
            limit: 5,
        };

        assert_eq!(use_case(&request), Some("coding"));
    }

    #[test]
    fn version_is_diagnostic_and_capability_probe_is_the_contract() {
        let status = AdvisorRuntimeStatus {
            availability: AdvisorAvailability::Ready,
            source: ADVISOR_SOURCE.to_owned(),
            diagnostic_version: Some("llmfit future-version".to_owned()),
            reason: None,
        };

        assert_eq!(status.availability, AdvisorAvailability::Ready);
    }

    #[test]
    fn missing_llmfit_keeps_a_real_native_profile_and_unknown_advice() {
        let profile = machine_profile_with(Err(AdvisorRuntimeStatus {
            availability: AdvisorAvailability::Unavailable,
            source: ADVISOR_SOURCE.to_owned(),
            diagnostic_version: None,
            reason: Some("missing fixture".to_owned()),
        }));

        assert_eq!(profile.source, FALLBACK_SOURCE);
        assert!(profile.total_memory_gb > 0.0);
        assert!(profile.cpu_cores > 0);
        assert_eq!(
            profile.advisor.availability,
            AdvisorAvailability::Unavailable
        );
    }

    #[test]
    fn adapter_refuses_every_side_effecting_llmfit_command() {
        let cli = LlmfitCli {
            executable: PathBuf::from("/does/not/matter"),
            diagnostic_version: None,
        };

        for command in ["download", "run", "bench", "update", "serve"] {
            let error = cli.json(&[command.to_owned()]).unwrap_err();
            assert_eq!(
                error,
                "AI-OS permits only read-only llmfit advisor commands"
            );
        }
    }

    #[test]
    #[ignore = "requires the official llmfit executable"]
    fn real_llmfit_machine_recommendation_and_fit_smoke() {
        let cli = LlmfitCli::discover().expect("official llmfit CLI must be installed");
        let machine = machine_profile_with(Ok(cli.clone()));
        assert_eq!(machine.advisor.availability, AdvisorAvailability::Ready);
        assert!(machine.total_memory_gb > 0.0);
        assert!(machine.cpu_cores > 0);

        for (provider, runtime_flag, runtime) in [
            ("omlx", "--runtime", "mlx"),
            ("ollama", "--force-runtime", "llamacpp"),
        ] {
            let response = cli
                .json(&[
                    "recommend".to_owned(),
                    "--json".to_owned(),
                    "--limit".to_owned(),
                    "1".to_owned(),
                    runtime_flag.to_owned(),
                    runtime.to_owned(),
                ])
                .expect("real provider recommendation should be JSON");
            let model = response["models"]
                .as_array()
                .and_then(|models| models.first())
                .expect("real provider recommendation should contain a model");
            let recommendation = recommendation_from_model(1, model);
            let assessment =
                assessment_from_model(&reference(provider, &recommendation.model_id), model);
            assert_ne!(assessment.fit, ModelFitLevel::Unknown);
            assert_eq!(assessment.provider_compatible, Some(true));
            assert!(assessment.quantization.is_some());
            assert!(assessment.recommended_context.is_some());
            assert!(assessment.estimated_memory_gb.is_some());
        }
    }
}

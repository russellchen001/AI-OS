mod adapters;
mod evidence;
mod profile;
mod router;

pub(crate) use evidence::{validate_bundle, EvidenceBundle};
pub(crate) use router::{
    route_distillation, CognitiveDistillationRouteRequest, CognitiveDistillationRouteResult,
};

use adapters::AdapterCatalog;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SubjectKind {
    SelfProfile,
    PrivatePerson,
    PublicPerson,
    HistoricalPerson,
    FictionalCharacter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceMediaKind {
    Text,
    Image,
    Audio,
    Video,
    Document,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceKind {
    Chat,
    Email,
    Article,
    Book,
    Interview,
    Meeting,
    Social,
    UserFile,
    PublicWeb,
    TaskEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveDistillationStatus {
    pub multimodal_first: bool,
    pub canonical_profile_authority: String,
    pub creator_adapters: Vec<adapters::AdapterStatus>,
    pub asr_adapters: Vec<adapters::ExtractorStatus>,
    pub media_demux: adapters::ExtractorStatus,
    pub visual_analysis_capabilities: Vec<String>,
    pub no_silent_cloud_upload: bool,
}

#[tauri::command]
pub(crate) fn get_cognitive_distillation_status() -> CognitiveDistillationStatus {
    let catalog = AdapterCatalog::probe();
    CognitiveDistillationStatus {
        multimodal_first: true,
        canonical_profile_authority: "ai-os".to_owned(),
        creator_adapters: catalog.statuses(),
        asr_adapters: adapters::probe_asr_adapters(),
        media_demux: adapters::probe_ffmpeg(),
        visual_analysis_capabilities: vec![
            "media.reference.image.analyze".to_owned(),
            "media.reference.video.analyze".to_owned(),
        ],
        no_silent_cloud_upload: true,
    }
}

#[tauri::command]
pub(crate) fn preview_cognitive_distillation_route(
    request: CognitiveDistillationRouteRequest,
) -> Result<CognitiveDistillationRouteResult, String> {
    route_distillation(&request, &AdapterCatalog::probe()).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn validate_cognitive_distillation_evidence(
    bundle: EvidenceBundle,
) -> Result<EvidenceBundle, String> {
    validate_bundle(&bundle).map_err(|error| error.to_string())?;
    Ok(bundle)
}

pub(crate) fn prepare_from_value(input: &Value) -> Result<Value, String> {
    let request: CognitiveDistillationRouteRequest = serde_json::from_value(input.clone())
        .map_err(|_| {
            "Cognitive Distillation input does not match the canonical contract.".to_owned()
        })?;
    let result = preview_cognitive_distillation_route(request)?;
    serde_json::to_value(result)
        .map_err(|_| "Cognitive Distillation route could not be serialized.".to_owned())
}

use super::{
    evidence::{validate_bundle, EvidenceBundle, EvidenceError},
    SubjectKind,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProfileStatus {
    Draft,
    Reviewed,
    Active,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveClaim {
    pub claim_id: String,
    pub statement: String,
    pub confirmed: bool,
    pub confidence: f32,
    pub evidence_ids: Vec<String>,
    pub contradictory_evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RevisionRecord {
    pub revision: u32,
    pub reason: String,
    pub evidence_bundle_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersonDistillationProfile {
    pub profile_id: String,
    pub revision: u32,
    pub status: ProfileStatus,
    pub subject_kind: SubjectKind,
    pub identity: Vec<CognitiveClaim>,
    pub domain_expertise: Vec<CognitiveClaim>,
    pub knowledge_model: Vec<CognitiveClaim>,
    pub decision_patterns: Vec<CognitiveClaim>,
    pub reasoning_frameworks: Vec<CognitiveClaim>,
    pub preferences: Vec<CognitiveClaim>,
    pub constraints: Vec<CognitiveClaim>,
    pub behavioral_patterns: Vec<CognitiveClaim>,
    pub communication_style: Vec<CognitiveClaim>,
    pub representative_examples: Vec<CognitiveClaim>,
    pub evidence_bundle_id: String,
    pub contradictions: Vec<String>,
    pub revision_history: Vec<RevisionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunnablePersonaSkill {
    pub skill_id: String,
    pub profile_id: String,
    pub profile_revision: u32,
    pub instructions: String,
    pub examples: Vec<String>,
    #[serde(default)]
    pub raw_media_assets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileError {
    Evidence(EvidenceError),
    InvalidProfile,
    HumanReviewRequired,
    RawMediaForbidden,
}
impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evidence(error) => write!(formatter, "{error}"),
            Self::InvalidProfile => formatter.write_str("Person Distillation profile is invalid."),
            Self::HumanReviewRequired => {
                formatter.write_str("Human review is required before profile activation.")
            }
            Self::RawMediaForbidden => {
                formatter.write_str("Runnable Persona Skill cannot contain raw multimodal assets.")
            }
        }
    }
}
impl Error for ProfileError {}

pub(crate) struct AiOsProfileAuthority;
impl AiOsProfileAuthority {
    pub(crate) fn activate(
        mut profile: PersonDistillationProfile,
        bundle: &EvidenceBundle,
        human_reviewed: bool,
    ) -> Result<PersonDistillationProfile, ProfileError> {
        validate_bundle(bundle).map_err(ProfileError::Evidence)?;
        if !human_reviewed {
            return Err(ProfileError::HumanReviewRequired);
        }
        if profile.profile_id.trim().is_empty()
            || profile.revision == 0
            || profile.evidence_bundle_id != bundle.bundle_id
            || profile.subject_kind != bundle.subject_kind
        {
            return Err(ProfileError::InvalidProfile);
        }
        profile.status = ProfileStatus::Active;
        Ok(profile)
    }

    pub(crate) fn package_skill(
        profile: &PersonDistillationProfile,
        skill: RunnablePersonaSkill,
    ) -> Result<RunnablePersonaSkill, ProfileError> {
        if profile.status != ProfileStatus::Active
            || skill.profile_id != profile.profile_id
            || skill.profile_revision != profile.revision
        {
            return Err(ProfileError::InvalidProfile);
        }
        if !skill.raw_media_assets.is_empty() {
            return Err(ProfileError::RawMediaForbidden);
        }
        Ok(skill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*},
        SourceKind, SourceMediaKind,
    };

    fn profile_and_bundle() -> (PersonDistillationProfile, EvidenceBundle) {
        let source = source("source", SourceMediaKind::Text, SourceKind::Chat, true);
        let evidence = normalize_extraction(
            SubjectKind::PrivatePerson,
            &source,
            &local_policy(),
            extraction(item("evidence", SourceMediaKind::Text)),
        )
        .unwrap();
        let bundle = EvidenceBundle {
            bundle_id: "bundle".to_owned(),
            subject_kind: SubjectKind::PrivatePerson,
            media_kind: SourceMediaKind::Text,
            sources: vec![source],
            evidence,
        };
        let profile = PersonDistillationProfile {
            profile_id: "profile".to_owned(),
            revision: 1,
            status: ProfileStatus::Draft,
            subject_kind: SubjectKind::PrivatePerson,
            identity: Vec::new(),
            domain_expertise: Vec::new(),
            knowledge_model: Vec::new(),
            decision_patterns: Vec::new(),
            reasoning_frameworks: Vec::new(),
            preferences: Vec::new(),
            constraints: Vec::new(),
            behavioral_patterns: Vec::new(),
            communication_style: Vec::new(),
            representative_examples: Vec::new(),
            evidence_bundle_id: "bundle".to_owned(),
            contradictions: Vec::new(),
            revision_history: vec![RevisionRecord {
                revision: 1,
                reason: "initial distillation".to_owned(),
                evidence_bundle_id: "bundle".to_owned(),
            }],
        };
        (profile, bundle)
    }

    #[test]
    fn only_ai_os_authority_activates_after_human_review() {
        let (profile, bundle) = profile_and_bundle();
        assert_eq!(
            AiOsProfileAuthority::activate(profile.clone(), &bundle, false).unwrap_err(),
            ProfileError::HumanReviewRequired
        );
        assert_eq!(
            AiOsProfileAuthority::activate(profile, &bundle, true)
                .unwrap()
                .status,
            ProfileStatus::Active
        );
    }

    #[test]
    fn runnable_skill_never_bundles_raw_multimodal_files() {
        let (profile, bundle) = profile_and_bundle();
        let profile = AiOsProfileAuthority::activate(profile, &bundle, true).unwrap();
        let skill = RunnablePersonaSkill {
            skill_id: "persona-profile".to_owned(),
            profile_id: profile.profile_id.clone(),
            profile_revision: profile.revision,
            instructions: "Use the reviewed bounded profile.".to_owned(),
            examples: Vec::new(),
            raw_media_assets: vec!["artifact://private-video".to_owned()],
        };
        assert_eq!(
            AiOsProfileAuthority::package_skill(&profile, skill).unwrap_err(),
            ProfileError::RawMediaForbidden
        );
    }
}

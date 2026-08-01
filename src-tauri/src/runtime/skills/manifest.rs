use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManifest {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub executor: SkillExecutor,
    pub enabled: bool,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExecutor {
    #[serde(rename = "type")]
    pub kind: String,
    pub handler: String,
}

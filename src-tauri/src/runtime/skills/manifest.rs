use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SkillManifest {
    pub id: String,
    pub name: String,
    pub category: String,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub executor: SkillExecutor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SkillExecutor {
    pub kind: String,
    pub handler: String,
}

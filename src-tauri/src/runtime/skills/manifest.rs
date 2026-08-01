use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {

    pub id: String,

    pub name: String,

    pub category: String,

    pub capabilities: Vec<String>,

    pub permissions: Vec<String>,

    pub executor: SkillExecutor,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExecutor {

    pub kind: String,

    pub handler: String,
}

use serde_json::json;
use uuid::Uuid;

use crate::memory;

#[derive(Debug, Clone)]
pub enum MemoryType {
    User,
    Conversation,
    Council,
    Skill,
    AgentProfile,
    Task,
    System,
}

impl MemoryType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Conversation => "conversation",
            Self::Council => "council",
            Self::Skill => "skill",
            Self::AgentProfile => "agent_profile",
            Self::Task => "task",
            Self::System => "system",
        }
    }
}

pub(crate) fn remember(
    memory_type: MemoryType,
    content: String,
    importance: u8,
) -> Result<(), String> {
    let entry = json!({
        "id": Uuid::new_v4().to_string(),
        "type": memory_type.as_str(),
        "content": content,
        "metadata": {
            "importance": importance
        }
    });

    memory::save_memory(entry)
}

pub(crate) fn remember_user_preference(
    content: String,
) -> Result<(), String> {
    remember(
        MemoryType::User,
        content,
        5,
    )
}

use super::manifest::{SkillExecutor, SkillManifest};

fn skill(
    id: &str,
    name: &str,
    category: &str,
    description: &str,
    capabilities: &[&str],
    permissions: &[&str],
    executor_kind: &str,
    handler: &str,
) -> SkillManifest {
    SkillManifest {
        id: id.to_owned(),
        name: name.to_owned(),
        category: category.to_owned(),
        description: description.to_owned(),
        version: "1.0.0".to_owned(),
        capabilities: capabilities
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect(),
        permissions: permissions
            .iter()
            .map(|permission| (*permission).to_owned())
            .collect(),
        executor: SkillExecutor {
            kind: executor_kind.to_owned(),
            handler: handler.to_owned(),
        },
        enabled: true,
        built_in: true,
    }
}

pub(crate) fn built_in_skills() -> Vec<SkillManifest> {
    vec![
        skill(
            "document",
            "Documents",
            "productivity",
            "Read, create and convert document, spreadsheet and presentation files.",
            &[
                "document.read",
                "document.create",
                "document.convert",
            ],
            &["filesystem.read", "filesystem.write"],
            "openclaw",
            "document",
        ),
        skill(
            "filesystem",
            "Filesystem",
            "storage",
            "Read, scan, move and write approved files through OpenClaw.",
            &[
                "filesystem.read",
                "filesystem.write",
                "filesystem.scan",
                "filesystem.move",
            ],
            &["filesystem.read", "filesystem.write"],
            "openclaw",
            "filesystem",
        ),
        skill(
            "openclaw-session",
            "OpenClaw Session",
            "system",
            "Create and manage approved OpenClaw gateway sessions.",
            &["sessions.create", "ai.openclaw.gateway"],
            &["sessions.create"],
            "openclaw",
            "sessions",
        ),
        skill(
            "browser",
            "Browser",
            "browser",
            "Search and interact with web pages through approved browser tools.",
            &["browser.search", "browser.control"],
            &["network.access", "browser.control"],
            "mcp",
            "browser",
        ),
        skill(
            "local-models",
            "Local Models",
            "system",
            "List, inspect, download and remove local Ollama models.",
            &[
                "models.list",
                "models.show",
                "models.pull",
                "models.delete",
            ],
            &["models.read", "models.manage"],
            "local",
            "ollama",
        ),
        skill(
            "downloads",
            "Downloads",
            "network",
            "Manage downloads through approved download providers including HTTP, FTP, torrents and cloud storage.",
            &[
                "download.start",
                "download.pause",
                "download.resume",
                "download.cancel",
                "download.status",
                "download.list",
            ],
            &[
                "download.network",
                "download.credentials",
                "download.manage",
            ],
            "openclaw",
            "downloads",
        ),
    ]
}

pub(crate) fn find_by_capability(capability: &str) -> Option<SkillManifest> {
    let capability = capability.trim();

    if capability.is_empty() {
        return None;
    }

    built_in_skills().into_iter().find(|skill| {
        skill.enabled
            && skill
                .capabilities
                .iter()
                .any(|candidate| candidate == capability)
    })
}

pub(crate) fn get_by_id(skill_id: &str) -> Option<SkillManifest> {
    let skill_id = skill_id.trim();

    if skill_id.is_empty() {
        return None;
    }

    built_in_skills()
        .into_iter()
        .find(|skill| skill.id == skill_id)
}

#[tauri::command]
pub fn list_skills() -> Vec<SkillManifest> {
    built_in_skills()
}

#[tauri::command]
pub fn get_skill(skill_id: String) -> Result<SkillManifest, String> {
    get_by_id(&skill_id).ok_or_else(|| format!("Skill was not found: {}", skill_id.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_contains_current_runtime_capabilities() {
        for capability in [
            "filesystem.read",
            "filesystem.write",
            "filesystem.scan",
            "filesystem.move",
            "sessions.create",
            "ai.openclaw.gateway",
            "browser.search",
            "browser.control",
            "models.list",
            "models.show",
            "models.pull",
            "models.delete",
            "download.start",
            "download.pause",
            "download.resume",
            "download.cancel",
            "download.status",
            "download.list",
        ] {
            assert!(
                find_by_capability(capability).is_some(),
                "missing built-in capability: {capability}"
            );
        }
    }

    #[test]
    fn capability_matching_is_exact() {
        assert!(find_by_capability("filesystem.scan").is_some());
        assert!(find_by_capability("Filesystem.Scan").is_none());
        assert!(find_by_capability("filesystem.scan.extra").is_none());
        assert!(find_by_capability("").is_none());
    }

    #[test]
    fn built_in_executors_have_non_empty_handlers() {
        assert!(built_in_skills().iter().all(|skill| {
            !skill.executor.kind.trim().is_empty() && !skill.executor.handler.trim().is_empty()
        }));
    }

    #[test]
    fn list_command_returns_canonical_registry() {
        let skills = list_skills();

        assert_eq!(skills.len(), 6);

        assert_eq!(skills[0].id, "document");
        assert_eq!(skills[0].executor.kind, "openclaw");
        assert_eq!(skills[0].executor.handler, "document");

        assert_eq!(skills[1].id, "filesystem");
        assert_eq!(skills[2].id, "openclaw-session");
        assert_eq!(skills[3].id, "browser");

        assert_eq!(skills[4].id, "local-models");
        assert_eq!(skills[4].executor.kind, "local");
        assert_eq!(skills[4].executor.handler, "ollama");

        assert_eq!(skills[5].id, "downloads");
        assert_eq!(skills[5].executor.kind, "openclaw");
        assert_eq!(skills[5].executor.handler, "downloads");
    }

    #[test]
    fn get_command_returns_skill_by_stable_id() {
        let browser = get_skill("browser".to_owned()).unwrap();

        assert_eq!(browser.id, "browser");
        assert_eq!(browser.executor.kind, "mcp");
        assert_eq!(browser.executor.handler, "browser");
    }

    #[test]
    fn get_command_rejects_unknown_or_empty_id() {
        assert!(get_skill("unknown".to_owned()).is_err());
        assert!(get_skill("   ".to_owned()).is_err());
    }

    #[test]
    fn manifest_serialization_matches_frontend_contract() {
        let filesystem = get_skill("filesystem".to_owned()).unwrap();
        let value = serde_json::to_value(filesystem).unwrap();

        assert_eq!(value["id"], json!("filesystem"));
        assert_eq!(value["category"], json!("storage"));
        assert_eq!(value["version"], json!("1.0.0"));
        assert_eq!(value["enabled"], json!(true));
        assert_eq!(value["builtIn"], json!(true));
        assert_eq!(value["executor"]["type"], json!("openclaw"));
        assert_eq!(value["executor"]["handler"], json!("filesystem"));
        assert!(value["capabilities"].is_array());
        assert!(value["permissions"].is_array());
        assert!(value.get("createdAt").is_none());
        assert!(value.get("updatedAt").is_none());
    }
}

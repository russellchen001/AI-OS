use super::manifest::{SkillExecutor, SkillManifest};

fn skill(
    id: &str,
    name: &str,
    category: &str,
    capabilities: &[&str],
    permissions: &[&str],
    executor_kind: &str,
    handler: &str,
) -> SkillManifest {
    SkillManifest {
        id: id.to_owned(),
        name: name.to_owned(),
        category: category.to_owned(),
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
    }
}

pub(crate) fn built_in_skills() -> Vec<SkillManifest> {
    vec![
        skill(
            "filesystem",
            "Filesystem",
            "storage",
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
            &["sessions.create", "ai.openclaw.gateway"],
            &["sessions.create"],
            "openclaw",
            "sessions",
        ),
        skill(
            "browser",
            "Browser",
            "browser",
            &["browser.search", "browser.control"],
            &["network.access", "browser.control"],
            "mcp",
            "browser",
        ),
    ]
}

pub(crate) fn find_by_capability(capability: &str) -> Option<SkillManifest> {
    let capability = capability.trim();

    if capability.is_empty() {
        return None;
    }

    built_in_skills().into_iter().find(|skill| {
        skill
            .capabilities
            .iter()
            .any(|candidate| candidate == capability)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

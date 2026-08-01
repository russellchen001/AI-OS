use super::{manifest::SkillManifest, registry};

pub(crate) fn resolve(capability: &str) -> Option<SkillManifest> {
    registry::find_by_capability(capability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_openclaw_filesystem_skill() {
        let skill = resolve("filesystem.scan").expect("filesystem skill");

        assert_eq!(skill.id, "filesystem");
        assert_eq!(skill.executor.kind, "openclaw");
        assert_eq!(skill.executor.handler, "filesystem");
    }

    #[test]
    fn resolves_mcp_browser_skill_without_claiming_openclaw_support() {
        let skill = resolve("browser.search").expect("browser skill");

        assert_eq!(skill.id, "browser");
        assert_eq!(skill.executor.kind, "mcp");
    }

    #[test]
    fn unknown_capability_is_not_fabricated() {
        assert!(resolve("unknown.capability").is_none());
    }
}

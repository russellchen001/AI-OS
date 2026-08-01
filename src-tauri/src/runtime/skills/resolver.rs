use super::{
    manifest::SkillManifest,
    registry,
};


pub fn resolve(
    capability: &str,
) -> Option<SkillManifest> {

    registry::find_by_capability(
        capability
    )
}

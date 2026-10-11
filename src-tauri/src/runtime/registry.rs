use super::models::{
    RuntimeAdapterKind, RuntimeCapability, RuntimeDefinition, RuntimeLocation, RuntimePlatform,
};

fn capabilities(values: &[RuntimeCapability]) -> Vec<RuntimeCapability> {
    values.to_vec()
}

pub fn definitions() -> Vec<RuntimeDefinition> {
    use RuntimeCapability::{Discover, Health, Open, Start, Stop};

    vec![
        RuntimeDefinition {
            id: "openclaw".to_string(),
            adapter_kind: RuntimeAdapterKind::Openclaw,
            display_key: "runtime.openclaw".to_string(),
            icon_key: "openclaw".to_string(),
            supported_platforms: vec![RuntimePlatform::Macos],
            location: RuntimeLocation::Hybrid,
            dependencies: vec![],
            capabilities: capabilities(&[Discover, Health, Open]),
        },



        RuntimeDefinition {
            id: "cherry-studio".to_string(),
            adapter_kind: RuntimeAdapterKind::CherryStudio,
            display_key: "runtime.cherryStudio".to_string(),
            icon_key: "cherry-studio".to_string(),
            supported_platforms: vec![RuntimePlatform::Macos],
            location: RuntimeLocation::Local,
            dependencies: vec![],
            capabilities: capabilities(&[Discover, Health, Start, Stop, Open]),
        },
    ]
}

pub(crate) fn contains_id(runtime_id: &str) -> bool {
    adapter_kind(runtime_id).is_some()
}

pub(crate) fn adapter_kind(runtime_id: &str) -> Option<RuntimeAdapterKind> {
    #[cfg(test)]
    if matches!(
        runtime_id,
        "test-runtime-a" | "test-runtime-b" | "test-runtime-c"
    ) {
        return Some(RuntimeAdapterKind::CherryStudio);
    }

    definitions()
        .into_iter()
        .find(|definition| definition.id == runtime_id)
        .map(|definition| definition.adapter_kind)
}

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
            id: "ollama".to_string(),
            adapter_kind: RuntimeAdapterKind::Ollama,
            display_key: "runtime.ollama".to_string(),
            icon_key: "ollama".to_string(),
            supported_platforms: vec![RuntimePlatform::Macos],
            location: RuntimeLocation::Local,
            dependencies: vec![],
            capabilities: capabilities(&[Discover, Health, Start, Stop, Open]),
        },
        RuntimeDefinition {
            id: "docker-desktop".to_string(),
            adapter_kind: RuntimeAdapterKind::DockerDesktop,
            display_key: "runtime.dockerDesktop".to_string(),
            icon_key: "docker".to_string(),
            supported_platforms: vec![RuntimePlatform::Macos],
            location: RuntimeLocation::Local,
            dependencies: vec![],
            capabilities: capabilities(&[Discover, Health, Start, Stop, Open]),
        },
        RuntimeDefinition {
            id: "open-webui".to_string(),
            adapter_kind: RuntimeAdapterKind::OpenWebui,
            display_key: "runtime.openWebUi".to_string(),
            icon_key: "open-webui".to_string(),
            supported_platforms: vec![RuntimePlatform::Macos],
            location: RuntimeLocation::Local,
            dependencies: vec!["docker-desktop".to_string()],
            capabilities: capabilities(&[Discover, Health, Start, Stop, Open]),
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
    definitions()
        .iter()
        .any(|definition| definition.id == runtime_id)
}

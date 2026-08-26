use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OfficeProviderId {
    MacosNative,
    MicrosoftOffice,
    AppleIwork,
    WpsOffice,
    GoogleWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficeProvider {
    pub id: OfficeProviderId,
    pub name: &'static str,
    pub local: bool,
    pub priority: u16,
    pub capabilities: &'static [&'static str],
    pub available: bool,
}

impl OfficeProvider {
    pub(crate) fn supports(&self, capability: &str) -> bool {
        self.capabilities.contains(&capability)
    }
}

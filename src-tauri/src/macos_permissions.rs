use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum MacosAuthorizationState {
    NotDetermined,
    Granted,
    Denied,
}

impl MacosAuthorizationState {
    pub(crate) fn requires_request(self) -> bool {
        matches!(self, Self::NotDetermined)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacosPermissionStatus {
    pub mail_automation: PermissionState,
    pub calendar_access: PermissionState,
    pub guidance: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionState {
    pub granted: bool,
    pub authorization_state: MacosAuthorizationState,
    pub description: String,
}

#[tauri::command]
pub(crate) fn check_macos_mail_calendar_permissions() -> MacosPermissionStatus {
    #[cfg(target_os = "macos")]
    {
        MacosPermissionStatus {
            mail_automation: PermissionState {
                granted: false,
                authorization_state: MacosAuthorizationState::NotDetermined,
                description: "Mail Automation permission detection foundation is ready.".to_owned(),
            },
            calendar_access: PermissionState {
                granted: false,
                authorization_state: MacosAuthorizationState::NotDetermined,
                description: "Calendar permission detection foundation is ready.".to_owned(),
            },
            guidance: vec![
                "Open System Settings → Privacy & Security → Automation for Mail access."
                    .to_owned(),
                "Open System Settings → Privacy & Security → Calendars for Calendar access."
                    .to_owned(),
            ],
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        MacosPermissionStatus {
            mail_automation: PermissionState {
                granted: false,
                authorization_state: MacosAuthorizationState::Denied,
                description: "Mail access requires macOS.".to_owned(),
            },
            calendar_access: PermissionState {
                granted: false,
                authorization_state: MacosAuthorizationState::Denied,
                description: "Calendar access requires macOS.".to_owned(),
            },
            guidance: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_system_permission_is_reused_without_another_request() {
        assert!(!MacosAuthorizationState::Granted.requires_request());
        assert!(MacosAuthorizationState::NotDetermined.requires_request());
        assert!(!MacosAuthorizationState::Denied.requires_request());
    }
}

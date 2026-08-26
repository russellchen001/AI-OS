use serde::Serialize;

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
    pub description: String,
}

#[tauri::command]
pub(crate) fn check_macos_mail_calendar_permissions() -> MacosPermissionStatus {
    #[cfg(target_os = "macos")]
    {
        MacosPermissionStatus {
            mail_automation: PermissionState {
                granted: false,
                description: "Mail Automation permission detection foundation is ready.".to_owned(),
            },
            calendar_access: PermissionState {
                granted: false,
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
                description: "Mail access requires macOS.".to_owned(),
            },
            calendar_access: PermissionState {
                granted: false,
                description: "Calendar access requires macOS.".to_owned(),
            },
            guidance: vec![],
        }
    }
}

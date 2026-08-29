use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use tauri::Manager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ConnectionProvider {
    Microsoft,
    GoogleWorkspace,
    Wps,
    AppleIwork,
    Ebay,
    Amazon,
    Taobao,
    Jd,
    Pinduoduo,
}

impl ConnectionProvider {
    fn id(self) -> &'static str {
        match self {
            Self::Microsoft => "microsoft-graph",
            Self::GoogleWorkspace => "google-workspace",
            Self::Wps => "wps-office",
            Self::AppleIwork => "apple-iwork",
            Self::Ebay => "ebay",
            Self::Amazon => "amazon-consumer",
            Self::Taobao => "taobao-consumer",
            Self::Jd => "jd-consumer",
            Self::Pinduoduo => "pinduoduo-consumer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ConnectionMethod {
    OfficialOAuth,
    AuthenticatedBrowser,
    BackendBroker,
    NativeApplication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum UnifiedConnectionState {
    NotConfigured,
    Disconnected,
    Connecting,
    WaitingForUser,
    Connected,
    Expired,
    LoginRequired,
    AuthorizationRequired,
    DeveloperApprovalRequired,
    BackendBrokerRequired,
    AppNotInstalled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConnectionCapability {
    provider_id: String,
    display_name: String,
    method: ConnectionMethod,
    state: UnifiedConnectionState,
    login_url: Option<String>,
    profile_ref: Option<String>,
    developer_approval_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalApplicationAvailability {
    application_id: String,
    display_name: String,
    installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalApplicationAvailabilityReport {
    applications: Vec<LocalApplicationAvailability>,
    iwork_state: UnifiedConnectionState,
}

fn local_application_availability_with(
    app_exists: impl Fn(&str) -> bool,
    bundle_exists: impl Fn(&str) -> bool,
) -> LocalApplicationAvailabilityReport {
    let definitions: [(&str, &str, &[&str], &[&str]); 5] = [
        (
            "pages",
            "Pages",
            &["com.apple.Pages", "com.apple.iWork.Pages"],
            &["/Applications/Pages.app"],
        ),
        (
            "numbers",
            "Numbers",
            &["com.apple.Numbers", "com.apple.iWork.Numbers"],
            &["/Applications/Numbers.app"],
        ),
        (
            "keynote",
            "Keynote",
            &["com.apple.Keynote", "com.apple.iWork.Keynote"],
            &["/Applications/Keynote.app"],
        ),
        (
            "wps-office",
            "WPS Office",
            &["com.kingsoft.wpsoffice.mac"],
            &[
                "/Applications/wpsoffice.app",
                "/Applications/WPS Office.app",
            ],
        ),
        (
            "microsoft-excel",
            "Microsoft Excel",
            &["com.microsoft.Excel"],
            &["/Applications/Microsoft Excel.app"],
        ),
    ];
    let applications = definitions
        .into_iter()
        .map(
            |(application_id, display_name, bundle_ids, paths)| LocalApplicationAvailability {
                application_id: application_id.to_owned(),
                display_name: display_name.to_owned(),
                installed: bundle_ids.iter().any(|id| bundle_exists(id))
                    || paths.iter().any(|path| app_exists(path)),
            },
        )
        .collect::<Vec<_>>();
    let installed_iwork = applications
        .iter()
        .filter(|application| {
            matches!(
                application.application_id.as_str(),
                "pages" | "numbers" | "keynote"
            )
        })
        .filter(|application| application.installed)
        .count();
    LocalApplicationAvailabilityReport {
        applications,
        iwork_state: if installed_iwork == 0 {
            UnifiedConnectionState::AppNotInstalled
        } else {
            UnifiedConnectionState::AuthorizationRequired
        },
    }
}

fn local_application_availability() -> LocalApplicationAvailabilityReport {
    let user_applications = dirs::home_dir().map(|home| home.join("Applications"));
    let mut bundle_ids = std::collections::HashSet::new();
    for directory in [
        Some(std::path::PathBuf::from("/Applications")),
        user_applications.clone(),
    ]
    .into_iter()
    .flatten()
    {
        for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
            let info = entry.path().join("Contents/Info.plist");
            let output = std::process::Command::new("/usr/libexec/PlistBuddy")
                .args(["-c", "Print :CFBundleIdentifier"])
                .arg(info)
                .output();
            if let Ok(output) = output {
                if output.status.success() {
                    bundle_ids.insert(String::from_utf8_lossy(&output.stdout).trim().to_owned());
                }
            }
        }
    }
    local_application_availability_with(
        |path| {
            let system_path = std::path::Path::new(path);
            system_path.exists()
                || user_applications
                    .as_ref()
                    .and_then(|directory| system_path.file_name().map(|name| directory.join(name)))
                    .is_some_and(|candidate| candidate.exists())
        },
        |bundle_id| bundle_ids.contains(bundle_id),
    )
}

const IWORK_AUTHORIZATION_MARKER: &str = "iwork-authorization-verified";

fn installed_iwork_count(applications: &[LocalApplicationAvailability]) -> usize {
    applications
        .iter()
        .filter(|application| {
            matches!(
                application.application_id.as_str(),
                "pages" | "numbers" | "keynote"
            )
        })
        .filter(|application| application.installed)
        .count()
}

fn resolved_iwork_state(
    installed_iwork: usize,
    previously_verified: bool,
    authorization_probe_succeeded: bool,
) -> UnifiedConnectionState {
    if installed_iwork == 0 {
        UnifiedConnectionState::AppNotInstalled
    } else if previously_verified && authorization_probe_succeeded {
        UnifiedConnectionState::Connected
    } else {
        UnifiedConnectionState::AuthorizationRequired
    }
}

fn iwork_bundle_id(application_id: &str) -> Option<&'static str> {
    match application_id {
        "pages" => Some("com.apple.Pages"),
        "numbers" => Some("com.apple.Numbers"),
        "keynote" => Some("com.apple.Keynote"),
        _ => None,
    }
}

fn probe_iwork_authorization(applications: &[LocalApplicationAvailability]) -> Result<(), String> {
    for application in applications.iter().filter(|application| {
        application.installed && iwork_bundle_id(&application.application_id).is_some()
    }) {
        let bundle_id = iwork_bundle_id(&application.application_id)
            .ok_or_else(|| "Unknown iWork application".to_owned())?;

        let script = format!("tell application id \"{bundle_id}\" to get version");

        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .output()
            .map_err(|error| {
                format!(
                    "Could not verify {} Automation permission: {error}",
                    application.display_name
                )
            })?;

        if !output.status.success() {
            return Err(format!(
                "{} Automation permission is not currently available: {}",
                application.display_name,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }

    Ok(())
}

fn iwork_authorization_marker_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(IWORK_AUTHORIZATION_MARKER))
        .map_err(|_| "iWork authorization metadata storage is unavailable".to_owned())
}

fn iwork_was_previously_verified(app: &tauri::AppHandle) -> bool {
    iwork_authorization_marker_path(app)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

fn save_iwork_verified_marker(app: &tauri::AppHandle) -> Result<(), String> {
    let path = iwork_authorization_marker_path(app)?;

    let parent = path
        .parent()
        .ok_or_else(|| "iWork authorization metadata storage is unavailable".to_owned())?;

    std::fs::create_dir_all(parent)
        .map_err(|_| "iWork authorization metadata directory could not be created".to_owned())?;

    std::fs::write(path, b"verified\n")
        .map_err(|_| "iWork authorization metadata could not be saved".to_owned())
}

fn remove_iwork_verified_marker(app: &tauri::AppHandle) -> Result<(), String> {
    let path = iwork_authorization_marker_path(app)?;

    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|_| "iWork authorization metadata could not be removed".to_owned())?;
    }

    Ok(())
}

fn local_application_availability_for_app(
    app: &tauri::AppHandle,
) -> LocalApplicationAvailabilityReport {
    let mut availability = local_application_availability();

    let installed = installed_iwork_count(&availability.applications);

    if installed == 0 {
        availability.iwork_state = UnifiedConnectionState::AppNotInstalled;
        return availability;
    }

    let previously_verified = iwork_was_previously_verified(app);

    let probe_succeeded =
        previously_verified && probe_iwork_authorization(&availability.applications).is_ok();

    availability.iwork_state =
        resolved_iwork_state(installed, previously_verified, probe_succeeded);

    availability
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserLoginSession {
    provider_id: String,
    profile_ref: String,
    started_at: String,
    last_verified_at: Option<String>,
    state: UnifiedConnectionState,
}

fn browser_profiles_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("browser-profiles.json"))
        .map_err(|_| "Browser profile storage is unavailable".to_owned())
}

fn load_browser_profiles(
    app: &tauri::AppHandle,
) -> Result<BTreeMap<String, BrowserLoginSession>, String> {
    let path = browser_profiles_path(app)?;
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes =
        fs::read(path).map_err(|_| "Browser profile metadata could not be read".to_owned())?;
    serde_json::from_slice(&bytes).map_err(|_| "Browser profile metadata is invalid".to_owned())
}

fn save_browser_profile(
    app: &tauri::AppHandle,
    session: &BrowserLoginSession,
) -> Result<(), String> {
    let path = browser_profiles_path(app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Browser profile storage is unavailable".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|_| "Browser profile storage could not be created".to_owned())?;
    let mut profiles = load_browser_profiles(app)?;
    profiles.insert(session.provider_id.clone(), session.clone());
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&profiles)
        .map_err(|_| "Browser profile metadata could not be encoded".to_owned())?;
    fs::write(&temporary, bytes)
        .map_err(|_| "Browser profile metadata could not be saved".to_owned())?;
    fs::rename(temporary, path)
        .map_err(|_| "Browser profile metadata could not be committed".to_owned())
}

fn remove_browser_profile(app: &tauri::AppHandle, provider_id: &str) -> Result<bool, String> {
    let path = browser_profiles_path(app)?;
    let mut profiles = load_browser_profiles(app)?;
    let removed = profiles.remove(provider_id).is_some();
    if !removed {
        return Ok(false);
    }
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&profiles)
        .map_err(|_| "Browser profile metadata could not be encoded".to_owned())?;
    fs::write(&temporary, bytes)
        .map_err(|_| "Browser profile metadata could not be saved".to_owned())?;
    fs::rename(temporary, path)
        .map_err(|_| "Browser profile metadata could not be committed".to_owned())?;
    Ok(true)
}

impl BrowserLoginSession {
    fn waiting(provider_id: &str, profile_ref: &str) -> Result<Self, String> {
        if provider_id.trim().is_empty()
            || profile_ref.trim().is_empty()
            || profile_ref.contains(['/', '\\'])
        {
            return Err("Browser login session reference is invalid".to_owned());
        }
        Ok(Self {
            provider_id: provider_id.to_owned(),
            profile_ref: profile_ref.to_owned(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_verified_at: None,
            state: UnifiedConnectionState::WaitingForUser,
        })
    }

    fn apply_verification(&mut self, verified: bool) {
        if verified {
            self.last_verified_at = Some(chrono::Utc::now().to_rfc3339());
            self.state = UnifiedConnectionState::Connected;
        } else {
            self.state = UnifiedConnectionState::WaitingForUser;
        }
    }
}

fn capabilities() -> Vec<ConnectionCapability> {
    vec![
        ConnectionCapability {
            provider_id: ConnectionProvider::Microsoft.id().to_owned(),
            display_name: "Microsoft".to_owned(),
            method: ConnectionMethod::OfficialOAuth,
            state: UnifiedConnectionState::NotConfigured,
            login_url: None,
            profile_ref: None,
            developer_approval_required: false,
        },
        ConnectionCapability {
            provider_id: ConnectionProvider::GoogleWorkspace.id().to_owned(),
            display_name: "Google Workspace".to_owned(),
            method: ConnectionMethod::OfficialOAuth,
            state: UnifiedConnectionState::NotConfigured,
            login_url: None,
            profile_ref: None,
            developer_approval_required: false,
        },
        ConnectionCapability {
            provider_id: ConnectionProvider::Wps.id().to_owned(),
            display_name: "WPS Office".to_owned(),
            method: ConnectionMethod::BackendBroker,
            state: UnifiedConnectionState::BackendBrokerRequired,
            login_url: None,
            profile_ref: None,
            developer_approval_required: false,
        },
        ConnectionCapability {
            provider_id: ConnectionProvider::Ebay.id().to_owned(),
            display_name: "eBay".to_owned(),
            method: ConnectionMethod::BackendBroker,
            state: UnifiedConnectionState::NotConfigured,
            login_url: None,
            profile_ref: None,
            developer_approval_required: false,
        },
        browser_capability(
            ConnectionProvider::Amazon,
            "Amazon",
            "https://www.amazon.com/ap/signin",
        ),
        ConnectionCapability {
            provider_id: ConnectionProvider::AppleIwork.id().to_owned(),
            display_name: "Apple iWork".to_owned(),
            method: ConnectionMethod::NativeApplication,
            state: local_application_availability().iwork_state,
            login_url: None,
            profile_ref: None,
            developer_approval_required: false,
        },
        browser_capability(
            ConnectionProvider::Taobao,
            "Taobao",
            "https://login.taobao.com/",
        ),
        browser_capability(
            ConnectionProvider::Jd,
            "JD",
            "https://passport.jd.com/new/login.aspx",
        ),
        browser_capability(
            ConnectionProvider::Pinduoduo,
            "Pinduoduo",
            "https://mobile.yangkeduo.com/login.html",
        ),
    ]
}

fn browser_capability(
    provider: ConnectionProvider,
    name: &str,
    login_url: &str,
) -> ConnectionCapability {
    ConnectionCapability {
        provider_id: provider.id().to_owned(),
        display_name: name.to_owned(),
        method: ConnectionMethod::AuthenticatedBrowser,
        state: UnifiedConnectionState::Disconnected,
        login_url: Some(login_url.to_owned()),
        profile_ref: Some(format!("browser-profile:{}", provider.id())),
        developer_approval_required: false,
    }
}

#[tauri::command]
pub(crate) fn list_connection_capabilities() -> Vec<ConnectionCapability> {
    capabilities()
}

#[tauri::command]
pub(crate) fn rescan_local_application_availability(
    app: tauri::AppHandle,
) -> LocalApplicationAvailabilityReport {
    local_application_availability_for_app(&app)
}

#[tauri::command]
pub(crate) fn connect_apple_iwork(
    app: tauri::AppHandle,
) -> Result<LocalApplicationAvailabilityReport, String> {
    let mut availability = local_application_availability();

    if installed_iwork_count(&availability.applications) == 0 {
        return Err("Apple iWork is not installed".to_owned());
    }

    probe_iwork_authorization(&availability.applications)?;
    save_iwork_verified_marker(&app)?;

    availability.iwork_state = UnifiedConnectionState::Connected;

    Ok(availability)
}

#[tauri::command]
pub(crate) fn begin_browser_login(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<BrowserLoginSession, String> {
    let capability = capabilities()
        .into_iter()
        .find(|item| item.provider_id == provider_id)
        .ok_or_else(|| "Connection Provider is unknown".to_owned())?;
    if capability.method != ConnectionMethod::AuthenticatedBrowser {
        return Err("This Provider does not use Authenticated Browser login".to_owned());
    }
    let session = BrowserLoginSession::waiting(
        &capability.provider_id,
        capability
            .profile_ref
            .as_deref()
            .ok_or_else(|| "Browser profile is unavailable".to_owned())?,
    )?;
    save_browser_profile(&app, &session)?;
    Ok(session)
}

#[tauri::command]
pub(crate) fn verify_browser_login(
    app: tauri::AppHandle,
    mut session: BrowserLoginSession,
) -> Result<BrowserLoginSession, String> {
    // The current MCP Browser bridge has no safe account-state detector. Fail
    // closed until a configured provider returns an account marker from an
    // authenticated endpoint; opening a login URL is never sufficient.
    session.apply_verification(false);
    save_browser_profile(&app, &session)?;
    Ok(session)
}

#[tauri::command]
pub(crate) fn disconnect_connection_provider(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<bool, String> {
    if provider_id == ConnectionProvider::AppleIwork.id() {
        remove_iwork_verified_marker(&app)?;
        return Ok(true);
    }

    let capability = capabilities()
        .into_iter()
        .find(|item| item.provider_id == provider_id)
        .ok_or_else(|| "Connection Provider is unknown".to_owned())?;
    if capability.method == ConnectionMethod::AuthenticatedBrowser {
        return remove_browser_profile(&app, &capability.provider_id);
    }
    Ok(true)
}

#[derive(Debug, Clone)]
struct ConnectAllOnboarding {
    providers: Vec<ConnectionProvider>,
    current: usize,
    connected: Vec<ConnectionProvider>,
    skipped: Vec<ConnectionProvider>,
}

impl ConnectAllOnboarding {
    fn new() -> Self {
        Self {
            providers: vec![
                ConnectionProvider::Microsoft,
                ConnectionProvider::GoogleWorkspace,
                ConnectionProvider::Wps,
                ConnectionProvider::Ebay,
                ConnectionProvider::Amazon,
                ConnectionProvider::Taobao,
                ConnectionProvider::Jd,
                ConnectionProvider::Pinduoduo,
                ConnectionProvider::AppleIwork,
            ],
            current: 0,
            connected: Vec::new(),
            skipped: Vec::new(),
        }
    }

    fn verified(&mut self) {
        if let Some(provider) = self.providers.get(self.current).copied() {
            if !self.connected.contains(&provider) {
                self.connected.push(provider);
            }
            self.current += 1;
        }
    }

    fn skip(&mut self) {
        if let Some(provider) = self.providers.get(self.current).copied() {
            self.skipped.push(provider);
            self.current += 1;
        }
    }

    fn fail(&mut self) {
        // A later failure intentionally preserves earlier successful accounts.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iwork_connection_requires_prior_verification_and_live_probe() {
        assert_eq!(
            resolved_iwork_state(0, false, false),
            UnifiedConnectionState::AppNotInstalled
        );

        assert_eq!(
            resolved_iwork_state(3, false, false),
            UnifiedConnectionState::AuthorizationRequired
        );

        assert_eq!(
            resolved_iwork_state(3, true, false),
            UnifiedConnectionState::AuthorizationRequired
        );

        assert_eq!(
            resolved_iwork_state(3, true, true),
            UnifiedConnectionState::Connected
        );
    }

    #[test]
    fn iwork_bundle_mapping_is_explicit() {
        assert_eq!(iwork_bundle_id("pages"), Some("com.apple.Pages"));
        assert_eq!(iwork_bundle_id("numbers"), Some("com.apple.Numbers"));
        assert_eq!(iwork_bundle_id("keynote"), Some("com.apple.Keynote"));
        assert_eq!(iwork_bundle_id("wps-office"), None);
    }

    #[test]
    fn connect_all_order_skip_and_failure_preserve_success() {
        let mut flow = ConnectAllOnboarding::new();
        assert_eq!(flow.providers[flow.current], ConnectionProvider::Microsoft);
        flow.verified();
        flow.skip();
        assert_eq!(flow.providers[flow.current], ConnectionProvider::Wps);
        flow.fail();
        assert_eq!(flow.connected, vec![ConnectionProvider::Microsoft]);
        assert_eq!(flow.skipped, vec![ConnectionProvider::GoogleWorkspace]);
    }

    #[test]
    fn browser_opening_never_means_connected() {
        let session =
            BrowserLoginSession::waiting("amazon-consumer", "browser-profile:amazon-consumer")
                .unwrap();
        assert_eq!(session.state, UnifiedConnectionState::WaitingForUser);
        assert_ne!(session.state, UnifiedConnectionState::Connected);
    }

    #[test]
    fn browser_session_serialization_contains_no_credentials() {
        let session =
            BrowserLoginSession::waiting("taobao-consumer", "browser-profile:taobao-consumer")
                .unwrap();
        let value = serde_json::to_value(session).unwrap();
        assert!(value.get("password").is_none());
        assert!(value.get("cookie").is_none());
        assert!(value.get("token").is_none());
    }

    #[test]
    fn expired_provider_can_enter_waiting_reconnect() {
        let mut session =
            BrowserLoginSession::waiting("jd-consumer", "browser-profile:jd-consumer").unwrap();
        session.state = UnifiedConnectionState::Expired;
        let reconnect =
            BrowserLoginSession::waiting(&session.provider_id, &session.profile_ref).unwrap();
        assert_eq!(reconnect.state, UnifiedConnectionState::WaitingForUser);
    }

    #[test]
    fn local_app_availability_is_recomputed_and_iwork_is_component_accurate() {
        let none = local_application_availability_with(|_| false, |_| false);
        assert_eq!(none.iwork_state, UnifiedConnectionState::AppNotInstalled);
        assert!(none
            .applications
            .iter()
            .all(|application| !application.installed));

        let partial = local_application_availability_with(
            |path| {
                matches!(
                    path,
                    "/Applications/Pages.app"
                        | "/Applications/WPS Office.app"
                        | "/Applications/Microsoft Excel.app"
                )
            },
            |_| false,
        );
        assert_eq!(
            partial.iwork_state,
            UnifiedConnectionState::AuthorizationRequired
        );
        assert!(
            partial
                .applications
                .iter()
                .find(|app| app.application_id == "pages")
                .unwrap()
                .installed
        );
        assert!(
            !partial
                .applications
                .iter()
                .find(|app| app.application_id == "numbers")
                .unwrap()
                .installed
        );
        assert!(
            !partial
                .applications
                .iter()
                .find(|app| app.application_id == "keynote")
                .unwrap()
                .installed
        );
        assert!(
            partial
                .applications
                .iter()
                .find(|app| app.application_id == "wps-office")
                .unwrap()
                .installed
        );
        assert!(
            partial
                .applications
                .iter()
                .find(|app| app.application_id == "microsoft-excel")
                .unwrap()
                .installed
        );

        let changed = local_application_availability_with(
            |_| false,
            |bundle_id| bundle_id == "com.apple.Numbers",
        );
        assert!(
            !changed
                .applications
                .iter()
                .find(|app| app.application_id == "pages")
                .unwrap()
                .installed
        );
        assert!(
            changed
                .applications
                .iter()
                .find(|app| app.application_id == "numbers")
                .unwrap()
                .installed
        );
    }
}

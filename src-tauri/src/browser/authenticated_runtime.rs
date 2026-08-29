//! Persistent Authenticated Browser runtime — BROWSER-A foundation.
//!
//! This layer owns an AI-OS managed Chromium-family browser process, a
//! dedicated browser profile stored beneath the Tauri application data
//! directory, and a loopback-only DevTools control channel.
//!
//! Deliberate boundaries:
//!
//! - the generic MCP Runtime stays stateless; this runtime is the only owner
//!   of persistent authenticated browser sessions;
//! - the user's ordinary Chrome/Safari profile is never adopted or reused;
//! - only processes this runtime spawned are ever terminated;
//! - a running browser is never evidence of an authenticated account.
//!   Real provider account verification is BROWSER-B; until then every
//!   session reports `authenticated: false` and cannot become `CONNECTED`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::provider_selection::AuthorizationRef;
use tauri::Manager;

/// Directory beneath the application data directory that holds every managed
/// browser profile. Never the user's own browser profile root.
const MANAGED_BROWSER_ROOT: &str = "authenticated-browser";
const MANAGED_PROFILE_ROOT: &str = "profiles";
const DEVTOOLS_ACTIVE_PORT_FILE: &str = "DevToolsActivePort";
const READINESS_TIMEOUT: Duration = Duration::from_secs(25);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);
const CONTROL_CHANNEL_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Supported browsers and deterministic discovery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ManagedBrowserKind {
    GoogleChrome,
    Chromium,
    MicrosoftEdge,
    BraveBrowser,
}

impl ManagedBrowserKind {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::GoogleChrome => "google-chrome",
            Self::Chromium => "chromium",
            Self::MicrosoftEdge => "microsoft-edge",
            Self::BraveBrowser => "brave-browser",
        }
    }

    /// macOS executable path for the supported Chromium-family browser.
    fn executable_path(self) -> &'static str {
        match self {
            Self::GoogleChrome => {
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            }
            Self::Chromium => "/Applications/Chromium.app/Contents/MacOS/Chromium",
            Self::MicrosoftEdge => {
                "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"
            }
            Self::BraveBrowser => {
                "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
            }
        }
    }
}

/// Discovery order is fixed and deterministic. It must not be reordered at
/// runtime, so repeated runs on one machine always select the same browser.
const MACOS_DISCOVERY_ORDER: [ManagedBrowserKind; 4] = [
    ManagedBrowserKind::GoogleChrome,
    ManagedBrowserKind::Chromium,
    ManagedBrowserKind::MicrosoftEdge,
    ManagedBrowserKind::BraveBrowser,
];

fn discover_supported_browser_with(
    executable_exists: impl Fn(&str) -> bool,
) -> Option<ManagedBrowserKind> {
    MACOS_DISCOVERY_ORDER
        .into_iter()
        .find(|kind| executable_exists(kind.executable_path()))
}

fn discover_supported_browser() -> Option<ManagedBrowserKind> {
    discover_supported_browser_with(|path| Path::new(path).exists())
}

// ---------------------------------------------------------------------------
// Managed profile location
// ---------------------------------------------------------------------------

/// Provider ids are used as a directory name, so they are restricted to a
/// conservative character set. Fail-closed on anything else.
fn validate_provider_id(provider_id: &str) -> Result<&str, String> {
    let trimmed = provider_id.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("Managed browser provider id is invalid".to_owned());
    }
    Ok(trimmed)
}

/// `<app data>/authenticated-browser/profiles/<provider id>`.
fn managed_profile_directory(
    app_data_dir: &Path,
    provider_id: &str,
) -> Result<PathBuf, String> {
    let provider_id = validate_provider_id(provider_id)?;
    Ok(app_data_dir
        .join(MANAGED_BROWSER_ROOT)
        .join(MANAGED_PROFILE_ROOT)
        .join(provider_id))
}

/// True only when the profile lives beneath the AI-OS managed browser root.
/// The user's own Chrome/Edge/Brave/Safari profiles can never satisfy this.
fn profile_is_ai_os_owned(app_data_dir: &Path, profile_directory: &Path) -> bool {
    profile_directory.starts_with(app_data_dir.join(MANAGED_BROWSER_ROOT))
}

/// Outside this module a profile is only ever identified by an opaque
/// reference. The raw filesystem path never leaves the runtime.
fn opaque_session_ref(provider_id: &str) -> AuthorizationRef {
    AuthorizationRef::opaque(format!("browser-profile:{provider_id}"))
}

// ---------------------------------------------------------------------------
// Loopback-only control channel
// ---------------------------------------------------------------------------

/// Fail-closed check that a DevTools control endpoint is loopback-only.
fn is_loopback_control_endpoint(endpoint: &str) -> bool {
    let Some(remainder) = endpoint.strip_prefix("http://") else {
        return false;
    };
    let authority = remainder.split('/').next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = authority.rsplit_once(':').map(|(host, _)| host).unwrap_or(authority);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

fn control_endpoint(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Launch arguments always pin the managed profile and a loopback debugging
/// address, and never point at a browser profile AI-OS does not own.
fn launch_arguments(profile_directory: &Path) -> Vec<String> {
    vec![
        format!("--user-data-dir={}", profile_directory.display()),
        "--remote-debugging-port=0".to_owned(),
        "--remote-debugging-address=127.0.0.1".to_owned(),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
    ]
}

fn parse_devtools_active_port(contents: &str) -> Option<u16> {
    contents
        .lines()
        .next()?
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
}

fn devtools_response_is_ready(response: &str) -> bool {
    response.starts_with("HTTP/1.1 200") && response.contains("webSocketDebuggerUrl")
}

fn probe_control_channel(port: u16) -> Result<String, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, CONTROL_CHANNEL_TIMEOUT)
        .map_err(|_| "Managed browser control channel is not reachable".to_owned())?;
    let _ = stream.set_read_timeout(Some(CONTROL_CHANNEL_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONTROL_CHANNEL_TIMEOUT));
    stream
        .write_all(b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .map_err(|_| "Managed browser control channel refused the request".to_owned())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| "Managed browser control channel returned no response".to_owned())?;
    Ok(response)
}

/// Readiness is only reported after the browser has published its DevTools
/// port and the loopback control channel actually answers.
fn wait_until_ready(profile_directory: &Path) -> Result<u16, String> {
    let port_file = profile_directory.join(DEVTOOLS_ACTIVE_PORT_FILE);
    let deadline = Instant::now() + READINESS_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(port) = std::fs::read_to_string(&port_file)
            .ok()
            .as_deref()
            .and_then(parse_devtools_active_port)
        {
            let endpoint = control_endpoint(port);
            if is_loopback_control_endpoint(&endpoint) {
                if let Ok(response) = probe_control_channel(port) {
                    if devtools_response_is_ready(&response) {
                        return Ok(port);
                    }
                }
            }
        }
        std::thread::sleep(READINESS_POLL_INTERVAL);
    }
    Err("Managed browser did not become ready".to_owned())
}

// ---------------------------------------------------------------------------
// Provider / origin validation (region aware, fail-closed)
// ---------------------------------------------------------------------------

/// Amazon is explicitly not bound to one country. The verified origin is
/// whatever regional Amazon site the user actually signed in to.
const AMAZON_HOSTS: [&str; 9] = [
    "www.amazon.com",
    "www.amazon.com.au",
    "www.amazon.co.jp",
    "www.amazon.co.uk",
    "www.amazon.de",
    "www.amazon.fr",
    "www.amazon.it",
    "www.amazon.ca",
    "www.amazon.in",
];

const TAOBAO_HOSTS: [&str; 4] = [
    "www.taobao.com",
    "login.taobao.com",
    "i.taobao.com",
    "world.taobao.com",
];

const JD_HOSTS: [&str; 3] = ["www.jd.com", "passport.jd.com", "order.jd.com"];

const PINDUODUO_HOSTS: [&str; 3] = [
    "mobile.yangkeduo.com",
    "www.yangkeduo.com",
    "www.pinduoduo.com",
];

fn provider_hosts(provider_id: &str) -> Option<&'static [&'static str]> {
    match provider_id {
        "amazon-consumer" => Some(&AMAZON_HOSTS),
        "taobao-consumer" => Some(&TAOBAO_HOSTS),
        "jd-consumer" => Some(&JD_HOSTS),
        "pinduoduo-consumer" => Some(&PINDUODUO_HOSTS),
        _ => None,
    }
}

/// Fail-closed origin check consumed by the BROWSER-B provider verifiers.
/// HTTPS is required and the host must belong to the expected provider.
#[allow(dead_code)]
pub(crate) fn origin_belongs_to_provider(provider_id: &str, origin: &str) -> bool {
    let Some(hosts) = provider_hosts(provider_id) else {
        return false;
    };
    let Some(remainder) = origin.strip_prefix("https://") else {
        return false;
    };
    let authority = remainder.split('/').next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = authority
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(authority);
    hosts.contains(&host)
}

// ---------------------------------------------------------------------------
// Safe session metadata
// ---------------------------------------------------------------------------

/// Everything this runtime is allowed to expose outside itself.
///
/// Deliberately absent: raw profile path, control-channel port, cookies,
/// passwords, bearer tokens and authentication headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManagedBrowserSession {
    pub provider_id: String,
    pub browser_kind: ManagedBrowserKind,
    pub session_ref: AuthorizationRef,
    pub running: bool,
    pub ready: bool,
    pub profile_reused: bool,
    /// Always false in BROWSER-A. Only a real BROWSER-B account verifier may
    /// ever set this.
    pub authenticated: bool,
    pub verified_origin: Option<String>,
    pub account_marker: Option<String>,
    pub started_at: String,
    pub last_inspected_at: Option<String>,
}

impl ManagedBrowserSession {
    fn managed(
        provider_id: &str,
        browser_kind: ManagedBrowserKind,
        running: bool,
        ready: bool,
        profile_reused: bool,
        started_at: String,
    ) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            browser_kind,
            session_ref: opaque_session_ref(provider_id),
            running,
            ready,
            profile_reused,
            authenticated: false,
            verified_origin: None,
            account_marker: None,
            started_at,
            last_inspected_at: None,
        }
    }

    /// A managed browser being open, ready, or reusing a persisted profile is
    /// never sufficient evidence of an authenticated account.
    pub(crate) fn permits_connected_state(&self) -> bool {
        self.authenticated && self.verified_origin.is_some() && self.account_marker.is_some()
    }
}

// ---------------------------------------------------------------------------
// Owned process registry
// ---------------------------------------------------------------------------

struct OwnedBrowserProcess {
    child: Child,
    browser_kind: ManagedBrowserKind,
    started_at: String,
    ready: bool,
}

fn registry() -> &'static Mutex<BTreeMap<String, OwnedBrowserProcess>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<String, OwnedBrowserProcess>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Termination only ever considers processes this runtime spawned and
/// recorded. An unknown provider yields nothing, so no unrelated user browser
/// process can be selected.
fn take_owned_process<T>(
    owned: &mut BTreeMap<String, T>,
    provider_id: &str,
) -> Option<T> {
    owned.remove(provider_id)
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn app_data_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|_| "Managed browser storage is unavailable".to_owned())
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Open (or reuse) the AI-OS managed browser for a provider.
///
/// This never marks a commerce account as connected.
#[tauri::command]
pub(crate) fn open_authenticated_browser(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ManagedBrowserSession, String> {
    let provider_id = validate_provider_id(&provider_id)?.to_owned();
    let app_data_dir = app_data_directory(&app)?;
    let profile_directory = managed_profile_directory(&app_data_dir, &provider_id)?;

    if !profile_is_ai_os_owned(&app_data_dir, &profile_directory) {
        return Err("Managed browser profile is outside AI-OS application data".to_owned());
    }

    let mut owned = registry()
        .lock()
        .map_err(|_| "Managed browser runtime is unavailable".to_owned())?;

    let still_running = match owned.get_mut(&provider_id) {
        Some(existing) => match existing.child.try_wait() {
            Ok(None) => Some((
                existing.browser_kind,
                existing.ready,
                existing.started_at.clone(),
            )),
            _ => None,
        },
        None => None,
    };

    if let Some((browser_kind, ready, started_at)) = still_running {
        return Ok(ManagedBrowserSession::managed(
            &provider_id,
            browser_kind,
            true,
            ready,
            true,
            started_at,
        ));
    }
    owned.remove(&provider_id);

    let profile_reused = profile_directory.join(DEVTOOLS_ACTIVE_PORT_FILE).exists()
        || profile_directory.join("Default").exists();

    std::fs::create_dir_all(&profile_directory)
        .map_err(|_| "Managed browser profile could not be created".to_owned())?;
    // A stale port file would otherwise be read as this launch's port.
    let _ = std::fs::remove_file(profile_directory.join(DEVTOOLS_ACTIVE_PORT_FILE));

    let browser_kind = discover_supported_browser()
        .ok_or_else(|| "No supported managed browser was found".to_owned())?;

    let child = Command::new(browser_kind.executable_path())
        .args(launch_arguments(&profile_directory))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Managed browser could not be started".to_owned())?;

    let started_at = now_rfc3339();
    let mut process = OwnedBrowserProcess {
        child,
        browser_kind,
        started_at: started_at.clone(),
        ready: false,
    };

    match wait_until_ready(&profile_directory) {
        Ok(_) => process.ready = true,
        Err(error) => {
            let _ = process.child.kill();
            let _ = process.child.wait();
            return Err(error);
        }
    }

    owned.insert(provider_id.clone(), process);

    Ok(ManagedBrowserSession::managed(
        &provider_id,
        browser_kind,
        true,
        true,
        profile_reused,
        started_at,
    ))
}

/// Report the live state of the managed browser without asserting anything
/// about the website account.
#[tauri::command]
pub(crate) fn inspect_authenticated_browser(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<Option<ManagedBrowserSession>, String> {
    let provider_id = validate_provider_id(&provider_id)?.to_owned();
    let app_data_dir = app_data_directory(&app)?;
    let profile_directory = managed_profile_directory(&app_data_dir, &provider_id)?;

    let mut owned = registry()
        .lock()
        .map_err(|_| "Managed browser runtime is unavailable".to_owned())?;

    let observed = match owned.get_mut(&provider_id) {
        Some(process) => {
            let running = matches!(process.child.try_wait(), Ok(None));
            (
                process.browser_kind,
                running,
                running && process.ready,
                process.started_at.clone(),
            )
        }
        None => return Ok(None),
    };

    let (browser_kind, running, ready, started_at) = observed;

    if !running {
        owned.remove(&provider_id);
    }

    let mut session = ManagedBrowserSession::managed(
        &provider_id,
        browser_kind,
        running,
        ready,
        profile_directory.exists(),
        started_at,
    );
    session.last_inspected_at = Some(now_rfc3339());

    Ok(Some(session))
}

/// Close only the managed browser process AI-OS started for this provider.
/// The persisted profile is kept so website sessions survive a restart.
#[tauri::command]
pub(crate) fn close_authenticated_browser(
    provider_id: String,
) -> Result<bool, String> {
    let provider_id = validate_provider_id(&provider_id)?.to_owned();
    let mut owned = registry()
        .lock()
        .map_err(|_| "Managed browser runtime is unavailable".to_owned())?;

    let Some(mut process) = take_owned_process(&mut owned, &provider_id) else {
        return Ok(false);
    };

    let _ = process.child.kill();
    let _ = process.child.wait();
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_data() -> PathBuf {
        PathBuf::from("/Users/example/Library/Application Support/com.ai-os.dashboard")
    }

    #[test]
    fn browser_discovery_order_is_deterministic() {
        assert_eq!(
            MACOS_DISCOVERY_ORDER,
            [
                ManagedBrowserKind::GoogleChrome,
                ManagedBrowserKind::Chromium,
                ManagedBrowserKind::MicrosoftEdge,
                ManagedBrowserKind::BraveBrowser,
            ]
        );
        assert_eq!(
            discover_supported_browser_with(|_| true),
            Some(ManagedBrowserKind::GoogleChrome)
        );
        assert_eq!(
            discover_supported_browser_with(|path| path.contains("Brave Browser")),
            Some(ManagedBrowserKind::BraveBrowser)
        );
        assert_eq!(
            discover_supported_browser_with(|path| path.contains("Microsoft Edge")
                || path.contains("Brave Browser")),
            Some(ManagedBrowserKind::MicrosoftEdge)
        );
        assert_eq!(discover_supported_browser_with(|_| false), None);
    }

    #[test]
    fn managed_profile_lives_under_app_data_and_never_in_the_user_browser_profile() {
        let profile = managed_profile_directory(&app_data(), "amazon-consumer").unwrap();
        assert!(profile.starts_with(app_data()));
        assert!(profile.ends_with("authenticated-browser/profiles/amazon-consumer"));
        assert!(profile_is_ai_os_owned(&app_data(), &profile));

        let user_chrome =
            PathBuf::from("/Users/example/Library/Application Support/Google/Chrome/Default");
        assert!(!profile_is_ai_os_owned(&app_data(), &user_chrome));
        let user_safari = PathBuf::from("/Users/example/Library/Safari");
        assert!(!profile_is_ai_os_owned(&app_data(), &user_safari));

        assert!(managed_profile_directory(&app_data(), "../../Google/Chrome").is_err());
        assert!(managed_profile_directory(&app_data(), "amazon/consumer").is_err());
        assert!(managed_profile_directory(&app_data(), "  ").is_err());
    }

    #[test]
    fn session_reference_is_opaque_and_carries_no_credentials_or_raw_path() {
        let session = ManagedBrowserSession::managed(
            "amazon-consumer",
            ManagedBrowserKind::GoogleChrome,
            true,
            true,
            true,
            "2026-08-29T00:00:00+00:00".to_owned(),
        );
        let value = serde_json::to_value(&session).unwrap();
        let encoded = serde_json::to_string(&value).unwrap();

        assert_eq!(value["sessionRef"], "browser-profile:amazon-consumer");
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains("Library"));
        assert!(!encoded.contains("user-data-dir"));
        for forbidden in ["password", "cookie", "token", "authorization", "port"] {
            assert!(
                !encoded.to_lowercase().contains(forbidden),
                "session metadata leaked {forbidden}"
            );
        }
    }

    #[test]
    fn running_or_reused_managed_browser_is_never_authenticated() {
        let session = ManagedBrowserSession::managed(
            "taobao-consumer",
            ManagedBrowserKind::GoogleChrome,
            true,
            true,
            true,
            "2026-08-29T00:00:00+00:00".to_owned(),
        );
        assert!(session.running);
        assert!(session.ready);
        assert!(session.profile_reused);
        assert!(!session.authenticated);
        assert!(session.verified_origin.is_none());
        assert!(session.account_marker.is_none());
        assert!(!session.permits_connected_state());
    }

    #[test]
    fn control_channel_is_loopback_only_and_readiness_requires_a_real_answer() {
        assert!(is_loopback_control_endpoint("http://127.0.0.1:51321"));
        assert!(is_loopback_control_endpoint("http://localhost:51321/json/version"));
        assert!(!is_loopback_control_endpoint("http://10.0.0.4:51321"));
        assert!(!is_loopback_control_endpoint("http://127.0.0.1@evil.example:80"));
        assert!(!is_loopback_control_endpoint("https://example.com:51321"));
        assert!(is_loopback_control_endpoint(&control_endpoint(51321)));

        assert_eq!(parse_devtools_active_port("51321\n/devtools/browser/x"), Some(51321));
        assert_eq!(parse_devtools_active_port("0\n"), None);
        assert_eq!(parse_devtools_active_port(""), None);

        assert!(devtools_response_is_ready(
            "HTTP/1.1 200 OK\r\n\r\n{\"webSocketDebuggerUrl\":\"ws://127.0.0.1:51321/devtools/browser/x\"}"
        ));
        assert!(!devtools_response_is_ready("HTTP/1.1 500 Internal Server Error\r\n\r\n"));
        assert!(!devtools_response_is_ready("HTTP/1.1 200 OK\r\n\r\n{}"));
    }

    #[test]
    fn launch_arguments_pin_the_managed_profile_and_a_loopback_debug_channel() {
        let profile = managed_profile_directory(&app_data(), "jd-consumer").unwrap();
        let arguments = launch_arguments(&profile);

        assert!(arguments
            .iter()
            .any(|argument| argument == &format!("--user-data-dir={}", profile.display())));
        assert!(arguments
            .iter()
            .any(|argument| argument == "--remote-debugging-address=127.0.0.1"));
        assert!(arguments
            .iter()
            .any(|argument| argument == "--remote-debugging-port=0"));
        for argument in &arguments {
            assert!(!argument.contains("Google/Chrome"));
            assert!(!argument.contains("BraveSoftware"));
            assert!(!argument.contains("Microsoft Edge/"));
        }
    }

    #[test]
    fn only_ai_os_owned_processes_are_ever_selected_for_termination() {
        let mut owned: BTreeMap<String, u32> = BTreeMap::new();
        owned.insert("amazon-consumer".to_owned(), 4242);
        owned.insert("taobao-consumer".to_owned(), 4243);

        assert_eq!(take_owned_process(&mut owned, "unknown-provider"), None);
        assert_eq!(owned.len(), 2);

        assert_eq!(take_owned_process(&mut owned, "amazon-consumer"), Some(4242));
        assert_eq!(owned.len(), 1);
        assert_eq!(owned.get("taobao-consumer"), Some(&4243));

        assert_eq!(take_owned_process(&mut owned, "amazon-consumer"), None);
    }

    #[test]
    fn amazon_is_region_aware_and_origin_validation_is_fail_closed() {
        for origin in [
            "https://www.amazon.com",
            "https://www.amazon.com.au",
            "https://www.amazon.co.jp",
            "https://www.amazon.co.uk",
            "https://www.amazon.de",
        ] {
            assert!(
                origin_belongs_to_provider("amazon-consumer", origin),
                "{origin} should be a valid Amazon origin"
            );
        }

        assert!(!origin_belongs_to_provider("amazon-consumer", "http://www.amazon.com"));
        assert!(!origin_belongs_to_provider(
            "amazon-consumer",
            "https://www.amazon.com.evil.example"
        ));
        assert!(!origin_belongs_to_provider(
            "amazon-consumer",
            "https://www.amazon.com@evil.example"
        ));
        assert!(!origin_belongs_to_provider("amazon-consumer", "https://www.taobao.com"));
        assert!(!origin_belongs_to_provider("unknown-provider", "https://www.amazon.com"));

        assert!(origin_belongs_to_provider("taobao-consumer", "https://login.taobao.com"));
        assert!(origin_belongs_to_provider("jd-consumer", "https://passport.jd.com"));
        assert!(origin_belongs_to_provider(
            "pinduoduo-consumer",
            "https://mobile.yangkeduo.com"
        ));
    }
}

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
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::devtools;
use crate::provider_selection::AuthorizationRef;
use tauri::Manager;

/// Directory beneath the application data directory that holds every managed
/// browser profile. Never the user's own browser profile root.
const MANAGED_BROWSER_ROOT: &str = "authenticated-browser";
const MANAGED_PROFILE_ROOT: &str = "profiles";
const DEVTOOLS_ACTIVE_PORT_FILE: &str = "DevToolsActivePort";
const READINESS_TIMEOUT: Duration = Duration::from_secs(25);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// How long a managed browser is given to close itself after being asked.
/// Only a graceful exit lets Chromium release the profile's Singleton lock.
const GRACEFUL_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const GRACEFUL_CLOSE_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
            Self::GoogleChrome => "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            Self::Chromium => "/Applications/Chromium.app/Contents/MacOS/Chromium",
            Self::MicrosoftEdge => "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            Self::BraveBrowser => "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
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
fn managed_profile_directory(app_data_dir: &Path, provider_id: &str) -> Result<PathBuf, String> {
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
    let host = authority
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(authority);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

fn control_endpoint(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Launch arguments always pin the managed profile and a loopback debugging
/// address, and never point at a browser profile AI-OS does not own.
fn launch_arguments(profile_directory: &Path, initial_url: Option<&str>) -> Vec<String> {
    let mut arguments = vec![
        format!("--user-data-dir={}", profile_directory.display()),
        "--remote-debugging-port=0".to_owned(),
        "--remote-debugging-address=127.0.0.1".to_owned(),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
    ];
    // Only an https destination may be handed to the managed browser.
    if let Some(url) = initial_url.filter(|url| url.starts_with("https://")) {
        arguments.push(url.to_owned());
    }
    arguments
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
    let status_ok = response
        .lines()
        .next()
        .is_some_and(|status| status.contains(" 200 "));

    if !status_ok {
        return false;
    }

    devtools::parse_browser_websocket_url(response)
        .ok()
        .and_then(|url| devtools::loopback_websocket_port(&url))
        .is_some()
}

fn probe_control_channel(port: u16) -> Result<String, String> {
    devtools::http_get(port, "/json/version")
}

/// Port a browser is currently publishing for a profile, if any.
fn published_devtools_port(port_file_contents: Option<&str>) -> Option<u16> {
    port_file_contents.and_then(parse_devtools_active_port)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadinessProgress {
    Ready(u16),
    OwnerExited,
    KeepWaiting,
}

/// One readiness decision, kept pure so the launch race stays testable.
///
/// Chromium hands a launch off to an instance that already owns the profile
/// and then exits immediately. Waiting out the full readiness timeout would
/// report that as a generic timeout and hide the real cause.
fn readiness_progress(answered_port: Option<u16>, owned_process_exited: bool) -> ReadinessProgress {
    match (answered_port, owned_process_exited) {
        (Some(port), _) => ReadinessProgress::Ready(port),
        (None, true) => ReadinessProgress::OwnerExited,
        (None, false) => ReadinessProgress::KeepWaiting,
    }
}

/// The port this profile's DevTools endpoint is answering on, if a browser is
/// live on it right now.
fn profile_is_owned_by_live_browser(profile_directory: &Path) -> Option<u16> {
    let contents = std::fs::read_to_string(profile_directory.join(DEVTOOLS_ACTIVE_PORT_FILE)).ok();
    let port = published_devtools_port(contents.as_deref())?;
    if !is_loopback_control_endpoint(&control_endpoint(port)) {
        return None;
    }
    match probe_control_channel(port) {
        Ok(response) if devtools_response_is_ready(&response) => Some(port),
        _ => None,
    }
}

/// Readiness is only reported after the browser has published its DevTools
/// port and the loopback control channel actually answers.
fn wait_until_ready(profile_directory: &Path, child: &mut Child) -> Result<u16, String> {
    let port_file = profile_directory.join(DEVTOOLS_ACTIVE_PORT_FILE);
    let deadline = Instant::now() + READINESS_TIMEOUT;
    while Instant::now() < deadline {
        let answered_port =
            published_devtools_port(std::fs::read_to_string(&port_file).ok().as_deref())
                .filter(|port| is_loopback_control_endpoint(&control_endpoint(*port)))
                .filter(|port| {
                    probe_control_channel(*port)
                        .map(|response| devtools_response_is_ready(&response))
                        .unwrap_or(false)
                });

        match readiness_progress(answered_port, matches!(child.try_wait(), Ok(Some(_)))) {
            ReadinessProgress::Ready(port) => return Ok(port),
            ReadinessProgress::OwnerExited => {
                return Err("The managed browser exited before it was ready. Another browser instance is probably still using this profile.".to_owned())
            }
            ReadinessProgress::KeepWaiting => std::thread::sleep(READINESS_POLL_INTERVAL),
        }
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

/// Fail-closed origin check consumed by the provider account verifiers.
/// HTTPS is required and the host must belong to the expected provider.
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
    /// Loopback DevTools port. Internal only — it never leaves this runtime
    /// through session metadata.
    control_port: u16,
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
fn take_owned_process<T>(owned: &mut BTreeMap<String, T>, provider_id: &str) -> Option<T> {
    owned.remove(provider_id)
}

/// Take every owned entry out of the registry in one pass.
///
/// Draining first means termination happens without holding the registry lock,
/// and a second shutdown pass finds nothing left to terminate.
fn drain_owned_processes<T>(owned: &mut BTreeMap<String, T>) -> Vec<(String, T)> {
    std::mem::take(owned).into_iter().collect()
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
    open_managed_browser(&app, &provider_id, None)
}

/// Internal entry point. `initial_url` is the provider login destination the
/// managed browser should show; it is only ever handed to the AI-OS owned
/// browser, never to the user's default browser.
pub(crate) fn open_managed_browser(
    app: &tauri::AppHandle,
    provider_id: &str,
    initial_url: Option<&str>,
) -> Result<ManagedBrowserSession, String> {
    let provider_id = validate_provider_id(provider_id)?.to_owned();
    let app_data_dir = app_data_directory(app)?;
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
                existing.control_port,
                existing.started_at.clone(),
            )),
            _ => None,
        },
        None => None,
    };

    if let Some((browser_kind, ready, control_port, started_at)) = still_running {
        if let Some(url) = initial_url.filter(|url| url.starts_with("https://")) {
            // Reuse the browser AI-OS already owns instead of launching another.
            let _ = open_managed_tab(control_port, url);
        }
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

    let profile_reused = profile_directory.join("Default").exists();

    std::fs::create_dir_all(&profile_directory)
        .map_err(|_| "Managed browser profile could not be created".to_owned())?;

    // A browser still answering on this profile's DevTools port holds the
    // profile's Singleton lock. It is not in the owned registry, so it is
    // neither AI-OS's to terminate nor to adopt. Launching regardless would
    // hand the URL to that instance and exit at once, which previously
    // surfaced only as a readiness timeout 25 seconds later.
    if profile_is_owned_by_live_browser(&profile_directory).is_some() {
        return Err(
            "A previous AI-OS managed browser is still using this profile. Quit that browser window, then reconnect."
                .to_owned(),
        );
    }

    // Nothing answers, so the port file is stale and would otherwise be read
    // as this launch's port.
    let _ = std::fs::remove_file(profile_directory.join(DEVTOOLS_ACTIVE_PORT_FILE));

    let browser_kind = discover_supported_browser()
        .ok_or_else(|| "No supported managed browser was found".to_owned())?;

    let child = Command::new(browser_kind.executable_path())
        .args(launch_arguments(&profile_directory, initial_url))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Managed browser could not be started".to_owned())?;

    let started_at = now_rfc3339();
    let mut process = OwnedBrowserProcess {
        child,
        browser_kind,
        control_port: 0,
        started_at: started_at.clone(),
        ready: false,
    };

    let readiness = wait_until_ready(&profile_directory, &mut process.child);
    match readiness {
        Ok(port) => {
            process.control_port = port;
            process.ready = true;
        }
        Err(error) => {
            terminate_owned_process(&mut process);
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

/// Ask an owned managed browser to close itself, then wait for it to exit.
///
/// The request goes over the loopback DevTools channel AI-OS already owns for
/// that process. This matters beyond tidiness: a killed Chromium never releases
/// its profile `Singleton` lock, and the next AI-OS launch is then handed off
/// to the surviving instance instead of starting its own.
///
/// A kill remains the fallback so shutdown cannot hang. Returns true when the
/// browser exited on request.
fn terminate_owned_process(process: &mut OwnedBrowserProcess) -> bool {
    if process.control_port != 0 {
        if let Ok(channel) = devtools::browser_websocket_url(process.control_port) {
            // Chromium may drop the socket before answering. The reply is not
            // the evidence; the process exiting is.
            let _ = devtools::protocol_call(&channel, 1, "Browser.close", serde_json::json!({}));
        }
    }

    let deadline = Instant::now() + GRACEFUL_CLOSE_TIMEOUT;
    while Instant::now() < deadline {
        match process.child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => std::thread::sleep(GRACEFUL_CLOSE_POLL_INTERVAL),
            Err(_) => break,
        }
    }

    let _ = process.child.kill();
    let _ = process.child.wait();
    false
}

/// Close only the managed browser process AI-OS started for this provider.
/// The persisted profile is kept so website sessions survive a restart.
#[tauri::command]
pub(crate) fn close_authenticated_browser(provider_id: String) -> Result<bool, String> {
    close_managed_browser(&provider_id)
}

pub(crate) fn close_managed_browser(provider_id: &str) -> Result<bool, String> {
    let provider_id = validate_provider_id(provider_id)?.to_owned();

    // Take the entry out under the lock, then release it: closing waits on the
    // browser and must not block every other caller of the registry.
    let taken = {
        let mut owned = registry()
            .lock()
            .map_err(|_| "Managed browser runtime is unavailable".to_owned())?;
        take_owned_process(&mut owned, &provider_id)
    };

    let Some(mut process) = taken else {
        return Ok(false);
    };

    terminate_owned_process(&mut process);
    Ok(true)
}

/// Close every browser process owned by this AI-OS runtime.
///
/// Only processes recorded in the private owned-process registry are touched.
/// User browser processes and unknown processes are never inspected or killed.
/// Safe to call more than once: the first pass empties the registry, so a
/// later shutdown event finds nothing left to close.
pub(crate) fn close_all_managed_browsers() -> Result<usize, String> {
    let drained = {
        let mut owned = registry()
            .lock()
            .map_err(|_| "Managed browser runtime is unavailable".to_owned())?;
        drain_owned_processes(&mut owned)
    };

    let mut closed = 0usize;
    for (_provider_id, mut process) in drained {
        terminate_owned_process(&mut process);
        closed += 1;
    }

    Ok(closed)
}

/// Loopback DevTools port of the running managed browser AI-OS owns for this
/// provider. `None` when AI-OS owns no live browser, so account verification
/// can never inspect a browser this runtime does not own.
pub(crate) fn control_port_for(provider_id: &str) -> Option<u16> {
    let provider_id = validate_provider_id(provider_id).ok()?;
    let mut owned = registry().lock().ok()?;
    let process = owned.get_mut(provider_id)?;
    if !matches!(process.child.try_wait(), Ok(None)) || !process.ready {
        return None;
    }
    Some(process.control_port).filter(|port| *port != 0)
}

/// True when AI-OS owns a live managed browser for this provider.
pub(crate) fn managed_browser_is_running(provider_id: &str) -> bool {
    control_port_for(provider_id).is_some()
}

/// Open a provider login destination in the browser AI-OS already owns.
fn open_managed_tab(control_port: u16, url: &str) -> Result<(), String> {
    let browser_channel = devtools::browser_websocket_url(control_port)?;
    devtools::protocol_call(
        &browser_channel,
        1,
        "Target.createTarget",
        serde_json::json!({ "url": url }),
    )
    .map(|_| ())
}

/// Remove the AI-OS managed profile for this provider. Only AI-OS owned data
/// beneath application data is ever removed.
pub(crate) fn remove_managed_profile(
    app: &tauri::AppHandle,
    provider_id: &str,
) -> Result<bool, String> {
    let provider_id = validate_provider_id(provider_id)?.to_owned();
    let app_data_dir = app_data_directory(app)?;
    let profile_directory = managed_profile_directory(&app_data_dir, &provider_id)?;

    if !profile_is_ai_os_owned(&app_data_dir, &profile_directory) {
        return Err("Managed browser profile is outside AI-OS application data".to_owned());
    }
    if !profile_directory.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&profile_directory)
        .map_err(|_| "Managed browser profile could not be removed".to_owned())?;
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
            discover_supported_browser_with(
                |path| path.contains("Microsoft Edge") || path.contains("Brave Browser")
            ),
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
        assert!(is_loopback_control_endpoint(
            "http://localhost:51321/json/version"
        ));
        assert!(!is_loopback_control_endpoint("http://10.0.0.4:51321"));
        assert!(!is_loopback_control_endpoint(
            "http://127.0.0.1@evil.example:80"
        ));
        assert!(!is_loopback_control_endpoint("https://example.com:51321"));
        assert!(is_loopback_control_endpoint(&control_endpoint(51321)));

        assert_eq!(
            parse_devtools_active_port("51321\n/devtools/browser/x"),
            Some(51321)
        );
        assert_eq!(parse_devtools_active_port("0\n"), None);
        assert_eq!(parse_devtools_active_port(""), None);

        assert!(devtools_response_is_ready(
            "HTTP/1.1 200 OK\r\n\r\n{\"webSocketDebuggerUrl\":\"ws://127.0.0.1:51321/devtools/browser/x\"}"
        ));
        assert!(!devtools_response_is_ready(
            "HTTP/1.1 500 Internal Server Error\r\n\r\n"
        ));
        assert!(!devtools_response_is_ready("HTTP/1.1 200 OK\r\n\r\n{}"));
    }

    #[test]
    fn launch_arguments_pin_the_managed_profile_and_a_loopback_debug_channel() {
        let profile = managed_profile_directory(&app_data(), "jd-consumer").unwrap();
        let arguments = launch_arguments(&profile, None);

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

        // A login destination is handed to the managed browser only over https.
        let with_login = launch_arguments(&profile, Some("https://passport.jd.com/new/login.aspx"));
        assert_eq!(
            with_login.last().map(String::as_str),
            Some("https://passport.jd.com/new/login.aspx")
        );
        assert_eq!(
            launch_arguments(&profile, Some("http://passport.jd.com/new/login.aspx")),
            arguments
        );
        assert_eq!(
            launch_arguments(&profile, Some("file:///etc/passwd")),
            arguments
        );
    }

    #[test]
    fn only_ai_os_owned_processes_are_ever_selected_for_termination() {
        let mut owned: BTreeMap<String, u32> = BTreeMap::new();
        owned.insert("amazon-consumer".to_owned(), 4242);
        owned.insert("taobao-consumer".to_owned(), 4243);

        assert_eq!(take_owned_process(&mut owned, "unknown-provider"), None);
        assert_eq!(owned.len(), 2);

        assert_eq!(
            take_owned_process(&mut owned, "amazon-consumer"),
            Some(4242)
        );
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

        assert!(!origin_belongs_to_provider(
            "amazon-consumer",
            "http://www.amazon.com"
        ));
        assert!(!origin_belongs_to_provider(
            "amazon-consumer",
            "https://www.amazon.com.evil.example"
        ));
        assert!(!origin_belongs_to_provider(
            "amazon-consumer",
            "https://www.amazon.com@evil.example"
        ));
        assert!(!origin_belongs_to_provider(
            "amazon-consumer",
            "https://www.taobao.com"
        ));
        assert!(!origin_belongs_to_provider(
            "unknown-provider",
            "https://www.amazon.com"
        ));

        assert!(origin_belongs_to_provider(
            "taobao-consumer",
            "https://login.taobao.com"
        ));
        assert!(origin_belongs_to_provider(
            "jd-consumer",
            "https://passport.jd.com"
        ));
        assert!(origin_belongs_to_provider(
            "pinduoduo-consumer",
            "https://mobile.yangkeduo.com"
        ));
    }

    #[test]
    fn shutdown_drains_the_owned_registry_and_repeats_safely() {
        let mut owned: BTreeMap<String, u32> = BTreeMap::new();
        owned.insert("amazon-consumer".to_owned(), 4242);
        owned.insert("taobao-consumer".to_owned(), 4243);

        let drained = drain_owned_processes(&mut owned);
        assert_eq!(drained.len(), 2);
        assert!(
            owned.is_empty(),
            "the registry must not keep handles it no longer owns"
        );
        assert!(drained
            .iter()
            .any(|(provider_id, pid)| provider_id == "amazon-consumer" && *pid == 4242));

        // A second shutdown event finds nothing left to terminate.
        assert!(drain_owned_processes(&mut owned).is_empty());
    }

    #[test]
    fn readiness_stops_as_soon_as_the_owned_browser_exits() {
        assert_eq!(
            readiness_progress(Some(51321), false),
            ReadinessProgress::Ready(51321)
        );
        // Readiness still wins if the process is observed gone in the same tick.
        assert_eq!(
            readiness_progress(Some(51321), true),
            ReadinessProgress::Ready(51321)
        );
        // A launch handed off to an instance that already owns the profile
        // exits at once and must not wait out the readiness timeout.
        assert_eq!(readiness_progress(None, true), ReadinessProgress::OwnerExited);
        assert_eq!(readiness_progress(None, false), ReadinessProgress::KeepWaiting);
    }

    #[test]
    fn a_live_profile_owner_is_recognised_from_the_published_port() {
        assert_eq!(
            published_devtools_port(Some("51321\n/devtools/browser/AB12")),
            Some(51321)
        );
        assert_eq!(published_devtools_port(Some("0\n")), None);
        assert_eq!(published_devtools_port(Some("not a port")), None);
        assert_eq!(published_devtools_port(Some("")), None);
        assert_eq!(published_devtools_port(None), None);
    }
}

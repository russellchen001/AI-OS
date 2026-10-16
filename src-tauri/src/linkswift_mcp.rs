use crate::{cloud_drive_baidu, cloud_drive_core::ShareLink, managed_assets, mcp, mcp_runtime};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const COMPONENT_ID: &str = "ai-os-cloud-drive-mcp";
const COMPONENT_NAME: &str = "AI-OS Cloud Drive MCP";
const PINNED_VERSION: &str = "0.1.0";
const LICENSE: &str = "Apache-2.0";
const SOURCE_URL: &str = "https://github.com/russellchen001/AI-OS";
const MCP_SERVER_ID: &str = "managed-ai-os-cloud-drive-mcp";
const EXECUTABLE_NAME: &str = "ai-os-cloud-drive-mcp";
const STATE_FILE: &str = "state.json";
const BAIDU_AUTH_WINDOW_LABEL: &str = "cloud-drive-baidu-authorization";

const PERMISSIONS: [&str; 4] = [
    "Connect only to supported cloud-drive HTTPS hosts for links you submit",
    "Use the selected download destination after Runtime confirmation",
    "Open the provider sign-in page when login, verification or access approval is required",
    "Run as an isolated local stdio MCP process",
];

#[derive(Debug, Clone)]
struct ReleaseSpec {
    version: String,
    source_url: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledState {
    version: String,
    source_url: String,
    sha256: String,
    installed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSwiftMcpStatus {
    component_id: &'static str,
    name: &'static str,
    pinned_version: &'static str,
    license: &'static str,
    source_url: &'static str,
    permissions: Vec<&'static str>,
    installed: bool,
    installed_version: Option<String>,
    integrity_verified: bool,
    registered: bool,
    update_available: bool,
    release_configured: bool,
    executable_path: Option<String>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSwiftMcpActionResult {
    success: bool,
    message: String,
    status: LinkSwiftMcpStatus,
}

fn component_root() -> Result<PathBuf, String> {
    let home =
        dirs::home_dir().ok_or_else(|| "Unable to determine the home directory.".to_owned())?;
    Ok(home.join(".ai-os").join("components").join(COMPONENT_ID))
}

fn release_spec() -> Result<ReleaseSpec, String> {
    let source_url = option_env!("AI_OS_CLOUD_DRIVE_MCP_RELEASE_URL")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "AI-OS Cloud Drive MCP v0.1.0 has not been published to the trusted release source. AI-OS did not install anything."
                .to_owned()
        })?;
    let sha256 = option_env!("AI_OS_CLOUD_DRIVE_MCP_RELEASE_SHA256")
        .filter(|value| {
            value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
        })
        .ok_or_else(|| "The pinned AI-OS Cloud Drive MCP SHA-256 is not configured.".to_owned())?;
    let size_bytes = option_env!("AI_OS_CLOUD_DRIVE_MCP_RELEASE_SIZE")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "The pinned AI-OS Cloud Drive MCP release size is not configured.".to_owned())?;

    validate_release_url(source_url)?;
    Ok(ReleaseSpec {
        version: PINNED_VERSION.to_owned(),
        source_url: source_url.to_owned(),
        size_bytes,
        sha256: sha256.to_ascii_lowercase(),
    })
}

fn validate_release_url(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value)
        .map_err(|_| "The AI-OS Cloud Drive MCP release URL is invalid.".to_owned())?;
    let release_prefix = format!(
        "/russellchen001/AI-OS/releases/download/cloud-drive-mcp-v{PINNED_VERSION}/"
    );
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with(&release_prefix)
        || url.path()[release_prefix.len()..].is_empty()
        || url.path()[release_prefix.len()..].contains('/')
    {
        return Err(
            "AI-OS Cloud Drive MCP releases must use the pinned repository and version path on github.com."
                .to_owned(),
        );
    }
    Ok(())
}

fn version_directory(root: &Path, version: &str) -> PathBuf {
    root.join(version)
}

fn executable_path(root: &Path, version: &str) -> PathBuf {
    version_directory(root, version).join(EXECUTABLE_NAME)
}

fn state_path(root: &Path) -> PathBuf {
    root.join(STATE_FILE)
}

fn read_state(root: &Path) -> Option<InstalledState> {
    let contents = fs::read_to_string(state_path(root)).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_state(root: &Path, state: &InstalledState) -> Result<(), String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("Unable to create the component directory: {error}"))?;
    let temporary = root.join(format!("{STATE_FILE}.new"));
    let contents = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("Unable to serialize AI-OS Cloud Drive MCP state: {error}"))?;
    fs::write(&temporary, contents)
        .map_err(|error| format!("Unable to stage AI-OS Cloud Drive MCP state: {error}"))?;
    fs::rename(&temporary, state_path(root))
        .map_err(|error| format!("Unable to commit AI-OS Cloud Drive MCP state: {error}"))
}

fn path_is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn registered_server(path: &Path) -> mcp::McpServer {
    mcp::McpServer {
        id: MCP_SERVER_ID.to_owned(),
        name: COMPONENT_NAME.to_owned(),
        description: format!(
            "Independent {LICENSE} cloud-drive share downloader. Login, CAPTCHA and provider permissions may still be required."
        ),
        enabled: true,
        transport: "stdio".to_owned(),
        command: Some(path.to_string_lossy().into_owned()),
        args: Vec::new(),
        url: None,
        environment: HashMap::new(),
    }
}

fn registered_for(path: &Path) -> bool {
    mcp::read_servers()
        .ok()
        .and_then(|servers| {
            servers
                .into_iter()
                .find(|server| server.id == MCP_SERVER_ID)
        })
        .and_then(|server| server.command)
        .is_some_and(|command| Path::new(&command) == path && path_is_regular_file(path))
}

fn status_at(root: &Path) -> LinkSwiftMcpStatus {
    let configured = release_spec().is_ok();
    let state = read_state(root);
    let path = state
        .as_ref()
        .map(|state| executable_path(root, &state.version));
    let installed = path.as_deref().is_some_and(path_is_regular_file);
    let integrity_verified = match (&state, &path) {
        (Some(state), Some(path)) if installed => managed_assets::sha256_file(path)
            .map(|actual| actual == state.sha256)
            .unwrap_or(false),
        _ => false,
    };
    let registered = path.as_deref().is_some_and(registered_for);
    let installed_version = state.as_ref().map(|state| state.version.clone());
    let update_available = installed_version
        .as_deref()
        .is_some_and(|version| version != PINNED_VERSION);
    let healthy = installed && integrity_verified && registered;
    let message = if healthy {
        format!("{COMPONENT_NAME} {PINNED_VERSION} is installed, verified and registered.")
    } else if installed {
        format!("{COMPONENT_NAME} is present but its integrity or MCP registration needs repair.")
    } else if configured {
        format!("{COMPONENT_NAME} is not installed.")
    } else {
        format!("{COMPONENT_NAME} is not installed and its trusted v{PINNED_VERSION} release is not published in this build.")
    };

    LinkSwiftMcpStatus {
        component_id: COMPONENT_ID,
        name: COMPONENT_NAME,
        pinned_version: PINNED_VERSION,
        license: LICENSE,
        source_url: SOURCE_URL,
        permissions: PERMISSIONS.to_vec(),
        installed,
        installed_version,
        integrity_verified,
        registered,
        update_available,
        release_configured: configured,
        executable_path: path.map(|path| path.to_string_lossy().into_owned()),
        message,
    }
}

fn remove_path_if_present(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("Refusing to remove a symlink from the managed component directory.".to_owned())
        }
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)
            .map_err(|error| format!("Unable to remove AI-OS Cloud Drive MCP files: {error}")),
        Ok(_) => fs::remove_file(path)
            .map_err(|error| format!("Unable to remove AI-OS Cloud Drive MCP file: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Unable to inspect AI-OS Cloud Drive MCP path: {error}")),
    }
}

fn install_at<D, F, G>(
    root: &Path,
    spec: &ReleaseSpec,
    download: D,
    register: F,
    rollback_registration: G,
) -> Result<(), String>
where
    D: FnOnce(&Path) -> Result<(), String>,
    F: FnOnce(&Path) -> Result<(), String>,
    G: FnOnce() -> Result<(), String>,
{
    let staging = root.join(format!(".installing-{}", spec.version));
    remove_path_if_present(&staging)?;
    fs::create_dir_all(&staging)
        .map_err(|error| format!("Unable to create AI-OS Cloud Drive MCP staging directory: {error}"))?;
    let staged_executable = staging.join(EXECUTABLE_NAME);
    let asset = managed_assets::ManagedAssetSpec {
        source_url: spec.source_url.clone(),
        relative_path: EXECUTABLE_NAME.to_owned(),
        size_bytes: spec.size_bytes,
        sha256: spec.sha256.clone(),
    };
    if let Err(error) = download(&staged_executable) {
        let _ = remove_path_if_present(&staging);
        return Err(format!(
            "AI-OS Cloud Drive MCP download or SHA-256 verification failed: {error}"
        ));
    }
    if !managed_assets::file_matches_spec(&staged_executable, &asset)? {
        let _ = remove_path_if_present(&staging);
        return Err("AI-OS Cloud Drive MCP download or SHA-256 verification failed: the package did not match the pinned release.".to_owned());
    }

    #[cfg(unix)]
    fs::set_permissions(&staged_executable, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Unable to make AI-OS Cloud Drive MCP executable: {error}"))?;

    let target = version_directory(root, &spec.version);
    let backup = root.join(format!(".rollback-{}", spec.version));
    remove_path_if_present(&backup)?;
    if target.exists() {
        fs::rename(&target, &backup).map_err(|error| {
            format!("Unable to preserve the previous AI-OS Cloud Drive MCP version: {error}")
        })?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("Unable to activate AI-OS Cloud Drive MCP: {error}"));
    }

    let executable = target.join(EXECUTABLE_NAME);
    if let Err(error) = register(&executable) {
        let _ = remove_path_if_present(&target);
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        let _ = rollback_registration();
        return Err(format!(
            "AI-OS Cloud Drive MCP registration failed and was rolled back: {error}"
        ));
    }

    let state = InstalledState {
        version: spec.version.clone(),
        source_url: spec.source_url.clone(),
        sha256: spec.sha256.clone(),
        installed_at: Utc::now().to_rfc3339(),
    };
    if let Err(error) = write_state(root, &state) {
        let _ = remove_path_if_present(&target);
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        let _ = rollback_registration();
        return Err(format!(
            "AI-OS Cloud Drive MCP state commit failed and was rolled back: {error}"
        ));
    }
    remove_path_if_present(&backup)?;
    Ok(())
}

fn install_blocking(approved: bool) -> Result<LinkSwiftMcpStatus, String> {
    if !approved {
        return Err(
            "AI-OS Cloud Drive MCP installation was declined. No files or configuration were changed."
                .to_owned(),
        );
    }
    let spec = release_spec()?;
    let root = component_root()?;
    let previous = std::sync::Mutex::new(None);
    install_at(
        &root,
        &spec,
        |destination| {
            let client = Client::builder()
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .map_err(|error| format!("Unable to create the trusted release client: {error}"))?;
            let asset = managed_assets::ManagedAssetSpec {
                source_url: spec.source_url.clone(),
                relative_path: EXECUTABLE_NAME.to_owned(),
                size_bytes: spec.size_bytes,
                sha256: spec.sha256.clone(),
            };
            managed_assets::ensure_asset(&client, &asset, destination)
        },
        |path| {
            let old = mcp::read_servers()?
                .into_iter()
                .find(|server| server.id == MCP_SERVER_ID);
            *previous
                .lock()
                .map_err(|_| "MCP rollback state lock failed.".to_owned())? = Some(old);
            mcp::replace_managed_server(registered_server(path))?;
            Ok(())
        },
        || {
            let old = previous
                .lock()
                .map_err(|_| "MCP rollback state lock failed.".to_owned())?
                .take()
                .flatten();
            mcp::restore_managed_server(MCP_SERVER_ID, old)
        },
    )?;
    Ok(status_at(&root))
}

#[tauri::command]
pub fn get_linkswift_mcp_status() -> Result<LinkSwiftMcpStatus, String> {
    Ok(status_at(&component_root()?))
}

#[tauri::command]
pub async fn install_linkswift_mcp(approved: bool) -> LinkSwiftMcpActionResult {
    match tauri::async_runtime::spawn_blocking(move || install_blocking(approved)).await {
        Ok(Ok(status)) => LinkSwiftMcpActionResult {
            success: true,
            message: format!(
                "{COMPONENT_NAME} {PINNED_VERSION} was verified, installed and registered."
            ),
            status,
        },
        Ok(Err(message)) => {
            let status = component_root()
                .map(|root| status_at(&root))
                .unwrap_or_else(|_| status_at(Path::new("/invalid")));
            LinkSwiftMcpActionResult {
                success: false,
                message,
                status,
            }
        }
        Err(error) => {
            let message = format!("AI-OS Cloud Drive MCP installer task failed: {error}");
            let status = component_root()
                .map(|root| status_at(&root))
                .unwrap_or_else(|_| status_at(Path::new("/invalid")));
            LinkSwiftMcpActionResult {
                success: false,
                message,
                status,
            }
        }
    }
}

#[tauri::command]
pub async fn update_linkswift_mcp(approved: bool) -> LinkSwiftMcpActionResult {
    install_linkswift_mcp(approved).await
}

#[tauri::command]
pub fn uninstall_linkswift_mcp(approved: bool) -> LinkSwiftMcpActionResult {
    let root = match component_root() {
        Ok(root) => root,
        Err(error) => {
            return LinkSwiftMcpActionResult {
                success: false,
                message: error,
                status: status_at(Path::new("/invalid")),
            }
        }
    };
    if !approved {
        return LinkSwiftMcpActionResult {
            success: false,
            message:
                "AI-OS Cloud Drive MCP uninstall was declined. No files or configuration were changed."
                    .to_owned(),
            status: status_at(&root),
        };
    }
    let previous = mcp::read_servers().ok().and_then(|servers| {
        servers
            .into_iter()
            .find(|server| server.id == MCP_SERVER_ID)
    });
    if let Err(error) = mcp::restore_managed_server(MCP_SERVER_ID, None) {
        return LinkSwiftMcpActionResult {
            success: false,
            message: error,
            status: status_at(&root),
        };
    }
    if let Err(error) = remove_path_if_present(&root) {
        let _ = mcp::restore_managed_server(MCP_SERVER_ID, previous);
        return LinkSwiftMcpActionResult {
            success: false,
            message: format!(
                "AI-OS Cloud Drive MCP uninstall failed; MCP registration was restored: {error}"
            ),
            status: status_at(&root),
        };
    }
    LinkSwiftMcpActionResult {
        success: true,
        message: "AI-OS Cloud Drive MCP was unregistered and removed.".to_owned(),
        status: status_at(&root),
    }
}

#[tauri::command]
pub fn call_linkswift_mcp(
    tool_name: String,
    arguments: Value,
) -> Result<mcp_runtime::McpResponse, String> {
    if !matches!(
        tool_name.as_str(),
        "inspect_share_link" | "download_resolved_file" | "select_share_files"
    ) {
        return Err("The requested AI-OS Cloud Drive MCP tool is not allowed.".to_owned());
    }
    let root = component_root()?;
    let status = status_at(&root);
    if !status.installed || !status.integrity_verified || !status.registered {
        return Err("AI-OS Cloud Drive MCP is not installed, verified and registered.".to_owned());
    }
    let command = status
        .executable_path
        .ok_or_else(|| "AI-OS Cloud Drive MCP executable is unavailable.".to_owned())?;
    mcp_runtime::call_mcp_tool(command, Vec::new(), tool_name, arguments)
}

#[tauri::command]
pub fn begin_baidu_share_authorization(
    app: tauri::AppHandle,
    url: String,
) -> Result<(), String> {
    let share = ShareLink::parse(&url, None)?;
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::Baidu {
        return Err("Controlled authorization is only available for Baidu shares.".to_owned());
    }
    if let Some(existing) = app.get_webview_window(BAIDU_AUTH_WINDOW_LABEL) {
        existing.close().map_err(|error| error.to_string())?;
    }
    let external = Url::parse(&share.canonical_url)
        .map_err(|_| "Baidu share URL is invalid.".to_owned())?;
    WebviewWindowBuilder::new(
        &app,
        BAIDU_AUTH_WINDOW_LABEL,
        WebviewUrl::External(external),
    )
    .title("百度网盘受控授权")
    .incognito(true)
    .on_navigation(|target| {
        target.scheme() == "https"
            && target
                .host_str()
                .is_some_and(|host| host == "baidu.com" || host.ends_with(".baidu.com"))
    })
    .build()
    .map_err(|error| format!("Unable to open Baidu authorization window: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn complete_baidu_share_download(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
    item_id: String,
    destination: String,
    retry_limit: Option<u8>,
) -> Result<mcp_runtime::McpResponse, String> {
    let window = app
        .get_webview_window(BAIDU_AUTH_WINDOW_LABEL)
        .ok_or_else(|| "Open the controlled Baidu authorization window first.".to_owned())?;
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let cookie_url = Url::parse(&share.canonical_url)
        .map_err(|_| "Baidu share URL is invalid.".to_owned())?;
    let authorization_cookie = window
        .cookies_for_url(cookie_url)
        .map_err(|error| format!("Unable to read the isolated Baidu authorization: {error}"))?
        .into_iter()
        .filter(|cookie| cloud_drive_baidu::is_authorization_cookie_name(cookie.name()))
        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
        .collect::<Vec<_>>()
        .join("; ");
    if authorization_cookie.is_empty() {
        return Err("Baidu login is not complete in the controlled authorization window.".to_owned());
    }
    let command = cloud_drive_mcp_command()?;
    let response = mcp_runtime::call_mcp_tool(
        command,
        Vec::new(),
        "download_authorized_baidu_share_file".to_owned(),
        serde_json::json!({
            "url": share.canonical_url,
            "extractionCode": extraction_code,
            "authorizationCookie": authorization_cookie,
            "itemId": item_id,
            "destination": destination,
            "retryLimit": retry_limit.unwrap_or(3).clamp(1, 5)
        }),
    )?;
    let tool_failed = response
        .result
        .as_ref()
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if response.success && !tool_failed {
        let _ = window.close();
    }
    Ok(response)
}

fn cloud_drive_mcp_command() -> Result<String, String> {
    let root = component_root()?;
    let status = status_at(&root);
    if status.installed && status.integrity_verified && status.registered {
        return status
            .executable_path
            .ok_or_else(|| "AI-OS Cloud Drive MCP executable is unavailable.".to_owned());
    }
    #[cfg(debug_assertions)]
    {
        let candidate = std::env::current_exe()
            .map_err(|error| format!("Unable to locate the development MCP: {error}"))?
            .with_file_name(EXECUTABLE_NAME);
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    Err("AI-OS Cloud Drive MCP is not installed, verified and registered.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_spec(payload: &[u8], source_url: String) -> ReleaseSpec {
        ReleaseSpec {
            version: PINNED_VERSION.to_owned(),
            source_url,
            size_bytes: payload.len() as u64,
            sha256: managed_assets::sha256_bytes(payload),
        }
    }

    #[test]
    fn verified_package_is_activated_and_registered() {
        let payload = b"#!/bin/sh\nexit 0\n";
        let root = tempfile::tempdir().unwrap();
        let registered = Arc::new(Mutex::new(None));
        let capture = Arc::clone(&registered);
        install_at(
            root.path(),
            &test_spec(payload, "https://github.com/test/release".to_owned()),
            |path| fs::write(path, payload).map_err(|error| error.to_string()),
            move |path| {
                *capture.lock().unwrap() = Some(path.to_path_buf());
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        let executable = executable_path(root.path(), PINNED_VERSION);
        assert_eq!(
            registered.lock().unwrap().as_deref(),
            Some(executable.as_path())
        );
        assert_eq!(fs::read(&executable).unwrap(), payload);
        assert_eq!(read_state(root.path()).unwrap().version, PINNED_VERSION);
    }

    #[test]
    fn hash_failure_changes_neither_installation_nor_registration() {
        let payload = b"not the pinned package";
        let root = tempfile::tempdir().unwrap();
        let mut spec = test_spec(payload, "https://github.com/test/release".to_owned());
        spec.sha256 = "0".repeat(64);
        let registrations = Arc::new(Mutex::new(0));
        let capture = Arc::clone(&registrations);
        let error = install_at(
            root.path(),
            &spec,
            |path| fs::write(path, payload).map_err(|error| error.to_string()),
            move |_| {
                *capture.lock().unwrap() += 1;
                Ok(())
            },
            || Ok(()),
        )
        .unwrap_err();
        assert!(error.contains("SHA-256 verification failed"));
        assert_eq!(*registrations.lock().unwrap(), 0);
        assert!(!executable_path(root.path(), PINNED_VERSION).exists());
    }

    #[test]
    fn registration_failure_rolls_back_new_files_and_registration() {
        let payload = b"#!/bin/sh\nexit 0\n";
        let root = tempfile::tempdir().unwrap();
        let rolled_back = Arc::new(Mutex::new(false));
        let capture = Arc::clone(&rolled_back);
        let error = install_at(
            root.path(),
            &test_spec(payload, "https://github.com/test/release".to_owned()),
            |path| fs::write(path, payload).map_err(|error| error.to_string()),
            |_| Err("simulated registry failure".to_owned()),
            move || {
                *capture.lock().unwrap() = true;
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.contains("rolled back"));
        assert!(*rolled_back.lock().unwrap());
        assert!(!executable_path(root.path(), PINNED_VERSION).exists());
    }

    #[test]
    fn production_release_requires_a_pinned_trusted_origin() {
        assert!(validate_release_url("http://github.com/example/release").is_err());
        assert!(validate_release_url("https://example.com/release").is_err());
        assert!(validate_release_url(
            "https://github.com/other/repo/releases/download/cloud-drive-mcp-v0.1.0/ai-os-cloud-drive-mcp"
        )
        .is_err());
        assert!(validate_release_url("https://github.com/russellchen001/AI-OS/releases/download/cloud-drive-mcp-v0.2.0/ai-os-cloud-drive-mcp").is_err());
        assert!(validate_release_url("https://github.com/russellchen001/AI-OS/releases/download/cloud-drive-mcp-v0.1.0/ai-os-cloud-drive-mcp").is_ok());
    }

    #[test]
    fn declined_install_does_not_start_release_or_filesystem_work() {
        let error = install_blocking(false).unwrap_err();
        assert!(error.contains("declined"));
        assert!(error.contains("No files or configuration were changed"));
    }

    #[test]
    fn verified_installed_process_accepts_standard_mcp_tool_calls() {
        let payload = br#"#!/bin/sh
read request
case "$request" in
  *tools/list*) printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"inspect_share_link","description":"Inspect a supported share link","inputSchema":{"type":"object"}}]}}' ;;
  *tools/call*) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"Login or provider verification may be required."}]}}' ;;
esac
"#;
        let root = tempfile::tempdir().unwrap();
        install_at(
            root.path(),
            &test_spec(payload, "https://github.com/test/release".to_owned()),
            |path| fs::write(path, payload).map_err(|error| error.to_string()),
            |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        let command = executable_path(root.path(), PINNED_VERSION)
            .to_string_lossy()
            .into_owned();
        let tools = mcp_runtime::list_mcp_tools(command.clone(), Vec::new()).unwrap();
        assert_eq!(tools[0].name, "inspect_share_link");
        let response = mcp_runtime::call_mcp_tool(
            command,
            Vec::new(),
            "inspect_share_link".to_owned(),
            serde_json::json!({"url":"https://pan.quark.cn/s/example"}),
        )
        .unwrap();
        assert!(response.success);
    }
}

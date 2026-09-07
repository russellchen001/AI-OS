use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct DesktopInstallation {
    id: String,
    status: String,
    #[serde(rename = "sourceId")]
    source_id: String,
    #[serde(rename = "installPath")]
    install_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComfyUiMacOsInstallation {
    pub instance_id: String,
    pub install_path: PathBuf,
    pub python_path: PathBuf,
    pub main_py_path: PathBuf,
    pub python_present: bool,
    pub main_py_present: bool,
}

pub(crate) fn parse_desktop_installations(
    contents: &str,
) -> Result<Vec<ComfyUiMacOsInstallation>, String> {
    let entries: Vec<DesktopInstallation> =
        serde_json::from_str(contents).map_err(|error| error.to_string())?;

    entries
        .into_iter()
        .filter(|entry| entry.source_id == "standalone" && entry.status == "installed")
        .map(|entry| {
            let install_path = entry
                .install_path
                .ok_or_else(|| format!("ComfyUI installation {} has no installPath", entry.id))?;
            let runtime_root = install_path.join("ComfyUI");
            let python_path = runtime_root.join(".venv/bin/python3");
            let main_py_path = runtime_root.join("main.py");

            Ok(ComfyUiMacOsInstallation {
                instance_id: entry.id,
                install_path,
                python_present: python_path.is_file(),
                main_py_present: main_py_path.is_file(),
                python_path,
                main_py_path,
            })
        })
        .collect()
}

pub(crate) fn discover_desktop_installations() -> Result<Vec<ComfyUiMacOsInstallation>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Home directory is unavailable".to_owned())?;
    let registry = home
        .join("Library/Application Support/Comfy Desktop")
        .join("installations.json");

    if !registry.is_file() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&registry)
        .map_err(|error| format!("Unable to read {}: {error}", registry.display()))?;

    parse_desktop_installations(&contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_broken_instance_remains_discoverable() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().to_string_lossy().into_owned();

        let registry = serde_json::json!([
            {
                "id": "local-test",
                "status": "installed",
                "sourceId": "standalone",
                "installPath": path
            },
            {
                "id": "cloud-test",
                "status": "installed",
                "sourceId": "cloud"
            }
        ])
        .to_string();

        let installations = parse_desktop_installations(&registry).unwrap();

        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].instance_id, "local-test");
        assert!(!installations[0].python_present);
        assert!(!installations[0].main_py_present);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod live_discovery_tests {
    use super::*;

    #[test]
    #[ignore = "requires a real Comfy Desktop local installation"]
    fn live_desktop_registry_discovers_usable_local_instance() {
        let installations = discover_desktop_installations()
            .expect("Comfy Desktop installations.json should be readable");

        assert!(
            !installations.is_empty(),
            "expected at least one installed standalone ComfyUI instance"
        );

        let usable = installations.iter().find(|installation| {
            installation.install_path.is_dir()
                && installation.python_present
                && installation.main_py_present
        });

        let usable = usable
            .expect("expected an installed standalone instance with python and main.py present");

        eprintln!(
            "instance={} install={} python={} main={}",
            usable.instance_id,
            usable.install_path.display(),
            usable.python_path.display(),
            usable.main_py_path.display()
        );
    }
}

pub(crate) struct ComfyUiMacOsBackend {
    pub instance_id: String,
    pub endpoint: String,
    child: Option<std::process::Child>,
}

impl ComfyUiMacOsBackend {
    pub(crate) fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(std::process::Child::id)
    }

    pub(crate) fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }

        self.child = None;
    }
}

impl Drop for ComfyUiMacOsBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

fn allocate_loopback_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Unable to allocate ComfyUI loopback port: {error}"))?;

    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("Unable to inspect ComfyUI loopback port: {error}"))
}

fn spawn_comfyui_backend(
    installation: &ComfyUiMacOsInstallation,
    port: u16,
) -> Result<ComfyUiMacOsBackend, String> {
    if !installation.python_path.is_file() {
        return Err(format!(
            "ComfyUI Python runtime is missing: {}",
            installation.python_path.display()
        ));
    }

    if !installation.main_py_path.is_file() {
        return Err(format!(
            "ComfyUI main.py is missing: {}",
            installation.main_py_path.display()
        ));
    }

    let home = dirs::home_dir().ok_or_else(|| "Home directory is unavailable".to_owned())?;

    let model_paths = home
        .join("Library/Application Support/Comfy Desktop/instance-model-paths")
        .join(format!("{}.yaml", installation.instance_id));

    let shared_root = home.join("ComfyUI-Shared");
    let input_directory = shared_root.join("input");
    let output_directory = shared_root.join("output");

    let mut command = std::process::Command::new(&installation.python_path);

    command
        .current_dir(&installation.install_path)
        .arg("-s")
        .arg("ComfyUI/main.py")
        .arg("--listen")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--disable-auto-launch")
        .arg("--enable-manager");

    if model_paths.is_file() {
        command.arg("--extra-model-paths-config").arg(model_paths);
    }

    if input_directory.is_dir() {
        command.arg("--input-directory").arg(input_directory);
    }

    if output_directory.is_dir() {
        command.arg("--output-directory").arg(output_directory);
    }

    let child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Unable to start ComfyUI backend: {error}"))?;

    Ok(ComfyUiMacOsBackend {
        instance_id: installation.instance_id.clone(),
        endpoint: format!("http://127.0.0.1:{port}"),
        child: Some(child),
    })
}

pub(crate) fn start_comfyui_backend_and_wait(
    installation: &ComfyUiMacOsInstallation,
    timeout: std::time::Duration,
) -> Result<ComfyUiMacOsBackend, String> {
    let port = allocate_loopback_port()?;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut backend = spawn_comfyui_backend(installation, port)?;
    let started = std::time::Instant::now();

    while started.elapsed() < timeout {
        if let Some(child) = backend.child.as_mut() {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("Unable to inspect ComfyUI process: {error}"))?
            {
                return Err(format!(
                    "ComfyUI backend exited before its API became ready: {status}"
                ));
            }
        }

        if std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(250))
            .is_ok()
        {
            if let Ok(health) =
                crate::generative_media::comfyui::probe_comfyui_api(&backend.endpoint)
            {
                if health.is_healthy() {
                    return Ok(backend);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    backend.stop();

    Err(format!(
        "ComfyUI backend did not become healthy within {} seconds",
        timeout.as_secs()
    ))
}

#[cfg(test)]
mod backend_lifecycle_tests {
    use super::*;

    #[test]
    fn missing_runtime_is_rejected_before_process_spawn() {
        let root = tempfile::tempdir().unwrap();

        let installation = ComfyUiMacOsInstallation {
            instance_id: "missing-runtime".to_owned(),
            install_path: root.path().to_path_buf(),
            python_path: root.path().join("missing-python"),
            main_py_path: root.path().join("missing-main.py"),
            python_present: false,
            main_py_present: false,
        };

        let error =
            start_comfyui_backend_and_wait(&installation, std::time::Duration::from_millis(10))
                .err()
                .expect("missing runtime must fail");

        assert!(error.contains("Python runtime is missing"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod live_backend_tests {
    use super::*;

    #[test]
    #[ignore = "starts a real local ComfyUI backend"]
    fn live_backend_starts_reaches_api_and_stops() {
        let installations = discover_desktop_installations().unwrap();
        let installation = installations
            .iter()
            .find(|item| item.install_path.is_dir() && item.python_present && item.main_py_present)
            .expect("usable standalone ComfyUI installation");

        let mut backend =
            start_comfyui_backend_and_wait(installation, std::time::Duration::from_secs(150))
                .expect("ComfyUI backend should become healthy");

        let pid = backend.process_id().expect("started backend PID");
        let endpoint = backend.endpoint.clone();
        let health = crate::generative_media::comfyui::probe_comfyui_api(&endpoint).unwrap();

        assert!(health.is_healthy());
        eprintln!(
            "LIVE_BACKEND instance={} pid={} endpoint={} version={}",
            backend.instance_id,
            pid,
            endpoint,
            health.comfyui_version.as_deref().unwrap_or("unknown")
        );

        let port = url::Url::parse(&endpoint).unwrap().port().unwrap();
        backend.stop();
        std::thread::sleep(std::time::Duration::from_millis(300));

        assert!(
            std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                std::time::Duration::from_millis(500),
            )
            .is_err(),
            "AI-OS-started ComfyUI backend must be stopped after the test"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComfyUiMacOsReadinessReport {
    pub instance_id: Option<String>,
    pub evidence: crate::generative_media::provider::LocalMediaReadinessEvidence,
    pub diagnostic: Option<String>,
}

impl ComfyUiMacOsReadinessReport {
    pub(crate) fn readiness(&self) -> crate::generative_media::provider::LocalMediaReadiness {
        self.evidence.classify()
    }
}

fn runtime_readiness_report(
    instance_id: Option<String>,
    engine_installed: bool,
    engine_startable: bool,
    api_reachable: bool,
    diagnostic: Option<String>,
) -> ComfyUiMacOsReadinessReport {
    ComfyUiMacOsReadinessReport {
        instance_id,
        evidence: crate::generative_media::provider::LocalMediaReadinessEvidence {
            engine_installed,
            engine_startable,
            api_reachable,
            workflow_ready: false,
            required_assets_ready: false,
            custom_nodes_ready: false,
            integrity_ok: false,
            smoke_generation_ok: false,
            output_retrieval_ok: false,
        },
        diagnostic,
    }
}

#[cfg(test)]
mod readiness_bridge_tests {
    use super::*;
    use crate::generative_media::provider::LocalMediaReadiness;

    #[test]
    fn runtime_facts_map_to_ready_first_states_without_fake_ready() {
        let missing = runtime_readiness_report(None, false, false, false, None);
        assert_eq!(missing.readiness(), LocalMediaReadiness::NotInstalled);

        let broken = runtime_readiness_report(
            Some("broken".to_owned()),
            true,
            false,
            false,
            Some("runtime missing".to_owned()),
        );
        assert_eq!(broken.readiness(), LocalMediaReadiness::InstalledBroken);

        let healthy_runtime =
            runtime_readiness_report(Some("healthy".to_owned()), true, true, true, None);

        assert_eq!(
            healthy_runtime.readiness(),
            LocalMediaReadiness::InstalledNotConfigured
        );
        assert_ne!(healthy_runtime.readiness(), LocalMediaReadiness::Ready);
    }
}

pub(crate) fn probe_comfyui_installation_readiness(
    installation: &ComfyUiMacOsInstallation,
    timeout: std::time::Duration,
) -> ComfyUiMacOsReadinessReport {
    if !installation.python_present || !installation.main_py_present {
        return runtime_readiness_report(
            Some(installation.instance_id.clone()),
            true,
            false,
            false,
            Some("ComfyUI local runtime files are incomplete".to_owned()),
        );
    }

    match start_comfyui_backend_and_wait(installation, timeout) {
        Ok(mut backend) => {
            let endpoint = backend.endpoint.clone();

            let api_reachable = crate::generative_media::comfyui::probe_comfyui_api(&endpoint)
                .map(|health| health.is_healthy())
                .unwrap_or(false);

            backend.stop();

            runtime_readiness_report(
                Some(installation.instance_id.clone()),
                true,
                true,
                api_reachable,
                (!api_reachable).then(|| "ComfyUI Local API health verification failed".to_owned()),
            )
        }
        Err(error) => runtime_readiness_report(
            Some(installation.instance_id.clone()),
            true,
            false,
            false,
            Some(error),
        ),
    }
}

pub(crate) fn probe_desktop_readiness(
    timeout: std::time::Duration,
) -> Result<Vec<ComfyUiMacOsReadinessReport>, String> {
    let installations = discover_desktop_installations()?;

    if installations.is_empty() {
        return Ok(vec![runtime_readiness_report(
            None, false, false, false, None,
        )]);
    }

    Ok(installations
        .iter()
        .map(|installation| probe_comfyui_installation_readiness(installation, timeout))
        .collect())
}

#[cfg(test)]
mod final_readiness_tests {
    use super::*;
    use crate::generative_media::provider::LocalMediaReadiness;

    #[test]
    fn incomplete_runtime_is_installed_broken_without_becoming_ready() {
        let root = tempfile::tempdir().unwrap();

        let installation = ComfyUiMacOsInstallation {
            instance_id: "broken-runtime".to_owned(),
            install_path: root.path().to_path_buf(),
            python_path: root.path().join("missing-python"),
            main_py_path: root.path().join("missing-main.py"),
            python_present: false,
            main_py_present: false,
        };

        let report = probe_comfyui_installation_readiness(&installation, std::time::Duration::ZERO);

        assert_eq!(report.readiness(), LocalMediaReadiness::InstalledBroken);
        assert!(report.evidence.engine_installed);
        assert!(!report.evidence.engine_startable);
        assert!(!report.evidence.api_reachable);
        assert_ne!(report.readiness(), LocalMediaReadiness::Ready);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "starts a real local ComfyUI backend"]
    fn live_desktop_runtime_is_healthy_but_not_ready_before_profile_checks() {
        let reports = probe_desktop_readiness(std::time::Duration::from_secs(150))
            .expect("real Comfy Desktop readiness probe");

        let report = reports
            .iter()
            .find(|report| {
                report.instance_id.is_some()
                    && report.evidence.engine_installed
                    && report.evidence.engine_startable
                    && report.evidence.api_reachable
            })
            .expect("expected a healthy installed local ComfyUI runtime");

        assert_eq!(
            report.readiness(),
            LocalMediaReadiness::InstalledNotConfigured
        );

        assert!(!report.evidence.workflow_ready);
        assert!(!report.evidence.required_assets_ready);
        assert!(!report.evidence.custom_nodes_ready);
        assert!(!report.evidence.integrity_ok);
        assert!(!report.evidence.smoke_generation_ok);
        assert!(!report.evidence.output_retrieval_ok);

        assert_ne!(report.readiness(), LocalMediaReadiness::Ready);

        eprintln!(
            "LIVE_READINESS instance={} state={:?} installed={} startable={} api={} ready=false",
            report.instance_id.as_deref().unwrap_or("none"),
            report.readiness(),
            report.evidence.engine_installed,
            report.evidence.engine_startable,
            report.evidence.api_reachable,
        );
    }
}

// GM-2B profile-aware readiness.
//
// HTTP/workflow/model semantics remain in the platform-neutral ComfyUI profile
// module. This adapter only resolves Comfy Desktop filesystem state and owns
// the local backend lifecycle.
fn parse_desktop_model_base_path(contents: &str) -> Option<PathBuf> {
    contents.lines().find_map(|line| {
        let value = line.trim().strip_prefix("base_path:")?.trim();

        let value = value
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
            .or_else(|| {
                value
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
            })
            .unwrap_or(value);

        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

pub(crate) fn resolve_desktop_model_root(instance_id: &str) -> Result<Option<PathBuf>, String> {
    if instance_id.is_empty()
        || !instance_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("ComfyUI instance id is invalid".to_owned());
    }

    let home = dirs::home_dir().ok_or_else(|| "Home directory is unavailable".to_owned())?;

    let config = home
        .join("Library/Application Support/Comfy Desktop/instance-model-paths")
        .join(format!("{instance_id}.yaml"));

    if !config.is_file() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&config)
        .map_err(|_| "Unable to read Comfy Desktop model-path configuration".to_owned())?;

    Ok(parse_desktop_model_base_path(&contents))
}

pub(crate) fn managed_profile_manifest_path() -> Result<PathBuf, String> {
    let data_root =
        dirs::data_dir().ok_or_else(|| "Application data directory is unavailable".to_owned())?;

    Ok(data_root
        .join("AI-OS")
        .join("generative-media")
        .join("comfyui")
        .join("profiles")
        .join(format!(
            "{}.json",
            crate::generative_media::comfyui_profile::CORE_TEXT_TO_IMAGE_PROFILE_ID
        )))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComfyUiMacOsProfileReadinessReport {
    pub instance_id: Option<String>,
    pub profile_id: String,
    pub evidence: crate::generative_media::provider::LocalMediaReadinessEvidence,
    pub smoke_generation_checked: bool,
    pub output_retrieval_checked: bool,
    pub diagnostics: Vec<String>,
}

impl ComfyUiMacOsProfileReadinessReport {
    pub(crate) fn readiness(&self) -> crate::generative_media::provider::LocalMediaReadiness {
        use crate::generative_media::provider::LocalMediaReadiness;

        if self.smoke_generation_checked && !self.evidence.smoke_generation_ok {
            return LocalMediaReadiness::InstalledBroken;
        }

        if self.output_retrieval_checked && !self.evidence.output_retrieval_ok {
            return LocalMediaReadiness::InstalledBroken;
        }

        let configured = self.evidence.engine_installed
            && self.evidence.engine_startable
            && self.evidence.api_reachable
            && self.evidence.workflow_ready
            && self.evidence.required_assets_ready
            && self.evidence.custom_nodes_ready
            && self.evidence.integrity_ok;

        if configured && (!self.smoke_generation_checked || !self.output_retrieval_checked) {
            return LocalMediaReadiness::InstalledNotConfigured;
        }

        self.evidence.classify()
    }
}

fn empty_profile_readiness_report(
    instance_id: Option<String>,
    engine_installed: bool,
    engine_startable: bool,
    api_reachable: bool,
    diagnostic: Option<&str>,
) -> ComfyUiMacOsProfileReadinessReport {
    let base = runtime_readiness_report(
        instance_id.clone(),
        engine_installed,
        engine_startable,
        api_reachable,
        None,
    );

    ComfyUiMacOsProfileReadinessReport {
        instance_id,
        profile_id: crate::generative_media::comfyui_profile::CORE_TEXT_TO_IMAGE_PROFILE_ID
            .to_owned(),
        evidence: base.evidence,
        smoke_generation_checked: false,
        output_retrieval_checked: false,
        diagnostics: diagnostic.into_iter().map(str::to_owned).collect(),
    }
}

pub(crate) fn probe_comfyui_installation_profile_readiness(
    installation: &ComfyUiMacOsInstallation,
    timeout: std::time::Duration,
) -> ComfyUiMacOsProfileReadinessReport {
    if !installation.python_present || !installation.main_py_present {
        return empty_profile_readiness_report(
            Some(installation.instance_id.clone()),
            true,
            false,
            false,
            Some("runtime-incomplete"),
        );
    }

    let mut backend = match start_comfyui_backend_and_wait(installation, timeout) {
        Ok(backend) => backend,
        Err(_) => {
            return empty_profile_readiness_report(
                Some(installation.instance_id.clone()),
                true,
                false,
                false,
                Some("backend-start-failed"),
            );
        }
    };

    let base = runtime_readiness_report(
        Some(installation.instance_id.clone()),
        true,
        true,
        true,
        None,
    );

    let model_root = match resolve_desktop_model_root(&installation.instance_id) {
        Ok(root) => root,
        Err(_) => {
            backend.stop();

            return empty_profile_readiness_report(
                Some(installation.instance_id.clone()),
                true,
                true,
                true,
                Some("desktop-model-root-invalid"),
            );
        }
    };

    let manifest_path = match managed_profile_manifest_path() {
        Ok(path) => path,
        Err(_) => {
            backend.stop();

            return empty_profile_readiness_report(
                Some(installation.instance_id.clone()),
                true,
                true,
                true,
                Some("managed-profile-location-unavailable"),
            );
        }
    };

    let profile = crate::generative_media::comfyui_profile::probe_core_text_to_image_profile(
        &backend.endpoint,
        model_root.as_deref(),
        &manifest_path,
    );

    backend.stop();

    match profile {
        Ok(profile) => ComfyUiMacOsProfileReadinessReport {
            instance_id: Some(installation.instance_id.clone()),
            profile_id: profile.profile_id.clone(),
            evidence: profile.apply_to_evidence(&base.evidence),
            smoke_generation_checked: false,
            output_retrieval_checked: false,
            diagnostics: profile.diagnostics,
        },
        Err(_) => empty_profile_readiness_report(
            Some(installation.instance_id.clone()),
            true,
            true,
            true,
            Some("profile-probe-failed"),
        ),
    }
}

pub(crate) fn probe_desktop_profile_readiness(
    timeout: std::time::Duration,
) -> Result<Vec<ComfyUiMacOsProfileReadinessReport>, String> {
    let installations = discover_desktop_installations()?;

    if installations.is_empty() {
        return Ok(vec![empty_profile_readiness_report(
            None,
            false,
            false,
            false,
            Some("engine-not-installed"),
        )]);
    }

    Ok(installations
        .iter()
        .map(|installation| probe_comfyui_installation_profile_readiness(installation, timeout))
        .collect())
}

pub(crate) fn select_usable_desktop_installation() -> Result<ComfyUiMacOsInstallation, String> {
    discover_desktop_installations()?
        .into_iter()
        .find(|installation| {
            installation.install_path.is_dir()
                && installation.python_present
                && installation.main_py_present
        })
        .ok_or_else(|| "No usable local ComfyUI installation was found".to_owned())
}

#[cfg(test)]
mod profile_readiness_tests {
    use super::*;
    use crate::generative_media::provider::LocalMediaReadiness;

    #[test]
    fn desktop_model_root_parser_reads_generated_yaml() {
        let contents = r#"
comfy.desktop_0:
  base_path: '/Users/example/ComfyUI-Shared/models'
  is_default: true
  'checkpoints': 'checkpoints/'
"#;

        assert_eq!(
            parse_desktop_model_base_path(contents),
            Some(PathBuf::from("/Users/example/ComfyUI-Shared/models"))
        );
    }

    #[test]
    fn desktop_model_root_parser_refuses_missing_base_path() {
        let contents = "comfy.desktop_0:\n  is_default: true\n";
        assert_eq!(parse_desktop_model_base_path(contents), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "starts a real local ComfyUI backend"]
    fn live_desktop_profile_readiness_reflects_real_inventory() {
        let reports = probe_desktop_profile_readiness(std::time::Duration::from_secs(150))
            .expect("real Comfy Desktop profile readiness probe");

        let report = reports
            .iter()
            .find(|report| {
                report.instance_id.is_some()
                    && report.evidence.engine_installed
                    && report.evidence.engine_startable
                    && report.evidence.api_reachable
            })
            .expect("expected a healthy installed local ComfyUI runtime");

        assert_eq!(
            report.profile_id,
            crate::generative_media::comfyui_profile::CORE_TEXT_TO_IMAGE_PROFILE_ID
        );

        assert!(report.evidence.workflow_ready);
        assert!(report.evidence.custom_nodes_ready);

        // GM-2B intentionally does not perform generation or output retrieval.
        assert!(!report.evidence.smoke_generation_ok);
        assert!(!report.evidence.output_retrieval_ok);
        assert_ne!(report.readiness(), LocalMediaReadiness::Ready);

        eprintln!(
            "LIVE_PROFILE_READINESS instance={} profile={} state={:?} workflow={} assets={} custom_nodes={} integrity={} smoke={} output={} ready=false",
            report.instance_id.as_deref().unwrap_or("none"),
            report.profile_id,
            report.readiness(),
            report.evidence.workflow_ready,
            report.evidence.required_assets_ready,
            report.evidence.custom_nodes_ready,
            report.evidence.integrity_ok,
            report.evidence.smoke_generation_ok,
            report.evidence.output_retrieval_ok,
        );
    }
}

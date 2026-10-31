//! Version checks, installs and updates for every official account CLI
//! (Claude Code, Grok CLI, Gemini CLI, …) through one registry. Each CLI is
//! one `CliProviderSpec`; how it is updated follows where the user installed
//! it (Homebrew, npm, the vendor's own installer, or an AI-OS managed copy),
//! so AI-OS never fights the user's package manager. Every install or update
//! needs an approved plan that is re-verified right before it runs.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Ordering,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// One official CLI that AI-OS can use as an account provider.
pub(crate) struct CliProviderSpec {
    /// The AI-OS provider id the CLI signs in for.
    pub(crate) provider_id: &'static str,
    pub(crate) display_name: &'static str,
    /// The vendor's official npm package: install source and version oracle.
    pub(crate) npm_package: &'static str,
    /// The Homebrew formula, when the vendor's CLI is packaged there.
    pub(crate) brew_formula: Option<&'static str>,
    /// The vendor installer's own update command; `{version}` is replaced.
    pub(crate) self_update: Option<&'static [&'static str]>,
    /// Whether AI-OS may install it into its own npm prefix when missing.
    pub(crate) managed_install: bool,
    /// A launcher the vendor installer owns, restored if a managed install fails.
    pub(crate) canonical_link: Option<&'static str>,
    pub(crate) login_command: &'static str,
    pub(crate) discover: fn() -> Option<PathBuf>,
    pub(crate) is_busy: fn() -> bool,
}

/// Adding a CLI provider means adding one entry here plus its request module.
pub(crate) const CLI_PROVIDERS: &[CliProviderSpec] = &[
    CliProviderSpec {
        provider_id: "anthropic",
        display_name: "Claude Code",
        npm_package: "@anthropic-ai/claude-code",
        brew_formula: None,
        self_update: Some(&["update"]),
        managed_install: false,
        canonical_link: None,
        login_command: "claude auth login",
        discover: crate::claude_code::discover_binary,
        is_busy: crate::claude_code::has_active_requests,
    },
    CliProviderSpec {
        provider_id: "grok",
        display_name: "Grok CLI",
        npm_package: "@xai-official/grok",
        brew_formula: None,
        self_update: Some(&["update", "--version", "{version}"]),
        managed_install: true,
        canonical_link: Some(".grok/bin/grok"),
        login_command: "grok login",
        discover: crate::grok_cli::discover_binary,
        is_busy: crate::grok_cli::has_active_requests,
    },
    CliProviderSpec {
        provider_id: "google",
        display_name: "Gemini CLI",
        npm_package: "@google/gemini-cli",
        brew_formula: Some("gemini-cli"),
        self_update: None,
        managed_install: true,
        canonical_link: None,
        login_command: "gemini",
        discover: crate::gemini_cli::discover_binary,
        is_busy: crate::gemini_cli::has_active_requests,
    },
];

fn spec_for(provider_id: &str) -> Result<&'static CliProviderSpec, String> {
    CLI_PROVIDERS
        .iter()
        .find(|spec| spec.provider_id == provider_id)
        .ok_or_else(|| "This provider is not an AI-OS managed CLI provider.".to_string())
}

/// Where the installed copy came from, which decides how it is updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallSource {
    Homebrew(String),
    AiOsManaged,
    NpmGlobal,
    OfficialInstaller,
    Unknown,
}

impl InstallSource {
    fn method(&self) -> &'static str {
        match self {
            Self::Homebrew(_) => "homebrew",
            Self::AiOsManaged => "ai-os-npm",
            Self::NpmGlobal => "npm-global",
            Self::OfficialInstaller => "official-installer",
            Self::Unknown => "manual",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Homebrew(_) => "Homebrew",
            Self::AiOsManaged => "AI-OS managed npm",
            Self::NpmGlobal => "npm (global)",
            Self::OfficialInstaller => "official installer",
            Self::Unknown => "unknown location",
        }
    }
}

/// From the resolved executable path: Homebrew keeps formulas under
/// `Cellar/<formula>/`, npm under `node_modules/<package>/`, and AI-OS its
/// own copies under `~/.ai-os/providers/<id>/`.
pub(crate) fn install_source(
    spec: &CliProviderSpec,
    resolved: &Path,
    home: Option<&Path>,
) -> InstallSource {
    let text = resolved.to_string_lossy();
    if let Some(home) = home {
        if resolved.starts_with(home.join(".ai-os/providers").join(spec.provider_id)) {
            return InstallSource::AiOsManaged;
        }
    }
    if let Some(formula) = spec.brew_formula {
        if text.contains(&format!("/Cellar/{formula}/")) {
            return InstallSource::Homebrew(formula.to_string());
        }
    }
    if text.contains(&format!("/node_modules/{}/", spec.npm_package)) {
        return InstallSource::NpmGlobal;
    }
    if spec.self_update.is_some() {
        return InstallSource::OfficialInstaller;
    }
    InstallSource::Unknown
}

/// Numeric `x.y.z` comparison; a pre-release ("1.2.0-beta.1") sorts before
/// its release.
pub(crate) fn compare_versions(left: &str, right: &str) -> Ordering {
    let split = |value: &str| -> (Vec<u64>, bool) {
        let (core, pre) = match value.split_once('-') {
            Some((core, _)) => (core, true),
            None => (value, false),
        };
        (
            core.split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect(),
            pre,
        )
    };
    let (mut left_parts, left_pre) = split(left.trim().trim_start_matches('v'));
    let (mut right_parts, right_pre) = split(right.trim().trim_start_matches('v'));
    let length = left_parts.len().max(right_parts.len());
    left_parts.resize(length, 0);
    right_parts.resize(length, 0);
    left_parts
        .cmp(&right_parts)
        .then_with(|| right_pre.cmp(&left_pre))
}

fn is_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.starts_with(|c: char| c.is_ascii_digit())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))
}

fn tool_candidates(name: &str, explicit_path: Option<OsString>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = explicit_path
        .map(|value| {
            std::env::split_paths(&value)
                .map(|entry| entry.join(name))
                .collect()
        })
        .unwrap_or_default();
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ]);
    candidates
}

fn tool_binary(name: &str) -> Result<PathBuf, String> {
    tool_candidates(name, std::env::var_os("PATH"))
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| match name {
            "npm" => "Node.js npm is required to install the official CLI package.".to_string(),
            _ => format!("{name} is required to update this CLI."),
        })
}

fn npm_metadata(package: &str, version: &str) -> Result<(String, String), String> {
    let output = Command::new(tool_binary("npm")?)
        .args([
            "view",
            &format!("{package}@{version}"),
            "version",
            "dist.integrity",
            "--json",
        ])
        .output()
        .map_err(|_| "AI-OS could not query the official npm registry.".to_string())?;
    if !output.status.success() {
        return Err(format!(
            "The official {package} package metadata could not be verified."
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| format!("The official {package} package returned invalid metadata."))?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let integrity = value
        .get("dist.integrity")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !is_version(&version) || !integrity.starts_with("sha512-") {
        return Err(format!(
            "The official {package} package did not publish verifiable integrity metadata."
        ));
    }
    Ok((version, integrity))
}

fn brew_stable_version(formula: &str) -> Result<String, String> {
    let output = Command::new(tool_binary("brew")?)
        .args(["info", "--json=v2", formula])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .output()
        .map_err(|_| "AI-OS could not query Homebrew.".to_string())?;
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Homebrew returned invalid formula information.".to_string())?;
    value
        .pointer("/formulae/0/versions/stable")
        .and_then(Value::as_str)
        .filter(|version| is_version(version))
        .map(str::to_string)
        .ok_or_else(|| format!("Homebrew has no stable version of {formula}."))
}

/// Bounded, in an empty folder like every other CLI call: a hung CLI must
/// not stall the daily check.
fn installed_version_of(binary: &Path) -> Option<String> {
    const VERSION_PROCESS: crate::cli_process::CliProcess = crate::cli_process::CliProcess {
        label: "CLI",
        workdir_name: "ai-os-cli-version",
        env_set: &[],
        env_remove: &[],
        secret_markers: &[],
    };
    let output = VERSION_PROCESS
        .run(binary, &["--version"], None, std::time::Duration::from_secs(20), None)
        .ok()?;
    crate::cli_process::parse_version(&String::from_utf8_lossy(&output))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliVersionReport {
    provider_id: String,
    display_name: String,
    installed: bool,
    installed_version: Option<String>,
    latest_version: Option<String>,
    update_available: bool,
    method: String,
    source_label: String,
    can_update: bool,
    message: String,
}

fn version_report(spec: &CliProviderSpec) -> CliVersionReport {
    let mut report = CliVersionReport {
        provider_id: spec.provider_id.to_string(),
        display_name: spec.display_name.to_string(),
        installed: false,
        installed_version: None,
        latest_version: None,
        update_available: false,
        method: "none".to_string(),
        source_label: String::new(),
        can_update: false,
        message: format!("{} is not installed.", spec.display_name),
    };
    let Some(binary) = (spec.discover)() else {
        return report;
    };
    let resolved = fs::canonicalize(&binary).unwrap_or_else(|_| binary.clone());
    let source = install_source(spec, &resolved, dirs::home_dir().as_deref());
    report.installed = true;
    report.installed_version = installed_version_of(&binary);
    report.method = source.method().to_string();
    report.source_label = source.label().to_string();
    let latest = match &source {
        InstallSource::Homebrew(formula) => brew_stable_version(formula),
        _ => npm_metadata(spec.npm_package, "latest").map(|(version, _)| version),
    };
    match latest {
        Ok(latest) => {
            report.update_available = report
                .installed_version
                .as_deref()
                .is_some_and(|installed| compare_versions(&latest, installed) == Ordering::Greater);
            report.can_update = report.update_available && source != InstallSource::Unknown;
            report.message = match (report.update_available, &source) {
                (false, _) => format!("{} is up to date.", spec.display_name),
                (true, InstallSource::Unknown) => format!(
                    "{} {latest} is available. Update it the way you installed it.",
                    spec.display_name
                ),
                (true, _) => format!("{} {latest} is available.", spec.display_name),
            };
            report.latest_version = Some(latest);
        }
        Err(message) => report.message = message,
    }
    report
}

/// Installed version, latest official version and update method for every
/// registered CLI. Read-only: nothing is installed here.
#[tauri::command]
pub(crate) async fn check_cli_provider_versions() -> Vec<CliVersionReport> {
    tauri::async_runtime::spawn_blocking(|| CLI_PROVIDERS.iter().map(version_report).collect())
        .await
        .unwrap_or_default()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliInstallPlan {
    provider_id: String,
    display_name: String,
    package_name: String,
    version: String,
    integrity: String,
    installed_version: Option<String>,
    action: String,
    method: String,
    source_label: String,
    requires_approval: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliInstallRequest {
    provider_id: String,
    version: String,
    integrity: String,
    #[serde(default)]
    method: Option<String>,
    user_approved: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CliInstallResult {
    success: bool,
    message: String,
    version: Option<String>,
    login_command: Option<String>,
}

fn install_plan(spec: &CliProviderSpec) -> Result<CliInstallPlan, String> {
    let binary = (spec.discover)();
    let installed_version = binary.as_deref().and_then(installed_version_of);
    let source = match &binary {
        Some(binary) => {
            let resolved = fs::canonicalize(binary).unwrap_or_else(|_| binary.clone());
            install_source(spec, &resolved, dirs::home_dir().as_deref())
        }
        None if spec.managed_install => InstallSource::AiOsManaged,
        None => {
            return Err(format!(
                "Install {} with its official installer, then return to AI-OS.",
                spec.display_name
            ))
        }
    };
    let (package_name, version, integrity) = match &source {
        InstallSource::Homebrew(formula) => (formula.clone(), brew_stable_version(formula)?, String::new()),
        InstallSource::Unknown => {
            return Err(format!(
                "{} was installed outside Homebrew, npm or its official installer; update it the way you installed it.",
                spec.display_name
            ))
        }
        _ => {
            let (version, integrity) = npm_metadata(spec.npm_package, "latest")?;
            (spec.npm_package.to_string(), version, integrity)
        }
    };
    Ok(CliInstallPlan {
        provider_id: spec.provider_id.to_string(),
        display_name: spec.display_name.to_string(),
        package_name,
        action: if binary.is_some() {
            "upgrade"
        } else {
            "install"
        }
        .to_string(),
        version,
        integrity,
        installed_version,
        method: source.method().to_string(),
        source_label: source.label().to_string(),
        requires_approval: true,
    })
}

#[tauri::command]
pub(crate) async fn prepare_cli_provider_install(
    provider_id: String,
) -> Result<CliInstallPlan, String> {
    tauri::async_runtime::spawn_blocking(move || install_plan(spec_for(&provider_id)?))
        .await
        .map_err(|_| "AI-OS could not prepare the CLI installation plan.".to_string())?
}

#[tauri::command]
pub(crate) async fn install_cli_provider(request: CliInstallRequest) -> CliInstallResult {
    let provider_id = request.provider_id.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || install_cli_provider_blocking(request)).await;
    let spec = spec_for(&provider_id).ok();
    match result {
        Ok(Ok(version)) => CliInstallResult {
            success: true,
            message: format!(
                "Official {} {version} installed and health-checked. Sign in to confirm available models.",
                spec.map_or("CLI", |spec| spec.display_name)
            ),
            version: Some(version),
            login_command: spec.map(|spec| spec.login_command.to_string()),
        },
        Ok(Err(message)) => CliInstallResult { success: false, message, version: None, login_command: None },
        Err(_) => CliInstallResult {
            success: false,
            message: "AI-OS could not complete the CLI installation task.".to_string(),
            version: None,
            login_command: None,
        },
    }
}

fn restore_previous_binary(canonical_binary: &Path, previous_target: &Option<PathBuf>) -> bool {
    let Some(target) = previous_target else {
        return false;
    };
    let _ = fs::remove_file(canonical_binary);
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, canonical_binary).is_ok();
    #[cfg(not(unix))]
    false
}

fn run_checked(mut command: Command, failure: &str) -> Result<(), String> {
    let status = command.status().map_err(|_| failure.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| failure.to_string())
}

fn install_cli_provider_blocking(request: CliInstallRequest) -> Result<String, String> {
    let spec = spec_for(&request.provider_id)?;
    let name = spec.display_name;
    if !request.user_approved {
        return Err(format!(
            "{name} installation requires an explicit approved plan."
        ));
    }
    if (spec.is_busy)() {
        return Err(format!(
            "{name} is busy. Finish or cancel the active request before installing an update."
        ));
    }
    // Re-derive the plan: what runs must be exactly what the user approved.
    let plan = install_plan(spec)?;
    if plan.version != request.version
        || plan.integrity != request.integrity
        || request
            .method
            .as_deref()
            .is_some_and(|method| method != plan.method)
    {
        return Err(format!(
            "The {name} package changed after review. Prepare and approve a new installation plan."
        ));
    }
    let tag = format!(
        "cli-provider id={} method={}",
        spec.provider_id, plan.method
    );
    crate::logs::append_ai_os_diagnostic(&format!("{tag} event=install-started"));
    let home = dirs::home_dir()
        .ok_or_else(|| "AI-OS could not determine the home directory.".to_string())?;
    let canonical_binary = spec.canonical_link.map(|link| home.join(link));
    let previous_target = canonical_binary
        .as_ref()
        .and_then(|link| fs::read_link(link).ok());
    let outcome = match plan.method.as_str() {
        "homebrew" => {
            let mut command = Command::new(tool_binary("brew")?);
            command
                .args(["upgrade", &plan.package_name])
                .env("HOMEBREW_NO_AUTO_UPDATE", "1");
            run_checked(command, &format!("Homebrew could not update {name}."))
        }
        "npm-global" => {
            let mut command = Command::new(tool_binary("npm")?);
            command.args([
                "install",
                "--global",
                &format!("{}@{}", spec.npm_package, plan.version),
                "--no-audit",
                "--no-fund",
            ]);
            run_checked(command, &format!("npm could not update {name}."))
        }
        "official-installer" => {
            let binary =
                (spec.discover)().ok_or_else(|| format!("{name} is no longer installed."))?;
            let arguments: Vec<String> = spec
                .self_update
                .unwrap_or_default()
                .iter()
                .map(|argument| argument.replace("{version}", &plan.version))
                .collect();
            let mut command = Command::new(binary);
            command.args(arguments).current_dir(std::env::temp_dir());
            run_checked(
                command,
                &format!("{name}'s official updater did not complete."),
            )
        }
        _ => {
            let prefix = home.join(".ai-os/providers").join(spec.provider_id);
            let mut command = Command::new(tool_binary("npm")?);
            command.args([
                "install",
                "--prefix",
                prefix.to_string_lossy().as_ref(),
                &format!("{}@{}", spec.npm_package, plan.version),
                "--no-audit",
                "--no-fund",
            ]);
            run_checked(
                command,
                &format!("The approved {name} npm installation failed."),
            )
        }
    };
    let rollback = |event: &str, message: String| -> String {
        let restored = canonical_binary
            .as_ref()
            .is_some_and(|link| restore_previous_binary(link, &previous_target));
        crate::logs::append_ai_os_diagnostic(&format!(
            "{tag} event={event} rollback={}",
            if restored { "restored" } else { "unavailable" }
        ));
        if restored {
            format!("{message} AI-OS restored the previous version.")
        } else {
            message
        }
    };
    if let Err(message) = outcome {
        return Err(rollback("install-failed", message));
    }
    let installed = (spec.discover)().as_deref().and_then(installed_version_of);
    match installed {
        Some(version) if compare_versions(&version, &plan.version) != Ordering::Less => {
            crate::logs::append_ai_os_diagnostic(&format!("{tag} event=health-verified"));
            Ok(version)
        }
        Some(version) => Err(rollback(
            "version-mismatch",
            format!(
                "{name} still reports {version} after the update to {}.",
                plan.version
            ),
        )),
        None => Err(rollback(
            "health-failed",
            format!("{name} failed its health check after the update."),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(provider_id: &str) -> &'static CliProviderSpec {
        spec_for(provider_id).unwrap()
    }

    #[test]
    fn tool_discovery_is_bounded_to_executable_name() {
        let paths = tool_candidates("npm", Some("/one:/two".into()));
        assert!(paths.iter().all(|path| path.ends_with("npm")));
        assert!(paths.contains(&"/opt/homebrew/bin/npm".into()));
    }

    #[test]
    fn every_cli_provider_is_registered_once_with_an_official_package() {
        let mut ids: Vec<_> = CLI_PROVIDERS.iter().map(|spec| spec.provider_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CLI_PROVIDERS.len());
        for spec in CLI_PROVIDERS {
            assert!(spec.npm_package.starts_with('@'), "{}", spec.display_name);
            assert!(!spec.login_command.is_empty());
        }
        assert!(spec_for("openai-api").is_err());
    }

    #[test]
    fn install_source_follows_where_the_user_installed_the_cli() {
        let home = Path::new("/Users/me");
        assert_eq!(
            install_source(
                spec("google"),
                Path::new("/opt/homebrew/Cellar/gemini-cli/0.46.0/libexec/bin/gemini"),
                Some(home)
            ),
            InstallSource::Homebrew("gemini-cli".into())
        );
        assert_eq!(
            install_source(
                spec("google"),
                Path::new("/opt/homebrew/lib/node_modules/@google/gemini-cli/dist/index.js"),
                Some(home)
            ),
            InstallSource::NpmGlobal
        );
        assert_eq!(
            install_source(spec("google"), Path::new("/Users/me/.ai-os/providers/google/node_modules/@google/gemini-cli/dist/index.js"), Some(home)),
            InstallSource::AiOsManaged
        );
        assert_eq!(
            install_source(
                spec("anthropic"),
                Path::new("/Users/me/.local/share/claude/versions/2.1.220"),
                Some(home)
            ),
            InstallSource::OfficialInstaller
        );
        assert_eq!(
            install_source(
                spec("grok"),
                Path::new("/Users/me/.grok/downloads/grok-macos-aarch64"),
                Some(home)
            ),
            InstallSource::OfficialInstaller
        );
        // Gemini has no self-updater: an unrecognised copy is left to the user.
        assert_eq!(
            install_source(
                spec("google"),
                Path::new("/Users/me/bin/gemini"),
                Some(home)
            ),
            InstallSource::Unknown
        );
    }

    #[test]
    fn versions_compare_numerically_with_prereleases_first() {
        assert_eq!(compare_versions("0.66.0", "0.46.0"), Ordering::Greater);
        assert_eq!(compare_versions("2.1.314", "2.1.220"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.70", "1.0.70"), Ordering::Equal);
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.2.0-beta.1", "1.2.0"), Ordering::Less);
        assert_eq!(compare_versions("v1.2", "1.2.0"), Ordering::Equal);
    }

    #[test]
    fn registry_versions_are_validated_before_use_in_commands() {
        assert!(is_version("0.66.0"));
        assert!(is_version("0.1.151-alpha.2"));
        assert!(!is_version("--global"));
        assert!(!is_version("1.0; rm -rf /"));
        assert!(!is_version(""));
    }
}

#[cfg(test)]
mod live_tests {
    /// Read-only: reports this Mac's CLIs against their official channels.
    #[test]
    #[ignore = "queries the installed CLIs, npm and Homebrew"]
    fn live_version_reports() {
        for spec in super::CLI_PROVIDERS {
            println!("{:?}", super::version_report(spec));
        }
    }
}

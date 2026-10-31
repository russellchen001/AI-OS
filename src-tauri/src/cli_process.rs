//! What every official account CLI (Grok CLI, Gemini CLI, …) shares: finding
//! the executable, running one bounded and cancellable headless request in an
//! empty folder, redacting credential-shaped errors, and tracking in-flight
//! operations. A CLI module only supplies its arguments and output parsing.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

/// One CLI's process settings; `label` names it in user-facing errors.
pub(crate) struct CliProcess<'a> {
    pub(crate) label: &'a str,
    pub(crate) workdir_name: &'a str,
    pub(crate) env_set: &'a [(&'a str, &'a str)],
    pub(crate) env_remove: &'a [&'a str],
    /// Lower-case fragments that mark an error line as credential-shaped.
    pub(crate) secret_markers: &'a [&'a str],
}

/// Explicit override first, then the official per-user install locations
/// (relative to home), then PATH, then the Homebrew locations; no duplicates.
pub(crate) fn candidate_paths(
    binary_name: &str,
    explicit: Option<OsString>,
    home: Option<PathBuf>,
    home_locations: &[&str],
    inherited_path: Option<OsString>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit {
        candidates.push(PathBuf::from(path));
    }
    if let Some(home) = home {
        candidates.extend(home_locations.iter().map(|location| home.join(location)));
    }
    if let Some(path) = inherited_path {
        candidates.extend(std::env::split_paths(&path).map(|entry| entry.join(binary_name)));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin").join(binary_name),
        PathBuf::from("/usr/local/bin").join(binary_name),
    ]);
    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(path.clone()));
    candidates
}

pub(crate) fn discover_executable(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| {
        path.is_file()
            && path.metadata().is_ok_and(|metadata| {
                #[cfg(unix)]
                {
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
    })
}

/// The first `x.y.z` in a `--version` line ("grok 1.0.70", "2.1.220 (Claude
/// Code)", "codex-cli 0.146.0").
pub(crate) fn parse_version(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|word| {
        let word = word
            .trim_start_matches('v')
            .trim_matches(|c: char| c == '(' || c == ')' || c == ',');
        let parts: Vec<&str> = word.split('.').collect();
        (parts.len() >= 2
            && parts[0].chars().all(|c| c.is_ascii_digit())
            && !parts[0].is_empty()
            && parts[1].chars().next().is_some_and(|c| c.is_ascii_digit()))
        .then(|| word.to_owned())
    })
}

impl CliProcess<'_> {
    /// An empty working directory, so the CLI never reads project files or
    /// instructions from wherever AI-OS happens to run.
    pub(crate) fn workdir(&self) -> Result<PathBuf, String> {
        let directory = std::env::temp_dir().join(self.workdir_name);
        std::fs::create_dir_all(&directory)
            .map_err(|_| format!("{} working folder could not be prepared", self.label))?;
        Ok(directory)
    }

    pub(crate) fn sanitized_error(&self, stderr: &[u8]) -> String {
        let value = String::from_utf8_lossy(stderr);
        let fallback = format!("{} could not complete the request", self.label);
        let lines: Vec<&str> = value
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        // The line naming the error, not a leading warning or log line
        // (Gemini CLI prints a terminal-colour warning first).
        let line = lines
            .iter()
            .find(|line| line.to_lowercase().contains("error"))
            .or_else(|| {
                lines
                    .iter()
                    .find(|line| !line.to_lowercase().starts_with("warning"))
            })
            .or(lines.first())
            .copied()
            .unwrap_or(&fallback);
        let safe: String = line
            .chars()
            .filter(|character| !character.is_control())
            .take(240)
            .collect();
        let lower = safe.to_lowercase();
        if lower.contains("bearer ")
            || lower.contains("token")
            || self
                .secret_markers
                .iter()
                .any(|marker| lower.contains(marker))
        {
            format!("{} returned a credential-related error", self.label)
        } else {
            safe
        }
    }

    /// A JSON `error.message` the CLI reported (on either stream, possibly
    /// after log lines); otherwise the first line of stderr, else stdout.
    fn failure_message(&self, stderr: &[u8], stdout: &[u8]) -> String {
        let json_message = |bytes: &[u8]| {
            let text = String::from_utf8_lossy(bytes);
            let start = text.find('{')?;
            let end = text.rfind('}')?;
            let value: serde_json::Value = serde_json::from_str(text.get(start..=end)?).ok()?;
            value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        if let Some(message) = json_message(stderr).or_else(|| json_message(stdout)) {
            return self.sanitized_error(message.as_bytes());
        }
        if !stderr.iter().all(u8::is_ascii_whitespace) {
            return self.sanitized_error(stderr);
        }
        self.sanitized_error(stdout)
    }

    pub(crate) fn run(
        &self,
        binary: &Path,
        arguments: &[&str],
        stdin: Option<&str>,
        timeout: Duration,
        cancellation: Option<&CancellationToken>,
    ) -> Result<Vec<u8>, String> {
        let label = self.label;
        let mut command = Command::new(binary);
        command.args(arguments).current_dir(self.workdir()?);
        for name in self.env_remove {
            command.env_remove(name);
        }
        for (name, value) in self.env_set {
            command.env(name, value);
        }
        command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|_| format!("{label} could not be started"))?;
        // Drain both pipes while the child is running; waiting first can
        // deadlock once a complete long answer fills the OS pipe buffer.
        let read_pipe = |mut pipe: Box<dyn Read + Send>| {
            let unreadable = format!("{label} output could not be read");
            let too_large = format!("{label} output exceeded the response limit");
            thread::spawn(move || {
                let mut bytes = Vec::new();
                pipe.by_ref()
                    .take(2_000_001)
                    .read_to_end(&mut bytes)
                    .map_err(|_| unreadable.clone())?;
                std::io::copy(&mut pipe, &mut std::io::sink()).map_err(|_| unreadable)?;
                if bytes.len() > 2_000_000 {
                    return Err(too_large);
                }
                Ok(bytes)
            })
        };
        let stdout = read_pipe(Box::new(child.stdout.take().expect("piped stdout")));
        let stderr = read_pipe(Box::new(child.stderr.take().expect("piped stderr")));
        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(input.as_bytes())
                .map_err(|_| format!("{label} could not receive the request"))?;
        }
        let started = Instant::now();
        loop {
            if cancellation.is_some_and(CancellationToken::is_cancelled) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{label} request was cancelled"));
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let output = stdout
                        .join()
                        .map_err(|_| format!("{label} output reader was interrupted"))??;
                    let error = stderr
                        .join()
                        .map_err(|_| format!("{label} error reader was interrupted"))??;
                    return if status.success() {
                        Ok(output)
                    } else {
                        Err(self.failure_message(&error, &output))
                    };
                }
                Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{label} request timed out"));
                }
                Err(_) => return Err(format!("{label} process state is unavailable")),
            }
        }
    }
}

/// In-flight requests of one CLI, so a request can be cancelled and an
/// update can wait until the CLI is idle.
pub(crate) struct OperationRegistry(Mutex<HashMap<String, CancellationToken>>);

impl OperationRegistry {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.0.lock().is_ok_and(|requests| !requests.is_empty())
    }

    pub(crate) fn register(
        &self,
        label: &str,
        operation_id: &str,
    ) -> Result<CancellationToken, String> {
        let token = CancellationToken::new();
        let mut requests = self
            .0
            .lock()
            .map_err(|_| format!("{label} request state is unavailable"))?;
        // Checked before inserting so a duplicate never replaces (and
        // orphans) the running request's cancellation.
        if requests.contains_key(operation_id) {
            return Err(format!("{label} operation is already active"));
        }
        requests.insert(operation_id.to_owned(), token.clone());
        Ok(token)
    }

    pub(crate) fn finish(&self, operation_id: &str) {
        if let Ok(mut requests) = self.0.lock() {
            requests.remove(operation_id);
        }
    }

    pub(crate) fn cancel(&self, label: &str, operation_id: &str) -> Result<bool, String> {
        let token = self
            .0
            .lock()
            .map_err(|_| format!("{label} request state is unavailable"))?
            .remove(operation_id);
        Ok(token.map(|token| token.cancel()).is_some())
    }
}

fn is_identifier(value: &str) -> bool {
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

pub(crate) fn validate_operation_id(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || !is_identifier(value) {
        return Err(format!("{label} operation ID is invalid"));
    }
    Ok(value.to_owned())
}

pub(crate) fn validate_model_id(label: &str, value: &str) -> Result<(), String> {
    // A leading "-" would be read by the CLI as another option.
    if !value.starts_with(|character: char| character.is_ascii_alphanumeric())
        || value.len() > 200
        || !is_identifier(value)
    {
        return Err(format!("{label} model ID is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHELL: CliProcess = CliProcess {
        label: "Test CLI",
        workdir_name: "ai-os-cli-process-test",
        env_set: &[],
        env_remove: &[],
        secret_markers: &["xai-"],
    };

    #[cfg(unix)]
    #[test]
    fn long_stdout_and_stderr_are_drained_before_process_exit() {
        let output = SHELL
            .run(
                Path::new("/bin/sh"),
                &[
                    "-c",
                    "head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2",
                ],
                None,
                Duration::from_secs(5),
                None,
            )
            .expect("long CLI output must not deadlock on a full pipe");
        assert_eq!(output.len(), 131072);
    }

    #[cfg(unix)]
    #[test]
    fn failures_reported_only_on_stdout_are_surfaced() {
        let error = SHELL
            .run(
                Path::new("/bin/sh"),
                &["-c", "echo quota exhausted; exit 1"],
                None,
                Duration::from_secs(5),
                None,
            )
            .unwrap_err();
        assert_eq!(error, "quota exhausted");
        let json = SHELL
            .run(
                Path::new("/bin/sh"),
                &[
                    "-c",
                    "printf '{\\n \"error\": {\"message\": \"Quota exceeded\"}}'; exit 1",
                ],
                None,
                Duration::from_secs(5),
                None,
            )
            .unwrap_err();
        assert_eq!(json, "Quota exceeded");
        let logged = SHELL
            .run(Path::new("/bin/sh"), &["-c", "echo 'Loaded cached credentials.' >&2; printf '{\\n  \"error\": {\"message\": \"Please set an Auth method\"}\\n}' >&2; exit 41"], None, Duration::from_secs(5), None)
            .unwrap_err();
        assert_eq!(logged, "Please set an Auth method");
    }

    #[test]
    fn candidates_cover_home_locations_and_path_without_duplicates() {
        let home = PathBuf::from("/tmp/cli-home");
        let path = std::env::join_paths(["/tmp/cli-path", "/tmp/cli-home/.local/bin"]).unwrap();
        let candidates = candidate_paths(
            "tool",
            None,
            Some(home.clone()),
            &[".tool/bin/tool", ".local/bin/tool"],
            Some(path),
        );
        assert_eq!(candidates[0], home.join(".tool/bin/tool"));
        assert!(candidates.contains(&PathBuf::from("/tmp/cli-path/tool")));
        assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/tool")));
        let unique: HashSet<_> = candidates.iter().collect();
        assert_eq!(unique.len(), candidates.len());
    }

    #[test]
    fn non_executable_cli_is_not_reported_as_installed() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("tool");
        std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
        assert_eq!(discover_executable(vec![binary]), None);
    }

    #[test]
    fn versions_are_read_from_common_version_lines() {
        assert_eq!(parse_version("grok 1.0.70").as_deref(), Some("1.0.70"));
        assert_eq!(
            parse_version("2.1.220 (Claude Code)").as_deref(),
            Some("2.1.220")
        );
        assert_eq!(parse_version("0.46.0\n").as_deref(), Some("0.46.0"));
        assert_eq!(
            parse_version("codex-cli v0.146.0").as_deref(),
            Some("0.146.0")
        );
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn credential_shaped_errors_are_redacted() {
        assert_eq!(
            SHELL.sanitized_error(b"Authorization: Bearer private-value"),
            "Test CLI returned a credential-related error"
        );
        assert_eq!(
            SHELL.sanitized_error(b"key xai-abc"),
            "Test CLI returned a credential-related error"
        );
        assert_eq!(SHELL.sanitized_error(b"rate limited\nmore"), "rate limited");
        assert_eq!(
            SHELL.sanitized_error(b"Warning: 256-color support not detected.\nError authenticating: IneligibleTierError: client unsupported\n    at frame"),
            "Error authenticating: IneligibleTierError: client unsupported"
        );
        assert_eq!(SHELL.sanitized_error(b"Warning: slow terminal\nquota exhausted"), "quota exhausted");
    }

    #[test]
    fn ids_are_bounded_and_path_free() {
        assert!(validate_model_id("Test", "gemini-2.5-pro").is_ok());
        assert!(validate_model_id("Test", "--yolo").is_err());
        assert!(validate_model_id("Test", "a b").is_err());
        assert!(validate_operation_id("Test", "../../secret").is_err());
    }

    #[test]
    fn cancellation_is_scoped_and_consumes_the_operation() {
        let registry = OperationRegistry::new();
        let token = registry.register("Test", "op-1").unwrap();
        assert!(registry.is_busy());
        assert!(registry.register("Test", "op-1").is_err());
        assert!(registry.cancel("Test", "op-1").unwrap());
        assert!(token.is_cancelled());
        assert!(!registry.cancel("Test", "op-1").unwrap());
        assert!(!registry.is_busy());
    }
}

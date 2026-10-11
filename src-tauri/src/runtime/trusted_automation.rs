use serde::Deserialize;
use std::{collections::HashSet, error::Error, fmt, fs, path::Path};

const TRUSTED_AUTOMATION_FILE: &str = "trusted-automation.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustedAutomationSettings {
    #[serde(default)]
    trusted_automation_capabilities: Vec<String>,
}

impl TrustedAutomationSettings {
    pub(crate) fn allowed_capabilities(&self) -> HashSet<String> {
        self.trusted_automation_capabilities
            .iter()
            .map(|capability| capability.trim().to_owned())
            .filter(|capability| !capability.is_empty())
            .collect()
    }
}

#[derive(Debug)]
pub(crate) enum TrustedAutomationConfigError {
    ConfigurationDirectoryUnavailable,
    Read,
    Malformed,
}

impl fmt::Display for TrustedAutomationConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ConfigurationDirectoryUnavailable => {
                "trusted automation configuration directory is unavailable"
            }
            Self::Read => "trusted automation configuration could not be read",
            Self::Malformed => "trusted automation configuration is malformed",
        })
    }
}

impl Error for TrustedAutomationConfigError {}

pub(crate) fn load_trusted_automation_settings(
) -> Result<TrustedAutomationSettings, TrustedAutomationConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        diagnose(&TrustedAutomationConfigError::ConfigurationDirectoryUnavailable);
        TrustedAutomationConfigError::ConfigurationDirectoryUnavailable
    })?;
    load_trusted_automation_settings_at(&base.join("AI OS").join(TRUSTED_AUTOMATION_FILE))
}

fn diagnose(error: &TrustedAutomationConfigError) {
    let message = match error {
        TrustedAutomationConfigError::ConfigurationDirectoryUnavailable => {
            "trusted automation disabled: configuration directory unavailable"
        }
        TrustedAutomationConfigError::Read => {
            "trusted automation disabled: configuration file unreadable"
        }
        TrustedAutomationConfigError::Malformed => {
            "trusted automation disabled: configuration file malformed"
        }
    };
    crate::logs::append_ai_os_diagnostic(message);
}

fn load_trusted_automation_settings_at(
    path: &Path,
) -> Result<TrustedAutomationSettings, TrustedAutomationConfigError> {
    load_trusted_automation_settings_at_with_diagnostic(path, diagnose)
}

fn load_trusted_automation_settings_at_with_diagnostic(
    path: &Path,
    diagnostic: impl Fn(&TrustedAutomationConfigError),
) -> Result<TrustedAutomationSettings, TrustedAutomationConfigError> {
    if !path.exists() {
        return Ok(TrustedAutomationSettings::default());
    }

    let contents = fs::read_to_string(path).map_err(|_| {
        let error = TrustedAutomationConfigError::Read;
        diagnostic(&error);
        error
    })?;
    serde_json::from_str(&contents).map_err(|_| {
        let error = TrustedAutomationConfigError::Malformed;
        diagnostic(&error);
        error
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn load(
        contents: Option<&str>,
    ) -> Result<TrustedAutomationSettings, TrustedAutomationConfigError> {
        let directory = tempdir().unwrap();
        let path = directory.path().join(TRUSTED_AUTOMATION_FILE);
        if let Some(contents) = contents {
            fs::write(&path, contents).unwrap();
        }
        load_trusted_automation_settings_at(&path)
    }

    #[test]
    fn missing_trusted_automation_configuration_defaults_to_empty() {
        assert!(load(None).unwrap().allowed_capabilities().is_empty());
    }

    #[test]
    fn existing_configuration_without_new_field_remains_valid() {
        assert!(load(Some("{}")).unwrap().allowed_capabilities().is_empty());
    }

    #[test]
    fn explicitly_configured_capability_is_loaded() {
        assert!(load(Some(
            r#"{"trustedAutomationCapabilities":["filesystem.scan"]}"#
        ))
        .unwrap()
        .allowed_capabilities()
        .contains("filesystem.scan"));
    }

    #[test]
    fn duplicate_capabilities_are_normalized_and_empty_entries_ignored() {
        let settings = load(Some(
            r#"{"trustedAutomationCapabilities":[" filesystem.scan ","filesystem.scan"," "]}"#,
        ))
        .unwrap();

        assert_eq!(
            settings.allowed_capabilities(),
            HashSet::from(["filesystem.scan".to_owned()])
        );
    }

    #[test]
    fn malformed_configuration_fails_closed() {
        assert!(matches!(
            load(Some(r#"{"trustedAutomationCapabilities":"*"}"#)),
            Err(TrustedAutomationConfigError::Malformed)
        ));
    }

    #[test]
    fn malformed_configuration_emits_only_sanitized_diagnostic() {
        let directory = tempdir().unwrap();
        let path = directory.path().join(TRUSTED_AUTOMATION_FILE);
        fs::write(&path, r#"{"trustedAutomationCapabilities":["secret",]}"#).unwrap();
        let diagnostics = std::sync::Mutex::new(Vec::new());

        let result = load_trusted_automation_settings_at_with_diagnostic(&path, |error| {
            diagnostics.lock().unwrap().push(error.to_string());
        });

        assert!(matches!(
            result,
            Err(TrustedAutomationConfigError::Malformed)
        ));
        assert_eq!(
            diagnostics.into_inner().unwrap(),
            vec!["trusted automation configuration is malformed"]
        );
    }

    #[test]
    fn no_default_capability_is_seeded() {
        assert_eq!(
            TrustedAutomationSettings::default()
                .allowed_capabilities()
                .len(),
            0
        );
    }
}


/// Whether the user has permanently authorized public web search.
#[tauri::command]
pub(crate) fn get_web_search_trust() -> Result<bool, String> {
    load_trusted_automation_settings()
        .map(|settings| settings.allowed_capabilities().contains("browser.search"))
        .map_err(|error| error.to_string())
}

/// Change only the persistent browser.search grant.
///
/// Never accept an Agent-supplied capability name here. This command is
/// intentionally limited to a single capability and does not execute a Skill.
#[tauri::command]
pub(crate) fn set_web_search_trust(enabled: bool) -> Result<bool, String> {
    use std::io::Write;

    let base = dirs::config_dir()
        .ok_or_else(|| "AI-OS configuration directory unavailable".to_owned())?;

    let directory = base.join("AI OS");
    let path = directory.join(TRUSTED_AUTOMATION_FILE);

    // Refuse to modify malformed configurations instead of replacing them.
    let mut document: serde_json::Value = if path.exists() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|_| "Unable to read existing AI-OS permissions".to_owned())?;

        serde_json::from_str(&contents)
            .map_err(|_| "Existing AI-OS permissions are malformed".to_owned())?
    } else {
        serde_json::json!({})
    };

    let object = document.as_object_mut()
        .ok_or_else(|| "Existing AI-OS permissions must be a JSON object".to_owned())?;

    let old = object.get("trustedAutomationCapabilities").cloned();

    let mut capabilities: Vec<String> = match old {
        None => Vec::new(),
        Some(serde_json::Value::Array(items)) => {
            let mut values = Vec::new();
            for item in items {
                let value = item.as_str()
                    .ok_or_else(|| "Existing permissions contain an invalid capability".to_owned())?;
                values.push(value.to_owned());
            }
            values
        }
        Some(_) => {
            return Err("Existing permissions have an invalid capability list".to_owned());
        }
    };

    capabilities.retain(|capability| capability.trim() != "browser.search");

    if enabled {
        capabilities.push("browser.search".to_owned());
    }

    object.insert(
        "trustedAutomationCapabilities".to_owned(),
        serde_json::json!(capabilities),
    );

    std::fs::create_dir_all(&directory)
        .map_err(|_| "Unable to create AI-OS configuration directory".to_owned())?;

    let temporary = directory.join(format!(
        ".trusted-automation-{}.tmp",
        uuid::Uuid::new_v4()
    ));

    let result = (|| -> Result<(), String> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options.open(&temporary)
            .map_err(|_| "Unable to create temporary permissions file".to_owned())?;

        serde_json::to_writer_pretty(&mut file, &document)
            .map_err(|_| "Unable to serialize permissions".to_owned())?;

        file.write_all(b"\n")
            .map_err(|_| "Unable to complete permissions file".to_owned())?;

        file.sync_all()
            .map_err(|_| "Unable to flush permissions file".to_owned())?;

        std::fs::rename(&temporary, &path)
            .map_err(|_| "Unable to save permissions".to_owned())?;

        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }

    result?;
    Ok(enabled)
}

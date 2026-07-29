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

use super::{provider::BrowserRequest, registry::resolve_browser_provider};

use crate::planner::{EvidenceMetadata, EvidenceState};
use serde_json::Value;

pub fn execute_browser_capability(capability: &str, input: Value) -> Result<Value, String> {
    let provider = resolve_browser_provider("mcp-browser")
        .ok_or_else(|| "No browser provider available.".to_string())?;

    let action = capability.split('.').last().unwrap_or("search").to_string();

    let response = provider.execute(BrowserRequest { action, input })?;
    let evidence = EvidenceMetadata::observed(
        capability,
        &response.provider,
        browser_evidence_state(capability, &response.output),
    );

    Ok(serde_json::json!({
        "provider": response.provider,
        "output": response.output,
        "evidence": evidence
    }))
}

fn browser_evidence_state(capability: &str, output: &Value) -> EvidenceState {
    if output
        .get("executable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        EvidenceState::Executable
    } else if output
        .get("authenticated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        EvidenceState::Authenticated
    } else if capability == "browser.search" {
        EvidenceState::Discovered
    } else {
        EvidenceState::Verified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn browser_evidence_distinguishes_discovery_and_verified_states() {
        assert_eq!(
            browser_evidence_state("browser.search", &json!({})),
            EvidenceState::Discovered
        );
        assert_eq!(
            browser_evidence_state("browser.control", &json!({})),
            EvidenceState::Verified
        );
        assert_eq!(
            browser_evidence_state("browser.control", &json!({"authenticated": true})),
            EvidenceState::Authenticated
        );
        assert_eq!(
            browser_evidence_state("browser.control", &json!({"executable": true})),
            EvidenceState::Executable
        );
    }
}

use crate::planner::EvidenceState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const INSTANCE_ID: &str = "google-workspace-default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GoogleResourceRef {
    file_id: String,
    range: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GoogleWorkspaceResult {
    resource: GoogleResourceRef,
    values: Value,
    provider: &'static str,
    observed_at: String,
    evidence: EvidenceState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GoogleResourceInput {
    resource: GoogleResourceRef,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GoogleCreateInput {
    title: String,
    values: Option<Vec<Vec<Value>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GoogleWriteInput {
    resource: GoogleResourceRef,
    values: Vec<Vec<Value>>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "AI-OS could not initialize Google Workspace".to_owned())
}

async fn request(
    method: reqwest::Method,
    url: String,
    body: Option<Value>,
) -> Result<Value, String> {
    let token = crate::providers::provider_access_token(INSTANCE_ID).await?;
    let mut request = client()?.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "Google Workspace request timed out".to_owned()
        } else {
            "Google Workspace could not be reached".to_owned()
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => "Google Workspace authorization expired; reconnect the account".to_owned(),
            403 => "Google Workspace permission was not granted".to_owned(),
            404 => "Google Workspace resource was not found".to_owned(),
            429 => "Google Workspace rate limit reached".to_owned(),
            code if code >= 500 => "Google Workspace is temporarily unavailable".to_owned(),
            code => format!("Google Workspace request failed (HTTP {code})"),
        });
    }
    response
        .json()
        .await
        .map_err(|_| "Google Workspace returned unreadable data".to_owned())
}

#[tauri::command]
pub(crate) async fn get_google_workspace_identity() -> Result<Value, String> {
    request(
        reqwest::Method::GET,
        "https://www.googleapis.com/oauth2/v3/userinfo".to_owned(),
        None,
    )
    .await
}

#[tauri::command]
pub(crate) async fn list_google_workspace_files() -> Result<Value, String> {
    request(reqwest::Method::GET, "https://www.googleapis.com/drive/v3/files?pageSize=100&fields=files(id,name,mimeType,modifiedTime)&q=trashed%3Dfalse".to_owned(), None).await
}

fn result(
    resource: GoogleResourceRef,
    values: Value,
    evidence: EvidenceState,
) -> GoogleWorkspaceResult {
    GoogleWorkspaceResult {
        resource,
        values,
        provider: "google-workspace",
        observed_at: chrono::Utc::now().to_rfc3339(),
        evidence,
    }
}

#[tauri::command]
pub(crate) async fn read_google_document(
    input: GoogleResourceInput,
) -> Result<GoogleWorkspaceResult, String> {
    let url = format!(
        "https://docs.googleapis.com/v1/documents/{}",
        input.resource.file_id
    );
    let value = request(reqwest::Method::GET, url, None).await?;
    Ok(result(input.resource, value, EvidenceState::Authenticated))
}

#[tauri::command]
pub(crate) async fn create_google_document(
    input: GoogleCreateInput,
) -> Result<GoogleWorkspaceResult, String> {
    let created = request(
        reqwest::Method::POST,
        "https://docs.googleapis.com/v1/documents".to_owned(),
        Some(serde_json::json!({"title": input.title})),
    )
    .await?;
    let id = created
        .get("documentId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Google Docs create returned no document id".to_owned())?
        .to_owned();
    let resource = GoogleResourceRef {
        file_id: id,
        range: None,
    };
    let read = read_google_document(GoogleResourceInput {
        resource: resource.clone(),
    })
    .await?;
    Ok(result(resource, read.values, EvidenceState::Completed))
}

#[tauri::command]
pub(crate) async fn read_google_spreadsheet(
    input: GoogleResourceInput,
) -> Result<GoogleWorkspaceResult, String> {
    let range = input.resource.range.as_deref().unwrap_or("A:ZZ");
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        input.resource.file_id, range
    );
    let value = request(reqwest::Method::GET, url, None).await?;
    Ok(result(input.resource, value, EvidenceState::Authenticated))
}

#[tauri::command]
pub(crate) async fn create_google_spreadsheet(
    input: GoogleCreateInput,
) -> Result<GoogleWorkspaceResult, String> {
    let created = request(
        reqwest::Method::POST,
        "https://sheets.googleapis.com/v4/spreadsheets".to_owned(),
        Some(serde_json::json!({"properties":{"title":input.title}})),
    )
    .await?;
    let id = created
        .get("spreadsheetId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Google Sheets create returned no spreadsheet id".to_owned())?
        .to_owned();
    let resource = GoogleResourceRef {
        file_id: id,
        range: Some("Sheet1!A1".to_owned()),
    };
    if let Some(values) = input.values {
        return write_google_spreadsheet(GoogleWriteInput { resource, values }).await;
    }
    Ok(result(resource, created, EvidenceState::Completed))
}

fn numeric_equal(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() < f64::EPSILON,
        _ => left == right,
    }
}

#[tauri::command]
pub(crate) async fn write_google_spreadsheet(
    input: GoogleWriteInput,
) -> Result<GoogleWorkspaceResult, String> {
    let range = input
        .resource
        .range
        .as_deref()
        .ok_or_else(|| "Google Sheets write requires an explicit range".to_owned())?;
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueInputOption=USER_ENTERED",
        input.resource.file_id, range
    );
    request(
        reqwest::Method::PUT,
        url,
        Some(serde_json::json!({"values": input.values})),
    )
    .await?;
    let read = read_google_spreadsheet(GoogleResourceInput {
        resource: input.resource.clone(),
    })
    .await?;
    let actual = read
        .values
        .get("values")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let expected: Vec<Value> = input.values.iter().cloned().map(Value::Array).collect();
    let matches = expected.len() == actual.len()
        && expected
            .iter()
            .zip(&actual)
            .all(|(a, b)| match (a.as_array(), b.as_array()) {
                (Some(a), Some(b)) => {
                    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| numeric_equal(x, y))
                }
                _ => false,
            });
    if !matches {
        return Err("Google Sheets write read-back validation failed".to_owned());
    }
    Ok(result(
        input.resource,
        read.values,
        EvidenceState::Completed,
    ))
}

#[tauri::command]
pub(crate) async fn read_google_presentation(
    input: GoogleResourceInput,
) -> Result<GoogleWorkspaceResult, String> {
    let url = format!(
        "https://slides.googleapis.com/v1/presentations/{}",
        input.resource.file_id
    );
    let value = request(reqwest::Method::GET, url, None).await?;
    Ok(result(input.resource, value, EvidenceState::Authenticated))
}

#[tauri::command]
pub(crate) async fn create_google_presentation(
    input: GoogleCreateInput,
) -> Result<GoogleWorkspaceResult, String> {
    let created = request(
        reqwest::Method::POST,
        "https://slides.googleapis.com/v1/presentations".to_owned(),
        Some(serde_json::json!({"title": input.title})),
    )
    .await?;
    let id = created
        .get("presentationId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Google Slides create returned no presentation id".to_owned())?
        .to_owned();
    let resource = GoogleResourceRef {
        file_id: id,
        range: None,
    };
    let read = read_google_presentation(GoogleResourceInput {
        resource: resource.clone(),
    })
    .await?;
    if read
        .values
        .get("slides")
        .and_then(Value::as_array)
        .is_none()
    {
        return Err("Google Slides create read-back validation failed".to_owned());
    }
    Ok(result(resource, read.values, EvidenceState::Completed))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refs_and_results_have_no_secret_fields() {
        let value = serde_json::to_value(GoogleResourceRef {
            file_id: "id".into(),
            range: None,
        })
        .unwrap();
        for key in [
            "password",
            "accessToken",
            "refreshToken",
            "authorizationCode",
            "verifier",
            "clientSecret",
        ] {
            assert!(value.get(key).is_none());
        }
    }
    #[test]
    fn sheets_validation_allows_numeric_equivalence() {
        assert!(numeric_equal(&Value::from(42), &Value::from(42.0)));
    }
}

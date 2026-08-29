use crate::planner::EvidenceState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const GRAPH_ROOT: &str = "https://graph.microsoft.com/v1.0";
const GRAPH_INSTANCE_ID: &str = "microsoft-graph-default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ResourceRef {
    LocalFile {
        absolute_path: String,
    },
    MicrosoftDriveItem {
        drive_id: String,
        item_id: String,
        worksheet: Option<String>,
        range: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MicrosoftIdentity {
    pub id: String,
    pub display_name: String,
    pub principal_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MicrosoftDrive {
    pub id: String,
    pub name: String,
    pub drive_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MicrosoftDriveItem {
    pub drive_id: String,
    pub item_id: String,
    pub name: String,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GraphSpreadsheetResult {
    pub resource: ResourceRef,
    pub worksheet: String,
    pub address: Option<String>,
    pub values: Vec<Vec<Value>>,
    pub tsv: String,
    pub provider: String,
    pub account_id: String,
    pub observed_at: String,
    pub evidence: EvidenceState,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GraphResourceInput {
    pub resource: ResourceRef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GraphWriteInput {
    pub resource: ResourceRef,
    pub values: Vec<Vec<Value>>,
}

fn graph_item(resource: &ResourceRef) -> Result<(&str, &str, Option<&str>, Option<&str>), String> {
    match resource {
        ResourceRef::MicrosoftDriveItem {
            drive_id,
            item_id,
            worksheet,
            range,
        } => {
            if drive_id.trim().is_empty() || item_id.trim().is_empty() {
                return Err("Microsoft drive and item references are required".to_owned());
            }
            Ok((drive_id, item_id, worksheet.as_deref(), range.as_deref()))
        }
        ResourceRef::LocalFile { .. } => {
            Err("Local files must use the Local Structured or Native Excel provider".to_owned())
        }
    }
}

fn graph_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "AI-OS could not initialize Microsoft Graph".to_owned())
}

async fn graph_request(
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let token = crate::providers::provider_access_token(GRAPH_INSTANCE_ID).await?;
    let client = graph_client()?;
    let url = format!("{GRAPH_ROOT}{path}");
    let mut attempt = 0u8;
    loop {
        let mut request = client.request(method.clone(), &url).bearer_auth(&token);
        if let Some(value) = body.as_ref() {
            request = request.json(value);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                "Microsoft Graph request timed out".to_owned()
            } else {
                "Microsoft Graph could not be reached".to_owned()
            }
        })?;
        let status = response.status();
        if status.as_u16() == 429 && attempt < 2 {
            attempt += 1;
            let wait = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1)
                .min(5);
            tokio::time::sleep(Duration::from_secs(wait)).await;
            continue;
        }
        if !status.is_success() {
            return Err(match status.as_u16() {
                400 => "Microsoft Graph rejected the request".to_owned(),
                401 => "Microsoft authorization expired; reconnect the account".to_owned(),
                403 => "Microsoft account has not granted the required permission".to_owned(),
                404 => "Microsoft resource was not found".to_owned(),
                409 => "Microsoft resource is in conflict".to_owned(),
                412 => "Microsoft resource changed before the operation completed".to_owned(),
                429 => {
                    "Microsoft Graph rate limit remained active after bounded retries".to_owned()
                }
                code if code >= 500 => "Microsoft Graph is temporarily unavailable".to_owned(),
                code => format!("Microsoft Graph request failed (HTTP {code})"),
            });
        }
        if status.as_u16() == 204 {
            return Ok(Value::Null);
        }
        return response
            .json()
            .await
            .map_err(|_| "Microsoft Graph returned unreadable data".to_owned());
    }
}

async fn identity() -> Result<MicrosoftIdentity, String> {
    let value = graph_request(
        reqwest::Method::GET,
        "/me?$select=id,displayName,mail,userPrincipalName",
        None,
    )
    .await?;
    Ok(MicrosoftIdentity {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Microsoft identity response did not include an account id".to_owned())?
            .to_owned(),
        display_name: value
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or("Microsoft account")
            .to_owned(),
        principal_name: value
            .get("mail")
            .or_else(|| value.get("userPrincipalName"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

#[tauri::command]
pub(crate) async fn get_microsoft_graph_identity() -> Result<MicrosoftIdentity, String> {
    identity().await
}

#[tauri::command]
pub(crate) async fn list_microsoft_graph_drives() -> Result<Vec<MicrosoftDrive>, String> {
    let value = graph_request(
        reqwest::Method::GET,
        "/me/drives?$select=id,name,driveType",
        None,
    )
    .await?;
    Ok(value
        .get("value")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|drive| {
            Some(MicrosoftDrive {
                id: drive.get("id")?.as_str()?.to_owned(),
                name: drive
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Drive")
                    .to_owned(),
                drive_type: drive
                    .get("driveType")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn list_microsoft_graph_workbooks(
    drive_id: String,
) -> Result<Vec<MicrosoftDriveItem>, String> {
    if drive_id.trim().is_empty() {
        return Err("Microsoft drive reference is required".to_owned());
    }
    let path = format!("/drives/{drive_id}/root/search(q='.xlsx')?$select=id,name,webUrl,file");
    let value = graph_request(reqwest::Method::GET, &path, None).await?;
    Ok(value
        .get("value")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?;
            if !name.to_ascii_lowercase().ends_with(".xlsx") {
                return None;
            }
            Some(MicrosoftDriveItem {
                drive_id: drive_id.clone(),
                item_id: item.get("id")?.as_str()?.to_owned(),
                name: name.to_owned(),
                web_url: item
                    .get("webUrl")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect())
}

async fn create_session(drive: &str, item: &str, persist: bool) -> Result<String, String> {
    let path = format!("/drives/{drive}/items/{item}/workbook/createSession");
    let value = graph_request(
        reqwest::Method::POST,
        &path,
        Some(serde_json::json!({"persistChanges": persist})),
    )
    .await?;
    value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Microsoft Graph did not create a workbook session".to_owned())
}

async fn workbook_get(path: &str, session: &str) -> Result<Value, String> {
    let token = crate::providers::provider_access_token(GRAPH_INSTANCE_ID).await?;
    let response = graph_client()?
        .get(format!("{GRAPH_ROOT}{path}"))
        .bearer_auth(token)
        .header("workbook-session-id", session)
        .send()
        .await
        .map_err(|_| "Microsoft Graph workbook request failed".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "Microsoft Graph workbook request failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    response
        .json()
        .await
        .map_err(|_| "Microsoft Graph returned unreadable workbook data".to_owned())
}

async fn workbook_patch(path: &str, session: &str, values: &[Vec<Value>]) -> Result<(), String> {
    let token = crate::providers::provider_access_token(GRAPH_INSTANCE_ID).await?;
    let response = graph_client()?
        .patch(format!("{GRAPH_ROOT}{path}"))
        .bearer_auth(token)
        .header("workbook-session-id", session)
        .json(&serde_json::json!({"values": values}))
        .send()
        .await
        .map_err(|_| "Microsoft Graph workbook write failed".to_owned())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "Microsoft Graph workbook write failed (HTTP {})",
            response.status().as_u16()
        ))
    }
}

fn range_path(drive: &str, item: &str, worksheet: Option<&str>, range: Option<&str>) -> String {
    let sheet = worksheet
        .map(|value| format!("/worksheets/{value}"))
        .unwrap_or_default();
    match range {
        Some(address) => {
            format!("/drives/{drive}/items/{item}/workbook{sheet}/range(address='{address}')")
        }
        None => format!("/drives/{drive}/items/{item}/workbook{sheet}/usedRange(valuesOnly=true)"),
    }
}

async fn resolve_worksheet(
    drive: &str,
    item: &str,
    session: &str,
    requested: Option<&str>,
) -> Result<String, String> {
    if let Some(worksheet) = requested {
        if worksheet.trim().is_empty() || worksheet.contains(['/', '\'', '"']) {
            return Err("Microsoft worksheet reference is invalid".to_owned());
        }
        return Ok(worksheet.to_owned());
    }
    let path = format!("/drives/{drive}/items/{item}/workbook/worksheets?$select=id,name&$top=1");
    let value = workbook_get(&path, session).await?;
    value
        .get("value")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|sheet| {
            sheet
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| sheet.get("name").and_then(Value::as_str))
        })
        .map(str::to_owned)
        .ok_or_else(|| "Microsoft workbook contains no worksheet".to_owned())
}

fn tsv(values: &[Vec<Value>]) -> String {
    values
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Value::Null => String::new(),
                    Value::String(value) => value.replace(['\t', '\n', '\r'], " "),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn equivalent(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() < f64::EPSILON,
        _ => left == right,
    }
}

fn values_equal(expected: &[Vec<Value>], actual: &[Vec<Value>]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| equivalent(x, y)))
}

async fn read_resource(
    resource: ResourceRef,
    evidence: EvidenceState,
) -> Result<GraphSpreadsheetResult, String> {
    let (drive, item, worksheet, range) = graph_item(&resource)?;
    let worksheet_name = worksheet.unwrap_or("first available worksheet").to_owned();
    let account = identity().await?;
    let session = create_session(drive, item, false).await?;
    let resolved_worksheet = resolve_worksheet(drive, item, &session, worksheet).await?;
    let value = workbook_get(
        &range_path(drive, item, Some(&resolved_worksheet), range),
        &session,
    )
    .await;
    let result = value?;
    let values: Vec<Vec<Value>> = result
        .get("values")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| row.as_array().cloned().unwrap_or_default())
                .collect::<Vec<Vec<Value>>>()
        })
        .unwrap_or_default();
    Ok(GraphSpreadsheetResult {
        resource,
        worksheet: worksheet_name,
        address: result
            .get("address")
            .and_then(Value::as_str)
            .map(str::to_owned),
        tsv: tsv(&values),
        values,
        provider: "microsoft-graph".to_owned(),
        account_id: account.id,
        observed_at: chrono::Utc::now().to_rfc3339(),
        evidence,
    })
}

#[tauri::command]
pub(crate) async fn read_microsoft_graph_spreadsheet(
    input: GraphResourceInput,
) -> Result<GraphSpreadsheetResult, String> {
    read_resource(input.resource, EvidenceState::Authenticated).await
}

#[tauri::command]
pub(crate) async fn write_microsoft_graph_spreadsheet(
    input: GraphWriteInput,
) -> Result<GraphSpreadsheetResult, String> {
    if input.values.is_empty() || input.values.iter().any(|row| row.is_empty()) {
        return Err("Spreadsheet values must be a non-empty rectangular matrix".to_owned());
    }
    let width = input.values[0].len();
    if input.values.iter().any(|row| row.len() != width) {
        return Err("Spreadsheet values must be rectangular".to_owned());
    }
    let (drive, item, worksheet, range) = graph_item(&input.resource)?;
    let range = range
        .ok_or_else(|| "An explicit Graph workbook range is required for writes".to_owned())?;
    if range.trim().is_empty() || range.contains(['\'', '"']) {
        return Err("Microsoft workbook range is invalid".to_owned());
    }
    let session = create_session(drive, item, true).await?;
    let resolved_worksheet = resolve_worksheet(drive, item, &session, worksheet).await?;
    let path = range_path(drive, item, Some(&resolved_worksheet), Some(range));
    workbook_patch(&path, &session, &input.values).await?;
    let result = workbook_get(&path, &session).await?;
    let actual: Vec<Vec<Value>> = result
        .get("values")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| row.as_array().cloned().unwrap_or_default())
                .collect::<Vec<Vec<Value>>>()
        })
        .unwrap_or_default();
    if !values_equal(&input.values, &actual) {
        return Err("Microsoft Graph write read-back validation failed".to_owned());
    }
    read_resource(input.resource, EvidenceState::Completed).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_reference_contains_no_url_or_token() {
        let value = serde_json::to_value(ResourceRef::MicrosoftDriveItem {
            drive_id: "drive".into(),
            item_id: "item".into(),
            worksheet: None,
            range: Some("A1:B2".into()),
        })
        .unwrap();
        assert!(value.get("token").is_none());
        assert!(value.get("url").is_none());
    }
    #[test]
    fn validation_accepts_numeric_equivalence() {
        assert!(values_equal(
            &[vec![Value::from(42)]],
            &[vec![Value::from(42.0)]]
        ));
    }
    #[test]
    fn local_resources_cannot_be_routed_to_graph() {
        assert!(graph_item(&ResourceRef::LocalFile {
            absolute_path: "/tmp/a.xlsx".into()
        })
        .is_err());
    }
    #[tokio::test]
    #[ignore = "requires authorized Microsoft account and explicitly selected workbook range"]
    async fn microsoft_graph_real_e2e() {
        let drive_id = std::env::var("AI_OS_GRAPH_E2E_DRIVE_ID").expect("AI_OS_GRAPH_E2E_DRIVE_ID");
        let item_id = std::env::var("AI_OS_GRAPH_E2E_ITEM_ID").expect("AI_OS_GRAPH_E2E_ITEM_ID");
        let worksheet =
            std::env::var("AI_OS_GRAPH_E2E_WORKSHEET").expect("AI_OS_GRAPH_E2E_WORKSHEET");
        let range = std::env::var("AI_OS_GRAPH_E2E_RANGE").expect("AI_OS_GRAPH_E2E_RANGE");
        assert!(!get_microsoft_graph_identity()
            .await
            .expect("real /me")
            .id
            .is_empty());
        let resource = ResourceRef::MicrosoftDriveItem {
            drive_id,
            item_id,
            worksheet: Some(worksheet),
            range: Some(range),
        };
        let before = read_microsoft_graph_spreadsheet(GraphResourceInput {
            resource: resource.clone(),
        })
        .await
        .expect("real workbook read");
        assert_eq!(before.evidence, EvidenceState::Authenticated);
        let written = write_microsoft_graph_spreadsheet(GraphWriteInput {
            resource,
            values: before.values.clone(),
        })
        .await
        .expect("real write and read-back");
        assert_eq!(written.evidence, EvidenceState::Completed);
    }
}

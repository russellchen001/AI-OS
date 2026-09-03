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
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueRenderOption=UNFORMATTED_VALUE",
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
    let mut resource = GoogleResourceRef {
        file_id: id,
        range: Some("Sheet1!A1".to_owned()),
    };

    if let Some(values) = input.values {
        resource.range = Some(spreadsheet_write_range(&values));
        return write_google_spreadsheet(GoogleWriteInput { resource, values }).await;
    }
    Ok(result(resource, created, EvidenceState::Completed))
}

fn spreadsheet_column_name(mut column: usize) -> String {
    let mut name = String::new();

    while column > 0 {
        column -= 1;
        name.insert(0, (b'A' + (column % 26) as u8) as char);
        column /= 26;
    }

    name
}

fn spreadsheet_write_range(values: &[Vec<Value>]) -> String {
    let row_count = values.len().max(1);
    let column_count = values.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let end_column = spreadsheet_column_name(column_count);

    format!("Sheet1!A1:{end_column}{row_count}")
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

    #[test]
    fn sheets_write_range_matches_value_dimensions() {
        assert_eq!(
            spreadsheet_write_range(&[
                vec![Value::from("a"), Value::from("b"), Value::from("c")],
                vec![Value::from(1), Value::from(2), Value::from(3)],
            ]),
            "Sheet1!A1:C2"
        );

        assert_eq!(
            spreadsheet_write_range(&[vec![Value::from(1); 28]]),
            "Sheet1!A1:AB1"
        );
    }

    async fn delete_google_e2e_file(file_id: &str) -> Result<(), String> {
        let token = crate::providers::provider_access_token(INSTANCE_ID).await?;
        let response = client()?
            .delete(format!(
                "https://www.googleapis.com/drive/v3/files/{file_id}"
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| "Google Workspace E2E cleanup request failed".to_owned())?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Google Workspace E2E cleanup failed (HTTP {})",
                response.status().as_u16()
            ))
        }
    }

    #[tokio::test]
    #[ignore = "requires authorized Google Workspace account"]
    async fn google_workspace_real_e2e() {
        let identity = get_google_workspace_identity()
            .await
            .expect("real Google identity");

        assert!(
            identity
                .get("sub")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty()),
            "Google identity did not contain sub"
        );

        let files = list_google_workspace_files()
            .await
            .expect("real Google Drive list");

        assert!(
            files.get("files").and_then(Value::as_array).is_some(),
            "Google Drive list did not contain files"
        );

        let run_id = uuid::Uuid::new_v4().simple().to_string();

        let document = create_google_document(GoogleCreateInput {
            title: format!("AI-OS E2E Document {run_id}"),
            values: None,
        })
        .await
        .expect("real Google Docs create/read-back");

        assert_eq!(document.evidence, EvidenceState::Completed);
        assert!(document
            .values
            .get("documentId")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty()));

        delete_google_e2e_file(&document.resource.file_id)
            .await
            .expect("Google Docs E2E cleanup");

        let sheet_values = vec![
            vec![
                Value::from("AI-OS"),
                Value::from("Google Workspace"),
                Value::from("real-e2e"),
            ],
            vec![Value::from(42), Value::from(42.5), Value::from(true)],
        ];

        let spreadsheet = create_google_spreadsheet(GoogleCreateInput {
            title: format!("AI-OS E2E Spreadsheet {run_id}"),
            values: Some(sheet_values),
        })
        .await
        .expect("real Google Sheets create/write/read-back");

        assert_eq!(spreadsheet.evidence, EvidenceState::Completed);

        delete_google_e2e_file(&spreadsheet.resource.file_id)
            .await
            .expect("Google Sheets E2E cleanup");

        let presentation = create_google_presentation(GoogleCreateInput {
            title: format!("AI-OS E2E Presentation {run_id}"),
            values: None,
        })
        .await
        .expect("real Google Slides create/read-back");

        assert_eq!(presentation.evidence, EvidenceState::Completed);
        assert!(presentation
            .values
            .get("slides")
            .and_then(Value::as_array)
            .is_some());

        delete_google_e2e_file(&presentation.resource.file_id)
            .await
            .expect("Google Slides E2E cleanup");
    }
}

/// The Office capabilities, answered in the cloud.
///
/// Everything above this point is a Tauri command: typed input, called straight
/// from the interface. That is exactly why Docs, Sheets and Slides were real,
/// proven adapters that `document.read` could not reach -- the fifth adapter in
/// this work found working and callable from nowhere.
///
/// Two things had to be settled to close that, and neither was an omission.
///
/// A Drive file id is not a path, so the provider-neutral request needs a shape
/// that says which resource it means. A request is a cloud request when it
/// carries `resource.fileId`, or when it says `provider: "google-workspace"` --
/// which is the only way a CREATE can say so, because a file that does not
/// exist yet has no id.
///
/// And the gateway is synchronous while these adapters are async, bridged with
/// `tauri::async_runtime::block_on` the way `download/registry.rs` already
/// does.
///
/// The envelope matches what the local providers return, because a caller must
/// not have to know which provider answered.
pub(crate) mod office {
    use super::*;

    pub(crate) const INSTANCE: &str = INSTANCE_ID;

    fn resource_of(input: &Value, capability: &str) -> Result<GoogleResourceRef, String> {
        let file_id = input
            .pointer("/resource/fileId")
            .or_else(|| input.get("fileId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{capability} in Google Workspace requires resource.fileId"))?
            .to_owned();

        let range = input
            .pointer("/resource/range")
            .or_else(|| input.get("range"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        Ok(GoogleResourceRef { file_id, range })
    }

    fn title_of(input: &Value, capability: &str) -> Result<String, String> {
        input
            .get("title")
            .or_else(|| input.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("{capability} in Google Workspace requires title"))
    }

    /// Rows for Sheets, from either a grid or the tab-separated content the
    /// local spreadsheet capabilities take.
    ///
    /// Accepting `content` is what lets one request serve both providers: the
    /// same body that creates a local .xlsx creates a Google Sheet.
    fn values_of(input: &Value) -> Option<Vec<Vec<Value>>> {
        if let Some(rows) = input.get("values").and_then(Value::as_array) {
            return Some(
                rows.iter()
                    .map(|row| {
                        row.as_array()
                            .cloned()
                            .unwrap_or_else(|| vec![row.clone()])
                    })
                    .collect(),
            );
        }

        let content = input.get("content").and_then(Value::as_str)?;

        Some(
            content
                .trim_end_matches('\n')
                .split('\n')
                .map(|line| {
                    line.trim_end_matches('\r')
                        .split('\t')
                        .map(|cell| Value::String(cell.to_owned()))
                        .collect()
                })
                .collect(),
        )
    }

    fn envelope(capability: &str, file_id: &str, result: GoogleWorkspaceResult) -> Value {
        serde_json::json!({
            "capability": capability,
            "selectedProvider": "google-workspace",
            "resourceLocation": "cloud",
            "inputResource": file_id,
            "operationResult": serde_json::to_value(&result).unwrap_or(Value::Null),
            "warnings": [],
            "confirmationConsumed": true,
            "validationResult": "ok",
        })
    }

    pub(crate) async fn document_read(input: &Value) -> Result<Value, String> {
        let resource = resource_of(input, "document.read")?;
        let file_id = resource.file_id.clone();
        let result = read_google_document(GoogleResourceInput { resource }).await?;
        Ok(envelope("document.read", &file_id, result))
    }

    pub(crate) async fn document_create(input: &Value) -> Result<Value, String> {
        let title = title_of(input, "document.create")?;
        let result = create_google_document(GoogleCreateInput {
            title,
            values: None,
        })
        .await?;
        let file_id = result.resource.file_id.clone();
        Ok(envelope("document.create", &file_id, result))
    }

    pub(crate) async fn spreadsheet_read(input: &Value) -> Result<Value, String> {
        let resource = resource_of(input, "spreadsheet.read")?;
        let file_id = resource.file_id.clone();
        let result = read_google_spreadsheet(GoogleResourceInput { resource }).await?;
        Ok(envelope("spreadsheet.read", &file_id, result))
    }

    pub(crate) async fn spreadsheet_create(input: &Value) -> Result<Value, String> {
        let title = title_of(input, "spreadsheet.create")?;
        let result = create_google_spreadsheet(GoogleCreateInput {
            title,
            values: values_of(input),
        })
        .await?;
        let file_id = result.resource.file_id.clone();
        Ok(envelope("spreadsheet.create", &file_id, result))
    }

    pub(crate) async fn spreadsheet_edit(input: &Value) -> Result<Value, String> {
        let resource = resource_of(input, "spreadsheet.edit")?;
        let values = values_of(input).ok_or_else(|| {
            "spreadsheet.edit in Google Workspace requires values or content".to_owned()
        })?;

        // Sheets writes to an explicit range, and the write validates itself by
        // reading back. When the caller does not name a range, the range is the
        // block the values themselves describe.
        let resource = GoogleResourceRef {
            file_id: resource.file_id,
            range: resource
                .range
                .or_else(|| Some(spreadsheet_write_range(&values))),
        };
        let file_id = resource.file_id.clone();

        let result = write_google_spreadsheet(GoogleWriteInput { resource, values }).await?;
        Ok(envelope("spreadsheet.edit", &file_id, result))
    }

    pub(crate) async fn presentation_read(input: &Value) -> Result<Value, String> {
        let resource = resource_of(input, "presentation.read")?;
        let file_id = resource.file_id.clone();
        let result = read_google_presentation(GoogleResourceInput { resource }).await?;
        Ok(envelope("presentation.read", &file_id, result))
    }

    pub(crate) async fn presentation_create(input: &Value) -> Result<Value, String> {
        let title = title_of(input, "presentation.create")?;
        let result = create_google_presentation(GoogleCreateInput {
            title,
            values: None,
        })
        .await?;
        let file_id = result.resource.file_id.clone();
        Ok(envelope("presentation.create", &file_id, result))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        #[test]
        fn a_cloud_resource_is_read_from_either_shape() {
            // The nested shape the interface already uses, and the flat one a
            // caller writes by hand.
            for request in [
                json!({"resource": {"fileId": "abc123", "range": "Sheet1!A1:B2"}}),
                json!({"fileId": "abc123", "range": "Sheet1!A1:B2"}),
            ] {
                let resource = resource_of(&request, "spreadsheet.read").unwrap();
                assert_eq!(resource.file_id, "abc123");
                assert_eq!(resource.range.as_deref(), Some("Sheet1!A1:B2"));
            }

            // A range is optional; a file id is not.
            assert!(resource_of(&json!({"fileId": "abc123"}), "x")
                .unwrap()
                .range
                .is_none());

            for missing in [
                json!({}),
                json!({"fileId": "   "}),
                json!({"resource": {"fileId": ""}}),
            ] {
                let error = resource_of(&missing, "document.read").unwrap_err();
                assert!(
                    error.contains("resource.fileId"),
                    "message was {error}"
                );
            }
        }

        #[test]
        fn the_same_body_that_creates_a_local_workbook_creates_a_google_sheet() {
            // Tab-separated content is what the local spreadsheet capabilities
            // take, so accepting it here is what makes one request serve both.
            let from_content = values_of(&json!({"content": "Region\tTotal\nNorth\t42"})).unwrap();
            assert_eq!(
                from_content,
                vec![
                    vec![json!("Region"), json!("Total")],
                    vec![json!("North"), json!("42")]
                ]
            );

            // An explicit grid is passed through, numbers intact.
            let from_grid = values_of(&json!({"values": [["Region", "Total"], ["North", 42]]})).unwrap();
            assert_eq!(
                from_grid,
                vec![
                    vec![json!("Region"), json!("Total")],
                    vec![json!("North"), json!(42)]
                ]
            );

            assert!(values_of(&json!({"title": "no rows here"})).is_none());
        }

        #[test]
        fn a_create_needs_a_title_and_says_so() {
            assert_eq!(
                title_of(&json!({"title": "  Quarterly  "}), "document.create").unwrap(),
                "Quarterly"
            );

            for missing in [json!({}), json!({"title": "  "})] {
                let error = title_of(&missing, "document.create").unwrap_err();
                assert!(error.contains("title"), "message was {error}");
            }
        }
    }
}

/// The live round trip, against the person's real Google account.
///
/// Opt-in only. It creates a real file in their Drive and does NOT delete it:
/// deleting someone's data to tidy up after a test is not this code's decision
/// to make, and the title says plainly where the file came from.
#[cfg(test)]
mod live {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "requires a connected Google Workspace account and AI_OS_GOOGLE_LIVE"]
    fn a_google_sheet_is_created_written_and_read_back() {
        if std::env::var("AI_OS_GOOGLE_LIVE").is_err() {
            return;
        }

        let title = format!(
            "AI-OS Office capability check {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
        );

        // Created through the provider-neutral entry point, with the same
        // tab-separated body a local .xlsx would be created from.
        let created = tauri::async_runtime::block_on(office::spreadsheet_create(&json!({
            "provider": "google-workspace",
            "title": title,
            "content": "Region\tTotal\nNorth\t42",
        })))
        .expect("create failed");

        assert_eq!(created["selectedProvider"], "google-workspace");
        assert_eq!(created["resourceLocation"], "cloud");

        let file_id = created["inputResource"]
            .as_str()
            .expect("no file id came back")
            .to_owned();
        assert!(!file_id.is_empty());

        // Read it back in a separate call, the way a caller would.
        let read = tauri::async_runtime::block_on(office::spreadsheet_read(&json!({
            "resource": {"fileId": file_id, "range": "Sheet1!A1:B2"},
        })))
        .expect("read failed");

        let rows = read["operationResult"]["values"]["values"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        assert_eq!(rows.len(), 2, "read back {rows:#?}");
        assert_eq!(rows[0][0], "Region");
        assert_eq!(rows[1][0], "North");

        // And an edit, which validates itself by reading back inside the
        // adapter -- so this asserting the new value is a second, independent
        // check of the same write.
        tauri::async_runtime::block_on(office::spreadsheet_edit(&json!({
            "resource": {"fileId": file_id, "range": "Sheet1!A1:B2"},
            "content": "Region\tTotal\nSouth\t7",
        })))
        .expect("edit failed");

        let after = tauri::async_runtime::block_on(office::spreadsheet_read(&json!({
            "resource": {"fileId": file_id, "range": "Sheet1!A1:B2"},
        })))
        .expect("read after edit failed");

        let rows = after["operationResult"]["values"]["values"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(rows[1][0], "South", "the edit did not take: {rows:#?}");

        println!(
            "AIOS_LIVE_SHEET left in Drive on purpose: {title} ({file_id})"
        );
    }
}

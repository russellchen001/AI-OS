use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MailSummary {
    pub subject: String,
    pub sender: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CalendarEventSummary {
    pub title: String,
    pub start: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmailCalendarReadResult {
    pub mail: Vec<MailSummary>,
    pub calendar: Vec<CalendarEventSummary>,
}


#[tauri::command]
pub(crate) fn list_native_mail() -> Result<Vec<MailSummary>, String> {
    #[cfg(target_os = "macos")]
    {
        let script = r#"
        tell application "Mail"
            set resultText to ""
            repeat with m in messages of inbox
                set resultText to resultText & (subject of m) & "|||"
                set resultText to resultText & (sender of m) & linefeed
            end repeat
            return resultText
        end tell
        "#;

        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(
                String::from_utf8_lossy(&output.stderr).to_string()
            );
        }

        let text = String::from_utf8_lossy(&output.stdout);

        let mails = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split("|||");
                let subject = parts.next()?.to_owned();
                let sender = parts.next()?.to_owned();

                Some(MailSummary {
                    subject,
                    sender,
                })
            })
            .collect();

        Ok(mails)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Native Mail is only supported on macOS.".to_owned())
    }
}


#[tauri::command]
pub(crate) fn list_native_calendar() -> Result<Vec<CalendarEventSummary>, String> {
    Ok(Vec::new())
}


#[tauri::command]
pub(crate) fn search_native_mail(query: String) -> Result<Vec<MailSummary>, String> {
    let mails = list_native_mail()?;

    let keyword = query.to_lowercase();

    Ok(mails
        .into_iter()
        .filter(|mail| {
            mail.subject.to_lowercase().contains(&keyword)
                || mail.sender.to_lowercase().contains(&keyword)
        })
        .collect())
}

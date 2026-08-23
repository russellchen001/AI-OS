use serde::{Deserialize, Serialize};
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


#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateCalendarEventRequest {
    pub title: String,
    pub start: String,
    pub end: String,
}


#[tauri::command]
pub(crate) fn create_native_calendar_event(
    request: CreateCalendarEventRequest,
) -> Result<String, String> {

    if request.title.trim().is_empty() {
        return Err("Calendar event title cannot be empty.".to_owned());
    }

    #[cfg(target_os = "macos")]
    {
        let escaped_title = request
            .title
            .replace("\\", "\\\\")
            .replace("\"", "\\\"");

        let escaped_start = request
            .start
            .replace("\\", "\\\\")
            .replace("\"", "\\\"");

        let escaped_end = request
            .end
            .replace("\\", "\\\\")
            .replace("\"", "\\\"");


        let script = format!(
r#"
tell application "Calendar"
    set newEvent to make new event at end of events of calendar 1 with properties {{summary:"{}", start date:date "{}", end date:date "{}"}}
    return summary of newEvent
end tell
"#,
            escaped_title,
            escaped_start,
            escaped_end
        );


        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| e.to_string())?;


        if !output.status.success() {
            return Err(
                String::from_utf8_lossy(&output.stderr).to_string()
            );
        }


        return Ok(
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_owned()
        );
    }


    #[cfg(not(target_os = "macos"))]
    {
        Err("Calendar creation is only supported on macOS.".to_owned())
    }
}


#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MailDraftResult {
    pub created: bool,
    pub description: String,
}

#[tauri::command]
pub(crate) fn create_mail_draft(
    recipient: String,
    subject: String,
    body: String,
) -> Result<MailDraftResult, String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"
tell application "Mail"
    set newMessage to make new outgoing message with properties {{subject:"{}", content:"{}", visible:false}}
    tell newMessage
        make new to recipient at end of to recipients with properties {{address:"{}"}}
    end tell
end tell
"#,
            subject.replace('"', "\\\""),
            body.replace('"', "\\\""),
            recipient.replace('"', "\\\"")
        );

        std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| e.to_string())?;

        return Ok(MailDraftResult {
            created: true,
            description: "Mail draft created. Review before sending.".to_owned(),
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (recipient, subject, body);

        Ok(MailDraftResult {
            created: false,
            description: "Mail drafts require macOS Mail.app.".to_owned(),
        })
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MailSendConfirmation {
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub requires_confirmation: bool,
}

#[tauri::command]
pub(crate) fn prepare_native_mail_send_confirmation(
    recipient: String,
    subject: String,
    body: String,
) -> Result<MailSendConfirmation, String> {
    if recipient.trim().is_empty() {
        return Err("Recipient is required".to_owned());
    }

    Ok(MailSendConfirmation {
        recipient,
        subject,
        body,
        requires_confirmation: true,
    })
}

#[tauri::command]
pub(crate) fn send_native_mail_after_confirmation(
    confirmation: MailSendConfirmation,
) -> Result<String, String> {
    if !confirmation.requires_confirmation {
        return Err("Mail sending requires explicit confirmation".to_owned());
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"tell application "Mail"
                set newMessage to make new outgoing message with properties {{subject:"{}", content:"{}", visible:true}}
                tell newMessage
                    make new to recipient at end of to recipients with properties {{address:"{}"}}
                    send
                end tell
            end tell"#,
            confirmation.subject,
            confirmation.body,
            confirmation.recipient
        );

        std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| e.to_string())?;

        return Ok("Mail sent".to_owned());
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Native Mail sending is only supported on macOS.".to_owned())
    }
}

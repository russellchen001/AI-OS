//! Plain-text clipboard operations.
//!
//! Computer Control owns the clipboard as deterministic system state. It does
//! not paste into an application, press Command-V, inspect a window or decide
//! where the text should go. Those belong to Computer Use or the Planner.
//!
//! C1 is deliberately text-only. Rich clipboard formats are not silently
//! flattened and claimed as equivalent capabilities.

use crate::system::SystemError;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

const MAX_CLIPBOARD_BYTES: usize = 65_536;

fn text_to_write(input: &Value) -> Result<&str, SystemError> {
    let text = input
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| SystemError::invalid("system.clipboard.write requires text"))?;

    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(SystemError::invalid(format!(
            "system.clipboard.write supports at most {MAX_CLIPBOARD_BYTES} UTF-8 bytes"
        )));
    }

    Ok(text)
}

pub(crate) fn read_clipboard(_input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("/usr/bin/pbpaste")
            .output()
            .map_err(|error| {
                SystemError::execution(format!(
                    "the macOS clipboard could not be read: {error}"
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

            return Err(SystemError::execution(if stderr.is_empty() {
                "the macOS clipboard did not provide plain text".to_owned()
            } else {
                format!("the macOS clipboard could not be read: {stderr}")
            }));
        }

        let bytes = output.stdout.len();

        if bytes > MAX_CLIPBOARD_BYTES {
            return Err(SystemError::execution(format!(
                "clipboard text exceeds the {MAX_CLIPBOARD_BYTES}-byte Computer Control bound"
            )));
        }

        let text = String::from_utf8(output.stdout).map_err(|_| {
            SystemError::execution("the clipboard content is not valid UTF-8 plain text")
        })?;

        return Ok(json!({
            "capability": "system.clipboard.read",
            "operationResult": {
                "type": "text",
                "text": text,
                "bytes": bytes
            }
        }));
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(SystemError::execution(
            "system.clipboard.read has no adapter on this operating system in v1.0",
        ))
    }
}

pub(crate) fn write_clipboard(input: &Value) -> Result<Value, SystemError> {
    let text = text_to_write(input)?;
    let bytes = text.len();

    #[cfg(target_os = "macos")]
    {
        let mut child = Command::new("/usr/bin/pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                SystemError::execution(format!(
                    "the macOS clipboard writer could not be started: {error}"
                ))
            })?;

        child
            .stdin
            .as_mut()
            .ok_or_else(|| SystemError::execution("the clipboard writer has no input stream"))?
            .write_all(text.as_bytes())
            .map_err(|error| {
                SystemError::execution(format!(
                    "text could not be sent to the macOS clipboard: {error}"
                ))
            })?;

        drop(child.stdin.take());

        let output = child.wait_with_output().map_err(|error| {
            SystemError::execution(format!(
                "the macOS clipboard writer did not complete: {error}"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

            return Err(SystemError::execution(if stderr.is_empty() {
                "the macOS clipboard rejected the text".to_owned()
            } else {
                format!("the macOS clipboard rejected the text: {stderr}")
            }));
        }

        return Ok(json!({
            "capability": "system.clipboard.write",
            "operationResult": {
                "status": "written",
                "type": "text",
                "bytes": bytes
            }
        }));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = bytes;

        Err(SystemError::execution(
            "system.clipboard.write has no adapter on this operating system in v1.0",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_write_requires_bounded_text() {
        for request in [json!({}), json!({"text": 42}), json!({"value": "hello"})] {
            let error = text_to_write(&request).unwrap_err();
            assert!(error.invalid_request);
        }

        let maximum = "x".repeat(MAX_CLIPBOARD_BYTES);
        assert_eq!(
            text_to_write(&json!({"text": maximum}))
                .unwrap()
                .len(),
            MAX_CLIPBOARD_BYTES
        );

        let too_large = "x".repeat(MAX_CLIPBOARD_BYTES + 1);
        let error = text_to_write(&json!({"text": too_large})).unwrap_err();
        assert!(error.invalid_request);
    }

    #[test]
    fn clipboard_bound_is_utf8_bytes_not_character_count() {
        let fits = "界".repeat(MAX_CLIPBOARD_BYTES / 3);
        assert!(fits.len() <= MAX_CLIPBOARD_BYTES);
        assert!(text_to_write(&json!({"text": fits})).is_ok());

        let exceeds = "界".repeat((MAX_CLIPBOARD_BYTES / 3) + 1);
        assert!(exceeds.len() > MAX_CLIPBOARD_BYTES);

        let error = text_to_write(&json!({"text": exceeds})).unwrap_err();
        assert!(error.invalid_request);
    }
}

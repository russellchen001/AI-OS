//! Read-only macOS permission state and deterministic System Settings handoff.
//!
//! This module never grants permissions. It reads public OS authorization
//! state where macOS exposes one and may open the relevant System Settings
//! pane. The person remains responsible for changing authorization.
//!
//! Some Apple APIs expose only a boolean preflight. In those cases AI-OS
//! reports `notGranted` rather than inventing whether the person previously
//! denied the permission or has never been asked.

use crate::system::SystemError;
use serde::Serialize;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSWorkspace;
#[cfg(target_os = "macos")]
use objc2_application_services::AXIsProcessTrusted;
#[cfg(target_os = "macos")]
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo,
};
#[cfg(target_os = "macos")]
use objc2_core_graphics::CGPreflightScreenCaptureAccess;
#[cfg(target_os = "macos")]
use objc2_foundation::{NSString, NSURL};
#[cfg(target_os = "macos")]
use objc2_user_notifications::{UNNotificationSettings, UNUserNotificationCenter};
#[cfg(target_os = "macos")]
use std::{ptr::NonNull, sync::mpsc, time::Duration};

#[cfg(target_os = "macos")]
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(30);

const SETTINGS_NOTIFICATIONS: &str =
    "x-apple.systempreferences:com.apple.Notifications-Settings.extension";
const SETTINGS_ACCESSIBILITY: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
const SETTINGS_SCREEN_RECORDING: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
const SETTINGS_CAMERA: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera";
const SETTINGS_MICROPHONE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
const SETTINGS_AUTOMATION: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation";
const SETTINGS_CALENDAR: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars";
const SETTINGS_FULL_DISK_ACCESS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionSnapshot {
    id: &'static str,
    state: &'static str,
    granted: bool,
    native_state: &'static str,
    settings_target: &'static str,
    description: &'static str,
}

fn validate_empty_input(input: &Value) -> Result<(), SystemError> {
    let object = input.as_object().ok_or_else(|| {
        SystemError::invalid("system.permission.list requires an empty object input")
    })?;

    if !object.is_empty() {
        return Err(SystemError::invalid(
            "system.permission.list does not accept parameters",
        ));
    }

    Ok(())
}

fn settings_target(input: &Value) -> Result<&'static str, SystemError> {
    let object = input.as_object().ok_or_else(|| {
        SystemError::invalid("system.permission.open_settings requires an object input")
    })?;

    if object.len() != 1 || !object.contains_key("permission") {
        return Err(SystemError::invalid(
            "system.permission.open_settings requires only permission",
        ));
    }

    let permission = object
        .get("permission")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SystemError::invalid("system.permission.open_settings requires permission text")
        })?;

    settings_url_for(permission).ok_or_else(|| {
        SystemError::invalid(format!(
            "unsupported System Settings permission target: {permission}"
        ))
    })
}

fn settings_url_for(permission: &str) -> Option<&'static str> {
    match permission {
        "notifications" => Some(SETTINGS_NOTIFICATIONS),
        "accessibility" => Some(SETTINGS_ACCESSIBILITY),
        "screenRecording" => Some(SETTINGS_SCREEN_RECORDING),
        "camera" => Some(SETTINGS_CAMERA),
        "microphone" => Some(SETTINGS_MICROPHONE),
        "automation" => Some(SETTINGS_AUTOMATION),
        "calendar" => Some(SETTINGS_CALENDAR),
        "fullDiskAccess" => Some(SETTINGS_FULL_DISK_ACCESS),
        _ => None,
    }
}

fn notification_state(raw: isize) -> (&'static str, bool, &'static str) {
    match raw {
        0 => ("notDetermined", false, "notDetermined"),
        1 => ("denied", false, "denied"),
        2 => ("granted", true, "authorized"),
        3 => ("granted", true, "provisional"),
        4 => ("granted", true, "ephemeral"),
        _ => ("unavailable", false, "unknown"),
    }
}

fn capture_state(raw: isize) -> (&'static str, bool, &'static str) {
    match raw {
        0 => ("notDetermined", false, "notDetermined"),
        1 => ("restricted", false, "restricted"),
        2 => ("denied", false, "denied"),
        3 => ("granted", true, "authorized"),
        _ => ("unavailable", false, "unknown"),
    }
}

fn unavailable(
    id: &'static str,
    settings_target: &'static str,
    description: &'static str,
) -> PermissionSnapshot {
    PermissionSnapshot {
        id,
        state: "unavailable",
        granted: false,
        native_state: "unavailable",
        settings_target,
        description,
    }
}

#[cfg(target_os = "macos")]
fn notification_snapshot() -> Result<PermissionSnapshot, SystemError> {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (sender, receiver) = mpsc::sync_channel(1);

    let completion = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        let settings = unsafe { settings.as_ref() };
        let _ = sender.send(settings.authorizationStatus().0);
    });

    center.getNotificationSettingsWithCompletionHandler(&completion);

    let raw = receiver.recv_timeout(CALLBACK_TIMEOUT).map_err(|_| {
        SystemError::execution("macOS did not return notification authorization state")
    })?;

    let (state, granted, native_state) = notification_state(raw);

    Ok(PermissionSnapshot {
        id: "notifications",
        state,
        granted,
        native_state,
        settings_target: SETTINGS_NOTIFICATIONS,
        description: "Permission to deliver AI-OS notifications.",
    })
}

#[cfg(target_os = "macos")]
fn accessibility_snapshot() -> PermissionSnapshot {
    let granted = unsafe { AXIsProcessTrusted() };

    PermissionSnapshot {
        id: "accessibility",
        state: if granted { "granted" } else { "notGranted" },
        granted,
        native_state: if granted { "trusted" } else { "notTrusted" },
        settings_target: SETTINGS_ACCESSIBILITY,
        description: "Accessibility trust for deterministic accessibility APIs.",
    }
}

#[cfg(target_os = "macos")]
fn screen_recording_snapshot() -> PermissionSnapshot {
    let granted = unsafe { CGPreflightScreenCaptureAccess() };

    PermissionSnapshot {
        id: "screenRecording",
        state: if granted { "granted" } else { "notGranted" },
        granted,
        native_state: if granted { "available" } else { "notAvailable" },
        settings_target: SETTINGS_SCREEN_RECORDING,
        description: "Screen Recording authorization visible through CoreGraphics preflight.",
    }
}

#[cfg(target_os = "macos")]
fn capture_snapshot(
    id: &'static str,
    media_type: Option<&'static objc2_av_foundation::AVMediaType>,
    settings: &'static str,
    description: &'static str,
) -> PermissionSnapshot {
    let Some(media_type) = media_type else {
        return unavailable(id, settings, description);
    };

    let raw = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };

    let raw_value = raw.0;
    let (state, granted, native_state) = capture_state(raw_value);

    PermissionSnapshot {
        id,
        state,
        granted,
        native_state,
        settings_target: settings,
        description,
    }
}

#[cfg(target_os = "macos")]
fn macos_snapshots() -> Result<Vec<PermissionSnapshot>, SystemError> {
    let video = unsafe { AVMediaTypeVideo };
    let audio = unsafe { AVMediaTypeAudio };

    Ok(vec![
        notification_snapshot()?,
        accessibility_snapshot(),
        screen_recording_snapshot(),
        capture_snapshot(
            "camera",
            video,
            SETTINGS_CAMERA,
            "Camera authorization reported by AVFoundation.",
        ),
        capture_snapshot(
            "microphone",
            audio,
            SETTINGS_MICROPHONE,
            "Microphone authorization reported by AVFoundation.",
        ),
        unavailable(
            "automation",
            SETTINGS_AUTOMATION,
            "Automation permission is target-specific rather than one global authorization state.",
        ),
        unavailable(
            "fullDiskAccess",
            SETTINGS_FULL_DISK_ACCESS,
            "macOS exposes no public global Full Disk Access status API.",
        ),
    ])
}

pub(crate) fn list_permissions(input: &Value) -> Result<Value, SystemError> {
    validate_empty_input(input)?;

    #[cfg(target_os = "macos")]
    {
        let permissions = macos_snapshots()?;

        return Ok(json!({
            "capability": "system.permission.list",
            "operationResult": {
                "status": "read",
                "provider": "macos-native-permission-apis",
                "permissions": permissions
            }
        }));
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(json!({
            "capability": "system.permission.list",
            "operationResult": {
                "status": "read",
                "provider": "unsupported-platform",
                "permissions": [
                    unavailable(
                        "notifications",
                        "",
                        "Native permission inspection is not implemented on this operating system in v1.0."
                    ),
                    unavailable(
                        "accessibility",
                        "",
                        "Native permission inspection is not implemented on this operating system in v1.0."
                    ),
                    unavailable(
                        "screenRecording",
                        "",
                        "Native permission inspection is not implemented on this operating system in v1.0."
                    ),
                    unavailable(
                        "camera",
                        "",
                        "Native permission inspection is not implemented on this operating system in v1.0."
                    ),
                    unavailable(
                        "microphone",
                        "",
                        "Native permission inspection is not implemented on this operating system in v1.0."
                    )
                ]
            }
        }))
    }
}

pub(crate) fn open_permission_settings(input: &Value) -> Result<Value, SystemError> {
    let target = settings_target(input)?;

    #[cfg(target_os = "macos")]
    {
        let native_target = NSString::from_str(target);
        let url = NSURL::URLWithString(&native_target).ok_or_else(|| {
            SystemError::execution("macOS System Settings target could not be represented as a URL")
        })?;

        let workspace = NSWorkspace::sharedWorkspace();

        if !workspace.openURL(&url) {
            return Err(SystemError::execution(
                "macOS did not open the requested System Settings pane",
            ));
        }

        return Ok(json!({
            "capability": "system.permission.open_settings",
            "operationResult": {
                "status": "opened",
                "provider": "macos-nsworkspace",
                "settingsTarget": target
            }
        }));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;

        Err(SystemError::execution(
            "system.permission.open_settings has no adapter on this operating system in v1.0",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_list_accepts_only_empty_object() {
        assert!(validate_empty_input(&json!({})).is_ok());

        for rejected in [
            json!(null),
            json!([]),
            json!("permissions"),
            json!({"permission":"camera"}),
        ] {
            assert!(validate_empty_input(&rejected).unwrap_err().invalid_request);
        }
    }

    #[test]
    fn settings_handoff_accepts_only_known_permission_names() {
        for (permission, expected) in [
            ("notifications", SETTINGS_NOTIFICATIONS),
            ("accessibility", SETTINGS_ACCESSIBILITY),
            ("screenRecording", SETTINGS_SCREEN_RECORDING),
            ("camera", SETTINGS_CAMERA),
            ("microphone", SETTINGS_MICROPHONE),
            ("automation", SETTINGS_AUTOMATION),
            ("calendar", SETTINGS_CALENDAR),
            ("fullDiskAccess", SETTINGS_FULL_DISK_ACCESS),
        ] {
            assert_eq!(
                settings_target(&json!({"permission":permission})).unwrap(),
                expected
            );
        }

        for rejected in [
            json!({}),
            json!({"permission":""}),
            json!({"permission":"unknown"}),
            json!({"permission":"camera","extra":true}),
            json!({"permission":7}),
        ] {
            assert!(settings_target(&rejected).unwrap_err().invalid_request);
        }
    }

    #[test]
    fn notification_authorization_mapping_preserves_apple_states() {
        assert_eq!(
            notification_state(0),
            ("notDetermined", false, "notDetermined")
        );
        assert_eq!(notification_state(1), ("denied", false, "denied"));
        assert_eq!(notification_state(2), ("granted", true, "authorized"));
        assert_eq!(notification_state(3), ("granted", true, "provisional"));
        assert_eq!(notification_state(4), ("granted", true, "ephemeral"));
        assert_eq!(notification_state(999), ("unavailable", false, "unknown"));
    }

    #[test]
    fn capture_authorization_mapping_preserves_avfoundation_states() {
        assert_eq!(
            capture_state(AVAuthorizationStatus::NotDetermined.0),
            ("notDetermined", false, "notDetermined")
        );
        assert_eq!(
            capture_state(AVAuthorizationStatus::Restricted.0),
            ("restricted", false, "restricted")
        );
        assert_eq!(
            capture_state(AVAuthorizationStatus::Denied.0),
            ("denied", false, "denied")
        );
        assert_eq!(
            capture_state(AVAuthorizationStatus::Authorized.0),
            ("granted", true, "authorized")
        );
        assert_eq!(capture_state(999), ("unavailable", false, "unknown"));
    }
}

//! Native operating-system notifications.
//!
//! Computer Control owns one atomic operation here: post an AI-OS notification.
//! It does not interact with Notification Center UI, click banners, schedule
//! reminders, impersonate another application, or decide when a person should
//! be notified.
//!
//! On macOS the implementation calls UserNotifications.framework in-process.
//! The first explicit send may cause Apple's own notification authorization
//! prompt when the status is still NotDetermined.

use crate::system::SystemError;
use serde_json::{json, Value};

const MAX_TITLE_CHARS: usize = 128;
const MAX_BODY_CHARS: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
struct NotificationInput {
    title: String,
    body: String,
}

fn parse_input(input: &Value) -> Result<NotificationInput, SystemError> {
    let object = input
        .as_object()
        .ok_or_else(|| SystemError::invalid("system.notification.send requires an object input"))?;

    for key in object.keys() {
        if key != "title" && key != "body" {
            return Err(SystemError::invalid(format!(
                "system.notification.send does not accept field {key:?}"
            )));
        }
    }

    let title = object
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SystemError::invalid("system.notification.send requires a non-empty title")
        })?;

    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(SystemError::invalid(format!(
            "notification title exceeds {MAX_TITLE_CHARS} characters"
        )));
    }

    let body = match object.get("body") {
        None => "",
        Some(value) => value
            .as_str()
            .ok_or_else(|| SystemError::invalid("system.notification.send body must be text"))?,
    };

    if body.chars().count() > MAX_BODY_CHARS {
        return Err(SystemError::invalid(format!(
            "notification body exceeds {MAX_BODY_CHARS} characters"
        )));
    }

    Ok(NotificationInput {
        title: title.to_owned(),
        body: body.to_owned(),
    })
}

fn authorization_label(raw: isize) -> &'static str {
    match raw {
        0 => "notDetermined",
        1 => "denied",
        2 => "authorized",
        3 => "provisional",
        4 => "ephemeral",
        _ => "unknown",
    }
}

fn authorization_allows_delivery(raw: isize) -> bool {
    matches!(raw, 2 | 3 | 4)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNNotificationSettings, UNUserNotificationCenter,
    };
    use std::{ptr::NonNull, sync::mpsc, time::Duration};

    const CALLBACK_TIMEOUT: Duration = Duration::from_secs(120);

    fn current_authorization_status() -> Result<isize, SystemError> {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (sender, receiver) = mpsc::sync_channel(1);

        let completion = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            let settings = unsafe { settings.as_ref() };
            let _ = sender.send(settings.authorizationStatus().0);
        });

        center.getNotificationSettingsWithCompletionHandler(&completion);

        receiver.recv_timeout(CALLBACK_TIMEOUT).map_err(|_| {
            SystemError::execution("macOS did not return notification authorization state")
        })
    }

    fn request_authorization() -> Result<bool, SystemError> {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (sender, receiver) = mpsc::sync_channel(1);

        let completion = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            let _ = sender.send((granted.as_bool(), error.is_null()));
        });

        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert,
            &completion,
        );

        let (granted, no_error) = receiver.recv_timeout(CALLBACK_TIMEOUT).map_err(|_| {
            SystemError::execution("macOS notification authorization did not complete")
        })?;

        if !no_error {
            return Err(SystemError::execution(
                "macOS could not complete notification authorization",
            ));
        }

        Ok(granted)
    }

    fn post(
        notification: NotificationInput,
        authorization: isize,
        authorization_prompted: bool,
    ) -> Result<Value, SystemError> {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let content = UNMutableNotificationContent::new();
        let title = NSString::from_str(&notification.title);
        let body = NSString::from_str(&notification.body);

        content.setTitle(&title);
        content.setBody(&body);

        let identifier = format!("ai-os-notification-{}", uuid::Uuid::new_v4());
        let native_identifier = NSString::from_str(&identifier);

        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &native_identifier,
            &content,
            None,
        );

        let (sender, receiver) = mpsc::sync_channel(1);

        let completion = RcBlock::new(move |error: *mut NSError| {
            let _ = sender.send(error.is_null());
        });

        center.addNotificationRequest_withCompletionHandler(&request, Some(&completion));

        let accepted = receiver.recv_timeout(CALLBACK_TIMEOUT).map_err(|_| {
            SystemError::execution("macOS did not finish submitting the notification")
        })?;

        if !accepted {
            return Err(SystemError::execution(
                "macOS rejected the notification request",
            ));
        }

        Ok(json!({
            "capability": "system.notification.send",
            "operationResult": {
                "status": "submitted",
                "identifier": identifier,
                "authorization": authorization_label(authorization),
                "authorizationPrompted": authorization_prompted,
                "provider": "macos-user-notifications"
            }
        }))
    }

    pub(super) fn send(input: &Value) -> Result<Value, SystemError> {
        let notification = parse_input(input)?;
        let mut status = current_authorization_status()?;
        let mut prompted = false;

        if status == 0 {
            prompted = true;

            if !request_authorization()? {
                return Err(SystemError::execution(
                    "notification permission was not granted",
                ));
            }

            status = current_authorization_status()?;
        }

        if !authorization_allows_delivery(status) {
            return Err(SystemError::execution(format!(
                "notification permission is {}",
                authorization_label(status)
            )));
        }

        post(notification, status, prompted)
    }
}

pub(crate) fn send_notification(input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        return macos::send(input);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = parse_input(input)?;

        Err(SystemError::execution(
            "system.notification.send has no adapter on this operating system in v1.0",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_input_is_bounded_and_exact() {
        assert_eq!(
            parse_input(&json!({
                "title": "AI-OS",
                "body": "Task complete"
            }))
            .unwrap(),
            NotificationInput {
                title: "AI-OS".to_owned(),
                body: "Task complete".to_owned(),
            }
        );

        assert_eq!(parse_input(&json!({"title": "AI-OS"})).unwrap().body, "");

        for rejected in [
            json!(null),
            json!({}),
            json!({"title": ""}),
            json!({"title": 7}),
            json!({"title": "AI-OS", "body": 7}),
            json!({"title": "AI-OS", "body": "", "sound": "Ping"}),
            json!({"title": "x".repeat(MAX_TITLE_CHARS + 1)}),
            json!({
                "title": "AI-OS",
                "body": "x".repeat(MAX_BODY_CHARS + 1)
            }),
        ] {
            assert!(parse_input(&rejected).unwrap_err().invalid_request);
        }
    }

    #[test]
    fn apple_notification_authorization_states_are_not_collapsed() {
        assert_eq!(authorization_label(0), "notDetermined");
        assert_eq!(authorization_label(1), "denied");
        assert_eq!(authorization_label(2), "authorized");
        assert_eq!(authorization_label(3), "provisional");
        assert_eq!(authorization_label(4), "ephemeral");
        assert_eq!(authorization_label(999), "unknown");
    }

    #[test]
    fn only_authorized_notification_states_allow_delivery() {
        assert!(!authorization_allows_delivery(0));
        assert!(!authorization_allows_delivery(1));
        assert!(authorization_allows_delivery(2));
        assert!(authorization_allows_delivery(3));
        assert!(authorization_allows_delivery(4));
        assert!(!authorization_allows_delivery(999));
    }
}

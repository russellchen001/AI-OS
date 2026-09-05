//! Applications, addressed rather than operated.
//!
//! Listing what is installed, seeing what is running, starting one and asking
//! one to stop. Not clicking anything inside them -- that is Computer Use's,
//! and the line is not fuzzy: this module never sends a keystroke and never
//! looks at a window.
//!
//! Everything here is addressed by BUNDLE IDENTIFIER, never by name. Names on
//! this machine come back localized -- `Safari浏览器`, `墙纸` -- so a name is
//! something to show a person and never something to look an application up
//! by. Names are reported; identifiers are used.
//!
//! Two things this platform does that the code has to work around, both found
//! by probe rather than assumed:
//!
//! - `open -b` exits 0 and says nothing when the application was ALREADY
//!   running, so a launch cannot report whether it started anything. What
//!   happened is established by looking before and after.
//! - `quit` reports success for an application that is not running at all. The
//!   return value is worth nothing on its own, for the same reason.

use crate::system::SystemError;
use serde_json::{json, Value};
use std::process::Command;

/// Enough applications to answer "what is installed" without answering it at
/// such length that nobody reads it.
const MAX_APPLICATIONS: usize = 500;
/// How long to wait for an application to appear or disappear, in tenths.
const SETTLE_ATTEMPTS: usize = 50;

fn run(program: &str, arguments: &[&str]) -> Result<(String, String, i32), SystemError> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| SystemError::execution(format!("{program} could not be run: {error}")))?;

    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    ))
}

fn bundle_id_of<'a>(input: &'a Value, field: &str) -> Result<&'a str, SystemError> {
    let value = input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            SystemError::invalid(format!(
                "this capability addresses applications by bundle identifier, so it needs {field} \
                 -- a name will not do, because names on this machine are localized"
            ))
        })?;

    // This checks that the value is SAFE to hand to the platform, and nothing
    // more. Whether an application actually answers to it is a question only
    // the machine can settle, and it does: `open -b` exits non-zero for an
    // identifier nothing is installed under.
    //
    // It used to also require a dot, on the theory that identifiers are
    // reverse-domain names. This machine has an application whose identifier is
    // `MacNetPlayer` -- no dot at all -- so that rule made a real, launchable
    // application unaddressable. And it could never have done the job it was
    // there for anyway: `MacNetPlayer` is indistinguishable from a display name
    // by shape, so shape cannot be what tells them apart. Non-ASCII still can,
    // which is what keeps `Safari浏览器` out; the rest is the machine's
    // question to answer, not a pattern's.
    let safe_to_pass_on = value.is_ascii()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
        && !value.starts_with('.')
        && !value.ends_with('.');

    if !safe_to_pass_on {
        return Err(SystemError::invalid(format!(
            "{value:?} cannot be a bundle identifier. Applications are addressed by identifier, not by name, and the name of Safari on this machine is Safari浏览器"
        )));
    }

    Ok(value)
}

/// Every application this machine has, with the identifier that addresses it.
pub(crate) fn list_applications(_input: &Value) -> Result<Value, SystemError> {
    let (stdout, _, _) = run(
        "/usr/bin/mdfind",
        &[
            "kMDItemContentType == 'com.apple.application-bundle'",
            "-attr",
            "kMDItemCFBundleIdentifier",
            "-attr",
            "kMDItemVersion",
        ],
    )?;

    let mut applications = Vec::new();
    let mut without_identifier = 0usize;

    // Split on the attribute NAMES, not on the run of spaces before them. The
    // first version matched four spaces and found almost nothing, because how
    // wide that gap is was never measured -- it was assumed. The names are the
    // part `mdfind` actually promises.
    const IDENTIFIER_KEY: &str = "kMDItemCFBundleIdentifier = ";
    const VERSION_KEY: &str = "kMDItemVersion = ";

    for line in stdout.lines() {
        let Some(at) = line.find(IDENTIFIER_KEY) else {
            continue;
        };

        let path = line[..at].trim();
        let attributes = &line[at + IDENTIFIER_KEY.len()..];

        let (identifier, version) = match attributes.find(VERSION_KEY) {
            Some(at) => (
                attributes[..at].trim(),
                Some(attributes[at + VERSION_KEY.len()..].trim()),
            ),
            None => (attributes.trim(), None),
        };

        // Spotlight prints `(null)` for a bundle that declares none. Seven of
        // this machine's applications do. They are counted rather than listed,
        // because nothing here could address them anyway.
        if identifier.is_empty() || identifier == "(null)" {
            without_identifier += 1;
            continue;
        }

        if applications.len() >= MAX_APPLICATIONS {
            break;
        }

        let name = std::path::Path::new(path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut entry = json!({
            "bundleId": identifier,
            "path": path,
            // Shown to a person, never used to look anything up.
            "name": name,
        });

        if let Some(version) = version.filter(|version| !version.is_empty() && *version != "(null)")
        {
            entry["version"] = json!(version);
        }

        applications.push(entry);
    }

    applications.sort_by(|left, right| left["bundleId"].as_str().cmp(&right["bundleId"].as_str()));

    let mut warnings = Vec::new();

    if without_identifier > 0 {
        warnings.push(format!(
            "{without_identifier} installed bundle(s) declare no identifier and are not listed, \
             because nothing could address them"
        ));
    }

    Ok(json!({
        "capability": "system.app.list",
        "operationResult": {
            "applicationCount": applications.len(),
            "applications": applications,
        },
        "warnings": warnings,
    }))
}

/// The bundle identifiers of everything running in front of the person.
///
/// Two ways of asking, because the better one costs a permission the machine
/// may not have granted. System Events is structured and documented and needs
/// Accessibility; `lsappinfo` needs nothing and is undocumented. Preferring the
/// first and falling back to the second means this capability does not vanish
/// on a machine that has not granted anything -- and refusing only when BOTH
/// fail means it never reports an empty desktop that is not empty.
fn running_bundle_ids() -> Result<(Vec<String>, &'static str), SystemError> {
    const SCRIPT: &str = r#"
tell application "System Events"
    set found to bundle identifier of every application process whose background only is false
end tell
set AppleScript's text item delimiters to linefeed
return found as text
"#;

    if let Ok((stdout, _, 0)) = run("/usr/bin/osascript", &["-e", SCRIPT]) {
        let found: Vec<String> = stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != "missing value")
            .map(str::to_owned)
            .collect();

        if !found.is_empty() {
            return Ok((found, "system-events"));
        }
    }

    // Accessibility is not granted, or System Events refused. `lsappinfo`
    // prints a block per application; the identifiers are in it among a good
    // deal of noise.
    let (stdout, _, status) = run("/usr/bin/lsappinfo", &["list"])?;

    if status != 0 {
        return Err(SystemError::execution(
            "neither System Events nor lsappinfo could say what is running. System Events needs \
             Accessibility permission; without it, and without lsappinfo, this machine will not \
             say -- which is reported rather than answered with an empty list",
        ));
    }

    let mut found = Vec::new();

    for token in stdout.split_whitespace() {
        let token = token.trim_matches(|character: char| !character.is_ascii_graphic());

        // A bundle identifier, and not one of the surrounding markers.
        if token.matches('.').count() >= 2
            && !token.contains('/')
            && !token.contains('=')
            && token
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
        {
            let owned = token.to_owned();
            if !found.contains(&owned) {
                found.push(owned);
            }
        }
    }

    Ok((found, "lsappinfo"))
}

fn is_running(bundle_id: &str) -> Result<bool, SystemError> {
    Ok(running_bundle_ids()?
        .0
        .iter()
        .any(|running| running.eq_ignore_ascii_case(bundle_id)))
}

/// Wait for an application to appear or disappear, and say whether it did.
fn settle(bundle_id: &str, wanted: bool) -> Result<bool, SystemError> {
    for _ in 0..SETTLE_ATTEMPTS {
        if is_running(bundle_id)? == wanted {
            return Ok(true);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(false)
}

pub(crate) fn list_running_applications(_input: &Value) -> Result<Value, SystemError> {
    let (running, source) = running_bundle_ids()?;

    Ok(json!({
        "capability": "system.app.running",
        "operationResult": {
            "runningCount": running.len(),
            "bundleIds": running,
            "source": source,
            "note": "applications running in front of the person; background services \
                     are not applications and are not listed here",
        },
        "warnings": [],
    }))
}

/// Start an application.
///
/// Reports whether it was ALREADY running, which the launch itself cannot say:
/// `open -b` exits 0 and prints nothing either way. That is established by
/// looking before and after, and it matters -- a caller told "launched" about
/// something that was already open would believe it had a fresh window.
pub(crate) fn launch_application(input: &Value) -> Result<Value, SystemError> {
    let bundle_id = bundle_id_of(input, "bundleId")?;
    let already = is_running(bundle_id)?;

    let (_, stderr, status) = run("/usr/bin/open", &["-b", bundle_id])?;

    if status != 0 {
        // This one IS trustworthy: an identifier nothing is installed under
        // exits non-zero and explains itself on stderr.
        return Err(SystemError::invalid(format!(
            "nothing is installed under {bundle_id}: {}",
            stderr.trim()
        )));
    }

    let appeared = already || settle(bundle_id, true)?;

    if !appeared {
        return Err(SystemError::execution(format!(
            "{bundle_id} was asked to start, the request was accepted, and it is still not \
             running. Reporting it as launched would be a guess"
        )));
    }

    Ok(json!({
        "capability": "system.app.launch",
        "operationResult": {
            "bundleId": bundle_id,
            "alreadyRunning": already,
            "status": if already { "already-running" } else { "launched" },
        },
        "warnings": [],
    }))
}

/// Ask an application to stop.
///
/// Asks. It does not kill: a person's unsaved work belongs to them, so an
/// application that puts up a "save your changes?" sheet is left showing it,
/// and this reports that it is still running rather than forcing the point.
///
/// The result is decided by observation, because `quit` reports success for an
/// application that was never running.
pub(crate) fn quit_application(input: &Value) -> Result<Value, SystemError> {
    let bundle_id = bundle_id_of(input, "bundleId")?;

    if !is_running(bundle_id)? {
        return Err(SystemError::invalid(format!(
            "{bundle_id} is not running. It is said rather than reported as a successful quit, \
             which is what the platform would have said"
        )));
    }

    let script = format!("tell application id \"{bundle_id}\" to quit");
    let (_, stderr, status) = run("/usr/bin/osascript", &["-e", &script])?;

    if status != 0 {
        return Err(SystemError::execution(format!(
            "{bundle_id} refused the request to quit: {}",
            stderr.trim()
        )));
    }

    let stopped = settle(bundle_id, false)?;

    Ok(json!({
        "capability": "system.app.quit",
        "operationResult": {
            "bundleId": bundle_id,
            "status": if stopped { "quit" } else { "still-running" },
            "note": if stopped {
                "the application stopped"
            } else {
                "the application accepted the request and is still running, which is what \
                 happens when it is asking the person about unsaved work. It was not forced"
            },
        },
        "warnings": [],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An application is addressed by identifier, and nothing else gets through.
    ///
    /// What this can and cannot establish is worth being exact about. It keeps
    /// a path, a shell fragment and anything non-ASCII off a command line,
    /// which is what stops a localized display name like `Safari浏览器` being
    /// used as an identifier. It CANNOT tell an ASCII display name from an
    /// identifier: this machine has an application whose identifier is
    /// `MacNetPlayer`, so shape does not separate them. Whether anything
    /// answers to a value is the machine's question, and `open -b` answers it.
    #[test]
    fn applications_are_addressed_by_identifier_and_never_by_name() {
        assert_eq!(
            bundle_id_of(&json!({"bundleId": " com.apple.Safari "}), "bundleId").unwrap(),
            "com.apple.Safari"
        );

        for (label, value) in [
            ("nothing at all", json!({})),
            ("an empty identifier", json!({"bundleId": "   "})),
            ("a display name", json!({"bundleId": "Safari浏览器"})),
            ("a name with a space", json!({"bundleId": "Google Chrome"})),
            ("a path", json!({"bundleId": "/Applications/Safari.app"})),
            ("a leading dot", json!({"bundleId": ".com.apple.Safari"})),
            ("a trailing dot", json!({"bundleId": "com.apple.Safari."})),
            // Anything that would mean something else on a command line.
            ("a shell fragment", json!({"bundleId": "com.apple.Safari;id"})),
            ("a quoted fragment", json!({"bundleId": "com.apple.Safari\""})),
        ] {
            assert!(
                bundle_id_of(&value, "bundleId").is_err(),
                "{label} must not be accepted as an identifier"
            );
        }
    }

    /// Launching and quitting refuse before they touch the machine.
    ///
    /// Deliberately no `#[ignore]`: these paths must fail on the request, and
    /// they must do it without starting or stopping anything, on any machine.
    #[test]
    fn a_request_that_names_no_application_is_refused_before_anything_happens() {
        for request in [json!({}), json!({"bundleId": ""})] {
            assert!(launch_application(&request).unwrap_err().invalid_request);
            assert!(quit_application(&request).unwrap_err().invalid_request);
        }
    }

    /// The real thing, against real applications.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS and starts Calculator"]
    fn applications_are_listed_started_and_stopped_real_e2e() {
        const CALCULATOR: &str = "com.apple.calculator";

        let installed = list_applications(&json!({})).unwrap();
        let applications = installed["operationResult"]["applications"]
            .as_array()
            .unwrap();

        assert!(
            applications.len() > 10,
            "a Mac has more applications than this -- {} were parsed out of mdfind, which \
             means the parsing is wrong rather than the machine being empty",
            applications.len()
        );

        // Every entry can address the thing it describes. That is the point of
        // the list; a row without an identifier is a row nothing can use, which
        // is why those are counted in the warnings instead of listed.
        for application in applications {
            let id = application["bundleId"].as_str().unwrap();
            // Not "contains a dot": this machine has `MacNetPlayer`. What has
            // to be true is that the value can address something.
            assert!(
                !id.is_empty() && id.is_ascii() && !id.contains(char::is_whitespace),
                "identifier {id:?} could not be used to address anything"
            );
        }

        assert!(
            applications
                .iter()
                .any(|application| application["bundleId"] == CALCULATOR),
            "Calculator is part of macOS and should be listed"
        );

        // Quitting something that is not running is refused rather than
        // reported as a success -- which is exactly what the platform WOULD
        // report, and the reason this adapter observes instead of trusting it.
        if !is_running(CALCULATOR).unwrap() {
            let refused = quit_application(&json!({"bundleId": CALCULATOR})).unwrap_err();
            assert!(refused.invalid_request);
            assert!(refused.message.contains("not running"));
        }

        let launched = launch_application(&json!({"bundleId": CALCULATOR})).unwrap();
        assert_eq!(launched["operationResult"]["status"], "launched");
        assert_eq!(launched["operationResult"]["alreadyRunning"], false);

        // Asked again, it says what actually happened, which is nothing.
        let again = launch_application(&json!({"bundleId": CALCULATOR})).unwrap();
        assert_eq!(again["operationResult"]["alreadyRunning"], true);
        assert_eq!(again["operationResult"]["status"], "already-running");

        let running = list_running_applications(&json!({})).unwrap();
        assert!(running["operationResult"]["bundleIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == CALCULATOR));

        let stopped = quit_application(&json!({"bundleId": CALCULATOR})).unwrap();
        assert_eq!(stopped["operationResult"]["status"], "quit");
        assert!(!is_running(CALCULATOR).unwrap());

        // An identifier nothing is installed under is a request problem, and
        // the platform's own exit status is what establishes it.
        let missing =
            launch_application(&json!({"bundleId": "com.ai-os.definitely.not.installed"}))
                .unwrap_err();
        assert!(missing.invalid_request);

        // A display name is refused too -- by the MACHINE rather than by a
        // pattern, which is the only thing that can tell them apart now that an
        // identifier is known not to need a dot.
        let by_name = launch_application(&json!({"bundleId": "Calculator"})).unwrap_err();
        assert!(by_name.invalid_request);
        assert!(!is_running(CALCULATOR).unwrap(), "a name must not have started it");
    }
}

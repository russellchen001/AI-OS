//! Deterministic system output-audio state.
//!
//! Computer Control owns direct output-volume and mute-state operations only.
//! It does not operate media applications, record input, interpret speech,
//! inspect the screen, press keys or decide which audio state is appropriate.

use crate::system::SystemError;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioState {
    output_volume: u8,
    output_muted: bool,
}

fn volume_from(input: &Value) -> Result<u8, SystemError> {
    let volume = input.get("volume").and_then(Value::as_u64).ok_or_else(|| {
        SystemError::invalid("system.audio.volume.set requires integer volume from 0 through 100")
    })?;

    if volume > 100 {
        return Err(SystemError::invalid(
            "system.audio.volume.set requires integer volume from 0 through 100",
        ));
    }

    Ok(volume as u8)
}

fn mute_from(input: &Value) -> Result<bool, SystemError> {
    input
        .get("muted")
        .and_then(Value::as_bool)
        .ok_or_else(|| SystemError::invalid("system.audio.mute.set requires boolean muted"))
}

fn parse_audio_state(output: &str) -> Result<AudioState, SystemError> {
    let value = output.trim();

    let (volume, muted) = value.split_once('|').ok_or_else(|| {
        SystemError::execution(format!(
            "macOS returned an unexpected audio-state response: {value:?}"
        ))
    })?;

    let output_volume = volume.parse::<u8>().map_err(|_| {
        SystemError::execution(format!(
            "macOS returned an invalid output volume: {volume:?}"
        ))
    })?;

    if output_volume > 100 {
        return Err(SystemError::execution(format!(
            "macOS returned output volume outside 0 through 100: {output_volume}"
        )));
    }

    let output_muted = match muted {
        "true" => true,
        "false" => false,
        other => {
            return Err(SystemError::execution(format!(
                "macOS returned an invalid output mute state: {other:?}"
            )));
        }
    };

    Ok(AudioState {
        output_volume,
        output_muted,
    })
}

#[cfg(target_os = "macos")]
fn run_audio_script(script: &str, args: &[&str]) -> Result<AudioState, SystemError> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            SystemError::execution(format!("macOS audio automation could not start: {error}"))
        })?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            SystemError::execution("macOS audio automation has no script input stream")
        })?;

        stdin.write_all(script.as_bytes()).map_err(|error| {
            SystemError::execution(format!(
                "macOS audio automation script could not be written: {error}"
            ))
        })?;
    }

    drop(child.stdin.take());

    let output = child.wait_with_output().map_err(|error| {
        SystemError::execution(format!("macOS audio automation did not complete: {error}"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

        return Err(SystemError::execution(if stderr.is_empty() {
            "macOS audio automation failed".to_owned()
        } else {
            format!("macOS audio automation failed: {stderr}")
        }));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| SystemError::execution("macOS audio automation returned non-UTF-8 output"))?;

    parse_audio_state(&stdout)
}

#[cfg(target_os = "macos")]
fn read_state() -> Result<AudioState, SystemError> {
    run_audio_script(
        r#"
on run
    set currentSettings to get volume settings
    return ((output volume of currentSettings) as text) & "|" & ((output muted of currentSettings) as text)
end run
"#,
        &[],
    )
}

#[cfg(target_os = "macos")]
fn apply_volume(volume: u8) -> Result<AudioState, SystemError> {
    let requested = volume.to_string();

    let state = run_audio_script(
        r#"
on run argv
    set requestedVolume to (item 1 of argv) as integer
    set volume output volume requestedVolume
    set currentSettings to get volume settings
    return ((output volume of currentSettings) as text) & "|" & ((output muted of currentSettings) as text)
end run
"#,
        &[requested.as_str()],
    )?;

    if state.output_volume != volume {
        return Err(SystemError::execution(format!(
            "macOS reported output volume {} after {} was requested",
            state.output_volume, volume
        )));
    }

    Ok(state)
}

#[cfg(target_os = "macos")]
fn apply_mute(muted: bool) -> Result<AudioState, SystemError> {
    let requested = if muted { "true" } else { "false" };

    let state = run_audio_script(
        r#"
on run argv
    set requestedMute to item 1 of argv

    if requestedMute is "true" then
        set volume with output muted
    else if requestedMute is "false" then
        set volume without output muted
    else
        error "AIOS_INVALID_MUTE"
    end if

    set currentSettings to get volume settings
    return ((output volume of currentSettings) as text) & "|" & ((output muted of currentSettings) as text)
end run
"#,
        &[requested],
    )?;

    if state.output_muted != muted {
        return Err(SystemError::execution(format!(
            "macOS reported output muted {} after {} was requested",
            state.output_muted, muted
        )));
    }

    Ok(state)
}

fn response(capability: &str, status: &str, state: AudioState) -> Value {
    json!({
        "capability": capability,
        "operationResult": {
            "status": status,
            "outputVolume": state.output_volume,
            "outputMuted": state.output_muted
        }
    })
}

pub(crate) fn get_output_volume(_input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        return read_state().map(|state| response("system.audio.volume.get", "read", state));
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(SystemError::execution(
            "system.audio.volume.get has no adapter on this operating system in v1.0",
        ))
    }
}

pub(crate) fn set_output_volume(input: &Value) -> Result<Value, SystemError> {
    let volume = volume_from(input)?;

    #[cfg(target_os = "macos")]
    {
        return apply_volume(volume).map(|state| response("system.audio.volume.set", "set", state));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = volume;

        Err(SystemError::execution(
            "system.audio.volume.set has no adapter on this operating system in v1.0",
        ))
    }
}

pub(crate) fn get_output_mute(_input: &Value) -> Result<Value, SystemError> {
    #[cfg(target_os = "macos")]
    {
        return read_state().map(|state| response("system.audio.mute.get", "read", state));
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(SystemError::execution(
            "system.audio.mute.get has no adapter on this operating system in v1.0",
        ))
    }
}

pub(crate) fn set_output_mute(input: &Value) -> Result<Value, SystemError> {
    let muted = mute_from(input)?;

    #[cfg(target_os = "macos")]
    {
        return apply_mute(muted).map(|state| response("system.audio.mute.set", "set", state));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = muted;

        Err(SystemError::execution(
            "system.audio.mute.set has no adapter on this operating system in v1.0",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_set_requires_an_integer_percent() {
        for rejected in [
            json!({}),
            json!({"volume": -1}),
            json!({"volume": 101}),
            json!({"volume": 25.5}),
            json!({"volume": "25"}),
            json!({"volume": true}),
        ] {
            let error = volume_from(&rejected).unwrap_err();
            assert!(error.invalid_request);
        }

        assert_eq!(volume_from(&json!({"volume": 0})).unwrap(), 0);
        assert_eq!(volume_from(&json!({"volume": 37})).unwrap(), 37);
        assert_eq!(volume_from(&json!({"volume": 100})).unwrap(), 100);
    }

    #[test]
    fn mute_set_requires_a_boolean() {
        for rejected in [
            json!({}),
            json!({"muted": 0}),
            json!({"muted": 1}),
            json!({"muted": "true"}),
        ] {
            let error = mute_from(&rejected).unwrap_err();
            assert!(error.invalid_request);
        }

        assert!(!mute_from(&json!({"muted": false})).unwrap());
        assert!(mute_from(&json!({"muted": true})).unwrap());
    }

    #[test]
    fn audio_state_parser_is_strict() {
        assert_eq!(
            parse_audio_state("37|false\n").unwrap(),
            AudioState {
                output_volume: 37,
                output_muted: false,
            }
        );

        for rejected in ["", "37", "37:false", "101|false", "37|maybe", "loud|false"] {
            assert!(
                parse_audio_state(rejected).is_err(),
                "{rejected:?} should not be accepted"
            );
        }
    }

    #[cfg(target_os = "macos")]
    struct RestoreAudioState {
        original: AudioState,
    }

    #[cfg(target_os = "macos")]
    impl Drop for RestoreAudioState {
        fn drop(&mut self) {
            let _ = apply_volume(self.original.output_volume);
            let _ = apply_mute(self.original.output_muted);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "changes macOS output audio state and restores it"]
    fn audio_state_changes_and_returns_to_exact_original_real_e2e() {
        let original = read_state().unwrap();
        let _restore = RestoreAudioState { original };

        let read_volume = get_output_volume(&json!({})).unwrap();
        assert_eq!(
            read_volume["operationResult"]["outputVolume"],
            json!(original.output_volume)
        );

        let read_mute = get_output_mute(&json!({})).unwrap();
        assert_eq!(
            read_mute["operationResult"]["outputMuted"],
            json!(original.output_muted)
        );

        apply_mute(true).unwrap();

        let target_volume = if original.output_volume < 100 {
            original.output_volume + 1
        } else {
            99
        };

        let changed = set_output_volume(&json!({
            "volume": target_volume
        }))
        .unwrap();

        assert_eq!(
            changed["operationResult"]["outputVolume"],
            json!(target_volume)
        );

        assert_eq!(read_state().unwrap().output_volume, target_volume);

        apply_volume(0).unwrap();

        let unmuted = set_output_mute(&json!({"muted": false})).unwrap();

        assert_eq!(unmuted["operationResult"]["outputMuted"], json!(false));

        let muted = set_output_mute(&json!({"muted": true})).unwrap();

        assert_eq!(muted["operationResult"]["outputMuted"], json!(true));

        apply_volume(original.output_volume).unwrap();
        apply_mute(original.output_muted).unwrap();

        assert_eq!(read_state().unwrap(), original);
    }
}

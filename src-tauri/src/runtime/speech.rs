//! AI-OS Speech Skill.
//! Local First: Kokoro MLX; explicitly selected online: Edge TTS.
//! No shell execution and no Agent-controlled output path.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

use super::skill_invocation::{
    SkillInvocationError,
    SkillInvocationErrorKind,
    SkillInvocationRequest,
    SkillInvocationResult,
};

const MAX_TEXT_CHARS: usize = 2000;
const EXECUTION_TIMEOUT_SECS: u64 = 120;

const KOKORO_RUNNER: &str = include_str!("speech_kokoro.py");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpeechBackend {
    Auto,
    Local,
    Online,
}

#[derive(Debug)]
struct SpeechInput<'a> {
    text: &'a str,
    backend: SpeechBackend,
    voice: &'a str,
}

fn invalid() -> SkillInvocationError {
    SkillInvocationError::new(
        SkillInvocationErrorKind::InvalidRequest,
        "Invalid Speech Skill input.",
        false,
    )
}

fn failed() -> SkillInvocationError {
    SkillInvocationError::new(
        SkillInvocationErrorKind::ExecutionFailed,
        "Speech synthesis failed or timed out.",
        true,
    )
}

fn validate(input: &Value) -> Result<SpeechInput<'_>, SkillInvocationError> {
    let object = input.as_object().ok_or_else(invalid)?;

    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "text" | "voice" | "backend"))
    {
        return Err(invalid());
    }

    let text = object
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;

    if text.trim().is_empty() || text.chars().count() > MAX_TEXT_CHARS {
        return Err(invalid());
    }

    let backend = match object.get("backend") {
        None => SpeechBackend::Auto,
        Some(Value::String(value)) if value == "auto" => SpeechBackend::Auto,
        Some(Value::String(value)) if value == "local" => SpeechBackend::Local,
        Some(Value::String(value)) if value == "online" => SpeechBackend::Online,
        _ => return Err(invalid()),
    };

    let voice = match (backend, object.get("voice")) {
        (SpeechBackend::Auto | SpeechBackend::Local, None) => "zf_xiaobei",

        (SpeechBackend::Auto | SpeechBackend::Local, Some(Value::String(v)))
            if matches!(v.as_str(), "zf_xiaobei" | "zm_yunxi" | "af_heart") =>
        {
            v
        }

        (SpeechBackend::Online, None) => "zh-CN-XiaoxiaoNeural",

        (SpeechBackend::Online, Some(Value::String(v)))
            if matches!(
                v.as_str(),
                "zh-CN-XiaoxiaoNeural" | "en-US-JennyNeural"
            ) =>
        {
            v
        }

        _ => return Err(invalid()),
    };

    Ok(SpeechInput {
        text,
        backend,
        voice,
    })
}

fn kokoro_python() -> Result<PathBuf, SkillInvocationError> {
    let home = dirs::home_dir().ok_or_else(failed)?;

    let python = home
        .join("Library")
        .join("Application Support")
        .join("AI-OS")
        .join("speech")
        .join("kokoro-venv")
        .join("bin")
        .join("python");

    if !python.is_file() {
        return Err(SkillInvocationError::new(
            SkillInvocationErrorKind::BackendUnavailable,
            "Local Kokoro environment is not installed.",
            false,
        ));
    }

    Ok(python)
}

fn wait_for_process(
    child: &mut std::process::Child,
) -> Result<(), SkillInvocationError> {
    let deadline = Instant::now() + Duration::from_secs(EXECUTION_TIMEOUT_SECS);

    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),

            Ok(Some(_)) => return Err(failed()),

            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }

            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failed());
            }
        }
    }
}

fn execute_kokoro(
    input: &SpeechInput<'_>,
    output: &Path,
) -> Result<(), SkillInvocationError> {
    let python = kokoro_python()?;

    let mut child = Command::new(python)
        .arg("-c")
        .arg(KOKORO_RUNNER)
        .arg(input.voice)
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| failed())?;

    let write_result = child
        .stdin
        .take()
        .ok_or_else(failed)
        .and_then(|mut stdin| {
            stdin
                .write_all(input.text.as_bytes())
                .map_err(|_| failed())
        });

    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }

    wait_for_process(&mut child)
}

fn execute_edge(
    input: &SpeechInput<'_>,
    output: &Path,
) -> Result<(), SkillInvocationError> {
    // The executable path is fixed. Agent cannot supply commands or URLs.
    let mut child = Command::new("/opt/homebrew/bin/uvx")
        .args([
            "--from",
            "edge-tts",
            "edge-tts",
            "--voice",
            input.voice,
            "--text",
            input.text,
            "--write-media",
        ])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| failed())?;

    wait_for_process(&mut child)
}

fn output_path(extension: &str) -> Result<PathBuf, SkillInvocationError> {
    let directory = std::env::temp_dir().join("ai-os-speech");

    fs::create_dir_all(&directory).map_err(|_| failed())?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| failed())?
        .as_nanos();

    Ok(directory.join(format!(
        "speech-{}-{timestamp}.{extension}",
        std::process::id()
    )))
}

pub(super) fn synthesize(
    request: &SkillInvocationRequest,
) -> Result<SkillInvocationResult, SkillInvocationError> {
    if request.capability != "speech.synthesize" {
        return Err(invalid());
    }

    let input = validate(&request.input)?;

    // Auto is strictly Local First in v1.
    // It NEVER silently falls back to an online provider.
    let (provider, format, output) = match input.backend {
        SpeechBackend::Auto | SpeechBackend::Local => {
            ("kokoro-mlx", "wav", output_path("wav")?)
        }

        SpeechBackend::Online => {
            ("edge-tts", "mp3", output_path("mp3")?)
        }
    };

    let execution = match input.backend {
        SpeechBackend::Auto | SpeechBackend::Local => {
            execute_kokoro(&input, &output)
        }

        SpeechBackend::Online => execute_edge(&input, &output),
    };

    if let Err(error) = execution {
        let _ = fs::remove_file(&output);
        return Err(error);
    }

    let size = fs::metadata(&output)
        .map_err(|_| failed())?
        .len();

    if size < 128 {
        let _ = fs::remove_file(&output);
        return Err(failed());
    }

    Ok(SkillInvocationResult {
        invocation_id: request.invocation_id.clone(),
        capability: request.capability.clone(),
        backend: "speech".to_owned(),
        provider: Some(provider.to_owned()),
        output: json!({
            "status": "completed",
            "format": format,
            "provider": provider,
            "voice": input.voice,
            "filePath": output.to_string_lossy(),
            "bytes": size
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_local_kokoro() {
        let request = json!({"text": "你好"});
        let input = validate(&request).unwrap();

        assert_eq!(input.backend, SpeechBackend::Auto);
        assert_eq!(input.voice, "zf_xiaobei");
    }

    #[test]
    fn local_and_online_have_separate_voice_lists() {
        assert!(validate(&json!({
            "text": "你好",
            "backend": "local",
            "voice": "zm_yunxi"
        })).is_ok());

        assert!(validate(&json!({
            "text": "Hello",
            "backend": "online",
            "voice": "en-US-JennyNeural"
        })).is_ok());

        assert!(validate(&json!({
            "text": "Hello",
            "backend": "local",
            "voice": "en-US-JennyNeural"
        })).is_err());
    }

    #[test]
    fn rejects_unsafe_or_invalid_input() {
        for input in [
            json!({}),
            json!({"text": ""}),
            json!({"text": " "}),
            json!({"text": "x".repeat(2001)}),
            json!({"text": "hello", "command": "sh"}),
            json!({"text": "hello", "outputPath": "/tmp/unsafe"}),
            json!({"text": "hello", "backend": "unknown"}),
            json!({"text": "hello", "voice": "unknown"}),
        ] {
            assert!(validate(&input).is_err(), "{input:?}");
        }
    }

    #[test]
    #[ignore = "Requires local Kokoro model"]
    fn real_kokoro_produces_wav() {
        let request = SkillInvocationRequest::new(
            "kokoro-test",
            "speech.synthesize",
            json!({
                "text": "你好，这是 AI OS 的本地语音合成测试。",
                "backend": "local"
            }),
        )
        .unwrap();

        let result = synthesize(&request).unwrap();

        assert_eq!(result.provider.as_deref(), Some("kokoro-mlx"));
        assert_eq!(result.output["format"], "wav");

        let path = result.output["filePath"].as_str().unwrap();

        assert!(fs::metadata(path).unwrap().len() > 128);
    }

    #[test]
    #[ignore = "Requires online Edge TTS"]
    fn real_edge_tts_produces_mp3() {
        let request = SkillInvocationRequest::new(
            "edge-test",
            "speech.synthesize",
            json!({
                "text": "你好，这是 AI OS 在线语音测试。",
                "backend": "online"
            }),
        )
        .unwrap();

        let result = synthesize(&request).unwrap();

        assert_eq!(result.provider.as_deref(), Some("edge-tts"));
        assert_eq!(result.output["format"], "mp3");

        let path = result.output["filePath"].as_str().unwrap();

        assert!(fs::metadata(path).unwrap().len() > 128);
    }
}

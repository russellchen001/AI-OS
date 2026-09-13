//! Local audio transcription into timestamped AI-OS evidence.
//!
//! Audio is demuxed with ffmpeg and transcribed by whisper.cpp's `whisper-cli`,
//! both local, both MIT, neither needing an account, a key or a network call. The
//! recording never leaves the machine, which is what `noSilentCloudUpload`
//! promises.
//!
//! ## Video is read, not reduced to its soundtrack
//!
//! The evidence contract requires, per modality:
//!
//! ```text
//! Audio  ->  transcript AND timestamps
//! Video  ->  transcript AND timestamps AND visual observations
//! ```
//!
//! A tutorial screen recording often has NO speech at all, so transcribing the
//! soundtrack would return nothing from exactly the videos people most want
//! distilled. Video therefore goes through `visual`, which samples frames and
//! reads the text on screen, and the two are combined:
//!
//! - where someone is speaking, the item carries the speech and records what was
//!   on screen while it was said
//! - where nothing is said, the item carries the on-screen text itself
//!
//! Both shapes satisfy the Video requirement honestly, and neither pretends the
//! picture was understood beyond the text in it. See `visual` for what OCR can
//! and cannot observe.
//!
//! ## Confidence is measured, not encoded
//!
//! `whisper-cli -ojf` emits a probability `p` for every token. A segment's
//! confidence is the mean of its own tokens' probabilities — the model's own
//! number, not a constant chosen by AI-OS. (Contrast the research-enrichment
//! adapter, where a three-level editorial grade had to be encoded because nothing
//! better existed.)

use super::{
    evidence::{
        normalize_extraction, DistillationEvidence, EvidenceAssertion, EvidenceExtraction,
        EvidenceLocation, ExtractedEvidenceItem, ExtractionPolicy, ExtractionTarget,
        SourceArtifact,
    },
    SourceKind, SourceMediaKind, SubjectKind,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) const TRANSCRIPTION_EXTRACTOR: &str = "whisper.cpp";

const MAX_MEDIA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SEGMENTS: usize = 5_000;
const MAX_SEGMENT_TEXT_BYTES: usize = 8 * 1024;

/// The whisper-cli JSON contract, as written by `examples/cli/cli.cpp`.
///
/// Only the fields AI-OS actually consumes are declared. `offsets` are
/// milliseconds: the writer emits `t0 * 10`, and whisper's internal t0/t1 are
/// centiseconds.
#[derive(Debug, Deserialize)]
struct WhisperOutput {
    model: WhisperModel,
    params: WhisperParams,
    transcription: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperModel {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct WhisperParams {
    model: String,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    offsets: WhisperOffsets,
    text: String,
    #[serde(default)]
    tokens: Vec<WhisperToken>,
}

#[derive(Debug, Deserialize)]
struct WhisperOffsets {
    from: i64,
    to: i64,
}

#[derive(Debug, Deserialize)]
struct WhisperToken {
    #[serde(default)]
    p: f32,
}

pub(crate) struct TranscriptionTools {
    pub ffmpeg: PathBuf,
    pub whisper: PathBuf,
    pub model: PathBuf,
    /// Only needed for video. Absent is fine for audio, and is reported by name
    /// if a video arrives without it.
    pub tesseract: Option<PathBuf>,
    pub ocr_languages: String,
    /// Only needed for a video that has neither speech nor screen text. Absent is
    /// fine everywhere else.
    pub vision: Option<super::visual::VisionModel>,
}

impl TranscriptionTools {
    /// Resolve the local toolchain, or say precisely which part is missing.
    ///
    /// A vague "transcription unavailable" would leave a person guessing which of
    /// three things to install, so each is reported by name.
    pub(crate) fn resolve() -> Result<Self, String> {
        // These ship inside AI-OS. If one is missing the installation itself is
        // damaged, which is a different thing from a model not being downloaded
        // yet, and is worded so the two are never confused.
        let ffmpeg = which("ffmpeg").ok_or_else(|| {
            "The AI-OS media tools are missing from this installation.".to_owned()
        })?;
        let whisper = which("whisper-cli").ok_or_else(|| {
            "The AI-OS media tools are missing from this installation.".to_owned()
        })?;
        // A model, by contrast, is downloaded on demand, so its absence is a
        // normal state with an action attached rather than a failure.
        let model = resolve_model().ok_or_else(|| {
            super::toolchain::requirement(
                super::toolchain::MediaTier::Speech,
                super::toolchain::ModelProfile::for_this_machine(),
            )
            .message
            .unwrap_or_else(|| "Speech reading is not set up yet.".to_owned())
        })?;
        let tesseract = super::visual::find_tesseract();
        let ocr_languages = tesseract
            .as_deref()
            .map(super::visual::ocr_languages)
            .unwrap_or_else(|| "eng".to_owned());
        Ok(Self {
            ffmpeg,
            whisper,
            model,
            tesseract,
            ocr_languages,
            vision: super::visual::find_vision_model(),
        })
    }
}

/// Reading a picture needs a model that is fetched on demand, so its absence is
/// a setup state with a size attached — not an error, and never an install
/// command aimed at whoever built AI-OS.
fn picture_reading_not_set_up(situation: &str) -> String {
    let requirement = super::toolchain::requirement(
        super::toolchain::MediaTier::Picture,
        super::toolchain::ModelProfile::for_this_machine(),
    );
    match requirement.message {
        Some(message) => format!("{situation} {message}"),
        None => format!("{situation} Describing the picture did not produce anything."),
    }
}

/// What kind of thing the person actually handed over.
///
/// Decided by asking ffprobe what streams the file contains rather than trusting
/// its extension, because a `.mov` holding one frame is a picture and a `.png`
/// renamed to `.mp4` is still a picture. Extension is the fallback when ffprobe
/// cannot say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaShape {
    Image,
    Audio,
    Video,
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "avif",
];

pub(crate) fn detect_shape(tools: &TranscriptionTools, media: &Path) -> MediaShape {
    if let Some(shape) = probe_shape(tools, media) {
        return shape;
    }
    let extension = media
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        MediaShape::Image
    } else {
        MediaShape::Video
    }
}

fn probe_shape(tools: &TranscriptionTools, media: &Path) -> Option<MediaShape> {
    let ffprobe = ffprobe_path(tools)?;
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type:format=duration",
            // key=value output, so fields are read BY NAME. ffprobe emits csv
            // fields in its own order, not the order they were requested, and
            // reading them positionally is how a parser silently misclassifies.
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(media)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let mut has_audio = false;
    let mut has_video = false;
    let mut playable_duration = false;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        match (key, value) {
            ("codec_type", "audio") => has_audio = true,
            ("codec_type", "video") => has_video = true,
            // A still picture reports no duration; a clip always has one.
            ("duration", value) => {
                playable_duration = value.parse::<f64>().is_ok_and(|seconds| seconds > 0.0)
            }
            _ => {}
        }
    }

    if !has_audio && !has_video {
        return None;
    }
    Some(if has_video && !has_audio && !playable_duration {
        MediaShape::Image
    } else if has_video {
        MediaShape::Video
    } else {
        MediaShape::Audio
    })
}

fn ffprobe_path(tools: &TranscriptionTools) -> Option<PathBuf> {
    let beside = tools.ffmpeg.parent()?.join("ffprobe");
    if beside.is_file() {
        return Some(beside);
    }
    which("ffprobe")
}

/// Read one still image: the text in it, or failing that a description of it.
///
/// The Image modality asks for text OR visual observations, not both, so a
/// screenshot full of text needs no model at all and a photograph needs no OCR.
pub(crate) fn evidence_from_image(
    media: &Path,
    subject_kind: SubjectKind,
    correlation_group: &str,
    work_dir: &Path,
    tools: &TranscriptionTools,
) -> Result<MediaTranscription, String> {
    let digest = validated_digest(media)?;

    let text = tools
        .tesseract
        .as_deref()
        .map(|tesseract| super::visual::read_image_text(media, tesseract, &tools.ocr_languages))
        .unwrap_or_default();

    let mut note = None;
    let item = if !text.is_empty() {
        ExtractedEvidenceItem {
            evidence_id: "image-text".to_owned(),
            location: EvidenceLocation {
                page_start: None,
                page_end: None,
                time_start_ms: None,
                time_end_ms: None,
                region: None,
            },
            speaker: None,
            extracted_text: Some(text),
            visual_observations: Vec::new(),
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            assertion: EvidenceAssertion::Inferred,
            confidence: 0.5,
        }
    } else {
        // Nothing legible. Describing the picture is the only thing left, and it
        // is the same last resort a soundless, textless video falls back to.
        let vision = tools
            .vision
            .as_ref()
            .ok_or_else(|| picture_reading_not_set_up("This image has no legible text."))?;
        let described = super::visual::describe_image(media, work_dir, vision)?;
        note = Some(
            "No legible text was found in this image, so it was described by a local vision model; what follows is a description of the picture, not something written in it."
                .to_owned(),
        );
        ExtractedEvidenceItem {
            evidence_id: "image-description".to_owned(),
            location: EvidenceLocation {
                page_start: None,
                page_end: None,
                time_start_ms: None,
                time_end_ms: None,
                region: None,
            },
            speaker: None,
            // A description is not text that was in the picture.
            extracted_text: None,
            visual_observations: vec![described],
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            assertion: EvidenceAssertion::Inferred,
            confidence: 0.35,
        }
    };

    let source = SourceArtifact {
        source_id: format!("image-{}", &digest[..16]),
        media_kind: SourceMediaKind::Image,
        source_kind: SourceKind::UserFile,
        opaque_reference: format!("local image {digest}"),
        source_digest: digest,
        correlation_group: correlation_group.to_owned(),
        authorized: true,
        private: true,
    };

    let evidence = normalize_extraction(
        subject_kind,
        &source,
        &ExtractionPolicy {
            target: ExtractionTarget::Local,
            cloud_authorized: false,
            public_research_authorized: false,
        },
        EvidenceExtraction {
            extractor_identity: if item.extracted_text.is_some() {
                "tesseract".to_owned()
            } else {
                "llama.cpp vision".to_owned()
            },
            extractor_revision: "local".to_owned(),
            asserted_sensitive_traits: Vec::new(),
            items: vec![item],
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(MediaTranscription {
        evidence,
        source,
        screen_states_read: 0,
        note,
    })
}

/// One entry point for whatever the person hands over.
pub(crate) fn evidence_from_any_media(
    media: &Path,
    subject_kind: SubjectKind,
    correlation_group: &str,
    work_dir: &Path,
    tools: &TranscriptionTools,
) -> Result<MediaTranscription, String> {
    match detect_shape(tools, media) {
        MediaShape::Image => {
            evidence_from_image(media, subject_kind, correlation_group, work_dir, tools)
        }
        _ => evidence_from_media(media, subject_kind, correlation_group, work_dir, tools),
    }
}

/// Transcribe one local audio OR video file into validated, timestamped evidence.
///
/// A video is accepted; only its soundtrack is used, and the result says so.
pub(crate) fn evidence_from_media(
    media: &Path,
    subject_kind: SubjectKind,
    correlation_group: &str,
    work_dir: &Path,
    tools: &TranscriptionTools,
) -> Result<MediaTranscription, String> {
    let metadata =
        fs::symlink_metadata(media).map_err(|_| "The media file could not be read.".to_owned())?;
    if metadata.file_type().is_symlink() {
        return Err("The media file is a symbolic link and was not transcribed.".to_owned());
    }
    if metadata.len() == 0 {
        return Err("The media file is empty.".to_owned());
    }
    if metadata.len() > MAX_MEDIA_BYTES {
        return Err("The media file exceeds the bounded transcription limit.".to_owned());
    }

    // The ORIGINAL file is the source of record. The demuxed wav is a derived
    // working file, so digesting the wav would record provenance for something the
    // person never gave us.
    let digest = file_digest(media)?;

    let carries_picture = has_video_stream(tools, media).unwrap_or(false);

    // Audio first: a video may also be spoken over, and the soundtrack is read
    // the same way either way.
    let wav = work_dir.join("audio-16k-mono.wav");
    demux_to_wav(tools, media, &wav)?;
    let prefix = work_dir.join("transcript");
    run_whisper(tools, &wav, &prefix)?;
    let output: WhisperOutput = serde_json::from_str(
        &fs::read_to_string(prefix.with_extension("json"))
            .map_err(|_| "whisper-cli produced no JSON transcript.".to_owned())?,
    )
    .map_err(|_| {
        "The whisper-cli transcript did not match its documented JSON contract.".to_owned()
    })?;
    let speech = segments_to_items(&output).unwrap_or_default();

    let media_kind = if carries_picture {
        SourceMediaKind::Video
    } else {
        SourceMediaKind::Audio
    };

    let mut note = None;
    let mut screen_states_read = 0;
    let items = if carries_picture {
        let tesseract = tools.tesseract.as_deref().ok_or_else(|| {
            "The AI-OS media tools are missing from this installation.".to_owned()
        })?;
        let reading = super::visual::read_screen_text(
            media,
            work_dir,
            &tools.ffmpeg,
            tesseract,
            &tools.ocr_languages,
        )?;
        screen_states_read = reading.states.len();
        if reading.no_legible_text {
            note = Some(
                "No legible text was found on screen. Only text can be read from the picture; imagery, gestures and physical demonstrations are not described."
                    .to_owned(),
            );
        }
        let mut items = video_items(&speech, &reading.states);

        // Nothing said and nothing written: the only thing left is to look at the
        // picture. This is the soundless demonstration case — hands showing a
        // technique — which every other path returns empty for.
        if items.is_empty() {
            if let Some(vision) = tools.vision.as_ref() {
                let described = super::visual::describe_frames(work_dir, vision)?;
                screen_states_read = described.len();
                note = Some(format!(
                    "No speech and no legible text on screen. {} sampled frames were described by a local vision model; everything here is a description of the picture, not something written or said.",
                    described.len()
                ));
                items = described_items(&described);
            }
        }

        if items.is_empty() {
            return Err(picture_reading_not_set_up(
                "This video has no speech and no legible text on screen.",
            ));
        }
        items
    } else {
        if speech.is_empty() {
            return Err("The transcript contained no usable speech.".to_owned());
        }
        speech
    };

    let source = SourceArtifact {
        source_id: format!("media-{}", &digest[..16]),
        media_kind,
        source_kind: SourceKind::UserFile,
        opaque_reference: if carries_picture {
            format!("local video {digest}")
        } else {
            format!("local audio {digest}")
        },
        source_digest: digest,
        correlation_group: correlation_group.to_owned(),
        authorized: true,
        // Media someone hands to AI-OS is private unless they say otherwise, and
        // private media must never trigger public research.
        private: true,
    };

    let evidence = normalize_extraction(
        subject_kind,
        &source,
        &ExtractionPolicy {
            target: ExtractionTarget::Local,
            cloud_authorized: false,
            public_research_authorized: false,
        },
        EvidenceExtraction {
            extractor_identity: TRANSCRIPTION_EXTRACTOR.to_owned(),
            extractor_revision: format!(
                "{} ({})",
                output.model.kind,
                Path::new(&output.params.model)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown-model".to_owned())
            ),
            asserted_sensitive_traits: Vec::new(),
            items,
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(MediaTranscription {
        evidence,
        source,
        screen_states_read,
        note,
    })
}

/// Turn vision-model descriptions into evidence.
///
/// `extracted_text` stays EMPTY: a generated description is not text that was in
/// the source, and putting it there would let a description be quoted later as if
/// someone had written or said it. It is a visual observation, which is exactly
/// what the Video modality requires.
fn described_items(described: &[super::visual::FrameDescription]) -> Vec<ExtractedEvidenceItem> {
    described
        .iter()
        .enumerate()
        .map(|(index, description)| ExtractedEvidenceItem {
            evidence_id: format!("described-{index:05}"),
            location: EvidenceLocation {
                page_start: None,
                page_end: None,
                time_start_ms: Some(description.start_ms),
                time_end_ms: Some(description.end_ms),
                region: None,
            },
            speaker: None,
            extracted_text: None,
            visual_observations: vec![description.text.clone()],
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            // A model describing a picture is inference by definition.
            assertion: EvidenceAssertion::Inferred,
            // Lower than a transcript: this is a small local model's reading of a
            // sampled frame, and review should treat it as the weakest evidence
            // in the bundle.
            confidence: 0.35,
        })
        .collect()
}

/// Combine what was said with what was shown.
///
/// A spoken segment records the screen text visible while it was said; a stretch
/// of screen with nothing spoken over it becomes an item in its own right. That
/// second case is what makes a silent tutorial distillable at all.
fn video_items(
    speech: &[ExtractedEvidenceItem],
    states: &[super::visual::ScreenState],
) -> Vec<ExtractedEvidenceItem> {
    let mut items = Vec::new();

    for segment in speech {
        let (start, end) = (
            segment.location.time_start_ms.unwrap_or(0),
            segment.location.time_end_ms.unwrap_or(0),
        );
        let on_screen = states
            .iter()
            .filter(|state| state.start_ms < end && state.end_ms > start)
            .map(|state| state.text.as_str())
            .collect::<Vec<_>>();
        let mut item = segment.clone();
        item.visual_observations = vec![if on_screen.is_empty() {
            "no legible text on screen while this was said".to_owned()
        } else {
            format!("on screen while this was said: {}", on_screen.join(" | "))
        }];
        items.push(item);
    }

    let spoken_over = |state: &super::visual::ScreenState| {
        speech.iter().any(|segment| {
            let start = segment.location.time_start_ms.unwrap_or(0);
            let end = segment.location.time_end_ms.unwrap_or(0);
            start < state.end_ms && end > state.start_ms
        })
    };

    for (index, state) in states.iter().enumerate() {
        if spoken_over(state) {
            continue;
        }
        items.push(ExtractedEvidenceItem {
            evidence_id: format!("screen-{index:05}"),
            location: EvidenceLocation {
                page_start: None,
                page_end: None,
                time_start_ms: Some(state.start_ms),
                time_end_ms: Some(state.end_ms),
                region: None,
            },
            speaker: None,
            // On-screen text IS text extracted from the source, which is what the
            // field means; it is not a claim that anyone said it.
            extracted_text: Some(state.text.clone()),
            visual_observations: vec![format!(
                "text held on screen for {:.1}s with nothing spoken over it",
                (state.end_ms.saturating_sub(state.start_ms)) as f64 / 1000.0
            )],
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            // Reading a screen records what was displayed, not whether it is true
            // or characteristic of anyone.
            assertion: EvidenceAssertion::Inferred,
            // OCR reports no per-character confidence here, so nothing is claimed
            // beyond the midpoint; review judges the text.
            confidence: 0.5,
        });
    }

    items.sort_by_key(|item| item.location.time_start_ms.unwrap_or(0));
    items
}

pub(crate) struct MediaTranscription {
    pub evidence: Vec<DistillationEvidence>,
    /// The artefact the evidence came from. A bundle needs its sources, and a
    /// caller that had to rebuild this would be inventing a digest for a file the
    /// reader already hashed.
    pub source: SourceArtifact,
    /// How many distinct screen states were read. Zero for audio.
    pub screen_states_read: usize,
    /// Said plainly when something about the input limits what was observed, so a
    /// person is never left assuming the video was understood in full.
    pub note: Option<String>,
}

/// Does this file carry a video stream? `None` when it could not be determined.
fn has_video_stream(tools: &TranscriptionTools, media: &Path) -> Option<bool> {
    let ffprobe = tools.ffmpeg.parent()?.join("ffprobe");
    let ffprobe = if ffprobe.is_file() {
        ffprobe
    } else {
        which("ffprobe")?
    };
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(media)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).contains("video"))
}

fn segments_to_items(output: &WhisperOutput) -> Result<Vec<ExtractedEvidenceItem>, String> {
    if output.transcription.len() > MAX_SEGMENTS {
        return Err("The transcript exceeds the bounded segment limit.".to_owned());
    }

    let mut items = Vec::new();
    for (index, segment) in output.transcription.iter().enumerate() {
        let text = segment.text.trim();
        if text.is_empty() || text.len() > MAX_SEGMENT_TEXT_BYTES {
            continue;
        }
        if segment.offsets.from < 0 || segment.offsets.to < segment.offsets.from {
            return Err("The transcript contains an invalid time range.".to_owned());
        }

        items.push(ExtractedEvidenceItem {
            evidence_id: format!("transcript-{index:05}"),
            location: EvidenceLocation {
                page_start: None,
                page_end: None,
                time_start_ms: Some(segment.offsets.from as u64),
                time_end_ms: Some(segment.offsets.to as u64),
                region: None,
            },
            // No diarization is performed, so no speaker is claimed. Guessing who
            // spoke would be inventing an attribution.
            speaker: None,
            extracted_text: Some(text.to_owned()),
            visual_observations: Vec::new(),
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            // A transcript records what was said. Whether it is true, or
            // characteristic of the person, is not something transcription can
            // judge — so every segment is Inferred, never Confirmed.
            assertion: EvidenceAssertion::Inferred,
            confidence: segment_confidence(segment),
        });
    }

    if items.is_empty() {
        return Err("The transcript contained no usable speech.".to_owned());
    }
    Ok(items)
}

/// The model's own mean token probability for this segment.
fn segment_confidence(segment: &WhisperSegment) -> f32 {
    if segment.tokens.is_empty() {
        // Without `-ojf` there are no token probabilities. Rather than invent one,
        // sit at the midpoint and let review judge the text.
        return 0.5;
    }
    let total: f32 = segment.tokens.iter().map(|token| token.p).sum();
    (total / segment.tokens.len() as f32).clamp(0.0, 1.0)
}

fn demux_to_wav(tools: &TranscriptionTools, media: &Path, wav: &Path) -> Result<(), String> {
    // whisper.cpp requires 16 kHz mono signed 16-bit PCM.
    let output = Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-y", "-i"])
        .arg(media)
        .args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "-vn"])
        .arg(wav)
        .output()
        .map_err(|_| "ffmpeg could not be executed.".to_owned())?;
    if !output.status.success() || !wav.is_file() {
        return Err("ffmpeg could not decode the media file into audio.".to_owned());
    }
    Ok(())
}

fn run_whisper(tools: &TranscriptionTools, wav: &Path, prefix: &Path) -> Result<(), String> {
    let output = Command::new(&tools.whisper)
        .arg("-m")
        .arg(&tools.model)
        .arg("-f")
        .arg(wav)
        // -ojf: JSON including per-token probabilities, which is what makes the
        // confidence a measurement rather than a guess.
        .arg("-ojf")
        .arg("-of")
        .arg(prefix)
        .output()
        .map_err(|_| "whisper-cli could not be executed.".to_owned())?;
    if !output.status.success() {
        return Err("whisper-cli failed to transcribe the audio.".to_owned());
    }
    Ok(())
}

fn resolve_model() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("AI_OS_WHISPER_MODEL").map(PathBuf::from) {
        if configured.is_file() {
            return Some(configured);
        }
    }
    let directory = super::toolchain::asset_root()?.join("whisper");
    let mut models = fs::read_dir(&directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .collect::<Vec<_>>();
    models.sort();
    models.pop()
}

fn which(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(command))
        .find(|candidate| candidate.is_file())
}

/// Check the file is one AI-OS may read, and digest it.
///
/// The ORIGINAL file is always the source of record. Digesting a derived working
/// file instead would record provenance for something the person never gave us.
fn validated_digest(media: &Path) -> Result<String, String> {
    let metadata =
        fs::symlink_metadata(media).map_err(|_| "The media file could not be read.".to_owned())?;
    if metadata.file_type().is_symlink() {
        return Err("The media file is a symbolic link and was not read.".to_owned());
    }
    if metadata.len() == 0 {
        return Err("The media file is empty.".to_owned());
    }
    if metadata.len() > MAX_MEDIA_BYTES {
        return Err("The media file exceeds the bounded limit.".to_owned());
    }
    file_digest(media)
}

fn file_digest(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|_| "The media file could not be read.".to_owned())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A genuine `whisper-cli -ojf` output, captured from a real run of
    /// whisper.cpp 1.9.4. Its transcription is empty because the run used the
    /// repository's stub test model, but the SHAPE is the real writer's, which is
    /// what the deserializer has to match.
    const REAL_EMPTY: &str = r#"{"systeminfo": "WHISPER : COREML = 1 | METAL = 1 |", "model": {"type": "tiny", "multilingual": false, "vocab": 51864, "audio": {"ctx": 1500, "state": 384, "head": 6, "layer": 4}, "text": {"ctx": 448, "state": 384, "head": 6, "layer": 4}, "mels": 80, "ftype": 1}, "params": {"model": "models/for-tests-ggml-tiny.en.bin", "language": "en", "translate": false}, "result": {"language": "en"}, "transcription": []}"#;

    /// Segment shape exactly as `examples/cli/cli.cpp` writes it: `timestamps`
    /// as strings for humans, `offsets` as integers for machines, and per-token
    /// `p` present because `-ojf` was used.
    fn with_segments() -> String {
        r#"{
          "systeminfo": "WHISPER : COREML = 1 |",
          "model": {"type": "large-v3-turbo"},
          "params": {"model": "/Users/x/.cache/ai-os/whisper/ggml-large-v3-turbo-q5_0.bin", "language": "en", "translate": false},
          "result": {"language": "en"},
          "transcription": [
            {
              "timestamps": {"from": "00:00:00,000", "to": "00:00:11,000"},
              "offsets": {"from": 0, "to": 11000},
              "text": " And so my fellow Americans, ask not what your country can do for you.",
              "tokens": [{"text":"And","p":0.9},{"text":"so","p":0.7}]
            },
            {
              "timestamps": {"from": "00:00:11,000", "to": "00:00:14,500"},
              "offsets": {"from": 11000, "to": 14500},
              "text": " Ask what you can do for your country.",
              "tokens": [{"text":"Ask","p":0.6},{"text":"what","p":0.4}]
            }
          ]
        }"#
        .to_owned()
    }

    #[test]
    fn the_deserializer_matches_output_a_real_whisper_cli_actually_produced() {
        let output: WhisperOutput = serde_json::from_str(REAL_EMPTY).unwrap();
        assert_eq!(output.model.kind, "tiny");
        assert!(output.transcription.is_empty());

        // An empty transcript is refused rather than yielding zero evidence
        // silently, which would look like a successful run that found nothing.
        let error = segments_to_items(&output).unwrap_err();
        assert!(error.contains("no usable speech"), "got: {error}");
    }

    /// whisper's internal t0/t1 are centiseconds and the writer emits `t0 * 10`,
    /// so `offsets` are already milliseconds. Getting this wrong by a factor of
    /// ten would put every quotation in the wrong place in the recording.
    #[test]
    fn offsets_are_carried_as_milliseconds_without_rescaling() {
        let output: WhisperOutput = serde_json::from_str(&with_segments()).unwrap();
        let items = segments_to_items(&output).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].location.time_start_ms, Some(0));
        assert_eq!(items[0].location.time_end_ms, Some(11_000));
        assert_eq!(items[1].location.time_start_ms, Some(11_000));
        assert_eq!(items[1].location.time_end_ms, Some(14_500));
    }

    /// Confidence is the model's own mean token probability, not a constant.
    #[test]
    fn confidence_is_measured_from_the_models_own_token_probabilities() {
        let output: WhisperOutput = serde_json::from_str(&with_segments()).unwrap();
        let items = segments_to_items(&output).unwrap();

        assert!(
            (items[0].confidence - 0.8).abs() < 1e-5,
            "{}",
            items[0].confidence
        );
        assert!(
            (items[1].confidence - 0.5).abs() < 1e-5,
            "{}",
            items[1].confidence
        );
        // Different segments get different confidences, which a constant could not.
        assert_ne!(items[0].confidence, items[1].confidence);
    }

    /// A transcript records what was said, not whether it is true or
    /// characteristic. Marking any of it Confirmed would overstate what
    /// transcription can know.
    #[test]
    fn every_transcribed_segment_is_inferred_and_claims_no_speaker() {
        let output: WhisperOutput = serde_json::from_str(&with_segments()).unwrap();
        for item in segments_to_items(&output).unwrap() {
            assert_eq!(item.assertion, EvidenceAssertion::Inferred);
            assert!(item.speaker.is_none(), "no diarization was performed");
        }
    }

    /// Audio evidence must carry transcript AND timestamps. This proves the items
    /// satisfy that before they ever reach the evidence layer.
    #[test]
    fn items_satisfy_what_the_audio_modality_requires() {
        let output: WhisperOutput = serde_json::from_str(&with_segments()).unwrap();
        for item in segments_to_items(&output).unwrap() {
            assert!(item
                .extracted_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty()));
            assert!(item.location.time_start_ms.is_some());
            assert!(item.location.time_end_ms.is_some());
        }
    }

    #[test]
    fn a_reversed_or_negative_time_range_is_refused() {
        for offsets in [
            "{\"from\": 900, \"to\": 100}",
            "{\"from\": -5, \"to\": 100}",
        ] {
            let json = format!(
                r#"{{"model":{{"type":"t"}},"params":{{"model":"m"}},"transcription":[{{"offsets":{offsets},"text":"x","tokens":[]}}]}}"#
            );
            let output: WhisperOutput = serde_json::from_str(&json).unwrap();
            assert!(segments_to_items(&output)
                .unwrap_err()
                .contains("time range"));
        }
    }

    use crate::cognitive_distillation::visual::ScreenState;

    fn state(start_ms: u64, end_ms: u64, text: &str) -> ScreenState {
        ScreenState {
            start_ms,
            end_ms,
            text: text.to_owned(),
        }
    }

    /// The case that matters most: a tutorial with no speech at all. Transcription
    /// alone yields nothing from it, so the screen has to carry the evidence.
    #[test]
    fn a_silent_tutorial_still_produces_video_evidence() {
        let states = vec![
            state(0, 4_000, "Step 1: Open Settings"),
            state(4_000, 6_000, "Step 2: Click Generate Token"),
        ];
        let items = video_items(&[], &states);

        assert_eq!(items.len(), 2);
        for item in &items {
            // Exactly what SourceMediaKind::Video requires: time, text, and a
            // visual observation.
            assert!(item.location.time_start_ms.is_some());
            assert!(item.location.time_end_ms.is_some());
            assert!(item
                .extracted_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty()));
            assert!(!item.visual_observations.is_empty());
        }
        assert_eq!(
            items[0].extracted_text.as_deref(),
            Some("Step 1: Open Settings")
        );
        assert!(items[0].visual_observations[0].contains("4.0s"));
    }

    /// When someone IS speaking, the item records what was on screen while they
    /// said it — which is the thing a transcript alone throws away.
    #[test]
    fn spoken_segments_record_what_was_on_screen_while_they_were_said() {
        let mut spoken = item_at(1_000, 5_000, " Click the green button.");
        spoken.visual_observations.clear();
        let states = vec![
            state(0, 4_000, "Generate Token"),
            state(4_000, 8_000, "Settings saved"),
        ];

        let items = video_items(&[spoken], &states);
        let said = &items[0];
        assert_eq!(
            said.extracted_text.as_deref(),
            Some(" Click the green button.")
        );
        // Both overlapping screens are recorded, in order.
        assert!(said.visual_observations[0].contains("Generate Token"));
        assert!(said.visual_observations[0].contains("Settings saved"));
    }

    /// A screen already covered by speech must not also appear as a second,
    /// standalone item, or the same moment would be counted twice.
    #[test]
    fn a_screen_spoken_over_is_not_also_emitted_on_its_own() {
        let spoken = item_at(0, 4_000, " talking over the first screen");
        let states = vec![
            state(0, 4_000, "covered by speech"),
            state(4_000, 6_000, "silent afterwards"),
        ];
        let items = video_items(&[spoken], &states);

        assert_eq!(items.len(), 2);
        assert!(items
            .iter()
            .all(|item| item.extracted_text.as_deref() != Some("covered by speech")));
        assert!(items
            .iter()
            .any(|item| item.extracted_text.as_deref() == Some("silent afterwards")));
    }

    use crate::cognitive_distillation::visual::FrameDescription;

    /// The soundless demonstration: no speech, nothing written. Descriptions of
    /// the picture are all there is, and they must be admissible.
    #[test]
    fn described_frames_become_admissible_video_evidence() {
        let described = vec![
            FrameDescription {
                start_ms: 0,
                end_ms: 4_000,
                text: "hands fold dough toward the centre on a wooden board".to_owned(),
            },
            FrameDescription {
                start_ms: 4_000,
                end_ms: 8_000,
                text: "the dough is pressed flat with the heel of one hand".to_owned(),
            },
        ];
        let items = described_items(&described);

        assert_eq!(items.len(), 2);
        for item in &items {
            // Video needs time and visuals; with the contract corrected it does
            // NOT also need text, which is what makes this case representable.
            assert!(item.location.time_start_ms.is_some());
            assert!(item.location.time_end_ms.is_some());
            assert!(!item.visual_observations.is_empty());

            // A generated description must never be quotable as something written
            // or said, so it never lands in extracted_text.
            assert!(
                item.extracted_text.is_none(),
                "a model's description is not text from the source"
            );
            assert_eq!(item.assertion, EvidenceAssertion::Inferred);
        }
    }

    /// A description is the weakest thing in a bundle and must be ranked below a
    /// transcript, so review can see which claims rest on a guess about a picture.
    #[test]
    fn a_description_is_less_confident_than_a_transcript_or_a_screen_read() {
        let described = described_items(&[FrameDescription {
            start_ms: 0,
            end_ms: 2_000,
            text: "a person gestures at a whiteboard".to_owned(),
        }]);
        let screen = video_items(&[], &[state(0, 2_000, "Step 1")]);
        let spoken = item_at(0, 2_000, " said aloud");

        assert!(described[0].confidence < screen[0].confidence);
        assert!(described[0].confidence < spoken.confidence);
    }

    #[test]
    fn video_items_are_ordered_by_time() {
        let spoken = item_at(9_000, 11_000, " said last");
        let states = vec![state(0, 2_000, "shown first")];
        let items = video_items(&[spoken], &states);
        assert_eq!(items[0].extracted_text.as_deref(), Some("shown first"));
        assert_eq!(items[1].extracted_text.as_deref(), Some(" said last"));
    }

    fn item_at(start_ms: u64, end_ms: u64, text: &str) -> ExtractedEvidenceItem {
        ExtractedEvidenceItem {
            evidence_id: format!("speech-{start_ms}"),
            location: EvidenceLocation {
                page_start: None,
                page_end: None,
                time_start_ms: Some(start_ms),
                time_end_ms: Some(end_ms),
                region: None,
            },
            speaker: None,
            extracted_text: Some(text.to_owned()),
            visual_observations: Vec::new(),
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            assertion: EvidenceAssertion::Inferred,
            confidence: 0.7,
        }
    }

    /// Two very different situations must not be reported the same way.
    ///
    /// A bundled tool missing means the INSTALLATION is damaged — naming
    /// `whisper-cli` to the person who just dragged in a voice memo helps nobody.
    /// A model missing is a normal first-use state with a size and an action.
    #[test]
    fn a_damaged_installation_reads_differently_from_a_model_not_yet_fetched() {
        match TranscriptionTools::resolve() {
            Ok(_) => {}
            Err(error) => {
                let damaged = error.contains("missing from this installation");
                let not_set_up = error.contains("one-time") || error.contains("not set up");
                assert!(
                    damaged || not_set_up,
                    "a failure must say which of the two situations this is: {error}"
                );
                // Neither wording may hand a person a command to run.
                assert!(!error.contains("whisper-cli"), "{error}");
                assert!(!error.contains("tesseract"), "{error}");
            }
        }
    }
}

#[cfg(test)]
mod real_smoke {
    use super::*;

    /// The real media path, end to end, on a file the OWNER supplies.
    ///
    /// Takes whatever is handed over — image, audio or video — and routes it by
    /// inspecting the file's streams rather than its name.
    ///
    /// The file is not chosen by AI-OS and not bundled: a recording is somebody's
    /// voice, and which recording gets transcribed is the owner's call. It is also
    /// never copied anywhere — it is read in place, and only a derived 16 kHz wav
    /// is written into a temporary directory that is dropped at the end.
    ///
    /// Nothing leaves the machine.
    #[test]
    #[ignore = "requires whisper.cpp, ffmpeg, a model, and an owner-supplied media file"]
    fn real_local_transcription_produces_timestamped_evidence() {
        assert_eq!(
            std::env::var("AI_OS_RUN_TRANSCRIPTION_REAL_SMOKE").as_deref(),
            Ok("1")
        );
        let media = std::env::var("AI_OS_TRANSCRIPTION_MEDIA")
            .expect("set AI_OS_TRANSCRIPTION_MEDIA to the audio or video file to read");
        let media = Path::new(&media);

        let tools = TranscriptionTools::resolve().expect("local transcription toolchain");
        println!(
            "USING ffmpeg={} whisper={} model={}",
            tools.ffmpeg.display(),
            tools.whisper.display(),
            tools.model.display()
        );

        let work = tempfile::tempdir().unwrap();
        let result = evidence_from_any_media(
            media,
            SubjectKind::PrivatePerson,
            "owner-media-smoke",
            work.path(),
            &tools,
        )
        .unwrap();
        let evidence = result.evidence;

        assert!(!evidence.is_empty(), "no evidence was produced");
        println!("SEGMENTS={}", evidence.len());
        println!("SCREEN_STATES={}", result.screen_states_read);
        if let Some(note) = &result.note {
            println!("NOTE={note}");
        }

        for item in &evidence {
            // Every segment must be placeable in the recording, or a reviewer
            // cannot check a quotation against the source.
            let start = item.location.time_start_ms.expect("start timestamp");
            let end = item.location.time_end_ms.expect("end timestamp");
            assert!(end >= start);
            assert!(item
                .extracted_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty()));
            assert_eq!(item.extractor_identity, TRANSCRIPTION_EXTRACTOR);
            assert!(item.assertion == EvidenceAssertion::Inferred);
            assert!((0.0..=1.0).contains(&item.confidence));
        }

        // The recording is private, so nothing here may be treated as public.
        println!(
            "FIRST_SEGMENT [{}ms-{}ms] p={:.3} {:?}",
            evidence[0].location.time_start_ms.unwrap(),
            evidence[0].location.time_end_ms.unwrap(),
            evidence[0].confidence,
            evidence[0].extracted_text.as_deref().unwrap_or_default()
        );
        println!("EXTRACTOR_REVISION={}", evidence[0].extractor_revision);

        // The derived wav is working state, not a second copy of the person's
        // recording left lying around.
        drop(work);
    }
}

#[cfg(test)]
mod shape_detection {
    use super::*;

    fn tools(ffmpeg: PathBuf) -> TranscriptionTools {
        TranscriptionTools {
            ffmpeg,
            whisper: PathBuf::from("/unused"),
            model: PathBuf::from("/unused"),
            tesseract: None,
            ocr_languages: "eng".to_owned(),
            vision: None,
        }
    }

    fn ffmpeg() -> Option<PathBuf> {
        which("ffmpeg")
    }

    /// What decides which path a file takes must be tested against real files of
    /// each kind, not against filenames. A misrouted file silently produces the
    /// wrong evidence, or none.
    #[test]
    #[ignore = "requires ffmpeg and ffprobe"]
    fn real_files_of_each_kind_are_routed_correctly() {
        let Some(ffmpeg) = ffmpeg() else { return };
        let tools = tools(ffmpeg.clone());
        let dir = tempfile::tempdir().unwrap();

        let png = dir.path().join("still.png");
        let wav = dir.path().join("sound.wav");
        let mp4 = dir.path().join("clip.mp4");
        let silent_mp4 = dir.path().join("silent.mp4");

        let run = |args: &[&str]| {
            std::process::Command::new(&ffmpeg)
                .args(["-nostdin", "-y"])
                .args(args)
                .output()
                .unwrap();
        };
        run(&[
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:d=1",
            "-frames:v",
            "1",
            png.to_str().unwrap(),
        ]);
        run(&[
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=16000:cl=mono",
            "-t",
            "2",
            "-c:a",
            "pcm_s16le",
            wav.to_str().unwrap(),
        ]);
        run(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=s=64x64:d=2",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=16000:cl=mono",
            "-t",
            "2",
            "-shortest",
            mp4.to_str().unwrap(),
        ]);
        run(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=s=64x64:d=2",
            "-t",
            "2",
            silent_mp4.to_str().unwrap(),
        ]);

        assert_eq!(
            detect_shape(&tools, &png),
            MediaShape::Image,
            "a still image"
        );
        assert_eq!(detect_shape(&tools, &wav), MediaShape::Audio, "sound only");
        assert_eq!(
            detect_shape(&tools, &mp4),
            MediaShape::Video,
            "picture with sound"
        );
        assert_eq!(
            detect_shape(&tools, &silent_mp4),
            MediaShape::Video,
            "a silent CLIP is still video, not a still image"
        );

        // An image whose extension lies is still an image, because the streams
        // are what is inspected rather than the name.
        let disguised = dir.path().join("actually-a-picture.mp4");
        std::fs::copy(&png, &disguised).unwrap();
        assert_eq!(detect_shape(&tools, &disguised), MediaShape::Image);
    }

    /// With no ffprobe available the extension decides, and an unknown extension
    /// must not be guessed into the image path — an image that is really a video
    /// would lose its soundtrack silently.
    #[test]
    fn without_ffprobe_the_extension_decides_and_defaults_to_video() {
        let tools = tools(PathBuf::from("/definitely/not/ffmpeg"));
        assert_eq!(
            detect_shape(&tools, Path::new("/x/photo.HEIC")),
            MediaShape::Image
        );
        assert_eq!(
            detect_shape(&tools, Path::new("/x/photo.png")),
            MediaShape::Image
        );
        assert_eq!(
            detect_shape(&tools, Path::new("/x/thing.mkv")),
            MediaShape::Video
        );
        assert_eq!(
            detect_shape(&tools, Path::new("/x/no-extension")),
            MediaShape::Video
        );
    }
}

#[cfg(test)]
mod image_reading {
    use super::*;

    /// The real image path, with real OCR. Proves a screenshot becomes evidence
    /// without any model, since the Image modality accepts text alone.
    #[test]
    #[ignore = "requires ffmpeg and tesseract, plus an image at AI_OS_IMAGE_TEST_FILE"]
    fn a_real_image_with_text_becomes_evidence_without_a_model() {
        let image = std::env::var("AI_OS_IMAGE_TEST_FILE")
            .expect("set AI_OS_IMAGE_TEST_FILE to an image containing text");
        let tesseract = super::super::visual::find_tesseract().expect("tesseract");
        let languages = super::super::visual::ocr_languages(&tesseract);
        let tools = TranscriptionTools {
            ffmpeg: which("ffmpeg").expect("ffmpeg"),
            // Images never reach whisper, so these are deliberately unusable: if
            // the image path ever started transcribing, this test would fail.
            whisper: PathBuf::from("/must/not/be/used"),
            model: PathBuf::from("/must/not/be/used"),
            tesseract: Some(tesseract),
            ocr_languages: languages,
            vision: None,
        };

        let work = tempfile::tempdir().unwrap();
        let result = evidence_from_any_media(
            Path::new(&image),
            SubjectKind::PrivatePerson,
            "image-smoke",
            work.path(),
            &tools,
        )
        .unwrap();

        assert_eq!(result.evidence.len(), 1);
        let item = &result.evidence[0];
        println!("MEDIA_KIND={:?}", item.media_kind);
        println!("EXTRACTOR={}", item.extractor_identity);
        println!(
            "TEXT={:?}",
            item.extracted_text.as_deref().unwrap_or_default()
        );

        assert_eq!(item.media_kind, SourceMediaKind::Image);
        assert_eq!(item.extractor_identity, "tesseract");
        assert!(item
            .extracted_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()));
        // A still image has no time range to claim.
        assert!(item.location.time_start_ms.is_none());
        assert!(item.location.page_start.is_none());
    }
}

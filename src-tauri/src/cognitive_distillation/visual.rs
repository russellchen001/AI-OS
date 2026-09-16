//! Reading what a video SHOWS, so a video can be distilled even in silence.
//!
//! A tutorial screen recording often has no speech at all. Transcription alone
//! would return nothing from it, which is why the evidence contract insists a
//! video carry visual observations and not only a soundtrack.
//!
//! What this module can honestly observe is **text on screen**: menus, buttons,
//! code, captions, slide titles. A screen recording is crisp, high-contrast and
//! set in ordinary UI fonts, which is the case OCR handles well. Frames are
//! sampled at a fixed interval and consecutive frames showing the same text are
//! merged into one screen state with a time range, so "Step 2" that stays up for
//! six seconds is one observation rather than three.
//!
//! ## When there is nothing to read
//!
//! OCR reads text; it cannot describe imagery. A soundless video of someone
//! kneading dough has no speech and nothing written on screen, and would yield
//! nothing at all.
//!
//! A local vision model (llama.cpp + InternVL3-2B) describes sampled frames when
//! visual understanding is required. Silent textless video depends on it entirely;
//! talking video also uses it alongside speech and OCR because readable text does
//! not describe the people, objects, or actions visible in the picture. Its output
//! is a visual observation and never `extracted_text`: generated description is
//! not source text.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Seconds between sampled frames. Two seconds is short enough to catch a step
/// in a tutorial and long enough that a ten-minute video stays a few hundred
/// frames rather than tens of thousands.
const SAMPLE_INTERVAL_SECONDS: u64 = 2;
const MAX_FRAMES: usize = 900;
const MAX_STATE_TEXT_BYTES: usize = 4 * 1024;

/// A stretch of video during which the screen showed the same text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScreenState {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisualReading {
    pub states: Vec<ScreenState>,
    /// True when frames were read but none of them carried legible text. The
    /// caller must not present this as "the video showed nothing".
    pub no_legible_text: bool,
}

/// Sample a video's frames and read the text on each.
/// Extract the same bounded frame sample used by screen-text reading without
/// requiring OCR. Purely visual video uses this before local vision description.
pub(crate) fn prepare_video_frames(
    video: &Path,
    work_dir: &Path,
    ffmpeg: &Path,
) -> Result<(), String> {
    let frames_dir = work_dir.join("frames");
    fs::create_dir_all(&frames_dir)
        .map_err(|_| "Could not prepare the frame working directory.".to_owned())?;

    extract_frames(ffmpeg, video, &frames_dir)
}

pub(crate) fn read_screen_text(
    video: &Path,
    work_dir: &Path,
    ffmpeg: &Path,
    tesseract: &Path,
    languages: &str,
) -> Result<VisualReading, String> {
    let frames_dir = work_dir.join("frames");
    fs::create_dir_all(&frames_dir)
        .map_err(|_| "Could not prepare the frame working directory.".to_owned())?;

    extract_frames(ffmpeg, video, &frames_dir)?;

    let mut frames = fs::read_dir(&frames_dir)
        .map_err(|_| "Could not read the extracted frames.".to_owned())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect::<Vec<_>>();
    frames.sort();

    if frames.is_empty() {
        return Err("No frames could be extracted from the video.".to_owned());
    }
    if frames.len() > MAX_FRAMES {
        frames.truncate(MAX_FRAMES);
    }

    // ffmpeg's `fps` filter emits frame N at N * interval, which the probe
    // confirmed against showinfo's pts_time.
    let mut readings = BTreeMap::new();
    for (index, frame) in frames.iter().enumerate() {
        let at_ms = index as u64 * SAMPLE_INTERVAL_SECONDS * 1_000;
        readings.insert(at_ms, read_frame(tesseract, frame, languages));
    }

    Ok(merge_states(readings))
}

/// Collapse consecutive frames showing the same text into one timed state.
fn merge_states(readings: BTreeMap<u64, String>) -> VisualReading {
    let step_ms = SAMPLE_INTERVAL_SECONDS * 1_000;
    let mut states: Vec<ScreenState> = Vec::new();

    for (at_ms, text) in readings {
        let text = normalise(&text);
        if text.is_empty() {
            continue;
        }
        match states.last_mut() {
            // The same text still on screen: extend, do not repeat.
            Some(last) if last.text == text && last.end_ms == at_ms => {
                last.end_ms = at_ms + step_ms;
            }
            _ => states.push(ScreenState {
                start_ms: at_ms,
                end_ms: at_ms + step_ms,
                text,
            }),
        }
    }

    VisualReading {
        no_legible_text: states.is_empty(),
        states,
    }
}

/// OCR output is noisy at the edges: collapse whitespace so that two frames of
/// the same screen compare equal instead of differing by a stray space.
fn normalise(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.len() > MAX_STATE_TEXT_BYTES {
        return joined.chars().take(MAX_STATE_TEXT_BYTES / 4).collect();
    }
    joined
}

fn extract_frames(ffmpeg: &Path, video: &Path, frames_dir: &Path) -> Result<(), String> {
    let output = Command::new(ffmpeg)
        .args(["-nostdin", "-y", "-i"])
        .arg(video)
        .args([
            "-vf",
            &format!("fps=1/{SAMPLE_INTERVAL_SECONDS}"),
            "-vsync",
            "vfr",
        ])
        .arg(frames_dir.join("f_%05d.png"))
        .output()
        .map_err(|_| "ffmpeg could not be executed to extract frames.".to_owned())?;
    if !output.status.success() {
        return Err("ffmpeg could not extract frames from the video.".to_owned());
    }
    Ok(())
}

/// Read one frame. A frame that cannot be read is empty text, not an error: a
/// blank or purely pictorial frame is a normal thing for a video to contain.
fn read_frame(tesseract: &Path, frame: &Path, languages: &str) -> String {
    Command::new(tesseract)
        .arg(frame)
        .arg("stdout")
        .args(["-l", languages])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

/// A described stretch of video, produced only when nothing could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameDescription {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub(crate) struct VisionModel {
    pub cli: PathBuf,
    pub model: PathBuf,
    pub projector: PathBuf,
}

/// A vision model will speculate if invited to. It is asked for what is visible
/// and nothing else, because a guess about intent or identity would enter the
/// evidence record as though it had been observed.
const DESCRIBE_PROMPT: &str =
    "Describe only what is visibly happening in this frame, in one short sentence. \
Describe actions and objects you can actually see. \
Do not guess intent, identity, emotion, age, health, or anything not visible. \
If the frame is blank or unclear, say exactly: nothing discernible.";

/// Frames a vision model is asked to look at. Each costs seconds, so a long video
/// is sampled across rather than described exhaustively.
const MAX_DESCRIBED_FRAMES: usize = 16;

/// Describe frames already extracted by `read_screen_text`, reusing them rather
/// than decoding the video a second time.
pub(crate) fn describe_frames(
    work_dir: &Path,
    vision: &VisionModel,
) -> Result<Vec<FrameDescription>, String> {
    let frames_dir = work_dir.join("frames");
    let mut frames = fs::read_dir(&frames_dir)
        .map_err(|_| "No extracted frames were available to describe.".to_owned())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect::<Vec<_>>();
    frames.sort();
    if frames.is_empty() {
        return Err("No extracted frames were available to describe.".to_owned());
    }

    let step_ms = SAMPLE_INTERVAL_SECONDS * 1_000;
    let total = frames.len();
    let stride = total.div_ceil(MAX_DESCRIBED_FRAMES).max(1);

    let mut described = Vec::new();
    for (index, frame) in frames.iter().enumerate().step_by(stride) {
        let Some(text) = describe_frame(vision, frame) else {
            continue;
        };
        let start_ms = index as u64 * step_ms;
        // The description stands for the stretch up to the next frame looked at,
        // not for a single instant, because that is the span it was sampled from.
        let end_ms = start_ms + (stride as u64 * step_ms).min(u64::MAX);
        described.push(FrameDescription {
            start_ms,
            end_ms: end_ms.min(total as u64 * step_ms),
            text,
        });
    }

    if described.is_empty() {
        return Err("The vision model described none of the sampled frames.".to_owned());
    }
    Ok(described)
}

fn describe_frame(vision: &VisionModel, frame: &Path) -> Option<String> {
    let output = Command::new(&vision.cli)
        .arg("-m")
        .arg(&vision.model)
        .arg("--mmproj")
        .arg(&vision.projector)
        .arg("--image")
        .arg(frame)
        .args(["-p", DESCRIBE_PROMPT])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = normalise(&String::from_utf8_lossy(&output.stdout));
    // The model was told to say this when it cannot see anything; taking it at its
    // word beats recording "nothing discernible" as an observation.
    if text.is_empty() || text.to_lowercase().contains("nothing discernible") {
        return None;
    }
    Some(text)
}

/// Prepare a still image for local OCR / vision without changing the source.
///
/// iPhone photos commonly arrive as HEIC/HEIF, while the local VLM and OCR
/// engines are most reliable with ordinary raster input. The converted PNG is
/// temporary working state only; provenance and hashing continue to refer to the
/// original owner-supplied file.
pub(crate) fn prepare_still_image(image: &Path, work_dir: &Path) -> Result<PathBuf, String> {
    let staged = work_dir.join("image-normalized.png");

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("/usr/bin/sips")
            .args(["-s", "format", "png"])
            .arg(image)
            .arg("--out")
            .arg(&staged)
            .output()
            .map_err(|_| "The image could not be prepared for local reading.".to_owned())?;

        if !output.status.success() || !staged.is_file() {
            return Err(
                "The image could not be converted into a readable local format.".to_owned(),
            );
        }

        let metadata = fs::metadata(&staged)
            .map_err(|_| "The prepared image could not be read.".to_owned())?;

        if metadata.len() == 0 {
            return Err("The prepared image was empty.".to_owned());
        }

        return Ok(staged);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let extension = image
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("img");

        let staged = work_dir.join(format!("image-normalized.{extension}"));

        fs::copy(image, &staged)
            .map_err(|_| "The image could not be prepared for local reading.".to_owned())?;

        Ok(staged)
    }
}

/// Read the text in a single still image. Empty when there is none legible,
/// which is a normal thing for a photograph to be.
pub(crate) fn read_image_text(image: &Path, tesseract: &Path, languages: &str) -> String {
    normalise(&read_frame(tesseract, image, languages))
}

/// Describe a single still image with the local vision model.
pub(crate) fn describe_image(
    image: &Path,
    _work_dir: &Path,
    vision: &VisionModel,
) -> Result<String, String> {
    describe_frame(vision, image)
        .ok_or_else(|| "The vision model produced no description of this image.".to_owned())
}

pub(crate) fn find_vision_model() -> Option<VisionModel> {
    let cli = which_in_path("llama-mtmd-cli")?;
    let directory = super::toolchain::asset_root()?.join("vlm");
    let mut model = None;
    let mut projector = None;
    for entry in fs::read_dir(&directory).ok()? {
        let path = entry.ok()?.path();
        if path.extension().is_none_or(|extension| extension != "gguf") {
            continue;
        }
        let name = path.file_name()?.to_string_lossy().into_owned();
        if name.starts_with("mmproj") {
            projector = Some(path);
        } else {
            model = Some(path);
        }
    }
    Some(VisionModel {
        cli,
        model: model?,
        projector: projector?,
    })
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn find_tesseract() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("tesseract"))
        .find(|candidate| candidate.is_file())
}

/// Languages to hand OCR. Chinese and English together, when the Chinese data is
/// installed; English alone otherwise, because naming a missing language makes
/// tesseract fail outright rather than degrade.
pub(crate) fn ocr_languages(tesseract: &Path) -> String {
    let installed = Command::new(tesseract)
        .arg("--list-langs")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    if installed.contains("chi_sim") {
        "chi_sim+eng".to_owned()
    } else {
        "eng".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(pairs: &[(u64, &str)]) -> VisualReading {
        merge_states(
            pairs
                .iter()
                .map(|(at, text)| (*at, (*text).to_owned()))
                .collect(),
        )
    }

    /// The whole point of merging: a step that stays on screen is ONE observation
    /// with a time range, not one per sampled frame.
    #[test]
    fn the_same_screen_across_frames_becomes_one_timed_state() {
        let read = reading(&[
            (0, "Step 1: Open Settings"),
            (2_000, "Step 1: Open Settings"),
            (4_000, "Step 2: Click Generate Token"),
            (6_000, "Step 3: Paste into the config file"),
            (8_000, "Step 3: Paste into the config file"),
            (10_000, "Step 4: Restart and verify"),
        ]);

        assert_eq!(read.states.len(), 4);
        assert_eq!(read.states[0].start_ms, 0);
        assert_eq!(read.states[0].end_ms, 4_000);
        assert_eq!(read.states[0].text, "Step 1: Open Settings");
        assert_eq!(read.states[2].start_ms, 6_000);
        assert_eq!(read.states[2].end_ms, 10_000);
        assert_eq!(read.states[3].end_ms, 12_000);
        assert!(!read.no_legible_text);
    }

    /// OCR whitespace jitter must not split one screen into two states.
    #[test]
    fn whitespace_noise_does_not_split_a_screen_in_two() {
        let read = reading(&[
            (0, "Step  1:   Open Settings\n"),
            (2_000, "Step 1: Open Settings"),
        ]);
        assert_eq!(read.states.len(), 1);
        assert_eq!(read.states[0].end_ms, 4_000);
    }

    /// A screen that returns, after something else, is a NEW state. Merging it
    /// backwards would invent a time range in which it was not shown.
    #[test]
    fn a_screen_that_returns_later_is_a_separate_state() {
        let read = reading(&[(0, "Menu"), (2_000, "Editor"), (4_000, "Menu")]);
        assert_eq!(read.states.len(), 3);
        assert_eq!(read.states[2].start_ms, 4_000);
    }

    /// A video with no legible text says so, rather than returning an empty list
    /// that a caller might report as "the video showed nothing".
    #[test]
    fn a_video_with_no_legible_text_is_reported_as_such() {
        let read = reading(&[(0, "   "), (2_000, "\n\n")]);
        assert!(read.states.is_empty());
        assert!(read.no_legible_text);
    }

    #[test]
    fn an_absurdly_long_screen_text_is_bounded() {
        let huge = "x ".repeat(MAX_STATE_TEXT_BYTES);
        let read = reading(&[(0, &huge)]);
        assert!(read.states[0].text.len() <= MAX_STATE_TEXT_BYTES);
    }
}

#[cfg(test)]
mod real_reading {
    use super::*;

    /// Run the real ffmpeg + tesseract path over an actual video.
    ///
    /// Separate from the unit tests because it needs both binaries installed. It
    /// is what proves the frame timing and the OCR wiring, rather than the merge
    /// logic alone.
    #[test]
    #[ignore = "requires ffmpeg, tesseract and a video at AI_OS_VISUAL_TEST_VIDEO"]
    fn real_screen_reading_produces_timed_states() {
        let video = std::env::var("AI_OS_VISUAL_TEST_VIDEO")
            .expect("set AI_OS_VISUAL_TEST_VIDEO to a video file");
        let ffmpeg = which_binary("ffmpeg").expect("ffmpeg");
        let tesseract = find_tesseract().expect("tesseract");
        let languages = ocr_languages(&tesseract);
        let work = tempfile::tempdir().unwrap();

        let reading = read_screen_text(
            Path::new(&video),
            work.path(),
            &ffmpeg,
            &tesseract,
            &languages,
        )
        .unwrap();

        println!("LANGUAGES={languages}");
        println!("STATES={}", reading.states.len());
        for state in &reading.states {
            println!(
                "  [{}ms-{}ms] {:?}",
                state.start_ms, state.end_ms, state.text
            );
        }
        assert!(!reading.no_legible_text, "no legible screen text was read");
        assert!(!reading.states.is_empty());
        // States must be ordered and non-overlapping, or a quotation could not be
        // located in the recording.
        for pair in reading.states.windows(2) {
            assert!(pair[0].end_ms <= pair[1].start_ms);
            assert!(pair[0].start_ms < pair[0].end_ms);
        }
    }

    fn which_binary(name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    }
}

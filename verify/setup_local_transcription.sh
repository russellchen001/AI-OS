#!/bin/bash
# Install the local transcription toolchain for AI-OS Cognitive Distillation.
#
# Everything here is MIT-licensed, local, free, and needs no account:
#   ffmpeg        audio demux and video frame sampling
#   whisper-cpp   provides `whisper-cli`, the binary AI-OS probes for
#   tesseract     reads text on screen, so a SILENT tutorial video can be
#                 distilled too — transcription alone returns nothing from one
#   llama.cpp     provides `llama-mtmd-cli`, a local vision model runner, so a
#                 video with NO speech and NO on-screen text (hands showing a
#                 technique) can still be described
#   models        downloaded from Hugging Face, no login, no gate
#
# Nothing is sent anywhere. Transcription happens entirely on this machine.
set -u

# MUST match `toolchain::asset_root()` in Rust, which uses the platform cache
# directory. On macOS that is ~/Library/Caches, NOT ~/.cache — downloading to the
# wrong one means a 2.5 GB fetch the app then cannot find.
case "$(uname -s)" in
  Darwin) AI_OS_CACHE="$HOME/Library/Caches/ai-os" ;;
  *)      AI_OS_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/ai-os" ;;
esac

MODEL_DIR="$AI_OS_CACHE/whisper"
# large-v3-turbo q5_0: best quality-per-megabyte for Apple Silicon.
# Swap for ggml-small-q5_1.bin (181 MiB) if disk matters more than accuracy.
MODEL_FILE="ggml-large-v3-turbo-q5_0.bin"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$MODEL_FILE"

step() { printf '\n=== %s ===\n' "$1"; }

echo "assets will be installed under: $AI_OS_CACHE"

step "1/3  ffmpeg"
if command -v ffmpeg >/dev/null 2>&1; then
  echo "already installed: $(command -v ffmpeg)"
else
  command -v brew >/dev/null 2>&1 || { echo "Homebrew is required. See https://brew.sh"; exit 1; }
  brew install ffmpeg || exit 1
fi

step "2/3  whisper-cpp (provides whisper-cli)"
if command -v whisper-cli >/dev/null 2>&1; then
  echo "already installed: $(command -v whisper-cli)"
else
  command -v brew >/dev/null 2>&1 || { echo "Homebrew is required. See https://brew.sh"; exit 1; }
  # Ships a prebuilt bottle for Apple Silicon; no compiler needed.
  brew install whisper-cpp || exit 1
fi

step "3/4  tesseract (reads text on screen, so silent videos can be distilled)"
if command -v tesseract >/dev/null 2>&1; then
  echo "already installed: $(command -v tesseract)"
  tesseract --list-langs 2>&1 | tail -n +2 | tr '\n' ' '; echo
else
  command -v brew >/dev/null 2>&1 || { echo "Homebrew is required. See https://brew.sh"; exit 1; }
  # tesseract-lang carries the Chinese data; without it OCR falls back to English.
  brew install tesseract tesseract-lang || exit 1
fi

step "4/5  whisper model"
mkdir -p "$MODEL_DIR"
if [ -s "$MODEL_DIR/$MODEL_FILE" ]; then
  echo "already present: $MODEL_DIR/$MODEL_FILE ($(du -h "$MODEL_DIR/$MODEL_FILE" | cut -f1))"
else
  echo "downloading $MODEL_FILE (~547 MiB) to $MODEL_DIR"
  curl -L --fail --progress-bar -o "$MODEL_DIR/$MODEL_FILE.partial" "$MODEL_URL" \
    && mv "$MODEL_DIR/$MODEL_FILE.partial" "$MODEL_DIR/$MODEL_FILE" \
    || { echo "download failed"; rm -f "$MODEL_DIR/$MODEL_FILE.partial"; exit 1; }
fi

step "5/5  vision model (only needed for videos with no speech and no screen text)"
VLM_DIR="$AI_OS_CACHE/vlm"
VLM_MODEL="InternVL3-2B-Instruct-Q8_0.gguf"            # 1.89 GB
VLM_PROJ="mmproj-InternVL3-2B-Instruct-Q8_0.gguf"      # 337 MB
VLM_BASE="https://huggingface.co/ggml-org/InternVL3-2B-Instruct-GGUF/resolve/main"

if command -v llama-mtmd-cli >/dev/null 2>&1; then
  echo "already installed: $(command -v llama-mtmd-cli)"
else
  command -v brew >/dev/null 2>&1 || { echo "Homebrew is required. See https://brew.sh"; exit 1; }
  brew install llama.cpp || exit 1
fi

mkdir -p "$VLM_DIR"
for F in "$VLM_MODEL" "$VLM_PROJ"; do
  if [ -s "$VLM_DIR/$F" ]; then
    echo "already present: $F ($(du -h "$VLM_DIR/$F" | cut -f1))"
  else
    echo "downloading $F to $VLM_DIR"
    curl -L --fail --progress-bar -o "$VLM_DIR/$F.partial" "$VLM_BASE/$F" \
      && mv "$VLM_DIR/$F.partial" "$VLM_DIR/$F" \
      || { echo "download failed"; rm -f "$VLM_DIR/$F.partial"; exit 1; }
  fi
done

step "verify"
FAILED=0
for c in ffmpeg whisper-cli tesseract llama-mtmd-cli; do
  if command -v "$c" >/dev/null 2>&1; then echo "OK   $c  -> $(command -v $c)"; else echo "FAIL $c not found"; FAILED=1; fi
done
if [ -s "$MODEL_DIR/$MODEL_FILE" ]; then
  echo "OK   model -> $MODEL_DIR/$MODEL_FILE"
else
  echo "FAIL model missing"; FAILED=1
fi

# Prove the toolchain actually transcribes, rather than merely being installed.
if [ "$FAILED" -eq 0 ]; then
  step "smoke"
  TMP="$(mktemp -d)"
  # 3 seconds of silence is enough to prove the pipeline runs end to end.
  ffmpeg -nostdin -y -f lavfi -i anullsrc=r=16000:cl=mono -t 3 \
         -c:a pcm_s16le "$TMP/silence.wav" >/dev/null 2>&1
  if whisper-cli -m "$MODEL_DIR/$MODEL_FILE" -f "$TMP/silence.wav" -ojf -of "$TMP/out" >/dev/null 2>&1 \
     && [ -s "$TMP/out.json" ]; then
    echo "OK   whisper-cli produced JSON output"
  else
    echo "FAIL whisper-cli could not transcribe a test file"; FAILED=1
  fi
  # OCR: render a frame with known text and require tesseract to read it back.
  ffmpeg -nostdin -y -f lavfi -i "color=c=white:s=640x200:d=1" \
         -vf "drawtext=text='Open Settings':fontcolor=black:fontsize=48:x=40:y=70" \
         -frames:v 1 "$TMP/ocr.png" >/dev/null 2>&1
  if [ -s "$TMP/ocr.png" ] && tesseract "$TMP/ocr.png" stdout 2>/dev/null | grep -qi "settings"; then
    echo "OK   tesseract read text back from a rendered frame"
  else
    echo "WARN tesseract could not read a test frame; silent videos may yield nothing"
  fi
  # Vision model: describe a frame with a known object and require a sane answer.
  # This is the ONE engine that could not be verified before shipping, so its
  # first real output is printed rather than merely checked.
  ffmpeg -nostdin -y -f lavfi -i "color=c=white:s=512x512:d=1" \
         -vf "drawbox=x=128:y=128:w=256:h=256:color=red@1.0:t=fill" \
         -frames:v 1 "$TMP/shape.png" >/dev/null 2>&1
  if [ -s "$TMP/shape.png" ]; then
    echo "asking the vision model to describe a red square (first run loads ~2GB, be patient)..."
    VLM_OUT="$(llama-mtmd-cli -m "$VLM_DIR/$VLM_MODEL" --mmproj "$VLM_DIR/$VLM_PROJ" \
        --image "$TMP/shape.png" -p "Describe only what is visibly in this frame, in one short sentence." \
        2>/dev/null)"
    if [ -n "$VLM_OUT" ]; then
      echo "OK   vision model replied:"
      echo "     ${VLM_OUT}" | head -4
      echo "     (if that does not describe a red square, tell Claude — the output"
      echo "      parsing was written without being able to run this model.)"
    else
      echo "WARN vision model produced no output; soundless, textless videos will fail"
    fi
  fi
  rm -rf "$TMP"
fi

echo
if [ "$FAILED" -eq 0 ]; then
  echo "Local transcription is ready."
  echo
  echo "Then run the real smoke with your own audio OR video file:"
  echo
  echo "  cd ~/AI-OS/dashboard"
  echo "  AI_OS_RUN_TRANSCRIPTION_REAL_SMOKE=1 \\"
  echo "  AI_OS_TRANSCRIPTION_MEDIA=/full/path/to/your-file.mp4 \\"
  echo "  cargo test --manifest-path src-tauri/Cargo.toml \\"
  echo "    cognitive_distillation::transcription::real_smoke::real_local_transcription_produces_timestamped_evidence \\"
  echo "    -- --ignored --nocapture"
else
  echo "Setup incomplete; see the FAIL lines above."
  exit 1
fi

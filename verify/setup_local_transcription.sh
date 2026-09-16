#!/bin/bash

# Prepare the external/local media toolchain used by AI-OS Cognitive Distillation.
#
# Executables are installed as system tools. Model assets live in the exact same
# platform cache used by Rust `toolchain::asset_root()`.
#
# The hardware-profile threshold, filenames, sizes and SHA-256 values below MUST
# remain identical to src-tauri/src/cognitive_distillation/toolchain.rs.

set -u

step() {
  printf '\n=== %s ===\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1"
  exit 1
}

case "$(uname -s)" in
  Darwin)
    AI_OS_CACHE="$HOME/Library/Caches/ai-os"
    ;;
  *)
    AI_OS_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/ai-os"
    ;;
esac

WHISPER_HOST="https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
VLM_HOST="https://huggingface.co/ggml-org/InternVL3-2B-Instruct-GGUF/resolve/main"

# ------------------------------------------------------------
# Hardware profile
# Must match ModelProfile::for_this_machine():
# Accurate at >= 24 GiB, Compact otherwise.
# ------------------------------------------------------------

TOTAL_MEMORY_BYTES=""

if [ "$(uname -s)" = "Darwin" ]; then
  TOTAL_MEMORY_BYTES="$(sysctl -n hw.memsize 2>/dev/null || true)"
elif command -v getconf >/dev/null 2>&1; then
  PAGES="$(getconf _PHYS_PAGES 2>/dev/null || true)"
  PAGE_SIZE="$(getconf PAGE_SIZE 2>/dev/null || true)"
  if [ -n "$PAGES" ] && [ -n "$PAGE_SIZE" ]; then
    TOTAL_MEMORY_BYTES=$((PAGES * PAGE_SIZE))
  fi
fi

if [ -z "$TOTAL_MEMORY_BYTES" ]; then
  fail "could not determine installed memory"
fi

ACCURATE_THRESHOLD=$((24 * 1024 * 1024 * 1024))

if [ "$TOTAL_MEMORY_BYTES" -ge "$ACCURATE_THRESHOLD" ]; then
  PROFILE="accurate"

  WHISPER_FILE="ggml-large-v3-turbo-q5_0.bin"
  WHISPER_SIZE="574041195"
  WHISPER_SHA256="394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"

  VLM_MODEL="InternVL3-2B-Instruct-Q8_0.gguf"
  VLM_MODEL_SIZE="1893671520"
  VLM_MODEL_SHA256="b09d7858d8b111f103a38b4ac333c8d41ca6faf34da7d4c3f76c646318004410"
else
  PROFILE="compact"

  WHISPER_FILE="ggml-small-q5_1.bin"
  WHISPER_SIZE="190085487"
  WHISPER_SHA256="ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb"

  VLM_MODEL="InternVL3-2B-Instruct-Q4_K_M.gguf"
  VLM_MODEL_SIZE="1116758816"
  VLM_MODEL_SHA256="dc36eddc05ff1db5e11e0aa38efe7a5063b045aa5350c3b1a2510f3ff9107179"
fi

VLM_PROJ="mmproj-InternVL3-2B-Instruct-Q8_0.gguf"
VLM_PROJ_SIZE="337012000"
VLM_PROJ_SHA256="a91c525291f65b0469f82544e82f202c4df1c093268f0ce27deec10664078a0c"

WHISPER_DIR="$AI_OS_CACHE/whisper"
VLM_DIR="$AI_OS_CACHE/vlm"

echo "profile=$PROFILE"
echo "memory_bytes=$TOTAL_MEMORY_BYTES"
echo "asset_root=$AI_OS_CACHE"

# ------------------------------------------------------------
# Helpers
# ------------------------------------------------------------

sha256_file() {
  local path="$1"

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    return 1
  fi
}

verify_asset() {
  local path="$1"
  local expected_size="$2"
  local expected_sha="$3"

  [ -f "$path" ] || return 1

  local actual_size
  actual_size="$(wc -c < "$path" | tr -d ' ')"

  [ "$actual_size" = "$expected_size" ] || return 1

  local actual_sha
  actual_sha="$(sha256_file "$path")" || return 1

  [ "$actual_sha" = "$expected_sha" ]
}

fetch_asset() {
  local url="$1"
  local destination="$2"
  local expected_size="$3"
  local expected_sha="$4"

  mkdir -p "$(dirname "$destination")"

  if verify_asset "$destination" "$expected_size" "$expected_sha"; then
    echo "already verified: $destination"
    return 0
  fi

  if [ -e "$destination" ]; then
    echo "existing asset failed verification; replacing it:"
    echo "  $destination"
    rm -f "$destination"
  fi

  local partial="${destination}.partial"
  rm -f "$partial"

  echo "downloading:"
  echo "  $url"
  echo "to:"
  echo "  $destination"

  curl -L --fail --progress-bar \
    -o "$partial" \
    "$url" || {
      rm -f "$partial"
      fail "download failed"
    }

  if ! verify_asset "$partial" "$expected_size" "$expected_sha"; then
    rm -f "$partial"
    fail "downloaded asset failed pinned size/SHA-256 verification"
  fi

  mv "$partial" "$destination"

  verify_asset "$destination" "$expected_size" "$expected_sha" \
    || fail "installed asset failed final verification"

  echo "verified: $destination"
}

require_brew() {
  command -v brew >/dev/null 2>&1 \
    || fail "Homebrew is required to prepare the external media tools"
}

# ------------------------------------------------------------
# External executors
# ------------------------------------------------------------

step "1/7 ffmpeg"

if command -v ffmpeg >/dev/null 2>&1; then
  echo "already installed: $(command -v ffmpeg)"
else
  require_brew
  brew install ffmpeg || fail "ffmpeg installation failed"
fi

command -v ffprobe >/dev/null 2>&1 \
  || fail "ffprobe was not installed with ffmpeg"

step "2/7 whisper.cpp"

if command -v whisper-cli >/dev/null 2>&1; then
  echo "already installed: $(command -v whisper-cli)"
else
  require_brew
  brew install whisper-cpp || fail "whisper.cpp installation failed"
fi

step "3/7 Tesseract"

if command -v tesseract >/dev/null 2>&1; then
  echo "already installed: $(command -v tesseract)"
else
  require_brew
  brew install tesseract tesseract-lang \
    || fail "Tesseract installation failed"
fi

step "4/7 llama.cpp"

if command -v llama-mtmd-cli >/dev/null 2>&1; then
  echo "already installed: $(command -v llama-mtmd-cli)"
else
  require_brew
  brew install llama.cpp || fail "llama.cpp installation failed"
fi

# ------------------------------------------------------------
# Pinned managed assets
# ------------------------------------------------------------

step "5/7 Whisper model"

fetch_asset \
  "$WHISPER_HOST/$WHISPER_FILE?download=true" \
  "$WHISPER_DIR/$WHISPER_FILE" \
  "$WHISPER_SIZE" \
  "$WHISPER_SHA256"

step "6/7 Vision model"

fetch_asset \
  "$VLM_HOST/$VLM_MODEL?download=true" \
  "$VLM_DIR/$VLM_MODEL" \
  "$VLM_MODEL_SIZE" \
  "$VLM_MODEL_SHA256"

fetch_asset \
  "$VLM_HOST/$VLM_PROJ?download=true" \
  "$VLM_DIR/$VLM_PROJ" \
  "$VLM_PROJ_SIZE" \
  "$VLM_PROJ_SHA256"

# ------------------------------------------------------------
# Real engine smoke
# ------------------------------------------------------------

step "7/7 Real local engine smoke"

FAILED=0

for command in ffmpeg ffprobe whisper-cli tesseract llama-mtmd-cli; do
  if command -v "$command" >/dev/null 2>&1; then
    echo "OK   $command -> $(command -v "$command")"
  else
    echo "FAIL $command not found"
    FAILED=1
  fi
done

if ! verify_asset \
  "$WHISPER_DIR/$WHISPER_FILE" \
  "$WHISPER_SIZE" \
  "$WHISPER_SHA256"
then
  echo "FAIL Whisper model verification"
  FAILED=1
else
  echo "OK   Whisper model verified"
fi

if ! verify_asset \
  "$VLM_DIR/$VLM_MODEL" \
  "$VLM_MODEL_SIZE" \
  "$VLM_MODEL_SHA256"
then
  echo "FAIL VLM model verification"
  FAILED=1
else
  echo "OK   VLM model verified"
fi

if ! verify_asset \
  "$VLM_DIR/$VLM_PROJ" \
  "$VLM_PROJ_SIZE" \
  "$VLM_PROJ_SHA256"
then
  echo "FAIL VLM projector verification"
  FAILED=1
else
  echo "OK   VLM projector verified"
fi

if [ "$FAILED" -eq 0 ]; then
  TMP="$(mktemp -d)"

  # Speech execution.
  ffmpeg \
    -nostdin \
    -y \
    -f lavfi \
    -i anullsrc=r=16000:cl=mono \
    -t 3 \
    -c:a pcm_s16le \
    "$TMP/silence.wav" \
    >/dev/null 2>&1

  if whisper-cli \
      -m "$WHISPER_DIR/$WHISPER_FILE" \
      -f "$TMP/silence.wav" \
      -ojf \
      -of "$TMP/out" \
      >/dev/null 2>&1 \
      && [ -s "$TMP/out.json" ]; then
    echo "OK   local speech engine produced JSON"
  else
    echo "FAIL local speech engine smoke"
    FAILED=1
  fi

  # OCR execution.
  #
  # Do not depend on FFmpeg's optional `drawtext` filter: some Homebrew FFmpeg
  # builds omit it. SVG gives us deterministic text without introducing another
  # package dependency.
  cat > "$TMP/ocr.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="400">
  <rect width="1200" height="400" fill="white"/>
  <text
    x="100"
    y="240"
    font-family="Arial, Helvetica, sans-serif"
    font-size="110"
    font-weight="bold"
    fill="black">SETTINGS</text>
</svg>
SVG

  if command -v qlmanage >/dev/null 2>&1; then
    mkdir -p "$TMP/ql"
    qlmanage       -t       -s 1200       -o "$TMP/ql"       "$TMP/ocr.svg"       >/dev/null 2>&1 || true

    QL_PNG="$TMP/ql/ocr.svg.png"
    if [ -s "$QL_PNG" ]; then
      cp "$QL_PNG" "$TMP/ocr.png"
    fi
  fi

  if [ ! -s "$TMP/ocr.png" ] && command -v sips >/dev/null 2>&1; then
    sips       -s format png       "$TMP/ocr.svg"       --out "$TMP/ocr.png"       >/dev/null 2>&1 || true
  fi

  if [ -s "$TMP/ocr.png" ] \
    && tesseract "$TMP/ocr.png" stdout -l eng --psm 6 2>/dev/null \
       | grep -qi "settings"; then
    echo "OK   OCR engine read rendered text"
  else
    echo "FAIL OCR engine smoke"
    FAILED=1
  fi

  # Vision execution.
  ffmpeg \
    -nostdin \
    -y \
    -f lavfi \
    -i "color=c=white:s=512x512:d=1" \
    -vf "drawbox=x=128:y=128:w=256:h=256:color=red@1.0:t=fill" \
    -frames:v 1 \
    "$TMP/shape.png" \
    >/dev/null 2>&1

  echo "loading local vision model..."

  VLM_OUT="$(
    llama-mtmd-cli \
      -m "$VLM_DIR/$VLM_MODEL" \
      --mmproj "$VLM_DIR/$VLM_PROJ" \
      --image "$TMP/shape.png" \
      -p "Describe only what is visibly in this frame, in one short sentence. Do not infer identity, intent, emotion, age or health." \
      2>/dev/null
  )"

  if [ -n "$VLM_OUT" ]; then
    echo "OK   local vision engine produced output"
    printf '%s\n' "$VLM_OUT" | tail -8
  else
    echo "FAIL local vision engine smoke"
    FAILED=1
  fi

  rm -rf "$TMP"
fi

echo

if [ "$FAILED" -eq 0 ]; then
  echo "MEDIA_TOOLCHAIN_SETUP=PASS"
  echo "profile=$PROFILE"
  echo "asset_root=$AI_OS_CACHE"
else
  echo "MEDIA_TOOLCHAIN_SETUP=FAIL"
  exit 1
fi

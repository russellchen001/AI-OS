#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

pass() {
  printf 'PASS %s\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

echo "============================================================"
echo "P15 GM-4 — Cloud Providers"
echo "============================================================"

echo
echo "=== 1. Connected Provider Instance metadata — sanitized ==="

python3 <<'PY'
import json
from pathlib import Path

path = (
    Path.home()
    / "Library"
    / "Application Support"
    / "AI OS"
    / "provider-instances.json"
)

if not path.exists():
    raise SystemExit("FAIL provider-instances.json unavailable")

instances = json.loads(path.read_text())

required = {
    "grok": None,
    "openai": None,
}

for instance in instances:
    provider = str(instance.get("providerId", "")).strip().lower()
    if provider not in required:
        continue

    credential = instance.get("credential") or {}

    if (
        str(instance.get("connectionState", "")).strip().lower() == "connected"
        and str(credential.get("kind", "")).strip().lower() == "oauth"
    ):
        required[provider] = str(instance.get("id", ""))

missing = [provider for provider, instance in required.items() if not instance]

if missing:
    raise SystemExit(
        "FAIL connected OAuth Provider Instance missing: " + ", ".join(missing)
    )

for provider, instance in required.items():
    print(f"PASS connected OAuth {provider} instance={instance}")

print("PASS sanitized config only")
print("PASS Keychain values not read")
PY

echo
echo "=== 2. Targeted Rust formatting check ==="

rustfmt \
  --edition 2021 \
  --check \
  src-tauri/src/providers.rs \
  src-tauri/src/generative_media/comfyui_provider.rs \
  src-tauri/src/generative_media/cloud_provider.rs \
  src-tauri/src/generative_media/registry.rs

# generative_media/mod.rs declares the whole module tree. Normal rustfmt
# traversal would recursively inspect stable GM-1 files such as router.rs and
# executor.rs even though GM-4 does not modify them. Check the module root
# itself while explicitly skipping child traversal.
rustfmt \
  --edition 2021 \
  --check \
  --config skip_children=true \
  src-tauri/src/generative_media/mod.rs

pass "targeted rustfmt without stable-module drift"

echo
echo "=== 3. GM-4 Cloud Provider behavior ==="

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::cloud_provider::tests:: \
  -- --nocapture

pass "GM-4 Cloud Provider tests"

echo
echo "=== 4. Local First remains fail-closed ==="

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  local_first_with_no_local_provider_never_auto_routes_to_cloud \
  -- --nocapture

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  local_first_requires_setup_and_never_silently_uses_cloud \
  -- --nocapture

pass "Local First never silently routes to Cloud"

echo
echo "=== 5. Manual Provider selection remains exact ==="

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  manual_provider_selection_is_exact \
  -- --nocapture

pass "manual Provider selection exact"

echo
echo "=== 6. Provider failure still has no silent fallback ==="

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  provider_error_is_not_replaced_by_another_provider \
  -- --nocapture

pass "provider execution failure does not switch Provider"

echo
echo "=== 7. Compile ==="

cargo check --manifest-path src-tauri/Cargo.toml

pass "cargo check"

echo
echo "=== 8. Diff integrity ==="

git diff --check

pass "git diff --check"

echo
echo "=== 9. Optional real connected-account E2E ==="

if [ "${AI_OS_GM4_LIVE:-0}" = "1" ]; then
  cargo test \
    --manifest-path src-tauri/Cargo.toml \
    live_connected_grok_oauth_text_to_image \
    -- --ignored --nocapture

  pass "real connected Grok OAuth media E2E"
else
  echo "SKIP real cloud generation E2E — set AI_OS_GM4_LIVE=1 for explicit opt-in"
fi

echo
echo "=== 10. Expected change set ==="

git status --short

echo
echo "============================================================"
echo "PASS P15 GM-4 CLOUD PROVIDERS"
echo "============================================================"

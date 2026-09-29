#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

run_step() {
  label="$1"
  script="$2"
  echo "===== $label ====="
  if "$script"; then
    echo "✓ $label"
  else
    echo "FAIL Beta release: $label"
    exit 1
  fi
}

run_step "Managed OpenClaw lifecycle" verify/verify_beta_openclaw_lifecycle.sh
run_step "Clean-profile isolation" verify/verify_beta_clean_profile.sh
run_step "Beta version consistency" verify/verify_beta_version.sh
run_step "Package/data hygiene" verify/verify_beta_package_hygiene.sh

echo "===== Frontend production build ====="
if npm run build; then
  echo "✓ frontend production build"
else
  echo "FAIL Beta release: frontend production build"
  exit 1
fi

echo "===== Rust cargo check ====="
if (cd src-tauri && cargo check); then
  echo "✓ Rust cargo check"
else
  echo "FAIL Beta release: Rust cargo check"
  exit 1
fi

echo "===== Source integrity ====="
if git diff --check; then
  echo "✓ git diff --check"
else
  echo "FAIL Beta release: git diff --check"
  exit 1
fi

echo "PASS Beta release verification"

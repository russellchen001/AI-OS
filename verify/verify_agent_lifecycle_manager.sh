#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

if node_modules/.bin/esbuild verify/agent_lifecycle_manager.behavior.ts \
  --bundle --platform=node --format=esm \
  --outfile="$tmp_dir/agent-lifecycle-manager.mjs" >/dev/null \
  && node "$tmp_dir/agent-lifecycle-manager.mjs"; then
  echo "✓ Agent adapter registry and version capabilities"
else
  echo "✗ Agent Lifecycle Manager behavior"
  echo "FAIL: Agent Lifecycle Manager"
  exit 1
fi

if cargo test --manifest-path src-tauri/Cargo.toml --lib agent_lifecycle_manager >/dev/null; then
  echo "✓ Agent lifecycle backend behavior"
else
  echo "✗ Agent lifecycle backend behavior"
  echo "FAIL: Agent Lifecycle Manager"
  exit 1
fi

if npm run build >/dev/null; then
  echo "✓ Frontend production build"
  echo "PASS: Agent Lifecycle Manager"
else
  echo "✗ Frontend production build"
  echo "FAIL: Agent Lifecycle Manager"
  exit 1
fi

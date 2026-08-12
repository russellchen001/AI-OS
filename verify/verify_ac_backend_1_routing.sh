#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "FAIL: AC-BACKEND-1 Routing - $1"
  exit 1
}

trap 'fail "command failed at line $LINENO"' ERR

run_route_test() {
  local name="$1"
  cargo test --quiet --manifest-path src-tauri/Cargo.toml "$name"
  echo "✅ $name"
}

run_route_test auto_route_candidates_preserve_local_first_and_default_model_order
run_route_test auto_route_falls_back_to_cloud_when_no_local_models_are_available
run_route_test manual_route_executes_only_the_requested_candidate
run_route_test route_selection_allows_only_auto_pre_output_fallback

node --input-type=module <<'NODE'
const base = "http://127.0.0.1:11434";
const tagsResponse = await fetch(`${base}/api/tags`, {
  signal: AbortSignal.timeout(5000),
});
if (!tagsResponse.ok) throw new Error("Ollama tags request failed");
const tags = await tagsResponse.json();
const model = tags.models?.[0]?.model;
if (!model) throw new Error("Ollama has no installed model");

const response = await fetch(`${base}/api/chat`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    model,
    stream: false,
    messages: [{ role: "user", content: "Reply with exactly: LOCAL_OK" }],
    options: { num_predict: 16 },
  }),
  signal: AbortSignal.timeout(120000),
});
if (!response.ok) throw new Error("Ollama chat request failed");
const body = await response.json();
if (!body.message?.content?.trim()) {
  throw new Error("Ollama returned no output");
}
console.log(`✅ Live Ollama response through ${model}`);
NODE

echo "PASS: AC-BACKEND-1 Routing"

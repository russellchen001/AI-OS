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
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { arch } from "node:process";

const useOmlx = arch === "arm64";
const base = useOmlx ? "http://127.0.0.1:8000/v1" : "http://127.0.0.1:11434/api";
const headers = { "content-type": "application/json" };
if (useOmlx) {
  headers.authorization = `Bearer ${(await readFile(`${homedir()}/.openclaw/ai-os-secrets/omlx-api-key`, "utf8")).trim()}`;
}
const modelsResponse = await fetch(`${base}/${useOmlx ? "models" : "tags"}`, {
  headers,
  signal: AbortSignal.timeout(5000),
});
if (!modelsResponse.ok) throw new Error("Local model list request failed");
const models = await modelsResponse.json();
const model = useOmlx
  ? models.data?.find(({ id }) => id === "Qwen3.5-9B-4bit")?.id
  : models.models?.[0]?.model;
if (!model) throw new Error("Required local model is unavailable");

const response = await fetch(`${base}/chat${useOmlx ? "/completions" : ""}`, {
  method: "POST",
  headers,
  body: JSON.stringify({
    model,
    stream: false,
    messages: [{ role: "user", content: "Reply with exactly: LOCAL_OK" }],
    ...(useOmlx ? { max_tokens: 16 } : { options: { num_predict: 16 } }),
  }),
  signal: AbortSignal.timeout(120000),
});
if (!response.ok) throw new Error("Local chat request failed");
const body = await response.json();
const content = useOmlx ? body.choices?.[0]?.message?.content : body.message?.content;
if (!content?.trim()) {
  throw new Error("Local model returned no output");
}
console.log(`✅ Live ${useOmlx ? "oMLX" : "Ollama"} response through ${model}`);
NODE

echo "PASS: AC-BACKEND-1 Routing"

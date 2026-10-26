#!/usr/bin/env bash
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo 'FAIL: Council export — provide exported JSON and Markdown paths'
  exit 1
fi
node --input-type=module - "$1" "$2" <<'NODE'
import { readFileSync } from 'node:fs';
try {
  const [jsonPath, markdownPath] = process.argv.slice(2);
  const decode = path => new TextDecoder('utf-8', { fatal: true }).decode(readFileSync(path));
  const session = JSON.parse(decode(jsonPath));
  const markdown = decode(markdownPath);
  if (typeof session.prompt !== 'string' || !session.prompt.trim() ||
      !Array.isArray(session.steps) || !session.steps.length ||
      typeof session.finalAnswer !== 'string' || !session.finalAnswer.trim()) {
    throw new Error('exported session lacks request, steps or final answer');
  }
  console.log('✓ JSON and Markdown are valid UTF-8; session is complete');
  if (!markdown.includes(session.prompt) || !markdown.includes(session.finalAnswer)) {
    throw new Error('Markdown lost or changed the original request/final answer');
  }
  console.log('✓ request and final answer match the exported session exactly');
  for (const step of session.steps) {
    const expected = step.output || step.error || '_No output._';
    if (!markdown.includes(expected)) throw new Error(`step output missing: ${step.memberName}`);
  }
  console.log(`✓ all ${session.steps.length} step outputs/errors are preserved`);
  console.log('PASS: Council export');
} catch (error) {
  console.error(`FAIL: Council export — ${error.message}`);
  process.exit(1);
}
NODE

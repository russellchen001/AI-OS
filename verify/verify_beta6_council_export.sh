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
    if (step.error && !markdown.includes(step.error)) throw new Error(`step error missing: ${step.memberName}`);
    for (const attempt of step.attempts ?? []) {
      if (attempt.output && !markdown.includes(attempt.output)) throw new Error(`raw attempt missing: ${step.memberName}`);
      if (attempt.error && !markdown.includes(attempt.error)) throw new Error(`raw attempt error missing: ${step.memberName}`);
    }
  }
  console.log(`✓ all ${session.steps.length} step outputs/errors are preserved`);
  const synthesis = session.steps.find(step => step.stage === 'final-synthesis');
  if (session.recommendation && synthesis) {
    const raw = synthesis.output;
    const original = JSON.parse(raw.slice(raw.indexOf('{'), raw.lastIndexOf('}') + 1));
    for (const key of ['summary', 'recommendedPlan', 'rationale', 'disagreements', 'risks', 'assumptions', 'uncertainty']) {
      if (JSON.stringify(original[key]) !== JSON.stringify(session.recommendation[key])) throw new Error(`structured field changed: ${key}`);
      for (const value of [original[key]].flat()) if (!session.finalAnswer.includes(value)) throw new Error(`final answer lost content: ${key}`);
    }
    console.log('✓ all 7 raw synthesis fields and final answer content match');
  }
  console.log('PASS: Council export');
} catch (error) {
  console.error(`FAIL: Council export — ${error.message}`);
  process.exit(1);
}
NODE

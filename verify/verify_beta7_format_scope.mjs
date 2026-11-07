import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Fixed released baseline: committing Beta 7 must not erase the comparison.
const baseline = 'ccc3e5d44a26c6ae2b6ed0dff779238a5053dae2';
const root = execFileSync('git', ['rev-parse', '--show-toplevel'], { encoding: 'utf8' }).trim();
process.chdir(root);
const work = mkdtempSync(join(tmpdir(), 'beta7-format-check-'));
const git = args => execFileSync('git', args, { encoding: 'utf8' });
const files = [...new Set([
  ...git(['diff', '--name-only', baseline, '--', '*.rs']).trim().split('\n'),
  ...git(['ls-files', '--others', '--exclude-standard', '--', '*.rs']).trim().split('\n'),
].filter(Boolean))];

function formatted(input) {
  const result = spawnSync('rustfmt', ['--edition', '2021', '--config', 'skip_children=true', '--emit', 'stdout'], { input, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}

function corrections(input) {
  writeFileSync(join(work, 'raw.rs'), input);
  writeFileSync(join(work, 'formatted.rs'), formatted(input));
  const diff = spawnSync('diff', ['-U0', join(work, 'raw.rs'), join(work, 'formatted.rs')], { encoding: 'utf8' });
  assert.ok(diff.status === 0 || diff.status === 1, diff.stderr);
  const counts = new Map();
  for (const line of diff.stdout.split('\n')) {
    if (!/^[+-]/.test(line) || /^---|^\+\+\+/.test(line)) continue;
    counts.set(line, (counts.get(line) ?? 0) + 1);
  }
  return counts;
}

let failed = false;
try {
  for (const file of files) {
    const current = corrections(readFileSync(file, 'utf8'));
    let previous = new Map();
    const old = spawnSync('git', ['show', `${baseline}:${file}`], { encoding: 'utf8' });
    if (old.status === 0) previous = corrections(old.stdout);
    const introduced = [...current].filter(([line, count]) => count > (previous.get(line) ?? 0));
    if (introduced.length) {
      console.error(`❌ ${file}: formatting corrections introduced since Beta 6`);
      failed = true;
    } else {
      console.log(`✅ ${file}${current.size ? ' — remaining corrections exist in released baseline' : ''}`);
    }
  }
  assert.ok(files.length > 0, 'Beta 7 Rust scope must not be empty');
  if (failed) throw new Error('Beta 7 introduces formatting debt');
  console.log(`PASS: beta7_format_scope (${files.length} files; historical whole-repository debt is not a PASS)`);
} catch (error) {
  console.error(`FAIL: beta7_format_scope — ${error.message}`);
  process.exitCode = 1;
} finally {
  rmSync(work, { recursive: true, force: true });
}

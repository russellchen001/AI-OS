import { build } from 'esbuild';
import assert from 'node:assert/strict';

// Isolated: the Claude Code status is a fixture; no CLI, account or Keychain is used.
let status;
globalThis.__beta7Cli = async (command) => {
  if (command === 'get_claude_code_status') return status;
  throw new Error(`Unexpected command: ${command}`);
};
const result = await build({ entryPoints: ['src/services/cliProviderManager.ts'], bundle: true,
  platform: 'node', format: 'esm', write: false,
  plugins: [{ name: 'isolated-cli-platform', setup(builder) {
    builder.onResolve({ filter: /^@tauri-apps\/api\/core$/ }, args => ({ path: args.path, namespace: 'fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: 'export const invoke=(...args)=>globalThis.__beta7Cli(...args);' }));
  } }],
});
const service = await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
const adapter = service.getCliProviderAdapter('anthropic');

status = { installed: true, authenticated: true, message: 'ok', modelAliases: ['fable', 'opus', 'sonnet'] };
const models = adapter.models('anthropic-account', await adapter.probe());
assert.deepEqual(models.map(m => m.remoteModelId), ['claude-code-default', 'fable', 'opus', 'sonnet']);
assert.deepEqual(models.map(m => m.displayName), ['Claude Code account default', 'Claude Fable (latest)', 'Claude Opus (latest)', 'Claude Sonnet (latest)']);
assert.deepEqual(models.filter(m => m.isDefault).map(m => m.remoteModelId), ['claude-code-default']);
assert.ok(models.every(m => m.availability === undefined), 'every choice needs a real verification first');
console.log('✅ Signed-in Claude Code offers the account default plus the CLI-documented aliases, all unverified');

status = { installed: true, authenticated: true, message: 'ok' };
assert.deepEqual(adapter.models('a', await adapter.probe()).map(m => m.remoteModelId), ['claude-code-default']);
console.log('✅ Older CLI without documented aliases keeps the account default only');

status = { installed: true, authenticated: false, message: 'sign in', modelAliases: ['opus'] };
assert.deepEqual(adapter.models('a', await adapter.probe()), []);
console.log('✅ Signed-out Claude Code offers no models');
console.log('PASS: beta7_claude_models — isolated behavior');

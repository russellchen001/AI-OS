import { build } from 'esbuild';
import assert from 'node:assert/strict';

let installed = false, authenticated = false, installSucceeds = true, loginSucceeds = true;
let calls = [];
const plan = { providerId: 'anthropic', displayName: 'Claude Code', packageName: 'Claude Code native binary', version: '2.1.311', integrity: 'sha256-fixture', method: 'native-download', action: 'install', requiresApproval: true };
globalThis.__beta7Cli = async (command, args) => {
  calls.push({ command, args });
  if (command === 'get_claude_code_status') return { installed, authenticated, message: 'fixture' };
  if (command === 'prepare_cli_provider_install') return plan;
  if (command === 'install_cli_provider') {
    assert.equal(args.request.userApproved, true);
    assert.equal(args.request.version, plan.version);
    assert.equal(args.request.integrity, plan.integrity);
    if (installSucceeds) installed = true;
    return { success: installSucceeds, message: installSucceeds ? 'Installed' : 'Download interrupted.' };
  }
  if (command === 'sign_in_claude_code') {
    assert.equal(args.userApproved, true);
    assert.ok(args.operationId);
    if (!loginSucceeds) throw new Error('Sign-in timed out. Retry or open official setup help.');
    authenticated = true;
    return;
  }
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
function reset() { installed = false; authenticated = false; installSucceeds = true; loginSucceeds = true; calls = []; }

reset();
await assert.rejects(service.connectCliProvider(adapter, async reviewed => {
  assert.deepEqual(reviewed, plan);
  assert.equal(calls.some(c => c.command === 'install_cli_provider'), false);
  return false;
}), /cancelled/);
assert.equal(calls.some(c => c.command === 'install_cli_provider' || c.command === 'sign_in_claude_code'), false);
console.log('✅ Missing CLI requires explicit review; declining installs and signs in nothing');

reset(); installSucceeds = false;
await assert.rejects(service.connectCliProvider(adapter, async () => true), /official setup help/);
assert.equal(calls.some(c => c.command === 'sign_in_claude_code'), false);
console.log('✅ Installation failure shows recovery and never attempts login');

reset();
const progress = [];
const connected = await service.connectCliProvider(adapter, async () => true, message => progress.push(message));
assert.equal(connected.authenticated, true);
assert.equal(calls.filter(c => c.command === 'install_cli_provider').length, 1);
assert.equal(calls.filter(c => c.command === 'sign_in_claude_code').length, 1);
assert.equal(adapter.models('anthropic-account', connected)[0].remoteModelId, 'claude-code-default');
assert.ok(progress.some(s => s.includes('official page')));
assert.deepEqual(calls.map(c => c.command), ['get_claude_code_status','prepare_cli_provider_install','install_cli_provider','get_claude_code_status','sign_in_claude_code','get_claude_code_status']);
console.log('✅ Approved install → graphical official sign-in → status recheck → usable model');

reset(); installed = true; loginSucceeds = false;
await assert.rejects(service.connectCliProvider(adapter, async () => { throw new Error('must not install'); }), /Retry|retry/);
assert.equal(calls.some(c => c.command === 'install_cli_provider'), false);
console.log('✅ Login failure offers recovery without reinstalling');

reset(); installed = true; authenticated = true;
await service.connectCliProvider(adapter, async () => { throw new Error('must not install'); });
assert.deepEqual(calls.map(c => c.command), ['get_claude_code_status']);
console.log('✅ Existing signed-in connection skips installation and login; no credential writes');

reset(); const controller = new AbortController();
await assert.rejects(service.connectCliProvider(adapter, async () => { controller.abort(); return true; }, undefined, controller.signal));
assert.equal(calls.some(c => c.command === 'install_cli_provider' || c.command === 'sign_in_claude_code'), false);
console.log('✅ Closing setup before approval completes cannot install or open login');
console.log('PASS: beta7_claude_setup — isolated platform behavior');

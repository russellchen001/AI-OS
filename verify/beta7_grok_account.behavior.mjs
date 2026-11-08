import { build } from 'esbuild';
import assert from 'node:assert/strict';

// Isolated platform: no real Grok CLI, account or Keychain is touched.
let status;
let calls = [];
const plan = { providerId: 'grok', displayName: 'Grok CLI', packageName: '@xai-official/grok', version: '1.0.70', integrity: 'sha512-fixture', method: 'official-installer', action: 'install', requiresApproval: true };
const notSignedIn = { installed: true, authenticated: false, models: [], message: 'Run `grok login` in Terminal to sign in with your xAI account.' };
const signedIn = { installed: true, authenticated: true, models: ['grok-4.7'], defaultModel: 'grok-4.7', message: 'Grok account is connected through the official Grok CLI.' };
globalThis.__beta7Cli = async (command) => {
  calls.push(command);
  if (command === 'get_grok_cli_status') return status;
  if (command === 'prepare_cli_provider_install') return plan;
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
const adapter = service.getCliProviderAdapter('grok');

assert.ok(service.CLI_ACCOUNT_PROVIDER_IDS.has('grok'));
assert.equal(adapter.signIn, undefined, 'AI-OS must not run or imitate Grok sign-in itself');
console.log('✅ Grok account path is the official CLI; AI-OS performs no sign-in of its own');

status = notSignedIn; calls = [];
await assert.rejects(
  service.connectCliProvider(adapter, async () => { throw new Error('must not install'); }),
  (error) => {
    assert.match(error.message, /`grok login`/);
    assert.match(error.message, /API key/);
    assert.doesNotMatch(error.message, /official setup help/);
    return true;
  },
);
assert.deepEqual(calls, ['get_grok_cli_status']);
console.log('✅ Signed-out CLI names `grok login` and the API key option; nothing installed, no false connection');

status = { installed: false, authenticated: false, models: [], message: 'Install the official Grok CLI (x.ai/cli) to connect a Grok account.' }; calls = [];
await assert.rejects(service.connectCliProvider(adapter, async () => false), /cancelled/);
assert.deepEqual(calls, ['get_grok_cli_status', 'prepare_cli_provider_install']);
console.log('✅ Missing CLI requires explicit install review; declining installs nothing');

status = signedIn; calls = [];
const connected = await service.connectCliProvider(adapter, async () => { throw new Error('must not install'); });
assert.equal(connected.authenticated, true);
assert.deepEqual(calls, ['get_grok_cli_status']);
const models = adapter.models('grok-account', connected);
assert.equal(models[0].remoteModelId, 'grok-4.7');
console.log('✅ Signed-in CLI connects with its reported model; no credential reads or writes');

console.log('PASS: beta7_grok_account — isolated platform behavior');

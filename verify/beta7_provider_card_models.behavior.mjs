import { build } from 'esbuild';
import assert from 'node:assert/strict';

// Isolated: the native layer is stubbed; no saved connection or Keychain is read.
const result = await build({ entryPoints: ['src/services/providers.ts'], bundle: true,
  platform: 'node', format: 'esm', write: false,
  plugins: [{ name: 'isolated-platform', setup(builder) {
    builder.onResolve({ filter: /^@tauri-apps\/api\/core$/ }, args => ({ path: args.path, namespace: 'fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: 'export const invoke=async()=>{throw new Error("no native layer")};' }));
  } }],
});
const { connectedModelLabels } = await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);

const model = (displayName, extra = {}) => ({ id: displayName, providerInstanceId: 'openai-account', remoteModelId: displayName,
  displayName, capabilities: ['chat'], enabled: true, isDefault: false, availability: 'verified', ...extra });
const instance = (models) => ({ id: 'openai-account', providerId: 'openai', displayName: 'OpenAI', models,
  credential: { kind: 'oauth', refreshable: true }, connectionState: 'connected', createdAt: '', updatedAt: '' });

assert.deepEqual(connectedModelLabels(undefined), []);
console.log('✅ No connection → card keeps its generic examples');

assert.deepEqual(connectedModelLabels(instance([
  model('GPT-5.6'), model('GPT-6.1-Sol', { isDefault: true }), model('GPT-6.1-Mini'),
])), ['GPT-6.1-Sol', 'GPT-5.6', 'GPT-6.1-Mini']);
console.log('✅ Connected account shows its own verified models, default first');

assert.deepEqual(connectedModelLabels(instance([
  model('Unverified', { availability: undefined }), model('Unavailable', { availability: 'unavailable' }),
  model('Disabled', { enabled: false }), model('grok-4.7 (Grok CLI)', { isDefault: true }),
])), ['grok-4.7 (Grok CLI)']);
console.log('✅ Unverified, unavailable or disabled models are never shown as offered');

assert.deepEqual(connectedModelLabels(instance([model('A'), model('B'), model('C'), model('D')])).length, 3);
console.log('✅ Card shows at most three models');

console.log('PASS: beta7_provider_card_models — isolated behavior');

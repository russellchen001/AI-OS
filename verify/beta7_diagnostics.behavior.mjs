import { build } from 'esbuild';
import assert from 'node:assert/strict';

let openClawFails = false;
let permissionFails = false;
let repairSucceeds = true;
let pendingAgent = false;
globalThis.__beta7DiagnosticsFixture = {
  status: async () => {
    if (pendingAgent) return new Promise(() => {});
    if (openClawFails) throw new Error('isolated fixture failure');
    return { state: 'ready' };
  },
  permissions: async () => {
    if (permissionFails) throw new Error('isolated fixture failure');
    return { operationResult: { permissions: [{ id: 'camera', state: 'notDetermined' }] } };
  },
  repair: async () => ({ success: repairSucceeds }),
};
const result = await build({
  entryPoints: ['src/services/firstLaunch.ts'], bundle: true,
  platform: 'node', format: 'esm', write: false,
  plugins: [{ name: 'isolated-diagnostics', setup(builder) {
    builder.onResolve({ filter: /^(\.\/managedOpenClaw|\.\/providers|@tauri-apps\/api\/core)$/ }, args => ({ path: args.path, namespace: 'fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'fixture' }, args => ({ contents:
      args.path === './managedOpenClaw'
        ? 'export const getManagedOpenClawStatus=()=>globalThis.__beta7DiagnosticsFixture.status(); export const repairManagedOpenClaw=()=>globalThis.__beta7DiagnosticsFixture.repair();'
        : args.path === './providers'
          ? 'export const initializeProviderInstances=async()=>{}; export const listProviderInstances=()=>[{connectionState:"connected"}];'
          : 'export const invoke=(command)=>command==="get_credential_vault_diagnostics"?Promise.resolve({pid:123,readCalls:1,writeCalls:0,legacyReadCalls:0}):globalThis.__beta7DiagnosticsFixture.permissions();'
    }));
  } }],
});
const service = await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
const healthy = await service.runFirstLaunchDiagnostics();
assert.equal(healthy.openClawState, 'ready');
assert.equal(healthy.diagnostic, undefined);
console.log('✅ Healthy diagnostics');
openClawFails = true;
const optionalFailure = await service.runFirstLaunchDiagnostics();
assert.equal(optionalFailure.providerConnected, true);
assert.equal(optionalFailure.permissions.length, 1);
assert.equal(optionalFailure.openClawState, 'unavailable');
assert.ok(optionalFailure.diagnostic);
console.log('✅ Optional agent failure preserves connected AI and permission results');
permissionFails = true;
const bothFailed = await service.runFirstLaunchDiagnostics();
assert.equal(bothFailed.providerConnected, true);
assert.equal(bothFailed.permissions.length, 0);
assert.ok(bothFailed.diagnostic.includes('permission'));
console.log('✅ Permission failure reports diagnostics without blocking chat');
repairSucceeds = false;
await assert.rejects(service.repairFirstLaunchOpenClaw(), /repair did not complete/);
console.log('✅ Failed repair is not reported as success');
pendingAgent = true;
let fast;
const pending = service.runFirstLaunchDiagnostics(result => { fast = result; });
await new Promise(resolve => setImmediate(resolve));
assert.equal(fast.providerConnected, true);
assert.equal(fast.openClawState, 'checking');
const bounded = await pending;
assert.equal(bounded.providerConnected, true);
assert.equal(bounded.openClawState, 'unavailable');
assert.equal(bounded.vault.readCalls, 1);
console.log('✅ Real diagnostics publishes connection before optional work and bounds a never-returning Agent check');
delete globalThis.__beta7DiagnosticsFixture;
console.log('PASS: beta7_diagnostics_fault_behavior (isolated fixtures)');

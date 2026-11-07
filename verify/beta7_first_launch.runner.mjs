import { build } from 'esbuild';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const work = await mkdtemp(join(tmpdir(), 'beta7-setup-'));
try {
  const outfile = join(work, 'behavior.mjs');
  await build({ entryPoints: ['verify/beta7_first_launch.behavior.tsx'], bundle: true,
    platform: 'node', format: 'esm', jsx: 'automatic', outfile,
    plugins: [{ name: 'isolated-setup-platform', setup(builder) {
      builder.onResolve({ filter: /^(react|react\/jsx-runtime|react-dom\/client|happy-dom)$/ }, args => ({ path: import.meta.resolve(args.path), external: true }));
      builder.onResolve({ filter: /services\/firstLaunch$/ }, args => ({ path: args.path, namespace: 'fixture' }));
      builder.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: 'export const runFirstLaunchDiagnostics=(progress)=>globalThis.__beta7Setup.check(progress);export const repairFirstLaunchOpenClaw=runFirstLaunchDiagnostics;export const openPermissionSettings=async()=>{};' }));
    } }],
  });
  await import(pathToFileURL(outfile).href);
} finally { await rm(work, { recursive: true, force: true }); }

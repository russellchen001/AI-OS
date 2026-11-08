import { build } from 'esbuild';
import { createServer } from 'node:http';
import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { writeFile } from 'node:fs/promises';

const hash = createHash('sha256');
for (const path of ['src/App.css', 'src/components/FirstLaunchOnboarding.tsx', 'src/hooks/useFirstLaunch.ts', 'verify/beta7_setup_layout.fixture.tsx']) hash.update(await readFile(path));
const revision = hash.digest('hex');
function validate(receipt) {
  if (receipt.revision !== revision) throw new Error('Receipt is stale; render the current code again');
  for (const name of ['600x450', '900x500', '1100x720', '600x450-large']) {
    const result = receipt.results[name];
    if (!result?.initialReachable || (!result.reachable && !result.completed)) throw new Error(`${name}: completion button not initially reachable`);
    if (result.styles.display === 'none' || result.styles.visibility !== 'visible' || Number(result.styles.opacity) <= 0 || result.styles.pointerEvents === 'none') throw new Error(`${name}: completion action hidden or disabled by CSS`);
  }
  if (!receipt.results['600x450'].completed || receipt.results['600x450'].progress !== 'complete') throw new Error('Actual completion click not verified');
}
if (process.argv[2] === '--verify') {
  try {
    const receipt = JSON.parse(await readFile(process.argv[3], 'utf8'));
    validate(receipt);
    for (const name of ['600x450', '900x500', '1100x720', '600x450-large']) console.log(`✅ ${name}: completion action visible and reachable`);
    console.log('✅ 600x450: real completion click closed setup and saved complete');
    console.log('PASS: beta7_setup_layout — measured geometry, hit testing and completion');
  }
  catch (error) { console.error(`FAIL: beta7_setup_layout — ${error.message}`); process.exit(1); }
  process.exit(0);
}

const work = await mkdtemp(join(tmpdir(), 'beta7-layout-'));
const output = join(work, 'bundle.js');
await build({ entryPoints: ['verify/beta7_setup_layout.fixture.tsx'], bundle: true,
  platform: 'browser', format: 'esm', jsx: 'automatic', outfile: output,
  plugins: [{ name: 'isolated-setup-platform', setup(builder) {
    builder.onResolve({ filter: /services\/firstLaunch$/ }, () => ({ path: 'diagnostics', namespace: 'fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: `
      const large = new URLSearchParams(location.search).has('large');
      const result = { providerConnected: true, openClawState: large ? 'broken' : 'ready', diagnostic: large ? 'Optional diagnostic failed. This must not hide the completion action. '.repeat(8) : undefined, permissions: ['notifications','accessibility','screenRecording','camera','microphone','automation','fullDiskAccess'].map(id => ({id,state:'notDetermined',granted:false})), vault: {pid:1,readCalls:1,writeCalls:0,legacyReadCalls:0} };
      export const runFirstLaunchDiagnostics = async progress => { progress?.(result); return result; };
      export const repairFirstLaunchOpenClaw = runFirstLaunchDiagnostics;
      export const openPermissionSettings = async () => {};
    ` }));
  } }],
});
const html = '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="/bundle.css"><div id="root"></div><script type="module" src="/bundle.js"></script>';
createServer(async (req, res) => {
  const path = new URL(req.url, 'http://localhost').pathname;
  if (path === '/receipt' && req.method === 'POST') {
    let body = '';
    for await (const chunk of req) { body += chunk; if (body.length > 50000) { res.writeHead(413).end(); return; } }
    try {
      const receipt = { revision, results: JSON.parse(body) }; validate(receipt);
      await writeFile(join(work, 'receipt.json'), JSON.stringify(receipt, null, 2));
      console.log(`PASS: beta7_setup_layout — receipt ${join(work, 'receipt.json')}`);
      res.end('PASS');
    } catch (error) { res.writeHead(400).end(error.message); }
    return;
  }
  if (path === '/bundle.js' || path === '/bundle.css') {
    res.setHeader('Content-Type', path.endsWith('.js') ? 'text/javascript' : 'text/css');
    res.end(await readFile(join(work, path.slice(1))));
  } else if (path === '/' || path === '/case') {
    res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end(html);
  } else { res.statusCode = 404; res.end(); }
}).listen(1437, '127.0.0.1', () => console.log('Layout fixture: http://127.0.0.1:1437'));

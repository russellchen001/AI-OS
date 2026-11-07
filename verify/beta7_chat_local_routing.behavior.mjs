import { build } from 'esbuild';
import assert from 'node:assert/strict';

const calls = [];
const listeners = new Map();
globalThis.window = globalThis;
globalThis.__routingFixture = {
  listen: async (name, callback) => { listeners.set(name, callback); return () => listeners.delete(name); },
  invoke: async (command, args) => {
    calls.push({ command, args });
    if (command === 'execute_ai_center') return { text: '{"taskType":"DO"}' };
    if (command === 'submit_chat_task') return { taskId: 'isolated-task', status: args.request.taskType === 'ASK' ? 'READY' : 'PLANNING' };
    if (command === 'execute_ai_center_stream') {
      const { operationId } = args.input;
      listeners.get('ai-center://chunk')({ payload: { operationId, text: 'A hardware recommendation from the selected model.' } });
      listeners.get('ai-center://done')({ payload: { operationId, cancelled: false, metadata: { providerId: 'omlx', modelId: 'Qwen2.5-7B-Instruct-4bit' } } });
      return;
    }
    throw new Error(`Unexpected dependency: ${command}`);
  },
};
const bundle = await build({
  stdin: { contents: 'export {classifyAutomaticIntent} from "./src/services/intentRouting"; export {streamThroughAiCenter} from "./src/services/aiCenter"; export {submitChatTask} from "./src/services/tasks";', resolveDir: process.cwd() },
  bundle: true, platform: 'node', format: 'esm', write: false,
  plugins: [{ name: 'isolated-platform', setup(builder) {
    builder.onResolve({ filter: /^(@tauri-apps\/api\/(core|event)|\.\/providers|\.\/aiCenterObservability)$/ }, args => ({ path: args.path, namespace: 'fixture' }));
    builder.onLoad({ filter: /.*/, namespace: 'fixture' }, args => ({ contents:
      args.path.endsWith('/core') ? 'export const invoke=(...a)=>globalThis.__routingFixture.invoke(...a);'
      : args.path.endsWith('/event') ? 'export const listen=(...a)=>globalThis.__routingFixture.listen(...a);'
      : args.path === './providers' ? 'export const listProviderInstances=()=>[]; export const reloadProviderInstances=async()=>{};'
      : 'export const enrichInvocationPricing=x=>x; export const recordInvocationSuccess=()=>{};'
    }));
  } }],
});
const service = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const original = '我在悉尼,你能帮我配一台主机作为本地ai 算力部署吗';
const selected = { providerId: 'omlx', providerInstanceId: 'local-test', modelId: 'Qwen2.5-7B-Instruct-4bit', label: 'oMLX · Qwen2.5-7B-Instruct-4bit' };
for (const choice of [undefined, selected]) {
  calls.length = 0;
  const intent = await service.classifyAutomaticIntent(original, choice);
  assert.equal(intent.taskType, 'ASK');
  const task = await service.submitChatTask(original, intent.taskType);
  assert.equal(task.status, 'READY');
  const stream = service.streamThroughAiCenter([{ role: 'user', content: original }], choice, () => {});
  assert.ok((await stream.result).response.text);
  const request = calls.find(c => c.command === 'execute_ai_center_stream').args.input;
  assert.equal(request.routeMode, choice ? 'manual' : 'auto');
  assert.deepEqual(request.manualCandidate, choice);
  assert.deepEqual(calls.map(c => c.command), ['submit_chat_task', 'execute_ai_center_stream']);
  console.log(`✅ ${choice ? 'Manual Qwen' : 'Auto'}: consultation → ASK → selected AI stream; no classifier or OpenClaw invocation`);
}
for (const prompt of ['在主机上安装 OpenClaw', '帮我查看这台电脑的磁盘空间', '在桌面新建一个文件夹', '帮我购买你推荐的主机配置', '按这份配置下单']) {
  calls.length = 0;
  assert.equal((await service.classifyAutomaticIntent(prompt, selected)).taskType, 'DO');
  if (prompt.includes('购买') || prompt.includes('下单')) assert.equal(calls.length, 0);
}
assert.equal((await service.classifyAutomaticIntent('如果以后我想购买这些配置，应该怎么办？', selected)).taskType, 'ASK');
assert.equal((await service.classifyAutomaticIntent('购买主机有什么建议？', selected)).taskType, 'ASK');
console.log('✅ Explicit system operations remain DO');
delete globalThis.__routingFixture;
console.log('PASS: beta7_chat_local_routing (real services, isolated platform fixtures)');

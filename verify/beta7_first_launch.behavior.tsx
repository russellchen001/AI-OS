import React, { act, useState } from "react";
import { Window } from "happy-dom";
import assert from "node:assert/strict";
import FirstLaunchOnboarding from "../src/components/FirstLaunchOnboarding";

const dom = new Window({ url: "http://beta7.test" });
for (const key of ["window", "document", "localStorage", "navigator", "Event", "HTMLElement"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: (dom as unknown as Record<string, unknown>)[key] });
}
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const { createRoot } = await import("react-dom/client");
let connected = false;
let diagnosticFailure = false;
let completed = 0;
let optionalPending = false;
const optionalResolvers: Array<() => void> = [];
Object.assign(globalThis, { __beta7Setup: { check: async (progress?: (value: unknown) => void) => {
  if (diagnosticFailure) throw new Error("isolated diagnostic failure");
  const result = { providerConnected: connected, openClawState: "notInstalled", permissions: [] };
  progress?.(result);
  if (optionalPending) await new Promise<void>(resolve => optionalResolvers.push(resolve));
  return result;
} } });
const key = "ai-os.beta7.first-launch";
const host = document.createElement("div");
document.body.append(host);
let root = createRoot(host);
function Harness() {
  const [page, setPage] = useState("Workspace");
  return <><button onClick={() => setPage("Workspace")}>Workspace</button><p>{page}</p>
    <FirstLaunchOnboarding workspaceActive={page === "Workspace"} onOpenMyAi={() => setPage("My AI")} onOpenAgents={() => setPage("Agents")} onStartChat={() => { completed++; setPage("Workspace"); }} />
  </>;
}
async function mount(progress: string | null) {
  await act(async () => root.unmount());
  localStorage.clear();
  localStorage.setItem("openai-connection-fixture", "persisted");
  if (progress !== null) localStorage.setItem(key, progress);
  root = createRoot(host);
  await act(async () => { root.render(<Harness />); });
}
async function click(label: string) {
  const button = [...host.querySelectorAll("button")].find(b => b.textContent === label);
  assert.ok(button, `missing button: ${label}`);
  assert.equal(button.disabled, false, `disabled button: ${label}`);
  await act(async () => button.click());
}
function text() { return host.textContent ?? ""; }

await mount(null);
assert.ok(text().includes("Step 1 of 3"));
await click("Continue");
assert.ok(text().includes("Step 2 of 3"));
await click("Open My AI");
assert.equal(host.querySelector('[role="dialog"]'), null);
connected = true;
await act(async () => window.dispatchEvent(new Event("ai-os:providers-changed")));
assert.equal(host.querySelector('[role="dialog"]'), null, "connection must not cover My AI");
assert.equal(localStorage.getItem(key), "2");
await click("Workspace");
assert.ok(text().includes("Step 3 of 3"));
assert.equal(host.querySelector("#first-launch-title")?.textContent, "Step 3 of 3 · 完成设置");
assert.ok(text().includes("这是设置的最后一步"));
assert.equal(completed, 0);
await click("Manage Agents");
assert.equal(host.querySelector('[role="dialog"]'), null);
await click("Workspace");
assert.ok(text().includes("Step 3 of 3"));
await click("完成设置，开始聊天");
assert.equal(localStorage.getItem(key), "complete");
assert.equal(completed, 1);
assert.equal(host.querySelector('[role="dialog"]'), null);
assert.equal(localStorage.getItem("openai-connection-fixture"), "persisted");
console.log("✅ Complete welcome → My AI connection → Workspace → Step 3 → explicit completion flow");

await mount("complete");
await act(async () => window.dispatchEvent(new Event("ai-os:providers-changed")));
assert.equal(host.querySelector('[role="dialog"]'), null, "completed setup must remain complete");
await act(async () => window.dispatchEvent(new Event("ai-os:open-setup-diagnostics")));
assert.ok(text().includes("Step 3 of 3"));
assert.equal(localStorage.getItem(key), "complete");
console.log("✅ Completion survives remount/provider events; explicit diagnostics still opens");

connected = false;
await mount("1");
await click("Open My AI");
connected = true;
await click("Workspace");
assert.ok(text().includes("Step 3 of 3"));
await mount("2");
assert.ok(text().includes("Step 3 of 3"));
console.log("✅ Return and restart resume Step 3 even without a provider-change event");

connected = false;
await mount("1");
await click("Open My AI");
await click("Workspace");
assert.ok(text().includes("Step 2 of 3"));
diagnosticFailure = true;
await click("Check again");
assert.ok(host.querySelector('[role="alert"]'));
assert.equal(localStorage.getItem(key), "1");
assert.equal(completed, 1);
console.log("✅ Missing connection and diagnostic failures cannot complete setup");
diagnosticFailure = false;
optionalPending = true;
connected = false;
await mount("1");
await click("Open My AI");
connected = true;
await act(async () => window.dispatchEvent(new Event("ai-os:providers-changed")));
await click("Workspace");
assert.ok(text().includes("Step 3 of 3"), "optional pending diagnosis must not trap a connected user in Step 2");
await click("完成设置，开始聊天");
await act(async () => { optionalResolvers.splice(0).forEach(resolve => resolve()); });
assert.equal(host.querySelector('[role="dialog"]'), null, "late diagnostics must not reopen completed setup");
assert.equal(localStorage.getItem(key), "complete");
console.log("✅ Pending optional Keychain/Agent diagnosis cannot block Step 3 or reopen completed setup");
await act(async () => root.unmount());
await dom.happyDOM.close();
console.log("PASS: Beta 7 first-launch interactive behavior");

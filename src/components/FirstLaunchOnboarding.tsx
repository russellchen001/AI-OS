import { openPermissionSettings } from "../services/firstLaunch";
import { useFirstLaunch } from "../hooks/useFirstLaunch";

type Props = { workspaceActive?: boolean; onOpenMyAi: () => void; onOpenAgents: () => void; onStartChat: () => void };

export default function FirstLaunchOnboarding({ workspaceActive = true, onOpenMyAi, onOpenAgents, onStartChat }: Props) {
  const setup = useFirstLaunch(workspaceActive);
  if (!setup.visible) return null;

  const openMyAi = () => {
    setup.advance(1);
    setup.setVisible(false);
    onOpenMyAi();
  };
  const startChat = () => {
    setup.finish();
    onStartChat();
  };
  const openAgents = () => {
    setup.setVisible(false);
    onOpenAgents();
  };

  return (
    <div className="first-launch-backdrop" role="presentation">
      <section className="first-launch-card" role="dialog" aria-modal="true" aria-labelledby="first-launch-title">
        <div className="first-launch-content">
        <p className="first-launch-kicker">Beta 7 setup · Step {Math.min(setup.step + 1, 3)} of 3</p>
        {(setup.error || setup.diagnostics?.diagnostic) && <p role="alert">{setup.error || setup.diagnostics?.diagnostic}</p>}
        {setup.step === 0 ? <>
          <h1 id="first-launch-title">Welcome to AI‑OS</h1>
          <p>Connect one AI, review optional Mac permissions, then complete your first conversation.</p>
          <button type="button" onClick={() => setup.advance(1)}>Continue</button>
        </> : setup.step === 1 ? <>
          <h1 id="first-launch-title">Connect an AI</h1>
          <p>A user-authorized account, API key, or downloaded local model is required for chat.</p>
          <button type="button" onClick={openMyAi}>Open My AI</button>
          <button type="button" className="secondary-button" disabled={setup.busy} onClick={() => void setup.refresh()}>Check again</button>
        </> : <>
          <h1 id="first-launch-title">Step 3 of 3 · 完成设置</h1>
          <p>Optional permissions never block basic chat. Open System Settings only when you want a related capability.</p>
          <div className="first-launch-permissions">
            {setup.diagnostics?.permissions.map((item) => <button key={item.id} type="button" onClick={() => void openPermissionSettings(item.id)}><span>{item.id}</span><strong>{item.state}</strong></button>)}
          </div>
          <p>AI provider: {setup.diagnostics?.providerConnected ? "connected" : "connection needed"}</p>
          {setup.diagnostics?.providerConnected && <p role="status">AI 已连接。这是设置的最后一步；可选权限可以以后再开启。点击“完成设置，开始聊天”即可完成全部三步。</p>}
          <p>OpenClaw: {setup.diagnostics?.openClawState ?? "checking"} (optional)</p>
          {setup.diagnostics?.vault && <details><summary>Credential access diagnostics</summary><p>This app session (process {setup.diagnostics.vault.pid}): vault reads {setup.diagnostics.vault.readCalls}, writes {setup.diagnostics.vault.writeCalls}, legacy reads {setup.diagnostics.vault.legacyReadCalls}. These are storage accesses, not permission-window counts. Run diagnostics again to refresh.</p><p>Metadata-only logs: ~/Library/Logs/AI OS/vault-access-*.jsonl. No passwords or tokens are included.</p></details>}
          <div className="first-launch-actions">{!setup.diagnostics?.providerConnected && <button type="button" className="secondary-button" onClick={openMyAi}>Open My AI</button>}<button type="button" className="secondary-button" disabled={setup.busy} onClick={() => void setup.refresh()}>Run diagnostics</button>{setup.diagnostics?.openClawState === "broken" && <button type="button" className="secondary-button" disabled={setup.busy} onClick={() => void setup.repairOpenClaw()}>Repair OpenClaw</button>}<button type="button" className="secondary-button" onClick={openAgents}>Manage Agents</button></div>
        </>}
        </div>
        {setup.step === 2 && <div className="first-launch-footer">
          <button type="button" className="action-button" disabled={!setup.diagnostics?.providerConnected} onClick={startChat}>完成设置，开始聊天</button>
        </div>}
      </section>
    </div>
  );
}

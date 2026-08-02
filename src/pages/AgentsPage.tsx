import { useState, type FormEvent } from "react";
import {
  HERMES_AGENT_TEMPLATE,
  deleteCustomAgent,
  loadAgentRegistry,
  saveCustomAgent,
} from "../services/agentRegistry";
import type { AgentAdapterKind, AgentRecord } from "../types/agent";

type AgentsPageProps = {
  onMessage: (message: string) => void;
};

function AgentsPage({ onMessage }: AgentsPageProps) {
  const [agents, setAgents] = useState<AgentRecord[]>(loadAgentRegistry);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("Hermes Agent");
  const [adapterKind, setAdapterKind] = useState<AgentAdapterKind>("hermes-api");

  function deleteAgent(agent: AgentRecord) {
    if (agent.builtIn) return;
    const nextAgents = deleteCustomAgent(agent.id);
    setAgents(nextAgents);
    onMessage(agent.name + " was deleted.");
  }

  function addAgent(event: FormEvent) {
    event.preventDefault();
    const normalizedName = name.trim();
    if (!normalizedName) return;

    const template =
      adapterKind === "hermes-api"
        ? HERMES_AGENT_TEMPLATE
        : { ...HERMES_AGENT_TEMPLATE, id: crypto.randomUUID(), adapterKind };

    const next: AgentRecord = {
      ...template,
      id: adapterKind === "hermes-api" ? "hermes" : crypto.randomUUID(),
      name: normalizedName,
      description:
        adapterKind === "hermes-api"
          ? "Hermes Agent execution adapter"
          : "Custom execution agent",
    };

    setAgents(saveCustomAgent(next));
    setAdding(false);
    onMessage(`${normalizedName} was added. Configure its connection before enabling it.`);
  }

  return (
    <section className="agents-page">
      <header className="agents-header">
        <div>
          <p className="settings-kicker">Settings · Execution</p>
          <h1>Agents</h1>
          <p>Choose which execution agents AI‑OS may use and review their capabilities.</p>
        </div>
        <button type="button" className="add-provider-button" onClick={() => {
          setName("Hermes Agent");
          setAdapterKind("hermes-api");
          setAdding(true);
        }}>
          <span>+</span> Add Agent
        </button>
      </header>

      <div className="agent-record-list">
        {agents.map((agent) => (
          <article key={agent.id} className="agent-record-card">
            <div className="agent-record-heading">
              <div className="agent-record-mark">{agent.name.slice(0, 1)}</div>
              <div>
                <h2>{agent.name}</h2>
                <p>{agent.description}</p>
              </div>
              <div className="agent-record-status">
                {agent.isDefault && <span>Default</span>}
                <span className={agent.connectionState === "ready" ? "agent-ready" : ""}>
                  {agent.connectionState === "ready" ? "Ready" : "Setup required"}
                </span>
              </div>
            </div>

            <div className="agent-record-details">
              <div>
                <h3>Capabilities</h3>
                <p>{agent.capabilities.map((item) => item.replaceAll("-", " ")).join(" · ")}</p>
              </div>
              <div>
                <h3>Permissions</h3>
                <p>{agent.permissions.join(" · ")}</p>
              </div>
            </div>

            <footer>
              <span>{agent.adapterKind.replaceAll("-", " ")}</span>
              <div className="agent-record-actions">
                <button type="button" onClick={() => onMessage(
                  agent.builtIn
                    ? "OpenClaw connection is managed through the existing Gateway settings."
                    : `${agent.name} requires its adapter connection before it can run tasks.`,
                )}>
                  Manage
                </button>
                {!agent.builtIn && (
                  <button type="button" onClick={() => deleteAgent(agent)}>
                    Delete
                  </button>
                )}
              </div>
            </footer>
          </article>
        ))}
      </div>

      {adding && (
        <div className="provider-setup-backdrop" role="presentation">
          <form className="provider-setup-dialog agent-add-dialog" onSubmit={addAgent}>
            <header>
              <div className="provider-setup-mark">A</div>
              <div><p>Agent Registry</p><h2>Add an Agent</h2></div>
              <button type="button" className="provider-setup-close" aria-label="Close Agent setup" onClick={() => setAdding(false)}>×</button>
            </header>
            <div className="provider-setup-body">
              <label className="agent-form-field">
                <span>Name</span>
                <input value={name} onChange={(event) => setName(event.target.value)} />
              </label>
              <label className="agent-form-field">
                <span>Agent type</span>
                <select value={adapterKind} onChange={(event) => {
                  const nextKind = event.target.value as AgentAdapterKind;
                  setAdapterKind(nextKind);
                  setName(nextKind === "hermes-api" ? "Hermes Agent" : "Custom Agent");
                }}>
                  <option value="hermes-api">Hermes Agent</option>
                  <option value="custom">Custom Agent</option>
                </select>
              </label>
              <div className="provider-security-note">
                <span>i</span>
                <p><strong>Permissions stay visible</strong><small>AI‑OS records capabilities and permissions before an Agent can be enabled.</small></p>
              </div>
            </div>
            <footer>
              <button type="button" className="provider-setup-cancel" onClick={() => setAdding(false)}>Cancel</button>
              <button type="submit" className="provider-setup-continue">Add Agent <span>→</span></button>
            </footer>
          </form>
        </div>
      )}
    </section>
  );
}

export default AgentsPage;

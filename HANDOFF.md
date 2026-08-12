# AI-OS — Current State

> Updated: 2026-08-11
> This file is the single source of truth for current repository state.
> Historical detail lives in `docs/archive/HANDOFF_HISTORY.md`.

---

## What this product is

AI-OS is a local-first personal AI operating system. The user says what they
want in plain language; AI-OS works out how to do it and gets it done.

Work is carried out by **Agents** — local executors that can actually touch the
computer, the internet, and connected services. Agents are the hands. The AI
models behind them, managed by AI Center, are the brain. Models are replaceable;
the product does not depend on any single provider.

Two capabilities are the reason this product exists, and neither is a chatbot
feature:

**AI Council（AI 智囊团）** — for any objective, a Chief of Staff works out which
models should be involved, assembles them into an advisory team, runs the
discussion, and produces a recommendation. Other products stop at the report.
Here the recommendation is meant to become work: the conclusion is handed to
Task Engine as a Do task, Planner turns it into steps, the user confirms, and an
Agent executes it. **Advice that turns into action is the differentiator.**

**AI Arena（AI 竞技场）** — multiple AIs interact under shared rules: debate,
social-deduction games such as Werewolf, collaboration, competition, role play,
simulation, or just talking to each other. Model comparison is one mode, not the
point. Arena is where the user watches AI behave, not where the user gets work
done.

Everything else in the roadmap exists to make these two possible and reliable.

---

## Agent roadmap

**v1.0 — OpenClaw only.**

OpenClaw is the single operational execution Agent for v1.0. Hermes and Custom
Agent records exist in the Registry but are non-operational placeholders. All
non-built-in agents are deletable; OpenClaw is built-in protected.

**v2.0 — Hermes leads, OpenClaw assists.**

The intended v2.0 arrangement, recorded now so the architecture does not close
it off:

- **Hermes** becomes the primary Agent. It is a growth-type agent — the more it
  is used, the better it understands the user. It owns continuity and judgement.
- **OpenClaw** becomes Hermes's assistant, used for what it is best at:
  connecting to and operating external systems and services.
- **WorkBuddy** and other agents may follow.

Design constraint this places on v1.0 work: the Runtime-to-Agent boundary must
stay a general adapter contract, not an OpenClaw-shaped one. Adding a second
Agent must not require changes to Task Engine, Planner, or Runtime.

---

## Open design gap — Council to execution

**Status: not yet specified. Decide before P16 implementation begins.**

AI Council currently ends at a recommendation. The path from recommendation to
executed work is undefined, which means P16 as specified would produce a
report-writing feature and the user would have to restate the plan manually to
get it done.

The intended path:

```text
AI Council
  ↓  recommendation
Task Engine        creates a Do task with the recommendation as context
  ↓
Planner            decomposes it into executable steps
  ↓
User confirmation  required — Council plans may include irreversible actions
  ↓
Runtime → Agent    executes
```

Things to settle when this is specified:

- Who converts a prose recommendation into a task objective — Council, Task
  Engine, or Planner
- What the confirmation surface looks like, and which step granularity the user
  approves
- Whether a Council plan can be saved and re-run later
- What happens when a step fails midway through a Council-originated plan

---

## Repository state

| | |
|---|---|
| Branch | `feature/p13-ai-center` |
| HEAD | AC-BACKEND-1 completion commit (current HEAD) |
| Latest tag | `p13-m5-complete` |
| Working tree | Clean after AC-BACKEND-1 completion commit |
| Active phase | AC-BACKEND-1 completed; next AC-BACKEND-2 |

---

## Completed

| Phase | Scope | Evidence |
|---|---|---|
| P9 | Runtime foundation: lifecycle, executor, operations, scheduler, recovery | `src-tauri/src/runtime/` |
| P10 | Task Engine and Planner, through P10-M13 | `1d173be`, tag `v0.11.0` |
| P11 | OpenClaw integration: execution contract, gateway adapter, permission boundary, event forwarding | `1d173be`, tag `v0.11.0` |
| P12 | Skill Framework: registry, discovery, manifests, permission model, lifecycle | Completed; recorded retroactively |
| P13-M1 | Canonical Provider domain and adapter registry | |
| P13-M2 | Provider credential and account connection infrastructure | |
| P13-M3 | Local/cloud execution, model discovery, Local First routing | |
| P13-M4 | Provider-independent observability, cost and latency metadata | tag `p13-m4-complete` |
| P13-M5 | Shared multi-model invocation | `1e01cd2`, tag `p13-m5-complete` |
| AC-BACKEND-0 | Legacy MultiLLM removal; provider HTTP invocation moved to Rust | `verify/verify_p13_ai_center_migration.sh` |
| AC-BACKEND-1 | Auto ordering, Local First preference, and fallback selection moved to Rust | `verify/verify_ac_backend_1_routing.sh` |
| Provider setup dialog | Restored setup/manage dialog styles removed during P13 migration | `verify/verify_provider_setup_dialog.sh` |
| UI Refactor | White-first workspace across Chat, Sidebar, My AI, Agents, Arena, Council, Artifacts, Settings | |
| Agent Registry | Non-built-in agents (including Hermes and Custom Agents) are deletable; OpenClaw stays built-in protected | `c88f7b9`, `9b54873` |
| Legacy UI Step1 | Removed obsolete page entries from App routing while preserving Models and MCP entries | `verify/verify_legacy_ui_step1.sh` |
| Legacy UI Step2 | Removed obsolete PageName and Settings navigation entries while preserving new workspace structure | `verify/verify_legacy_ui_step2.sh` |
| Ollama streaming fix | Increased local Ollama generation capacity and timeout handling for long AI Center streaming responses | `43da32f` |
| Chat workspace layout fix | Adjusted Chat message container width and spacing so long responses stay inside the workspace boundary | `6192b6a` |
| P14 Memory Retrieval | User memories are injected as hidden system context for ordinary Chat requests without entering conversation history | `verify/verify_p14_memory_retrieval.sh` |
| P14 General Memory Policy | Structured language, response detail, currency, and budget defaults with request-only overrides | `verify/verify_p14_general_memory_policy.sh` |
| AI Center End-to-End QA | Auto/Local First, manual selection, multi-model, streaming, cancellation, fallback, and analytics verified | `verify/verify_ai_center_e2e_qa.sh` |

P14 General Memory Policy behavioral QA passed:

- Language: long-term Chinese, current English, then restored Chinese
- Response detail: long-term concise, current detailed, then restored concise
- Budget and currency: long-term AUD 500, current AUD 1000, then restored AUD 500

AI Center End-to-End QA passed:

- Auto routing selected local Ollama `qwen2.5:7b` when available, preserving Local First
- Manual OpenAI `gpt-5.6-sol` selection executed without silent fallback
- P13-M5 shared multi-model invocation evidence remains valid
- Real Ollama streaming completed successfully
- Cancelling a long OpenAI response stopped output immediately with no later continuation or fallback, and restored input readiness
- Auto fallback selected cloud Anthropic `sonnet` only while Ollama was unavailable, then returned to local routing after recovery
- Analytics records included invocation ID, auto route mode, cloud source, Anthropic provider/model, four attempts, and fallback state

### Connected AI providers

| Provider | Method | Status |
|---|---|---|
| Google Gemini | Native PKCE OAuth | Verified — 50 models |
| OpenAI Codex / ChatGPT | Official Codex OAuth, port 1455 | Verified — 7 models, GPT-5.6-Sol |
| xAI Grok | RFC 8628 device auth + API key | Verified — grok-4.5 |
| Anthropic Claude Code | Official `claude` CLI as local proxy | Verified — Claude Pro |
| Ollama (local) | Local runtime | Verified — qwen2.5:7b, qwen3:8b, deepseek-r1:8b |
| DeepSeek | API key | Implemented, key not yet entered |
| OpenRouter | PKCE + API key | Implemented, no account yet |
| Kimi Code | RFC 8628 device auth | Implemented, no account yet |
| Meta Model API | Developer key | Implemented, no account yet |

Credentials are stored in macOS Keychain only. Never in browser state, Provider
config, or the repository.

---

## In progress

AC-BACKEND-1 is complete. Rust owns Auto ordering, Local First preference, and
fallback selection while the existing frontend observability path remains in
place until AC-BACKEND-2.

---

## Next

1. AC-BACKEND-2

---

## Rejected — do not propose again

| Decision | Reason |
|---|---|
| Third-party Anthropic OAuth client | Anthropic publishes no public third-party client registration. Use the official `claude` CLI as a local authorization proxy instead. |
| Reading or persisting Claude Code's OAuth token | Claude Code owns its own credential. AI-OS stores only the local connection and model preference. |
| Consumer Meta AI login for third-party inference | Not authorized by Meta. Only the Meta Model API developer key is supported. |
| Top-right `Add AI` button | Duplicated the `Other AI` catalog entry. The catalog entry is the single generic Provider path. |
| Treating every Ollama model-discovery failure as "no models installed" | Misleading. The card now distinguishes not-installed, stopped, starting, running-without-models, and ready. |
| Auto fallback after output has started | Fallback only fires when a candidate fails *before* producing output. Explicit model selection never silently switches. |
| Hard-coding one game into Arena architecture | Werewolf and similar are representative cases, not the architecture. |

---

## Technical decisions

**AI Center**

- Shared Multi-Model Invocation is the canonical execution layer
- Each participant owns an independent `operationId`
- Multi-model execution is fully concurrent; participants never use Auto fallback
- Participant ordering is deterministic after normalization; duplicates removed pre-execution
- Aggregated results preserve per-model P13-M4 metadata
- Auto routing is Local First: local Ollama models are attempted before connected cloud defaults
- Explicit manual model selection never silently falls back
- Fallback occurs only before output has started
- Cancellation stops the current stream without later output continuation

**Memory**

- Ordinary Chat requests load long-term `user` memories and inject them as outbound message zero
- Memory Policy is separate from Memory Storage; `src/services/memoryPolicy.ts` owns parsing, resolution, and outbound constraints while SQLite CRUD remains unchanged
- ChatPage only retrieves Memory, calls the Memory Policy resolver, and assembles outbound context; it does not own concrete Policy rules
- Policy priority is current request override, then long-term Memory, then system default
- Resolved runtime policy enters only the outbound request and is also applied to a temporary clone of the current outbound user message for model compatibility
- UI and saved conversation user content always retain the original text; request overrides are never written into conversation history or long-term Memory and do not affect the next request
- Ordinary factual memories remain available as background context without forced structured parsing
- Explicit “记住…” commands remain a local save-and-confirm path and do not call AI Center
- Initial structured policies are `language`, `response_detail`, `currency`, and `budget`
- Conversation-scoped persistent overrides are not implemented
- Streaming Provider input accepts `system` messages; Anthropic combines them into the Messages API top-level `system` field and excludes them from `messages`

**Migration**

- Legacy MultiLLM service has been removed
- Provider Registry is the single source of provider identity
- Council and My AI consume AI Center provider models
- No new feature should introduce MultiLLM-specific storage keys or execution paths

**Removing legacy code**

- CSS class names have no compile-time link to TypeScript, so deleting a
  feature can silently remove styles the surviving UI still uses; the build
  will still pass
- When removing a module, grep `App.css` for its class names before and after
  and confirm every affected screen renders

**Credentials**

- All API keys and OAuth tokens pass to the native security layer and live in macOS Keychain
- A newly stored credential is removed automatically if live verification fails
- Claude Code subscription execution stays routed separately from Anthropic API-key execution

**Observability**

- AI Center owns the canonical invocation record; Workspace renders it but must
  not reconstruct routing decisions
- Records include route mode, execution source, latency, ordered attempts, and fallback state
- Records exclude prompts, outputs, raw Provider responses, credentials, and tokens

---

## Decided — 2026-08-02

**AI Center routing moves to the Rust backend.**

Provider invocation, Auto ordering, fallback selection, and credential use
belong in Rust. The frontend submits a request and renders the result. It must
not select providers, order candidates, decide fallback, or hold credentials in
memory.

Rationale: background and scheduled execution cannot depend on an open
application window; credentials must stay in the native security layer;
Runtime, Memory (P14), AI Council (P16), and AI Arena (P17) all need backend
access to AI Center.

Migration is incremental, not a rewrite. Each step ships its own acceptance
script and must not change observable behaviour:

This is architectural correction, not a P13 milestone. P13 is defined by the
Master Guide as M1-M5 and is complete. These steps are tracked as AC-BACKEND,
outside the phase numbering.

Provider HTTP invocation already moved to Rust as part of the legacy MultiLLM
removal (AC-BACKEND-0).

1. **AC-BACKEND-1 — Completed.** Rust owns Auto ordering, Local First
   preference, and fallback selection. The frontend sends an Auto-or-manual
   request and renders the selected result. Observability remains in the
   frontend until AC-BACKEND-2.
2. **AC-BACKEND-2 — Next.** Reduce `src/services/aiCenter.ts` to a thin
   call-and-render layer. Observability records are produced in Rust and
   passed up.

Constraints carried into the migration:

- Local First ordering, per-participant `operationId`, concurrent multi-model
  execution, and no-fallback-for-multi-model all survive unchanged
- Claude Code subscription execution stays routed separately from Anthropic
  API-key execution
- Fallback still fires only before output has started
- Invocation records still exclude prompts, outputs, credentials, and tokens

---

## Environment notes

- Repository is a Tauri app: Rust backend in `src-tauri/`, React/TypeScript frontend in `src/`
- Latest full validation: 394 Rust tests passing, `cargo check` passing with 4 pre-existing warnings, frontend production build passing
- Owner's machine is macOS; do not hard-code absolute user paths in this file

---

## Change log

<!-- ./done.sh appends here automatically -->

## 变更日志
- 2026-08-09 04:09  fix(myai): restore provider setup dialog styles lost in P13 migration
- 2026-08-09 05:23  fix(ui): restore chat, sidebar and markdown styles lost in P13 migration


## External Skill Architecture

### Agency Agents Skill Reference

Repository:
https://github.com/msitarzewski/agency-agents

Purpose:
- Use external Agent Skill packages to provide professional role definitions for AI Council.
- AI-OS should not maintain a duplicate internal agent talent library.
- Agent roles, expertise descriptions and workflow templates should come from installable Skills.

Architecture direction:

AI-OS owns:
- Skill discovery
- Skill loading
- Role selection
- Council orchestration
- Memory tracking
- Model assignment

Skills own:
- Agent role definitions
- Professional personas
- Expertise descriptions
- Workflow instructions
- Output standards

Initial reference Skill:
- Agency Agents

Important boundary:
- Agency Agents is a Skill resource, not a Runtime.
- AI-OS v1.0 execution remains OpenClaw-only.
- External Agent Skills provide Council roles only and do not introduce additional execution adapters.

Future Skill model:

AI-OS
 |
 Skill Runtime
 |
 Installed Skills
 |
 Agent Role Providers
 |
 AI Council
 |
 OpenClaw Execution

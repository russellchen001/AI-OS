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

The required v1.0 path:

```text
AI Council
  ↓  recommendation
Task Engine        creates a Do task with the recommendation as context
  ↓
Planner            decomposes it into executable steps
  ↓
User confirmation  required — Council plans may include irreversible actions
  ↓
Runtime → OpenClaw executes
```

Things to settle when this is specified:

- Who converts a prose recommendation into a task objective — Council, Task
  Engine, or Planner
- What the confirmation surface looks like, and which step granularity the user
  approves
- Whether a Council plan can be saved and re-run later
- What happens when a step fails midway through a Council-originated plan

---

## AC-EXEC-MODEL — completed 2026-08-23

**Status: completed.**

### Final architecture

Download execution uses dedicated OpenClaw execution agents selected by AI Center.

Current execution agents:

| Agent | Model | num_ctx | Skills | Tools | Used for |
| --- | --- | ---: | --- | --- | --- |
| `ai-os-files` | `ollama-ai-os/qwen3:4b-instruct` | 65536 | baidu-drive | exec, read | Filesystem scan/read/write/move |
| `ai-os-exec-standard` | `ollama/qwen3:8b` | 65536 | baidu-drive | exec, read | Download execution |

Filesystem operations remain on `ai-os-files`.

Download execution asks AI Center for eligible OpenClaw execution-agent ids and
uses them in preference order.

### Selection policy

1. Capability requirements are admission gates.
2. Each execution capability declares its own required context window.
3. `download.start` currently requires 65536 context, based on measured
   `baidu-drive` Skill execution requirements.
4. AI Center compares the requirement against the execution agent's configured
   effective context, not the model's theoretical maximum.
5. Agents that cannot satisfy the requirement are excluded.
6. Eligible candidates follow AI Center preference order.
7. Local First applies among eligible candidates.
8. Eligible cloud execution agents may follow local candidates.

Context requirements are capability-scoped rather than one global constant.
Future Skills may declare different context requirements without changing the
routing rule.

### Runtime fallback policy

Download Runtime consumes the ordered execution-agent candidate list.

A download may execute on at most two agents:

- preferred eligible agent;
- one fallback agent.

Fallback is allowed only when Runtime file verification rejects an otherwise
completed execution with the retryable file-verification `ProtocolFailure`.

Malformed requests, denied permissions, authentication failures, and other
non-file-verification failures stop immediately.

Each execution-agent attempt receives a distinct idempotency key containing the
execution id and agent id.

Runtime file verification remains authoritative. Agent self-report alone is
never sufficient for download success.

### Implementation

AI Center execution-agent selection is implemented in:

`src-tauri/src/providers.rs`

Responsibilities:

- capability-specific context requirements;
- effective context-window admission;
- execution-agent/model matching;
- AI Center preference ordering;
- returning agent ids without leaking routing internals.

Download fallback is implemented in:

`src-tauri/src/runtime/openclaw_gateway_adapter.rs`

Responsibilities:

- obtain ordered execution-agent candidates;
- preserve existing download routing;
- execute candidates sequentially;
- cap attempts at two;
- retry only the approved file-verification failure;
- preserve destination file verification.

Adding another eligible execution agent does not require Task Engine, Planner,
or capability-contract changes.

### Verification

Deterministic verification:

- `verify/verify_ac_exec_model_step1.sh`
- `verify/verify_ac_exec_model_step3a.sh`
- `verify/verify_ac_exec_model_step3b1.sh`
- `verify/verify_ac_exec_model_step3b2.sh`
- `verify/verify_ac_exec_model_step3b3.sh`
- `verify/verify_ac_exec_model_complete.sh`

Verified behavior:

- capability-specific context admission;
- AI Center candidate ordering;
- filesystem execution remains separate;
- downloads consume AI Center candidates;
- per-agent idempotency;
- retryable file-verification failure;
- sequential fallback;
- maximum two execution attempts;
- non-file-verification failures stop immediately;
- OpenClaw adapter tests pass;
- Rust compilation passes.

Real E2E verified 2026-08-23:

A Baidu share-link download ran through the app using
`ai-os-exec-standard`. The agent executed the provider Skill workflow and a
9.8 MB file landed in the selected destination.

### Technical decisions

Execution routing uses agent granularity rather than per-run model override.

An execution agent binds:

- model;
- configured context window;
- Skill allowlist;
- tool permissions;
- workspace.

Runtime does not hard-code provider selection by cloud-drive domain.

Runtime file verification must remain in place because model self-report cannot
be trusted as proof that a download actually completed.

### Constraints to preserve

- Filesystem operations stay on `ai-os-files`.
- Execution agents retain minimal Skill and tool exposure.
- Runtime file verification must not be removed.
- Runtime must not hard-code cloud-drive provider selection by domain.
- Do not reintroduce prose substring matching for machine error classification.
- Task Engine and Planner must not know individual execution-agent ids.

### Known non-blocking issues

- `bdpan download` may create a directory named after the downloaded file.
- Local 8B execution at 64K can be slow.
- Probabilistic real-model behavior remains a manual E2E confirmation rather
  than a deterministic CI assertion.

---
## Repository state

| | |
|---|---|
| Branch | `feature/p13-ai-center` |
| HEAD | P15 Local Model complete — Ollama runtime and My AI UI verified 2026-08-16 |
| Latest tag | `p13-m5-complete` |
| Baseline state | Working tree was clean at the stable baseline before this handoff update |
| Active phase | P15 Core Skills — File Management, Local Model Management, and Download Skill complete; remaining capability areas continue in P15. |

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
| AC-BACKEND-2 | Canonical AI Center invocation metadata moved to Rust | `verify/verify_ac_backend_2_observability.sh` |
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
| oMLX AI Center | Native oMLX provider, Keychain-backed Bearer auth, model discovery, My AI setup, Local First routing, and OpenAI-compatible streaming/non-streaming Chat execution | `verify/verify_omlx_ai_center_step1.sh`; automated provider suite and full Rust regression passed; real manual oMLX Chat UI E2E passed 2026-08-23 |
| oMLX local service UX | Connected oMLX instances move into On this Mac with runtime health, models, default model, refresh, start, and connection management; connected Apple Silicon installations auto-start when AI-OS opens | `verify/verify_omlx_ai_center_step1.sh`; 442 Rust tests and frontend production build passed; real Stopped → automatic Ready UI E2E passed 2026-08-23 |
| P15 Filesystem scan | Explicit PlanStep execution, one-time confirmation, permission enforcement, real OpenClaw `exec` scan, readable result rendering, Work-specific errors, and stable local message times | `verify/verify_p15_file_execution_contract.sh`, `verify/verify_p15_file_scan_confirmation.sh`; real UI E2E passed 2026-08-15 with `.DS_Store`, `Lable_副本.docx`, and `__副本.jpeg` |
| P15 Filesystem read | Explicit file picker and confirmation, real OpenClaw text read, MIME and size detection, 1 MB read limit, 64 KiB output limit, binary/unsupported handling, and readable Chat rendering | `verify/verify_p15_file_read.sh`; real UI E2E passed 2026-08-15 with repository `README.md` content |
| P15 Filesystem write | Explicit save-path selection and confirmation, real OpenClaw text creation, 4 KiB input limit, private 0600 permissions, and create-only/no-overwrite failure handling | `verify/verify_p15_file_write.sh`; isolated real OpenClaw create/no-overwrite smoke and real UI E2E passed 2026-08-15 with a 27-byte text file |
| P15 Filesystem move | Explicit source/destination selection and one-time confirmation, real OpenClaw move execution, no-overwrite behavior, absolute/different-path validation, fail-closed source/destination checks, and readable Chat rendering | `verify/verify_p15_file_move.sh`; isolated real OpenClaw smoke and real UI E2E passed 2026-08-15 moving `/private/tmp/ai-os-p15-ui-write-20260815.txt` to `/private/tmp/ai-os-p15-ui-move-20260815.txt` |
| P15 Local Model Core Skill | Ollama local model management through Runtime, including model list, inspect, pull, delete capabilities, Chat execution flow, My AI management UI, and unified Dialog interaction | `verify_p15_local_model_core_skill.sh`, `verify_p15_local_model_step1.sh`, `verify_p15_local_model_step2.sh`, `verify_p15_local_model_step3.sh`, `verify_p15_local_model_step4.sh`; completed 2026-08-16 |
| P15 Download Skill | Task/Plan/Runtime/OpenClaw execution; Direct HTTP, Web page, Thunder submission, aria2 tool routing, cloud-drive extension point, destination verification, safe errors, and readable Chat results | `verify/verify_p15_download_complete.sh`; real OpenClaw E2E passed 2026-08-21 |

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
| oMLX (local, Apple Silicon) | Local OpenAI-compatible API + API key | Verified — DeepSeek-R1-Distill-Qwen-7B-4bit discovered; authenticated SSE and real AI-OS Chat UI E2E passed 2026-08-23 |
| DeepSeek | API key | Implemented, key not yet entered |
| OpenRouter | PKCE + API key | Implemented, no account yet |
| Kimi Code | RFC 8628 device auth | Implemented, no account yet |
| Meta Model API | Developer key | Implemented, no account yet |

Credentials are stored in macOS Keychain only. Never in browser state, Provider
config, or the repository.

---

## In progress

P15 Core Skills remains active. File management, Local Model Management, and
Download Skill are complete. Download execution now remains inside the v1.0
Agent boundary: Task Engine → Planner → Runtime → Skill → OpenClaw → tool/Web.

All four Filesystem capabilities enter through Task Engine and Planner, resolve through the
Skill registry, execute through Runtime and the dedicated `ai-os-files`
OpenClaw worker, enforce one-time confirmation where required, normalize real
tool results, and render readable Chat output without exposing raw Gateway
history.

---

## Next

P15 Core Skills implementation order:

Completed:
1. File Management Skill
2. Local Model Management Skill
3. Download Skill

Download Skill architecture:
- Chat supplies only `source` and `destination`; it never selects a provider.
- Skill registry resolves Download to the common OpenClaw executor.
- Direct HTTP files use an OpenClaw `exec` tool workflow.
- HTML download pages use an OpenClaw Web workflow that reads the page, resolves
  its download link, and downloads the linked file.
- Thunder/ED2K use OpenClaw to submit to the installed Thunder application by
  bundle identity and URL scheme; submission does not imply completion.
- Magnet uses Thunder when available and otherwise routes to aria2; torrent and
  FTP route to the aria2 tool adapter. qBittorrent is not installed or required.
- Baidu links route through OpenClaw to the installed Baidu Netdisk application.
  Future cloud tools extend Download routing, not Task Engine or Planner.
- AI-OS verifies new files in the selected destination before reporting completed.

Next capability order:
4. Browser and Search Skill
5. NAS Management Skill
6. Document, Spreadsheet and Presentation Workflow Skill
7. Email and Calendar Skill
8. Smart Home and Device Control Skill
9. Local Generative Media Skill
10. Cognitive Distillation Foundation

Do not begin P16 until P15 capability foundations are implemented or explicitly deferred.

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
- Auto routing is Local First: connected oMLX models are attempted first, then local Ollama models, then connected cloud defaults
- oMLX is the preferred Apple Silicon local engine; machines without a connected oMLX instance naturally continue through Ollama without a separate platform-specific route
- oMLX uses the independent `omlx-local` Provider instance, Keychain-backed Bearer authentication, `/v1/models` discovery, and OpenAI-compatible Chat endpoints
- AI-OS auto-starts the installed oMLX macOS app only when an `omlx-local` instance is connected and the machine is Apple Silicon; unsupported machines retain Ollama without attempting oMLX startup
- My AI treats connected oMLX as a local service under On this Mac; it is removed from Add another Provider until disconnected
- The canonical AI Center streaming command owns oMLX candidate attempts, chunk emission, cancellation, and fallback decisions; the UI does not call oMLX directly
- OpenClaw execution agents and P15 local-model management remain Ollama-bound
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


**Download Skill**

- Download capability enters through Task Engine and Planner.
- Runtime resolves Download Skill through the Skill registry and the general
  Runtime-to-OpenClaw execution contract.
- Technical decision: Download execution belongs to OpenClaw; AI-OS owns
  Task/Plan/permissions/capability routing, not an independent downloader
  execution path.
- The former Runtime → aria2/Thunder executor and independent download-task store
  were removed. aria2 remains the non-interactive magnet/torrent/FTP tool OpenClaw
  can invoke; the GUI-only Thunder launcher was removed.
- Direct file URLs, Web pages, Thunder wrappers, magnet/torrent/FTP, and known
  cloud-drive sources are classified inside Download Skill, never ChatPage.
- A resource name without a URL is delegated to the main OpenClaw Agent, which
  searches for a working download source and must continue through download and
  destination-file verification without content, copyright, filename, or NSFW
  classification.
- Download execution uses the existing `ai-os-files` worker and its proven
  `ollama-ai-os/qwen3:4b-instruct` tool-calling model. The default main
  `qwen3:8b` produced no tokens or tool calls and hit OpenClaw's 180-second idle
  timeout in the first real Baidu E2E. Runtime model override was also rejected
  by OpenClaw policy, so Download never depends on that override.
- Direct and Web downloads must create a new file in the selected existing
  absolute destination before AI-OS returns `completed`.
- Technical decision: a Download result may only be `completed`; opening an
  external application or submitting a URL is never success. Every route must
  create a new file in the requested directory or fail.
- Download execution is content-neutral after the user confirms the PlanStep.
  The Agent performs file transfer and destination verification rather than
  inferring legality from filenames or replacing the user's decision. AI-OS does
  not implement adult-content recognition, copyright judgment, or content
  scanning in P15.
- Standard `thunder://` wrappers are decoded before routing. Magnet, torrent,
  and ED2K prefer an installed non-interactive Thunder Skill, CLI, or local API;
  opening the Thunder GUI is never accepted. If Thunder automation is absent,
  magnet/torrent may fall back to aria2 and ED2K may use another installed cloud
  offline-download Skill; otherwise ED2K fails with the real unsupported-tool
  reason. FTP continues through aria2.
- Cloud providers are not hard-coded in Runtime. OpenClaw first discovers an
  installed Download/cloud Skill that declares support for the supplied source;
  otherwise it falls back to the provider-neutral Browser flow. Provider login,
  OAuth, VIP sessions, and extraction codes belong to the installed Skill or
  persistent browser profile. Adding 115, Google Drive, or another provider must
  not require changes to Task, Planner, Runtime, or Download routing.
- The currently installed Baidu `baidu-drive` Skill and `bdpan` CLI remain the
  proven example of this contract. OAuth is completed once; normal downloads
  require no GUI interaction, and Runtime still requires a destination file.
- Web and cloud-share downloads use one provider-neutral OpenClaw Browser flow,
  not one adapter per website. It reuses an existing authenticated browser
  session, selects an authorized VIP/fast option when available, otherwise uses
  the free option, and handles JavaScript waits, buttons, hidden forms, and
  redirects without user clicks. Missing login is reported as authentication
  required; credentials are never requested or stored in Chat. Long transfers
  may run for up to four hours; only a completed destination file is success.
- Real official-CLI E2E on 2026-08-22 automatically resolved a public share,
  applied its extraction code, transferred it into the authorized app scope, and
  downloaded `EhViewer-2.0.2.2.apk` (27,747,451 bytes) to a temporary local
  destination without GUI interaction.
- Web E2E currently proves a normal HTML download link. JavaScript-heavy,
  authenticated, CAPTCHA, and interactive sites still depend on available
  OpenClaw Browser/cloud tools and remain unverified.
- Real E2E on 2026-08-21 proved Direct HTTP and HTML-link downloads through
  Task → Plan → Runtime → OpenClaw with byte-matching destination files. The
  earlier GUI-launch acceptance for Thunder/Baidu was invalidated and removed.
- `verify/verify_p15_download_complete.sh` also proves failure classification,
  readable Chat results, full Rust regression tests, frontend build, and cargo check.

**P15 Core Skills**

- Explicit Core Skill actions enter through Task Engine and Planner, resolve through the existing Skill registry, and execute through Runtime and OpenClaw
- One-time user confirmation is PlanStep-scoped and permits only the exact implemented `filesystem.scan`, `filesystem.read`, `filesystem.write`, `filesystem.move`, and `download.start` actions; it does not modify trusted automation
- Work/Core Skill errors use safe OpenClaw/Runtime reporting, while ASK errors retain AI Center/provider semantics
- `filesystem.scan` is an AI-OS capability identifier, not an OpenClaw Gateway RPC or tool id; the adapter translates it to the official `agent` / `agent.wait` path and reads the resulting session history
- Real `filesystem.scan` E2E passed through Task, Planner, Runtime, permission, OpenClaw agent, read-only `exec`, normalized output, and Chat UI on 2026-08-15
- `filesystem.read` always requires one-time confirmation, uses the same dedicated worker, detects MIME and size before reading, never sends unsupported binary content to Chat, and bounds text reads to 1 MB with at most 64 KiB returned
- `filesystem.write` always requires one-time confirmation, passes selected path and current text through the existing Plan/Runtime chain, limits UTF-8 content to 4 KiB, creates with 0600 permissions, and uses shell noclobber plus an existence check so overwrite attempts fail closed
- Real `filesystem.write` E2E passed through Task, Planner, Runtime, permission, OpenClaw agent, `exec`, normalized output, and Chat UI on 2026-08-15; the UI-created file contained the exact 27-byte requested text
- `filesystem.move` always requires one-time confirmation, requires absolute and different source/destination paths, forbids overwrite, verifies the source disappears and destination appears after `mv`, and reports source-missing, destination-existing, or execution failure without claiming success
- Real `filesystem.move` E2E passed through Task, Planner, Runtime, permission, OpenClaw agent, `exec`, normalized output, and Chat UI on 2026-08-15
- File execution uses the dedicated `ai-os-files` OpenClaw worker with no inherited skills or workspace bootstrap context and an isolated Ollama provider; `main` remains the default personal agent
- OpenClaw permission and confirmation remain authoritative; the dedicated worker does not enable trusted automation or bypass tool policy

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
2. **AC-BACKEND-2 — Completed.** Rust supplies canonical route mode, source,
   ordered attempts, outcomes, safe error categories, latency, token estimates,
   and fallback state. The frontend is a call-and-render layer that enriches
   pricing from the existing `modelPricing` configuration and persists
   Analytics to localStorage. Invocation records exclude prompt, output,
   credential, API key, OAuth token, access token, and refresh token contents.
   Multi-model concurrency, explicit choices, deterministic ordering, and
   per-participant operation IDs remain unchanged.

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
- Preserve Agency Agents as a candidate source and design reference for the P16
  AI Council expert Profile/Role Library and dynamic expert-team assembly.
- Do not integrate Agency Agents during P15 unless the Master Guide roadmap is
  explicitly revised.

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
- Before P16 implementation begins, specify the full Council-to-execution
  contract: Council recommendation → Task Engine → Planner → user confirmation
  → Runtime → OpenClaw.
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

## External Orchestration Architecture Reference

### Paperclip Reference

Repository:
https://github.com/paperclipai/paperclip

Local reference checkout:
- `../paperclip`
- Reference commit when recorded: `f0e6c0f54`

Purpose:
- Preserve Paperclip as an external architecture and implementation reference
  for AI-OS task orchestration and execution governance.
- Study its goal hierarchy, task lifecycle, delegation, approvals, budget
  controls, heartbeat scheduling, audit trail, persistent execution state, and
  runtime adapter boundaries before P16 implementation begins.
- Use Paperclip to inform the unresolved Council-to-execution contract rather
  than introducing a second AI-OS orchestration system.
- Do not integrate or run Paperclip as part of P15 Core Skills.
- Do not make Paperclip an AI-OS Runtime or execution Agent.

Architecture boundary:

AI-OS owns:
- AI Council
- Task Engine
- Planner
- User confirmation and approvals
- Runtime policy
- Memory
- AI Center
- OpenClaw execution integration

Paperclip is reference material for:
- Goal-to-task decomposition patterns
- Task ownership and delegation
- Approval and governance flows
- Budget and execution limits
- Heartbeat / resumable work patterns
- Persistent task state
- Audit and observability patterns
- Runtime adapter separation

Important boundary:
- Paperclip is an orchestration reference, not a dependency decision.
- AI-OS must not duplicate Paperclip wholesale or introduce its runtime model
  without an explicit architecture decision.
- AI-OS v1.0 execution remains OpenClaw-only.
- Paperclip support for other agent runtimes does not expand the AI-OS v1.0
  Agent scope.
- Before P16 coding begins, compare the Paperclip reference against the required
  AI-OS contract:
  Council recommendation → Task Engine → Planner → user confirmation
  → Runtime → OpenClaw.
- Adopt only mechanisms that fit the AI-OS architecture and Master Guide.

### Local external reference repositories

These repositories are intentionally kept outside the `dashboard` repository
and must not be committed into the AI-OS application source tree:

- `../agency-agents` — Agency Agents role/Profile reference, commit `ebe9c99`
- `../paperclip` — orchestration architecture reference, commit `f0e6c0f54`

They are source references only. P15 must not depend on either repository at
runtime.
- 2026-08-13 22:49  docs: record external agent architecture references
- 2026-08-14 01:29  feat: add p15 core skill execution contract
- 2026-08-14 01:53  feat: add explicit folder scan work action


## P15 Browser/Search

Status:
- Completed.

Implemented:
- Browser Skill contract (`browser.search`, `browser.control`).
- MCP runtime execution path.
- Browser Provider abstraction layer.
- Browser Provider Registry.
- Browser Runtime Dispatcher.
- MCP Browser Provider bridge.

Architecture decision:
- Browser capability does not directly depend on a specific browser tool.
- MCP Browser is the first provider implementation.
- Future browser integrations (Chrome DevTools MCP, BrowserSkill, etc.) must be added as providers without changing Browser Skill contracts.

Execution path:

Planner
→ Skill Resolver
→ Browser Skill
→ Browser Runtime Dispatcher
→ Browser Provider Registry
→ MCP Browser Provider
→ MCP Runtime
→ tools/call

Verification:
- verify_p15_browser_complete.sh


---

## Future roadmap additions

### P15 additions

**Local Generative Media Skill**

- Local-first image/video generation capability
- Draw Things and ComfyUI style local provider support
- Model and LoRA orchestration
- Prompt generation and iterative refinement
- Cloud generation only when local capability is unavailable

**Cognitive Distillation Foundation**

- Evidence-based cognitive model extraction
- Decision pattern modeling
- Reasoning framework representation
- Foundation for future Strategic Intelligence


### P16 direction

**Strategic Intelligence and Agent Expansion**

Planned capabilities:

- Cognitive Simulation Engine
- Strategic Council enhancement
- Second-order prediction
- Scenario simulation
- Linco Bridge style Agent Connectivity Layer
- Multi-device Agent access


## Additional technical decisions

- Cognitive Distillation is not personality cloning.
- Cognitive Models must expose evidence and confidence.
- Local Generative Media follows Local First routing.
- Agent Connectivity must not become a Runtime dependency.

## External Connectivity Architecture Reference

### Linco Bridge Reference

Repository:
https://github.com/lincotalk/linco-bridge

Reference commit:
27688263dc33d07a5a06b7613329705552373dc1

Purpose:
- Agent Connectivity architecture reference
- Remote client access pattern
- Agent session continuity
- Event streaming design
- OpenClaw and Hermes connector architecture reference

Boundary:
- Linco Bridge is reference architecture only during P15
- AI-OS Runtime must not depend on Linco Bridge
- Candidate input for P16 Agent Connectivity Layer

Future P16 consideration:

AI-OS Agent Connectivity Layer may provide:
- multi-device Agent access
- remote sessions
- channel adapters
- event synchronization
- secure external client communication

---

## AC-EXEC-MODEL — Agent execution model ownership

**Decided 2026-08-22. Not yet implemented.**

### What was wrong

`ai-os-files` is pinned to `ollama-ai-os/qwen3:4b-instruct`. AI-OS passes only
`agentId` to the gateway, so OpenClaw decides which model executes. AI Center's
Auto routing, Local First, and fallback cover conversation but not execution.

The 4B model reads a SKILL.md and then stops without running the commands it
just read. Verified 2026-08-22: the same Baidu Drive download failed repeatedly
under 4B (5 reads, 2 unrelated execs) and succeeded under `ollama/qwen3:8b`
(8 execs: `bdpan transfer` -> `transfer list` -> `transfer select` -> `download`,
10.3 MB file landed on disk).

Nothing else in the chain was broken. AI-OS, gateway, agent, skill, `bdpan`,
Baidu login, destination directory, and the Skill-first prompt were all correct.

### Why model override does not work

The gateway rejects a per-run model override from this caller:
`provider/model overrides are not authorized for this caller`. The CLI accepts
`--model`; the gateway `agent` method does not. This is OpenClaw's restriction,
not ours.

### Decision: select at agent granularity

`openclaw agents add --model <id>` exists, so AI-OS creates several execution
agents with fixed models, and AI Center chooses between agents rather than
models.

This is better than a model override anyway. An execution agent binds model,
context window, skill allowlist, tool permissions, and workspace together.
Swapping only the model breaks that pairing — verified: `qwen3:8b` is smarter
but has a 32K window, while `qwen3:4b-instruct` has 64K, and a long SKILL.md
plus history overflows 32K. Capability alone is not a sufficient selection
criterion.

Proposed agents (names not final):
- light — 4B, 64K window, read-only file tools
- standard — 8B, shell and browser, download skills
- heavy — cloud model, long-document and multi-step work

### Implementation order

1. Create the execution agents with fixed models
2. AI Center selects the agent; the adapter passes the chosen `agentId`
3. On Runtime file-verification failure, retry down the candidate list

Selection must consider context window, not just capability.

### Also found

- The model reported success while its `exec` had failed. Runtime file
  verification caught it. Never trust an agent's self-report.
- Frontend error mapping in `src/services/tasks.ts` matches on substrings, so
  `destination is unavailable` renders as "OpenClaw is unavailable". This false
  lead cost hours. Rust already returns typed error kinds
  (`AuthenticationRequired`, `PairingRequired`, `ConnectionUnavailable`,
  `ProtocolFailure`, `ExecutionFailed`) — pass the kind through and switch on it
  instead of guessing from text.
- Diagnosis method that worked: read the agent trajectory and count which tools
  it actually called:
  `grep -o '"name":"[a-z_]*"' <session>.trajectory.jsonl | sort | uniq -c`
  Do this before theorising about any agent execution failure.

### Cloud-drive download scope

Cloud drives stay in v1.0 as one download route among HTTP, FTP, BT, ED2K,
magnet, and Thunder, which are already implemented. Chinese-internet resources
are often only distributed through drives, so this is a real gap, not an
optional extra.

Do not evaluate new drive tooling until AC-EXEC-MODEL lands — any Skill-based
tool will fail the same way under a 4B model. Candidates found on ClawHub for
later evaluation: `pansou` (search across 12 drive services), `baidupcs-go`
(mature CLI, supports offline download), `baidu-netdisk-storage`. Note that
`baidu-drive` confines operations to `/apps/bdpan/`, which does not suit
downloading arbitrary shared links.

Drive services are private APIs, not open protocols. Any tool is a reverse
engineering effort, needs the user's credentials, and can break when the
provider changes. Expect maintenance; do not treat a working tool as permanent.

---

## P15 Download Skill — handoff 2026-08-22

### Current status

- P15 Download Skill must NOT currently be treated as fully completed.
- The 2026-08-21 Direct HTTP and ordinary HTML-link OpenClaw E2E results remain valid historical verification.
- Current unresolved work is the Web/cloud-share execution path, especially provider-neutral Skill-first execution.
- Do not begin the next Download milestone or mark Downloads complete until this path is fixed and real E2E verification passes.

### Current implementation

The Download capability already has substantial implementation in the current working tree:

- `download.start` is registered as a Core Skill capability.
- Task → Plan → Runtime → OpenClaw execution exists.
- `download.start` requires one-time user confirmation.
- Direct HTTP routing exists.
- Web/cloud-share routing exists.
- Search routing exists.
- aria2 routing exists.
- P2P / Thunder-preferred routing exists.
- Runtime verifies that a real file appears in the selected destination before reporting success.
- Real OpenClaw E2E test entry exists in `src-tauri/src/task_execution.rs`:
  `real_download_runs_through_task_plan_runtime_and_openclaw`.
- The real E2E test is intentionally ignored unless the OpenClaw gateway and P15 download fixture are available.

### Current blocking issue

The current Web route in:

`src-tauri/src/runtime/openclaw_gateway_adapter.rs`

function:

`execute_download_start()`

still uses the old curl-first prompt.

Current behavior begins by telling OpenClaw to inspect the Web page with `/usr/bin/curl`, and only later considers an installed download/cloud-drive Skill.

That is not the intended final architecture for cloud-share downloads.

Required behavior is Skill-first:

1. Inspect the installed OpenClaw Skills already available in OpenClaw context.
2. If a download/cloud-drive Skill declares support for the source, use that Skill first.
3. A Skill is an instruction document, not a callable tool.
4. Read the matching `SKILL.md` using OpenClaw's existing read capability.
5. Continue immediately after reading it and execute its instructions using existing tools such as `exec`.
6. Reading `SKILL.md` alone is NOT successful completion.
7. Preserve the source URL exactly when passing it to CLI commands; do not rewrite it as Markdown link syntax.
8. Continue until AI-OS verifies that a real downloaded file exists in the selected destination.
9. Only when no installed Skill supports the source may OpenClaw fall back to generic curl/browser handling.
10. AI-OS Runtime must remain provider-neutral and must not select a cloud provider by hard-coded domain.

### Intended Web prompt direction

The attempted replacement prompt explicitly required:

- Skill-first selection.
- `SKILL.md` read followed by actual execution.
- No attempt to call the Skill name as though it were an OpenClaw tool.
- Exact preservation and shell quoting of the source URL.
- Generic curl/browser only as fallback.
- No success after merely reading a Skill, opening a page, resolving a URL, or announcing a next action.
- Success only after a complete file exists in the destination.

The attempted replacement was accidentally pasted directly into zsh and therefore produced:

`zsh: parse error near ')'`

That shell error did NOT modify the Rust source.

The latest inspection confirmed that `DownloadExecutionRoute::Web` still contains the original curl-first prompt.

### Files currently modified — preserve this work

At handoff time `git status --short` showed existing uncommitted work in:

- `HANDOFF.md`
- `src-tauri/src/lib.rs`
- `src-tauri/src/runtime/executor.rs`
- `src-tauri/src/runtime/openclaw_gateway_adapter.rs`
- `src-tauri/src/runtime/openclaw_permission.rs`
- `src-tauri/src/runtime/plan_runtime_bridge.rs`
- `src-tauri/src/runtime/skills/registry.rs`
- `src-tauri/src/task_execution.rs`
- `src/App.css`
- `src/pages/ChatPage.tsx`
- `src/services/tasks.ts`

Untracked work included:

- `skills-lock.json`
- `src-tauri/src/download/`
- `update_master_guide_p15_p16.sh`
- `verify/verify_composer_tool_wrap.sh`
- `verify/verify_p15_baidu_official.sh`
- `verify/verify_p15_download_complete.sh`

Do NOT run `git reset --hard`, `git clean`, restore these files from HEAD, or otherwise discard this working tree.

### Next developer action

Continue from:

`src-tauri/src/runtime/openclaw_gateway_adapter.rs`

→ `execute_download_start()`

→ `DownloadExecutionRoute::Web`

First replace the old curl-first Web/cloud-share prompt with the intended provider-neutral Skill-first contract.

Then:

1. run focused Rust tests for the Download/OpenClaw adapter;
2. run `verify/verify_p15_download_complete.sh`;
3. run the real OpenClaw download E2E with an appropriate cloud-share fixture;
4. verify that OpenClaw actually reads the matching Skill instructions and continues into real execution;
5. verify that the source URL is passed unchanged to the provider CLI/tool;
6. verify that AI-OS reports success only after the destination contains the downloaded file.

Do not mark P15 Downloads complete solely because the existing Direct HTTP or ordinary HTML-link tests pass.

### Architecture boundary

Keep the existing architecture:

User request
→ Task Engine
→ Plan
→ user confirmation
→ Runtime
→ OpenClaw
→ installed Skill / existing OpenClaw tools
→ Runtime file verification

AI-OS Runtime owns Task/Plan/permission/capability routing and completion verification.

OpenClaw owns execution.

Installed provider Skills describe provider-specific execution.

Do not move provider-specific cloud-drive logic into the AI-OS Runtime and do not hard-code provider selection by domain.
- 2026-08-23 09:35  feat(p15): complete download skill with dedicated 8B execution agent
- 2026-08-23 09:59  fix(errors): classify runtime failures by kind instead of matching prose
- 2026-08-23 12:10  complete AC execution model
- 2026-08-23 15:54  完成 oMLX 自动启动与 My AI 本地服务卡片

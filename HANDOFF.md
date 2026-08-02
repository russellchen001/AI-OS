# AI-OS Project Handoff

> Last Updated: 2026-08-02
> Updated By: ChatGPT / Codex
> Purpose: Resume AI-OS development quickly in a new ChatGPT or Codex session.

---

# 0. Current P13 Handoff (Authoritative Resume Point)

This section supersedes the older current-status snapshots below. Historical P11 and UI Refactor notes are retained for context.

## Repository State

- Repository: `/Users/russellchen/AI-OS/dashboard`
- Branch: `feature/p13-ai-center`
- HEAD: `079d78e docs(p13): update provider auth handoff`
- Working tree: intentionally dirty with the uncommitted Claude Code and Auto-routing hardening implementation
- Active milestone: P13-M4, AI Center invocation observability — completed
- Next work item: define the next P13 milestone from the remaining AI Center goals before implementation

## Completed P13 Account Connections

### OpenRouter, Kimi Code, and Meta Model API

The current uncommitted P13 implementation also adds three truthful native Provider paths:

- OpenRouter supports its official PKCE account connection, which creates a user-controlled OpenRouter API key, plus a separate manually entered API-key path. The generated key is stored only in macOS Keychain; AI-OS supports the official OpenAI-compatible API and live model discovery. Its catalog currently returns more than 300 routed models with their provider-supplied names.
- Kimi Code supports the official RFC 8628 device authorization flow from Moonshot AI's public Kimi Code client, subscription-backed model discovery and chat through `api.kimi.com/coding/v1`, refresh metadata, cancellation, and a separate Kimi Code API-key path.
- Meta now targets the current Meta Model API and Muse Spark 1.1 through `api.meta.ai/v1`; it deliberately does not claim that a consumer Meta AI login can authorize third-party inference. The supported connection is a Meta Model API developer key.

Protocol validation completed without exposing credentials: the Kimi device endpoint returned the expected device-code contract with a `www.kimi.com` verification page, OpenRouter returned a live model catalog, and the unauthenticated Kimi and Meta model endpoints both correctly returned HTTP 401. The owner currently has no OpenRouter, Kimi Code, or Meta Model API account, so real account authorization and authenticated inference for these three Providers remain explicitly pending.

### Google Gemini

Google native/public PKCE OAuth is working end to end with a real configured Google OAuth client. The flow opens the system browser, receives the loopback callback, exchanges the authorization code, tests the connection, discovers live models, allows default-model selection, and saves a canonical `ProviderInstance` with truthful OAuth metadata.

The real manual validation completed successfully: Google returned 50 models and the Provider saved as connected.

### OpenAI Codex / ChatGPT Account

Commit `5cf0d19` added account login through the official OpenAI Codex public OAuth client and fixed callback at port 1455. It includes PKCE, token refresh metadata, ChatGPT account-ID extraction from the namespaced JWT claim, account-scoped headers, and a separate Codex backend route rather than pretending a ChatGPT subscription is an OpenAI Platform API key.

The real manual validation completed successfully: seven Codex models were discovered, `GPT-5.6-Sol` was selected, and the canonical Provider saved as connected.

Related commits, newest first:

- `5cf0d19 feat(p13): connect OpenAI Codex accounts`
- `f7d186e fix(p13): advertise only configured oauth providers`
- `47f941a fix(p13): align oauth credential schema`
- `9d64fc9 fix(p13): pin provider save errors above actions`
- `0d3f286 fix(p13): surface provider save failures`
- Earlier Google OAuth callback, exchange, Keychain, discovery, and metadata commits remain in this branch history.

### xAI Grok Account and API Connections; DeepSeek API Connection

xAI supports both official Grok account OAuth and developer API keys. DeepSeek remains API-key-only. API keys and OAuth tokens are passed directly to the native security layer, stored in macOS Keychain, and never persisted in browser or Provider configuration state.

The uncommitted P13 hardening now provides:

- xAI language-model discovery through the official `/v1/language-models` catalog, excluding image- and video-generation-only models
- Official xAI Grok Build RFC 8628 device authorization with the public xAI client, bounded polling, cancellation, refresh metadata, and Keychain storage
- Separate Grok account routing through `cli-chat-proxy.grok.com` with the required `X-XAI-Token-Auth` identity and Responses protocol
- Separate Grok API-key chat completion and streaming through the official xAI developer API
- DeepSeek model discovery and chat completion/streaming through the current official non-`/v1` endpoints
- Current DeepSeek V4 Flash and V4 Pro naming and reasoning capability reporting
- `Sign in with Grok` plus `Use API key` as two truthful, independent connection options; DeepSeek remains API-key-first
- Truthful connection errors and automatic removal of a newly stored Keychain credential when live verification fails

The live xAI device-code endpoint returned the complete expected contract for the public Grok Build client. Real Grok account authorization and inference also passed: the browser reported that the device was authorized, the account catalog returned `grok-4.5`, the Provider saved as Connected, Workspace selected `xAI · grok-4.5`, and the real response exactly matched `Grok UI verified`. DeepSeek real validation remains pending a key entered through AI-OS. No token or key should be pasted into chat or committed to the repository.

### Anthropic Claude Code Subscription

Do not invent or advertise a third-party Anthropic OAuth client. Anthropic does not provide AI-OS with a documented public/native third-party client registration equivalent to the Google client or OpenAI Codex client.

The approved safe architecture is to use the official Claude Code CLI as a local authorization and execution proxy:

- AI-OS launches and queries the official `claude` binary.
- Claude Code owns the Claude Pro/Max subscription OAuth credential.
- AI-OS must not read, copy, export, or persist Claude's OAuth token.
- Anthropic API-key mode remains a separate Provider credential path.
- Subscription-backed Claude Code execution must remain distinct from Anthropic API execution in routing, capability reporting, and persistence.

Official Claude Code is installed and authenticated:

- Version: `2.1.220`
- Binary: `/Users/russellchen/.local/bin/claude`
- Authentication method: `claude.ai`
- API provider: Anthropic first-party
- Subscription: Claude Pro

The authoritative status check now returns `loggedIn: true`. A real minimal request was executed with `ANTHROPIC_API_KEY` removed from the environment, structured JSON output enabled, permission mode set to `dontAsk`, and tools disabled. It returned the expected verification text successfully.

The uncommitted implementation adds:

- Claude binary discovery with an optional explicit override and standard macOS paths
- Authentication status and version reporting without reading Claude's credential
- Bounded structured execution with `ANTHROPIC_API_KEY` removed
- Disabled tool execution for AI Center chat requests
- Timeout termination, process-scoped cancellation, operation-ID validation, and cleanup
- Bounded credential-shaped error sanitization
- Claude Sonnet, Opus, and Haiku model mapping
- A truthful `local` Provider credential kind for the Claude Code subscription path
- A first-class `cli-account` authentication method in the shared Rust/TypeScript Provider contract
- Separate routing from the existing Anthropic API-key path
- My AI connection, model selection, management, and disconnect behavior
- Workspace model selection, normal response routing, and Stop response behavior
- Deterministic Auto routing: each connected Provider's default model is tried before its other enabled models
- Auto fallback only when a candidate fails before producing output; explicit model selection never silently switches Provider or model

AI-OS does not read, copy, export, or persist Claude Code's OAuth credential. Provider persistence records only the local connection and model preference.

## My AI Entry and Local Model State Correction

The redundant top-right `Add AI` action was removed because it opened the same generic Provider setup as the `Other AI` catalog entry. The catalog entry is now the single generic Provider path.

The Ollama card now consumes canonical Runtime status instead of treating every model-discovery failure as an empty model catalog. It distinguishes not installed, stopped, starting, running without models, and ready states; offers `Start Ollama`, `Add first model`, or `Manage local models` as appropriate; and includes a refresh action that rechecks both Runtime and model state. On the owner's Mac, Ollama 0.31.1 is installed but was not running, which explains the earlier misleading `No local models installed` display.

Workspace model selection now also consumes the live Ollama catalog instead of reading only connected cloud Provider instances. When Ollama is running, its installed models appear as explicit `Ollama · <model>` choices and route through the existing local Ollama backend. Auto routing follows AI-OS's Local First principle: available local Ollama models are attempted before connected cloud defaults, while cloud models remain fallback candidates if local inference fails before producing output. Stopping Ollama removes the unavailable local choices rather than leaving stale selector entries.

Real local validation passed on the owner's Mac. Ollama found the existing 14 GB model library containing `qwen2.5:7b`, `qwen3:8b`, and `deepseek-r1:8b`; a direct `qwen2.5:7b` inference returned the exact expected `Local AI verified` response.

Final M3 desktop acceptance also passed in a freshly built application with an isolated bundle identifier. Workspace displayed all three Ollama choices. Explicitly selecting `Ollama · qwen2.5:7b` returned the exact `Local UI verified` response. For the Auto-routing proof, `qwen2.5:7b` was first unloaded from memory; an Auto request then returned the exact `Auto Local First verified` response, and `ollama ps` confirmed that `qwen2.5:7b` had been reloaded on the GPU. This proves the real desktop Auto path selected the local candidate before cloud fallbacks. M3 is accepted as complete.

## P13-M4 AI Center Invocation Observability — Completed

The authoritative M4 specification and acceptance checklist now live in `docs/Milestones/P13-M4_AI_CENTER_OBSERVABILITY.md`. The scope follows `AI_OS_MASTER_GUIDE.md`: AI Center owns Provider-independent routing observability, cost, latency, and shared invocation metadata. Workspace may render and persist that canonical record, but it must not reconstruct routing decisions itself. Provider ranking, billing reconciliation, user-configurable policy, and Council/Arena execution remain outside M4.

The current uncommitted first implementation adds:

- A canonical AI Center invocation record containing invocation ID, Auto or manual route mode, local or cloud source, actual Provider instance, Provider, model, start/completion time, total latency, estimated input/output tokens, pricing-match state, optional estimated USD cost, fallback state, and ordered attempt records
- Safe attempt outcomes and normalized error categories without prompts, outputs, raw Provider responses, credentials, tokens, or authorization URLs
- Local First attempt ordering inherited from accepted M3 behavior
- Shared metadata generation for normal and streaming AI Center calls, including Claude Code and cancellation results
- One existing-Analytics success event per completed invocation using the same Provider, model, latency, token estimates, pricing state, route mode, source, fallback flag, and attempt count
- Optional invocation metadata on persisted conversation messages
- A compact Workspace response provenance row showing On this Mac or Cloud, Provider/model, latency, truthful cost availability, and fallback attempt count
- Unknown pricing is displayed as unavailable rather than falsely shown as zero; Ollama uses the existing verified local zero-cost wildcard and may show `$0.00 estimated`

M4 is now complete. The pure observability test passes, the frontend production build passes, all 394 Rust library tests pass, Rust formatting and `cargo check` pass with the same four pre-existing warnings, and `git diff --check` passes.

Real desktop acceptance passed in a freshly rebuilt application with the previously isolated acceptance identifier:

- Explicit Ollama selection returned the exact `M4 Manual verified` response and displayed `On this Mac · ollama · qwen2.5:7b`, measured latency, and `$0.00 estimated`.
- Reload preserved the complete provenance row with the conversation.
- Auto returned the exact `M4 Auto verified` response through local `qwen2.5:7b`, with no fallback marker.
- Explicit xAI selection returned the exact `M4 Cloud verified` response and displayed `Cloud · grok · grok-4.5`, measured latency, and a nonzero estimated cost.
- Cancelling a long local response preserved partial output plus canonical invocation metadata and did not record a success Analytics event.
- Deterministic tests cover safe error normalization, ordered pre-output fallback records, manual/no-output/cancellation routing decisions, the rule that emitted output forbids fallback, verified versus unavailable pricing, local zero cost, and Analytics field parity.

The next session should not reopen M4 unless a regression is discovered. Before starting new implementation, define the next P13 milestone against the remaining Master Guide goals, especially shared multi-model invocation readiness and any still-missing Provider-independent policy boundary.

## Real Desktop Validation

A temporary Debug application bundle with a distinct development identifier was used because the installed AI OS application and the unbundled development process share the production bundle identifier. The temporary application was closed after validation and did not change the checked-in Tauri configuration.

The following real UI flows passed:

1. Anthropic displayed operational `Sign in with Claude` when the authenticated CLI was available.
2. `Verify Claude Code` confirmed the local subscription connection.
3. Claude Sonnet, Opus, and Haiku appeared and Sonnet could be saved as default.
4. The Anthropic Provider saved as Connected without a Keychain secret.
5. Workspace explicitly selected `Anthropic · Claude Sonnet`.
6. A real message returned the exact expected `Claude UI verified` response.
7. A long request displayed `Stop response` and cancellation terminated the owned request.
8. The UI truthfully reported that the response stopped before text was received.
9. xAI displayed operational `Sign in with Grok` alongside the separate API-key path.
10. The official xAI device page accepted the generated code and reported the device authorized.
11. Grok account model discovery returned `grok-4.5`, and the Provider saved as Connected.
12. Workspace explicitly selected `xAI · grok-4.5`; a real message returned the exact expected `Grok UI verified` response.

Desktop QA also found and fixed stale browser-oriented copy in the Claude Code flow: `Continue in browser` became `Verify Claude Code`, and `Waiting for sign-in…` became `Checking Claude Code…`.

Do not ask the user to paste tokens, secrets, complete OAuth URLs, or authorization codes into AI-OS or a chat.

## Exact Resume Steps

1. Reconfirm branch `feature/p13-ai-center`, HEAD `079d78e` or later, and the intentional uncommitted files listed below.
2. Review the Claude Code diff without discarding, staging, or committing it unless the owner explicitly requests that action.
3. Preserve the separation between Claude Code subscription execution and Anthropic API-key execution.
4. Preserve the rule that Claude Code owns its credential and AI-OS stores no Claude OAuth token.
5. If implementation continues before a commit, add only explicitly approved P13 hardening or tests.
6. Immediately before any future commit, rerun Rust formatting, all Rust library tests, `cargo check`, the frontend production build, and `git diff --check`.

Official references used for this decision:

- `https://code.claude.com/docs/en/authentication`
- `https://code.claude.com/docs/en/troubleshoot-install`
- `https://code.claude.com/docs/en/desktop-quickstart`

## Last Known Validation

For the current uncommitted Claude Code implementation:

- Claude Code-focused Rust tests: 4 passed
- Full Rust library tests: 388 passed
- `cargo check`: passed with four pre-existing warnings
- Frontend production build: passed
- Formatting and diff checks: passed
- Real Claude Code CLI request: passed
- Real Claude Code desktop UI E2E: connection, model selection, Provider persistence, chat response, and cancellation passed
- Auto-routing hardening: TypeScript production build, full Rust tests, Rust check, formatting, and diff checks passed
- xAI/DeepSeek API hardening: frontend production build passed; full Rust suite now has 389 passing tests; Rust check and diff checks passed
- Grok OAuth correction: official device endpoint handshake and real account/UI/chat E2E passed; frontend build passed; full Rust suite now has 391 passing tests
- OpenRouter/Kimi Code/Meta expansion: frontend production build passed; full Rust suite now has 394 passing tests; Rust check passed with the same four pre-existing warnings; official Kimi device handshake and live endpoint status checks passed; formatting and diff checks passed
- Ollama Workspace selector integration: frontend production build passed; full Rust suite remains 394 passing tests; Rust check passed with the same four pre-existing warnings; direct local inference passed
- M3 final desktop acceptance: all three Ollama models appeared in Workspace; explicit local selection passed; Auto Local First routing passed with runtime-level confirmation that `qwen2.5:7b` was loaded locally
- M4 invocation observability: pure tests, frontend build, Rust formatting, 394 Rust tests, Rust check, diff check, isolated desktop build, manual local, Auto Local First, manual cloud, persistence, cost/latency provenance, cancellation, and Analytics parity all passed

Uncommitted implementation files:

- `src-tauri/src/claude_code.rs` — new
- `src-tauri/src/lib.rs` — registers Claude Code status, generation, and cancellation commands
- `src-tauri/src/providers.rs` — advertises the truthful Claude Code CLI-account authentication contract
- `src/pages/MyAiPage.tsx` — Claude Code connection and model-selection UI
- `src/services/aiCenter.ts` — Claude Code routing, cancellation, deterministic Auto ordering, safe pre-output fallback, and live Ollama model registration
- `src/App.tsx` — synchronizes running Ollama models into Workspace model selection
- `src/services/aiCenterObservability.ts` — new canonical M4 invocation, attempt, token estimate, cost, latency, and Analytics helpers
- `src/services/conversations.ts` — persists optional canonical invocation metadata with assistant messages
- `src/pages/ChatPage.tsx` — attaches and renders canonical response provenance without owning routing decisions
- `docs/Milestones/P13-M4_AI_CENTER_OBSERVABILITY.md` — new M4 specification and acceptance checklist
- `scripts/milestones/p13_m4_observability.ts` — deterministic M4 observability, fallback-policy, redaction, pricing, and Analytics tests
- `src/types/provider.ts` — shared frontend `cli-account` authentication contract
- `HANDOFF.md` — this handoff update

The owner requested delivery without Git operations in this session. Nothing was staged, committed, or pushed by this implementation or handoff update.

---

# 1. Current Project Status

## 2026-08-01 Product and Design Documentation Update

The repository is now attached to `feature/ui-refactor` at `1d173be`. Existing UI Refactor source changes and all previously untracked files were preserved; none were staged, committed, discarded, or deleted.

Documentation completed in this session:

- `AI_OS_MASTER_GUIDE.md` — version 2026.2; formal Council/Arena definitions, boundaries, roadmap, glossary, v1.0 acceptance, and companion-document authority.
- `AI_OS_PRODUCT_VISION.md` — version 1.0; mission, audience, promise, pillars, brand, differentiation, non-goals, and success criteria.
- `AI_OS_UI_SPEC.md` — version 1.0; information architecture, pages, states, interaction, responsive/accessibility, and architecture boundaries.
- `AI_OS_DESIGN_SYSTEM.md` — version 1.0; semantic tokens, themes, typography, layout, motion, states, primitives, and domain components.
- `AI_OS_FIGMA_BLUEPRINT.md` — version 1.0; Figma structure, variables, components, frames, prototypes, state matrix, and handoff.

Authority remains: Master Guide (product direction, architecture, roadmap, scope) → Product Vision (brand/vision), UI Spec (information architecture/experience), Design System (visual/components), Figma Blueprint (design-file delivery). The Master Guide prevails on conflict.

AI Council is now formally a Chief-of-Staff-facilitated dynamic expert organization for user decision quality. AI Arena is a controlled multi-AI interaction platform covering Compare, Debate, Collaboration, Competition, Role Play, Game, and Simulation while retaining reproducible evaluation records. Both use AI Center and do not replace the existing Task Engine, Planner, Runtime, or permission ownership.

Validation performed: terminology review across the five documents; Master Guide Arena-framing search; roadmap phase-presence and numbering check; `git diff --check`; `git diff --stat`; and `git status`. See the latest task report for exact results.

This update does **not** mark the UI Refactor complete and does **not** start or complete P12. Next: product-owner review and decisions, then implement/validate UI Refactor Final against these documents before P12.

Product-owner decisions on 2026-08-01: approved the five-document baseline, authorized a documentation-only commit, approved the UI Refactor Final → validation → P12 sequence, and requested a brighter white-first light theme. The UI Spec, Design System, and Figma Blueprint were updated accordingly; white is the dominant light workspace field, with near-white neutral separation and dark mode retained.

## 2026-08-01 UI Refactor Final Pass

Implemented the approved white-first premium workspace direction across Chat, Sidebar, My AI, Agents, AI Arena, AI Council, Artifacts, and Settings. The primary navigation now includes Workspace, Artifacts, My AI, Agents, AI Council, AI Arena, Skills, and Settings; selecting Workspace no longer creates a conversation because New conversation remains a separate action.

AI Arena now presents all seven target modes while keeping unavailable modes visibly disabled. AI Council presents the Brief → Assemble → Discuss → Synthesize target flow and explicitly labels the current saved-role implementation as the current workflow; dynamic Chief of Staff assembly remains P16 work. Settings and Artifacts retain their existing functionality while adopting the shared page, control, responsive, focus, and state language.

Validation: `npm run build` passed with TypeScript and Vite production output (2672 modules). `git diff --check` remains required immediately before any UI commit. This pass does not start P12. Product-owner visual review and repository-scope review remain before declaring the full UI Refactor milestone closed.

Desktop visual QA was subsequently completed in an isolated Chromium Agent Window for Workspace, AI Council in dark mode, Settings in the approved white-first light mode, and AI Arena in light mode. QA found and fixed: unreadable Recent-conversation text on the light sidebar, legacy Dashboard Header leakage into Council/Artifacts, missing dark-theme treatment for new Council surfaces, and the unstructured Settings About metadata block. The browser Agent Window could not be resized programmatically, so responsive behavior is covered by CSS rules and build validation but still needs a real narrow-viewport screenshot before final milestone closure.

Product-owner Arena clarification: Arena formations are not fixed to 1v1. The UI now provides a variable roster (2–12 participants in the current guardrail), connected-model selection through AI Center, Independent or Team A–D grouping, add/remove controls, and editable 1v1, 1v1v1, and 2v2 presets. The specifications now explicitly support free-for-all, uneven teams, and other custom formations. This is setup UI and product contract work only; P17 execution remains unavailable and Start arena remains disabled.

---

## Development Stage

Current Phase: Post-P11 stabilization before the UI Refactor and P12.

Current Milestone: P11 — OpenClaw Integration, completed.

Status: P11 implementation, validation, remote backup, and release tag are complete.

Current Branch: HEAD is detached at `v0.11.0`. The associated branch is `feature/p11-m5-plan-runtime-integration`, which points to the same commit.

Latest Local Commit: `1d173be feat(tasks): complete supervised plan execution`

Remote Status: `origin/feature/p11-m5-plan-runtime-integration` is synchronized with `1d173be` (0 ahead, 0 behind).

Repository Status: Dirty only because of one unstaged `.gitignore` change and two untracked P11 investigation files.

---

# 2. Session Summary

P11 was completed and closed. The production backend now composes shared in-memory Task and Plan repositories, a callable `TaskExecutionService`, Task–Plan orchestration, supervised Runtime execution, permission enforcement, and the OpenClaw Gateway adapter.

The session repaired persistence-compensation reporting, shared Supervisor scheduling and events, panic sanitization, repeated-attempt identity, trusted-automation diagnostics, Rust/TypeScript Runtime contracts, and no-output semantics.

Validation completed successfully: Rust formatting, 336 Rust unit tests, doc tests, TypeScript checking, Vite production build, and `git diff --check`.

P11 was committed as `1d173be`, pushed to its remote feature branch, and tagged `v0.11.0`.

---

# 3. Current Production Architecture

```text
Application composition
  ↓
Shared InMemoryTaskRepository
  ↓
Shared InMemoryPlanRepository
  ↓
TaskExecutionService
  ↓
TaskPlanExecutionOrchestrator
  ↓
RuntimeBackedPlanExecutor
  ↓
Runtime Scheduler and Supervisor
  ↓
PermissionEnforcingOpenClawExecutionAdapter
  ↓
ConfiguredCapabilityPermissionGate
  ↓
OpenClawGatewayExecutionAdapter
  ↓
Active OpenClaw Gateway
```

The production composition also constructs `TaskLifecycleManager` over the same shared Task repository.

There is no Task execution Tauri command and no frontend Task execution feature.

---

# 4. Completed Milestones

- **P9 — Runtime Foundation:** Canonical Runtime discovery, lifecycle, operations, scheduling, supervision, recovery, and frontend Runtime migration.
- **P10 — Task Engine and Planner:** Task domain and lifecycle, Planner domain and repositories, validation, activation, Plan/Step lifecycle, execution coordination, Task–Plan policy, and persistence orchestration.
- **P11 — OpenClaw Integration:** Runtime execution contract, Gateway adapter, permission boundary, Gateway event forwarding, and supervised Task–Plan Runtime integration.

The Master Guide's snapshot still labels P10 as the current priority and P11 as next. The implementation and Git history now show P10 and P11 completed; do not silently alter the Master Guide without an explicitly authorized documentation task.

---

# 5. Temporary Product Decisions

## Task Repository

Current implementation: One shared `InMemoryTaskRepository` in production composition.

Reason: Explicitly accepted as the temporary production implementation for P11.

Known limitation: Task state is process-local and is lost after application restart.

Future replacement: A persistent Task repository when explicitly scheduled and authorized.

## Plan Repository

Current implementation: One shared `InMemoryPlanRepository` in production composition.

Reason: Explicitly accepted as the temporary production implementation for P11.

Known limitation: Plan and PlanStep state is process-local and is lost after application restart.

Future replacement: A persistent Plan repository when explicitly scheduled and authorized.

## Task Execution Entry Point

Current implementation: Backend-only `TaskExecutionService`; no Tauri command or frontend execution UI.

Reason: P11 required a production-composed callable backend boundary without expanding into frontend or P12 work.

Known limitation: No current user-facing entry point initiates this service.

Future replacement: Integrate through an explicitly approved Task Engine/UI milestone without bypassing the service.

## Trusted Automation Configuration

Current implementation: Missing configuration means an empty allowlist. Unavailable, unreadable, or malformed configuration fails closed to deny-all and emits sanitized diagnostics.

Reason: External effects must never proceed when trusted-automation policy cannot be established safely.

Known limitation: No Settings UI or approval UI exists for this configuration.

Future replacement: User-facing configuration only when explicitly scheduled.

---

# 6. Important Architecture Decisions

Task Engine owns:

- Task identity and Task state
- Task lifecycle transitions
- Task-to-active-Plan relationship

Planner owns:

- Plan creation and validation
- Plan and PlanStep lifecycle transitions
- Plan persistence operations

`TaskPlanExecutionOrchestrator` owns:

- Task/Plan execution coordination
- Persistence ordering
- Runtime outcome-to-Plan transition coordination
- Truthful partial-persistence and compensation reporting

Runtime owns:

- execution admission
- scheduling and supervision
- operation state, progress, terminalization, and cleanup
- panic sanitization
- lifecycle-specific retry, recovery, and cancellation behavior

Permission and execution boundary:

```text
Runtime
  ↓
PermissionEnforcingOpenClawExecutionAdapter
  ↓
ConfiguredCapabilityPermissionGate
  ↓
OpenClawGatewayExecutionAdapter
```

No caller may bypass the orchestrator, Runtime supervision, or permission boundary to invoke the Gateway directly.

Each PlanStep execution attempt receives a distinct UUID-backed Runtime operation identity while retaining collision-safe Plan/Step correlation.

---

# 7. Repository Rules

Every future development session MUST:

1. Read `AGENTS.md` completely.
2. Read `AI_OS_MASTER_GUIDE.md` completely.
3. Read `HANDOFF.md` completely.

No source code modifications may occur before all three documents are read.

Every Codex task must explicitly confirm that these documents were read.

---

# 8. Repository Health

Current branch: Detached HEAD at `v0.11.0`; `feature/p11-m5-plan-runtime-integration` points to the same commit.

Latest commit: `1d173be feat(tasks): complete supervised plan execution`

Latest tag: `v0.11.0`

Outstanding uncommitted files:

- `.gitignore` — adds `.tokensave`; not part of the P11 commit.

Outstanding untracked files:

- `p11-m5-investigation.txt`
- `p11-m5-source-bundle.txt`

Outstanding ignored files:

- Build/dependency output: `dist/`, `node_modules/`, `src-tauri/target/`, `src-tauri/gen/`
- Local/tool state: `.DS_Store` files, `.chatgpt-review/`, `.tokensave/`, `openclaw.log`
- Existing local backups: `.openclaw-integration-backup-*`, `*.bak`, `*.analytics-backup`, and `*.before-*` files

Known repository issues:

- HEAD is detached at the P11 tag and must not receive new development commits.
- The working tree is not clean because of the files listed above.
- The Master Guide's current-status snapshot has not yet been updated to reflect completed P10/P11 work.

---

# 9. Validation Status

Latest successful validation: 2026-07-30 at commit `1d173be`.

## Rust

- `cargo fmt --manifest-path src-tauri/Cargo.toml` — passed
- Focused Task execution, orchestration, Runtime Supervisor, bridge, and trusted-automation suites — passed
- `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1` — 336 passed, 0 failed
- Rust doc tests — passed, 0 failed

## Frontend

- `npm test` — no test script is configured in `package.json`
- TypeScript checking through `npm run build` — passed
- Vite production build through `npm run build` — passed

## Repository

- `git diff --check` — passed before the P11 commit

---

# 10. Current Open Items

## Priority: Immediate

Description: Restore an intentional clean branching state from detached HEAD and decide whether the `.gitignore` change belongs in the UI Refactor branch or a separate documentation/tooling change.

Blocked by: Owner decision on the unstaged `.gitignore` change and handling of the two untracked investigation files.

Expected milestone: UI Refactor preparation.

## Priority: Next

Description: Create and complete the approved UI Refactor branch.

Blocked by: Clean branching state and an explicit UI Refactor scope.

Expected milestone: UI Refactor.

## Priority: After UI Refactor

Description: Begin P12 Skill Framework work from the completed P11 baseline.

Blocked by: UI Refactor completion.

Expected milestone: P12.

---

# 11. Next Immediate Work

Current planned order:

1. Verify Git status — completed; detached at `v0.11.0` with the known local files listed above.
2. Push latest completed milestone — completed; the remote P11 feature branch matches `1d173be`.
3. Create Git tag — completed; `v0.11.0` points to `1d173be`.
4. Create the UI Refactor branch from `1d173be`; do not commit while HEAD is detached.
5. Complete the explicitly scoped UI Refactor.
6. Begin P12 only after the UI Refactor is complete.

No other development work should start before these steps.

---

# 12. Session Checklist

Before starting development:

- [ ] Read `AGENTS.md`
- [ ] Read `AI_OS_MASTER_GUIDE.md`
- [ ] Read `HANDOFF.md`
- [ ] Check Git status
- [ ] Check whether HEAD is attached to the intended branch
- [ ] Verify latest commit
- [ ] Verify remote branch
- [ ] Verify latest tag
- [ ] Review the previous session summary

---

# 13. Known Risks

- Task and Plan state is lost on application restart because production repositories are temporarily in-memory.
- There is no frontend entry point for `TaskExecutionService`.
- The UI Refactor has not started.
- HEAD is currently detached.
- The working tree contains one unstaged change and two untracked investigation files.
- The Master Guide status snapshot is stale relative to completed P10/P11 implementation.

---

# 14. Notes for ChatGPT

When continuing this project, do not assume anything outside this repository.

Treat these as the authoritative project documents:

- `AGENTS.md`
- `AI_OS_MASTER_GUIDE.md`
- `HANDOFF.md`

The Master Guide owns product direction, architecture, roadmap, and scope. `HANDOFF.md` owns the recorded current repository state and immediate continuation order.

If they conflict on product scope or architecture, stop and ask for clarification rather than inventing architecture. A stale status snapshot may be reported as stale but must not be silently rewritten.

---

# 15. Notes for Codex

Before editing, completely read:

- `AGENTS.md`
- `AI_OS_MASTER_GUIDE.md`
- `HANDOFF.md`

Do not modify code until all three have been reviewed.

Always preserve:

- Task Engine ownership
- Planner ownership
- Task–Plan orchestration boundaries
- Runtime supervision boundaries
- Permission boundaries

Do not introduce new architecture, persistent storage, commands, or frontend execution paths without explicit authorization.

---

# 16. Change Log

## 2026-07-30

Completed:

- P11 OpenClaw Integration
- Production Task execution composition
- Persistence compensation and partial-state repair
- Shared Runtime Supervisor integration
- Attempt-safe operation identity
- Trusted-automation diagnostics
- Rust/TypeScript Runtime contract synchronization
- P11 validation, remote backup, and tag

Commit:

`1d173be feat(tasks): complete supervised plan execution`

Tag:

`v0.11.0`

Notes:

Ready to create the UI Refactor branch, complete the UI Refactor, and then begin P12.

## 2026-07-30 — Earlier P11 milestones

Completed:

- P11-M1 OpenClaw execution contract
- P11-M2 OpenClaw Gateway adapter
- P11-M3 OpenClaw permission boundary
- P11-M4A Gateway invocation event forwarding

Commits:

- `6fb084d`
- `eea3fd3`
- `fafeebc`
- `cbdd07e`

## 2026-07-28 and earlier

Completed:

- P10 Task Engine and Planner milestones through P10-M13
- P9 Runtime Foundation milestones

Notes:

Use Git history and accepted milestone commits for detailed provenance. Do not rely on superseded roadmap wording where it conflicts with `AI_OS_MASTER_GUIDE.md`.

- Agent Registry：Custom Agent 使用独立默认名称；所有非内置 Agent 可从 Agents 页面删除，OpenClaw 继续受内置保护。

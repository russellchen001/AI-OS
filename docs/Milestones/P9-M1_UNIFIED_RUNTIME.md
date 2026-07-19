# P9-M1 — Unified Runtime

## Objective

Establish one AI-OS-owned Runtime contract for discovery, status, lifecycle operations, progress, cancellation, normalized errors, and platform capabilities while preserving current product behavior and stored data.

## M1A — Contract and Read-only Status

Scope:

- Canonical Rust and TypeScript Runtime contracts
- Static five-runtime registry
- Read-only adapters for discovery and structured status
- Additive `list_runtimes` and `get_runtime_statuses` IPC
- Optional, unused read-only frontend hook
- Contract, serialization, mapping, and safe-error Rust tests
- No UI migration or lifecycle changes

Completion requires all existing legacy commands to remain registered and the current Dashboard, Services, and Settings composition to remain unchanged.

### Completion Status

- **M1A:** Completed
- **M1B:** Not started
- **M1C:** Not started
- **Completion date:** 2026-07-18
- **Implementation commits:** `0e44d67478b2a2ce56bbcf1afbb4d38caf0bf599`, `381d85278eb419c9dd8daf7e72099dc6324c89a0`, `d3f4c66ee697276a1855d165d902bac35a785a8b`
- **Validation completed:** TypeScript type-check, production frontend build, Rust formatting check, Rust compilation check, Rust tests, and Clippy for all targets
- **Compatibility:** No UI migration, lifecycle implementation, or stored-data migration was performed

Binding follow-ups:

1. Remote Ollama and Open WebUI endpoints must not receive local lifecycle capabilities.
2. Open WebUI must distinguish Docker dependency failure from a confirmed missing container before lifecycle operations.
3. `useRuntimes` refresh concurrency is deferred to M1C.
4. OpenClaw stale-observation presentation is deferred to M1C.

## M1B — Lifecycle Operations, Progress, and Cancellation

Deferred scope:

- Typed start, stop, restart, and open operations
- Operation IDs and terminal operation states
- Progress events
- Idempotent cancellation where supported
- Runtime-scoped normalized lifecycle errors
- macOS process-management isolation
- Compatibility façades for legacy lifecycle commands

### M1B1 — Operation Contract and State Manager

Foundation scope:

- Canonical Rust and TypeScript operation contracts
- Pure checked operation state machine and snapshot revision ordering
- In-memory bounded operation retention
- Per-runtime lifecycle exclusion for start, stop, and restart
- Typed version-1 event payload contract without event emission
- Typed cancellation outcomes without cancellation handles
- No lifecycle execution, IPC registration, legacy delegation, or UI integration

M1B1 operations are intentionally process-local. Active operations are never evicted; terminal operations are retained for 30 minutes with a maximum of 200 retained terminal snapshots.

`request_cancellation` is the exclusive entry into `cancelling`, and non-cancellable operations cannot enter `cancelling` or `cancelled`. Cancelling represents an in-progress request rather than a guaranteed outcome; a completion race may finish as succeeded, failed, or cancelled.

#### Completion Status

- **P9-M1B1:** Completed
- **P9-M1B2:** Not started
- **P9-M1C:** Not started
- **Completion date:** 2026-07-19
- **Implementation commits:** `778bf646cae1455767b7c78ca812a498fb14e3af`, `fa969942ff321c578f205964321da9e1c1a515d0`
- **Validation completed:** TypeScript type-check, production frontend build, Rust formatting check, Rust compilation check, Rust tests, Clippy for all targets, and Git whitespace validation
- **Rust tests:** 43 passed
- **Implementation boundary:** No lifecycle execution, lifecycle IPC registration, managed Tauri state registration, event emission, legacy command changes, UI migration, stored-data migration, or dependency addition was performed

Binding M1B2 follow-ups:

1. Operation-conflict IPC rejection must atomically include the existing operation ID or snapshot.
2. Canonical IPC exposure must enforce a bounded active-operation acceptance policy, especially for open operations.
3. Endpoint context must be explicitly provided and frozen into the accepted execution plan for Ollama and Open WebUI.
4. Remote endpoints must never receive local lifecycle operations.
5. M1B must not automatically start Docker for Open WebUI.

### M1B2A — Execution Plans and Native Adapters

Implementation scope:

- Immutable, typed, AI-OS-owned lifecycle execution plans
- Shared location classification, typed OpenClaw gateway validation, and strict browser URL validation
- Request-specific frozen OpenClaw, Ollama, Docker, Open WebUI, or Cherry Studio context
- Atomic `prepare_execution_plan` validation, action-specific collection, freezing, and planning
- Explicit ordered native commands, argument arrays, and typed no-op plans without shell interpolation
- Total-deadline command, readiness, and stopped-state verification primitives
- Typed launch-service-only OpenClaw lifecycle verification without retained Gateway credentials
- Explicit local Docker Desktop socket binding and per-command environment isolation
- Typed Docker container states with safe transitional-state rejection
- Stable internal progress phases without Tauri event emission
- Safe normalized dependency, location, container, and timeout errors
- Pure tests for planning, command construction, dependency classification, and security boundaries

Runtime policy:

- OpenClaw supports local `ws`, `wss`, `http`, and `https` profiles for start and stop; unloaded start performs ordered bootstrap then kickstart; restart is unsupported. Browser open accepts only an explicit safe HTTP or HTTPS endpoint with no query or fragment; AI-OS does not infer a dashboard URL from `ws` or `wss`.
- Ollama start requires an approved Homebrew formula path, while stop additionally requires verified Homebrew service ownership; remote endpoints support open only; restart is unsupported.
- Docker Desktop supports start, stop, and open; restart is unsupported.
- Open WebUI starts only stopped containers, treats running starts and stopped stops idempotently, and restarts only confirmed running containers; remote endpoints support open only; Docker is never started automatically.
- Cherry Studio supports start, stop, and open through fixed macOS application primitives; restart is unsupported.

M1B2A does not register lifecycle IPC or managed state, emit operation events, connect execution to the operation manager, delegate legacy commands, migrate UI, add dependencies, or change stored data. Canonical IPC exposure and operation-manager integration remain deferred to M1B2B.

Final safety rules require typed OpenClaw `Loaded`, `NotLoaded`, and `InspectionFailed` observations, verified no-op revalidation, separate child-process and readiness polling intervals, and bounded timeout cleanup that never waits indefinitely after a failed kill attempt. OpenClaw lifecycle completion verifies launch-service state only and does not claim Gateway authentication, health, readiness, or a fresh WebSocket connection. Active Gateway readiness requires a separately approved cancellable networking capability, and M1B2B must keep lifecycle completion separate from later health/readiness refresh.

AI-OS derives Docker Desktop ownership from the exact current-user `$HOME/.docker/run/docker.sock`, freezes the absolute `unix://` host into all management commands, and removes Docker host, context, TLS, certificate, and configuration selectors from every Docker child. Docker contexts and suffix-equivalent socket paths are not trusted. Docker open performs no home, socket, context, or daemon inspection; Open WebUI never starts Docker automatically.

Plan preparation rejects remote local-lifecycle requests and statically unsupported actions before native ownership or dependency inspection, then collects only facts required by the requested runtime and action. Only the verified macOS launchctl service-not-found exit establishes `NotLoaded`; every other nonzero exit, spawn failure, timeout, or internal wait failure is `InspectionFailed` and cannot authorize bootstrap or a stopped no-op. Debug output for requests, plans, commands, endpoints, verification, and planning contexts is structural and cannot reveal tokens, full URLs, query or fragment data, user paths, Docker sockets, launch plists, or raw arguments.

#### Completion Status

- **P9-M1B2A:** Completed
- **P9-M1B2B:** Not started
- **P9-M1C:** Not started
- **Completion date:** 2026-07-19
- **Implementation commits:** `754399eb2af13a3983ca6984e73cbf0e8c36370b`, `f83a1b66f534a37f81f1856f713f71dc88ebc2bb`, `efac039856fb8119f91cee7cd83561cbb158339c`, `c9526225a4d524ac2737c67f95a3ee70461a1c0a`, `f1793dc396347495ff0c1faaf7b891a83a599d7c`
- **Automated validation:** 70 Rust tests passed; TypeScript type-check and production build passed
- **Native smoke test:** `PASS_WITH_DOCKER_SKIPPED`; missing launch-service exit code 113 confirmed; existing OpenClaw service observed `Loaded`; Docker Desktop was not running, so Docker info validation was skipped under the approved rules
- **Repository integrity:** Clean after validation

Final architecture guarantees:

1. Execution inputs are frozen into typed plans.
2. Remote lifecycle requests are rejected before local ownership or dependency inspection.
3. OpenClaw lifecycle success verifies launch-service state only.
4. OpenClaw lifecycle success does not imply Gateway health or readiness.
5. OpenClaw lifecycle plans retain no Gateway token.
6. Docker and Open WebUI management use the exact AI-OS-derived local Docker Desktop socket.
7. Docker contexts and remote Docker environment selectors are not inherited.
8. Open WebUI never starts Docker automatically.
9. Ollama start and stop are limited to verified Homebrew ownership rules.
10. No broad process-name killing is used.
11. Request, plan, endpoint, path, and native-command Debug output is redacted.
12. Native command and verification waits are bounded.
13. No lifecycle IPC was registered.
14. No Tauri managed operation state was registered.
15. No runtime events were emitted.
16. No operation-manager execution wiring was added.
17. No legacy lifecycle command was changed or delegated.
18. No UI migration or stored-data migration occurred.
19. No dependency was added.

Binding P9-M1B2B requirements:

1. Operation-conflict IPC rejection must atomically include the existing operation ID or snapshot.
2. Canonical IPC must enforce a bounded active-operation acceptance policy, especially for open operations.
3. Endpoint and ownership context must be accepted and frozen before background execution.
4. M1B2B must connect lifecycle execution to the existing operation manager without redesigning its state machine.
5. Runtime events must carry the canonical operation snapshot and revision.
6. Lifecycle completion and status health/readiness refresh must remain separate.
7. Legacy UI and command migration remains P9-M1C.

### M1B2B1 — Bounded Admission, Managed State, and Contracts

Implementation scope:

- Atomic tagged accepted/conflict/rejected admission with the exact conflict snapshot captured under one manager lock
- Global limit of 16 queued, running, or cancelling operations, including open
- Per-runtime lifecycle exclusion without an open-operation lifecycle slot
- Stale lifecycle-slot repair under the admission lock
- Distinct retryable `operation-capacity-exceeded` error
- Explicit progress `Applied` and `Unchanged` mutation outcomes
- One managed `RuntimeExecutionState` containing one shared `Arc<RuntimeOperationManager>`
- Rust and TypeScript admission-contract parity and concurrency/serialization tests

M1B2B1 is intentionally separate from execution. It registers no lifecycle IPC, runs no frozen plan, emits no runtime operation event, and adds no frontend service wrapper or UI integration. M1B2B2 execution/IPC/events, M1B2B3 frontend wrappers/integration validation, and M1C migration remain unimplemented.

### M1B2B2 — Lifecycle Execution Supervisor, IPC, and Events

Implementation scope:

- Typed static preflight with canonical registry validation and frozen explicit endpoint/location context
- Immediate accepted queued response after best-effort queued emission and caller-independent scheduling
- No start gate; one sequential prepare → running → execute Supervisor transaction
- One absolute 30-second preparation deadline propagated into bounded native preparation probes
- Frozen-plan execution only after the Manager accepts the running transition
- Canonical full-snapshot `runtime://operation` events for accepted queued, running, meaningful progress, and terminal mutations
- Best-effort event delivery with revision-based ordering and no impact on operation truth
- Additive `start_runtime_operation`, `get_runtime_operation`, and `cancel_runtime_operation` native IPC
- Current adapters remain non-cancellable
- No automatic runtime status, health, readiness, or OpenClaw Gateway refresh

M1B2B2 is implemented separately from M1B2B3. It adds no frontend service wrapper, listener, reconciliation helper, hook, page, component, legacy delegation, stored operation persistence, or UI migration. M1B2B3 and M1C remain unimplemented.

Code-review hardening freezes Registry-owned `RuntimeAdapterKind` during preflight and uses exhaustive adapter dispatch throughout planning. Accepted cancellation mutations now have an explicit Applied/Unchanged outcome and emit only when applied. Scheduling rejection or panic is terminalized without returning queued state, while terminal races return the existing winner without duplicate emission. Canonical emission catches emitter errors and panics without changing operation truth; panic payloads do not enter state, IPC, or events. Process-global panic-hook behavior remains unchanged and outside this slice.

## M1C — Frontend Migration and Compatibility Cleanup

Deferred scope:

- Migrate Dashboard and Services to canonical Runtime records
- Replace fixed-delay lifecycle refresh with operation-aware status updates
- Preserve Settings and current user-facing behavior
- Consolidate duplicate service types, metadata, hooks, and health parsing
- Retain approved legacy IPC compatibility through the defined deprecation period

## Out of Scope for P9-M1

- Unified Session
- Unified Tool and MCP
- Unified Workspace
- Provider architecture replacement
- OpenClaw protocol redesign
- System metrics migration
- New provider integrations
- UI redesign

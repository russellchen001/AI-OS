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

M1B2B2 is implemented separately from M1B2B3. It adds no frontend service wrapper, listener, reconciliation helper, hook, page, component, legacy delegation, stored operation persistence, or UI migration. The typed client boundary is implemented in the later M1B2B3 slice; M1C remains unimplemented.

Code-review hardening freezes Registry-owned `RuntimeAdapterKind` during preflight and uses exhaustive adapter dispatch throughout planning. Accepted cancellation mutations now have an explicit Applied/Unchanged outcome and emit only when applied. Scheduling rejection or panic is terminalized without returning queued state, while terminal races return the existing winner without duplicate emission. Canonical emission catches emitter errors and panics without changing operation truth; panic payloads do not enter state, IPC, or events. Process-global panic-hook behavior remains unchanged and outside this slice.

### M1B2B3 — Typed Runtime Operation Client Boundary

Implementation scope:

- Canonical TypeScript start-request contract
- Typed invoke wrappers for start, get, and cancel
- One `runtime://operation` listener wrapper with version-1 filtering and full-snapshot delivery
- Revision-only snapshot reconciliation with fixed safe mismatched-ID rejection
- Consumer-owned unlisten lifecycle with no retained global subscription state
- Rust/TypeScript contract-parity and static wrapper-consumer audits

M1B2B3 was implemented separately from B1 and B2. No React page, hook, or component consumes the new client boundary, and no UI migration, legacy delegation, runtime-status refresh, or stored-data change was added. Complete M1B2B approval and merge remain pending final review and manual integration validation. M1C remains unimplemented.

### M1B2B Final Completion Record

- **P9-M1B2B:** Completed
- **P9-M1C:** Not started
- **Completion date:** 2026-07-19
- **Review:** B1, B2, and B3 were independently reviewed
- **Implementation commits:** `9790a8207d0f46446587266ba0da75e756fb5b06`, `e6b66a0243ecfbede19fec318e3603d425e4dc95`, `8a4f8f36ed98e283577d7dd674e317cc481bd5a1`, `76206d8f8ce36c9183f2bd5d10b4c6b680be19a7`, `bb312b8a89b258ac8f7c7d3841729c071edc3610`
- **Automated validation:** 108 Rust tests passed; TypeScript type-check and production build passed
- **Native Manual Integration result:** PASS
- **Manual test boundary:** Only Ollama open was used; no Start, Stop, or Restart action was executed
- **Repository integrity:** Repository remained clean; no dependency, manifest, lockfile, or stored schema changed; nothing was pushed

Final guarantees:

1. `RuntimeOperationManager` is the only canonical operation store.
2. `RuntimeExecutionState` shares exactly one Manager `Arc`.
3. Admission atomically returns accepted, conflicting Snapshot, or capacity rejection.
4. Global active-operation limit is 16.
5. Open counts globally but does not reserve a lifecycle slot.
6. Lifecycle plans are validated, prepared, and frozen before native execution.
7. Registry `AdapterKind` is the canonical lifecycle dispatch source.
8. Static remote and unsupported requests are rejected before admission.
9. Supervisor execution is independent of the IPC caller.
10. Start IPC returns the accepted queued Snapshot without waiting for preparation or execution.
11. Sequential Supervisor order is: prepare → running → execute → terminal.
12. Native execution cannot begin before accepted running transition.
13. Canonical event name is `runtime://operation`.
14. Events contain full version-1 Snapshots and use Revision as the only ordering field.
15. Event errors and panics never determine operation truth.
16. Applied cancellation mutations emit canonical events; rejected and unchanged cancellation does not.
17. Current adapters remain non-cancellable.
18. Lifecycle completion does not update or imply Runtime health, readiness, or OpenClaw Gateway connectivity.
19. TypeScript wrappers are additive and unused by React UI.
20. Legacy commands and UI remain unchanged.
21. No operation persistence or cross-restart recovery was added.
22. P9-M1C remains responsible for legacy command and UI migration.

## M1C — Frontend Migration and Compatibility Cleanup

Deferred scope:

- Migrate Dashboard and Services to canonical Runtime records
- Replace fixed-delay lifecycle refresh with operation-aware status updates
- Preserve Settings and current user-facing behavior
- Consolidate duplicate service types, metadata, hooks, and health parsing
- Retain approved legacy IPC compatibility through the defined deprecation period

### M1C1 — Canonical Runtime UI State and Individual-Control Migration

- **P9-M1C1:** Completed separately
- **P9-M1C2:** Not implemented
- **P9-M1C3:** Not implemented
- **Scope:** One App-owned Runtime Operation listener and one App-owned canonical Runtime status owner migrate Dashboard and Services individual Start, Stop, and Open controls
- **Identity:** Canonical IDs come only from Runtime Registry definitions/statuses; display labels remain presentation-only
- **State:** One `operationsById` Snapshot store with derived per-runtime lifecycle/Open state, synchronous submission channels, listener-ready admission gating, conflict attachment, and Revision-only reconciliation
- **Status boundary:** Terminal Start, Stop, and Restart schedule a separate deduplicated/coalesced best-effort status refresh; Open does not refresh status or imply Runtime health/readiness
- **Compatibility:** Start All and Stop All remain isolated legacy compatibility behavior; backend legacy commands and unused legacy frontend files remain for later review
- **UI boundary:** Internal Restart submission is supported, but no Restart or Cancel UI was added
- **Data/dependencies:** No operation persistence, stored schema change, list-active/list-recent IPC, or dependency addition

M1C1 does not complete M1C. M1C2 remains responsible for the remaining caller audit and frontend legacy dead-code cleanup. M1C3 remains responsible for the separately approved backend compatibility decision and final M1C validation/completion record.

M1C1 review corrections preserve independently successful Runtime definitions and statuses, centralize all status-query coalescing in `useRuntimes`, keep the operation listener mounted once, and bound terminal handling to observed state transitions. Hook-level active-channel rejection prevents duplicate admission before invocation. Canonical activity disables Legacy Bulk, active Legacy Bulk disables all canonical controls, and unstable Runtime lifecycle status disables only the lifecycle toggle. Fixed error-code classification handles rejected admissions and thrown IPC failures without exposing payload details. No M1C2 or M1C3 work was included.

Final review corrections keep Legacy Bulk isolation active until its existing final delayed status refresh settles and use synchronous ref-backed guards in both Bulk and canonical handlers. Endpoint configuration changes trigger one coalesced status refresh without definition reload; endpoint generations prevent old-endpoint responses from replacing current status. Error-code extraction accepts safe object, bounded JSON-string, and exact-code forms without exposing raw error data. M1C2 and M1C3 remain unimplemented.

The final status-coordinator correction replaces split first/trailing Promises with one shared drain-cycle Promise covering every required latest-generation query. Endpoint effects and stale-query completion cannot schedule the same generation twice; stale success/failure cannot replace or fail current-endpoint status, while a final applicable failure preserves prior status and rejects safely. Bulk final isolation waits for the complete shared refresh cycle, and post-invoke Bulk completion/failure notifications are unmount-safe. M1C2 and M1C3 remain unimplemented.

The V5 correction moves endpoint Ref and Generation updates into the committed layout effect, leaving speculative renders side-effect free while preserving one initial query and one coalesced latest-generation query per committed change. Coordinator Ref and loading cleanup now occur within the drain lifecycle before the shared Promise settles, preventing completion-boundary refresh loss. Existing stale-result, single-query, shared-Promise, and Bulk isolation guarantees remain unchanged. M1C2 and M1C3 remain unimplemented.

### P9-M1C1 Completion Record

- **P9-M1C1:** Completed
- **P9-M1C2:** Not started
- **P9-M1C3:** Not started
- **Completion date:** 2026-07-19
- **Implementation and review commits:** `3cb20a4296f709c6d0007548d352a3ad5b3192d8`, `f86c39e69a53f7ff06be47b4f9214d06c2411f4a`, `a3131487f51298dc3108fed8abe18280be98df7c`, `8a5300675e39b0760ce4b4612f0dc40e4fdb7575`, and `98d1ea7bc5f7c70f6ce9b421defa0ad0b0f1c675`
- **Validation:** TypeScript type-check passed; production build passed; 108 Rust tests passed
- **Manual Integration:** PASS; only Health Check and Ollama Open were used
- **Lifecycle safety:** No Start, Stop, Restart, Start All, or Stop All action was executed
- **Repository integrity:** Repository remained clean; no dependency, manifest, lockfile, backend, or stored schema changed; nothing was pushed

Final P9-M1C1 guarantees:

1. Dashboard and Services individual lifecycle controls use the canonical typed Runtime Operation boundary.
2. React components do not invoke canonical Tauri commands directly.
3. One App-owned Runtime Operation listener exists.
4. Runtime Operation response, lookup, and event Snapshots reconcile by Operation ID and Revision.
5. `operationsById` remains the single complete Snapshot store.
6. Canonical Runtime identity comes from Runtime definitions/statuses, not display labels.
7. Open and lifecycle actions use independent per-Runtime channels.
8. Same-channel duplicate submissions are rejected locally.
9. Conflict attaches to the returned existing Snapshot.
10. Current adapters remain non-cancellable and no Cancel UI exists.
11. No Restart UI was added.
12. Lifecycle Operation success never manufactures Runtime health, readiness, or lifecycle state.
13. Start/Stop/Restart terminal outcomes request one coalesced best-effort status refresh.
14. Open does not request a lifecycle/status refresh.
15. Runtime definitions and statuses load independently.
16. One shared Runtime-status drain-cycle coordinator owns every status query.
17. Endpoint Generation advances only for committed endpoint changes.
18. Stale endpoint successes and failures cannot replace or fail the current endpoint status result.
19. Coordinator cleanup occurs before its shared Promise settles, so a completion-boundary refresh cannot be lost.
20. Legacy Start All/Stop All remain isolated compatibility behavior.
21. Legacy Bulk and Canonical individual actions cannot overlap.
22. Start All isolation remains through its final 45-second status refresh; Stop All isolation remains through its 8-second refresh.
23. Raw operation identifiers, revisions, endpoints, native output, and backend error details are never rendered or logged.
24. Backend Legacy commands remain registered and unchanged.
25. M1C2 remains responsible for frontend Legacy dead-code cleanup.
26. M1C3 remains responsible for compatibility decisions and final M1C completion.

This record completes P9-M1C1 only; it does not record full P9-M1C completion or start P9-M1C2/P9-M1C3.

### P9-M1C2 Frontend Legacy Runtime Lifecycle Cleanup

P9-M1C2 frontend cleanup is completed. Repository-wide caller proof confirmed that `useServices` and `useServiceActions` were unreferenced dead paths, so both hooks and the unused individual Legacy frontend wrappers `startSingleService`, `stopSingleService`, and `openSingleService` were removed. No live frontend caller remains for `start_service`, `stop_service`, or `open_service`; individual Dashboard and Services controls continue through `useRuntimeOperations` and the canonical Runtime service.

Legacy Start All and Stop All frontend compatibility remains isolated through `useLegacyBulkRuntimeActions` and retained `startAllServices`/`stopAllServices` wrappers. Backend Legacy commands remain registered and unchanged. No Runtime UI behavior or canonical operation/status semantics changed. M1C3 remains unimplemented, and full P9-M1C is not yet complete.

#### P9-M1C2 Completion Record

- **P9-M1C2:** Completed
- **P9-M1C3:** Not started
- **Completion date:** 2026-07-19
- **Implementation commit:** `4105beafa2bf6014973e780de51a3ec07b520ea1`
- **Validation:** TypeScript type-check passed; production build passed; 108 Rust tests passed
- **Manual Smoke:** PASS; no Runtime action was executed
- **Repository integrity:** Repository remained clean; no dependency, manifest, lockfile, backend, or stored schema changed; nothing was pushed

Final P9-M1C2 guarantees:

1. Dead `useServices` and `useServiceActions` frontend paths are removed.
2. Unused `startSingleService`, `stopSingleService`, and `openSingleService` wrappers are removed.
3. No live frontend caller remains for `start_service`, `stop_service`, or `open_service`.
4. Individual Runtime lifecycle controls remain exclusively canonical.
5. Runtime status queries remain exclusively canonical.
6. Start All and Stop All remain the only live Legacy frontend Runtime lifecycle path.
7. Legacy Bulk remains isolated through `useLegacyBulkRuntimeActions` and `services/tauri.ts`.
8. Backend Legacy individual and Bulk commands remain registered and unchanged.
9. No Runtime UI behavior changed in M1C2.
10. No OpenClaw, MultiLLM, or unrelated page behavior changed.
11. M1C3 remains responsible for the backend compatibility decision and full P9-M1C closure.

This record completes P9-M1C2 only; it does not record full P9-M1C completion or start P9-M1C3.

## Out of Scope for P9-M1

- Unified Session
- Unified Tool and MCP
- Unified Workspace
- Provider architecture replacement
- OpenClaw protocol redesign
- System metrics migration
- New provider integrations
- UI redesign

# ADR-002: Runtime Lifecycle Operations

- **Status:** Accepted through P9-M1C1
- **Date:** 2026-07-19
- **Decision owners:** AI-OS

## Context

P9-M1A established canonical runtime discovery and status contracts while preserving legacy lifecycle IPC. P9-M1B requires observable lifecycle operations that remain queryable when events are missed, prevent conflicting work, and do not expose adapter or process details.

## Decision

AI-OS owns a canonical runtime operation contract with actions, states, timestamps, progress, results, normalized errors, cancellation metadata, and a monotonically increasing snapshot revision.

Accepted operations are inserted into an in-memory manager in the `queued` state at revision 1. Every accepted state or progress mutation increments the revision. Rejected mutations do not. The snapshot revision is the ordering field for the version-1 `runtime://operation` event payload planned for a later M1B increment; no separate event sequence is maintained.

The manager enforces these invariants:

- Terminal states are `succeeded`, `failed`, and `cancelled`.
- A terminal snapshot cannot transition again.
- Exactly one competing terminal transition can succeed within the running process.
- Results exist only for successful operations, errors only for failed operations, and completion timestamps only for terminal operations.
- `startedAt` is assigned when entering `running`.
- Start, stop, and restart reserve one lifecycle slot per runtime; open does not.
- The lifecycle slot is released exactly once when its operation becomes terminal.
- Cancellation requests return typed unsupported, too-late, and not-found errors and are idempotent once cancelling or cancelled.
- `request_cancellation` exclusively owns entry into `cancelling`; general transitions cannot bypass its authorization check, and operations with `cancellable: false` can never enter `cancelling` or `cancelled`.
- `cancelling` records that cancellation was requested, not that interruption is guaranteed. A completion race may terminate it as `succeeded`, `failed`, or `cancelled`, and the first accepted terminal transition is final.

Operations are process-local and are not persisted. Active operations are never evicted. Terminal operations are retained for 30 minutes and capped at 200 entries, with expired terminal entries removed before enforcing the cap. Cleanup occurs during creation, lookup, cancellation requests, and terminal transitions.

Operation IDs use the project's existing UUID v4 dependency and contain no runtime configuration or user data.

## Concurrency

The manager uses a mutex around short in-memory state mutations. No lock may be held while awaiting, sleeping, spawning processes, emitting events, or performing filesystem or network work. Poisoned locks return a sanitized operation-task error.

## M1B1 Boundary

This decision defines contracts and state management only. M1B1 does not register managed state or lifecycle IPC, emit events, execute runtime actions, delegate legacy commands, or integrate with the UI. Those integrations require later independently reviewable M1B increments.

Binding M1B2 follow-ups:

1. An operation-conflict IPC rejection must atomically include the existing operation ID or snapshot.
2. Canonical IPC exposure must enforce a bounded active-operation acceptance policy, especially for open operations that do not reserve lifecycle slots.

## M1B2A Execution-Plan Boundary

Every native lifecycle action must first produce an immutable AI-OS-owned execution plan. Plan validation freezes the runtime ID, action, effective location, explicit endpoint context, adapter variant, executable and argument array, verification strategy, and stable progress phases before execution begins. Plans contain no arbitrary metadata, credentials, raw configuration, unrestricted process output, or shell command strings.

Ollama and Open WebUI require explicit HTTP or HTTPS endpoints. Localhost and loopback addresses are local; other valid hosts are remote. Embedded credentials, hostless URLs, and non-network or executable schemes are rejected. OpenClaw derives its endpoint exclusively from the active native profile rather than accepting a request override. Gateway validation accepts `ws`, `wss`, `http`, and `https`, but browser opening is conservative: an already-safe HTTP or HTTPS browser endpoint without query or fragment data is accepted, while `ws` and `wss` endpoints are unsupported because AI-OS has no established dashboard conversion rule. Start and stop remain independent of browser-open support.

Remote endpoints support only `open`. Local lifecycle commands use fixed executable paths and argument arrays: the OpenClaw launch service identifier, a verified Homebrew Ollama service, Docker Desktop's fixed application and CLI paths, a confirmed Open WebUI container ID, and Cherry Studio's fixed application name and static graceful-quit script. No adapter supports cancellation in this increment, and broad process-name killing is forbidden.

Open WebUI planning distinguishes Docker installation, process, daemon inspection, container existence, container running state, and endpoint readiness. A missing container can be concluded only after a successful daemon inspection. Docker is never started automatically for an Open WebUI operation, and ambiguous container candidates are rejected.

Context collection is runtime- and action-specific. Preparation validates the runtime, rejects statically unsupported actions, validates and classifies explicit endpoints, and rejects remote local-lifecycle requests before native ownership or dependency discovery. Remote Ollama and Open WebUI management therefore cannot invoke Homebrew, process, Docker, container, or local readiness probes. Remote OpenClaw management cannot invoke UID, launchctl, or plist discovery. Docker open does not resolve a home directory or inspect a socket or daemon. An unloaded OpenClaw service freezes ordered `bootstrap` then `kickstart` commands; a loaded service freezes only `kickstart`.

`prepare_execution_plan` is the sole M1B2B-facing preparation boundary. It validates one request, performs only action-specific collection, freezes that context, and builds the typed plan atomically. Callers cannot supply a separately collected context. OpenClaw lifecycle plans retain no Gateway token and perform no WebSocket probe. Start succeeds only when a typed `launchctl print` inspection reports `Loaded`; stop succeeds only when it reports `NotLoaded`. Only the verified macOS service-not-found exit status may establish `NotLoaded`. Other nonzero exits, spawn failures, timeouts, malformed invocations, and internal wait failures map to `InspectionFailed` and can never produce a bootstrap or stopped no-op plan. An already-unloaded stop is a no-op that re-inspects before success.

OpenClaw lifecycle completion verifies launch-service state only. It does not claim Gateway authentication, health, readiness, or a fresh connection. Active Gateway readiness is deferred until a separately approved cancellable networking capability exists. M1B2B must keep lifecycle terminal success separate from any later runtime-status health or readiness refresh.

Open WebUI start is idempotent for running containers and starts only a confirmed stopped container. Stop is idempotent for a confirmed stopped container. Restart is accepted only for confirmed running states. Explicit no-op variants contain a validated container ID but no fake command. Ollama formula installation and Homebrew service ownership are separate facts: start accepts an approved installed formula, while stop requires evidence that Homebrew manages the service.

Every spawned native command has an elapsed-time deadline. AI-OS polls its owned child handle and, on timeout, terminates only that child before returning a sanitized error. Verification uses a total deadline that includes each bounded probe's execution time. Fixed executable paths are used, including `/usr/bin/curl`.

AI-OS derives the sole local Docker Desktop management target as the exact current-user `$HOME/.docker/run/docker.sock` and freezes its absolute `unix://` host. It does not read a Docker context, inherit the current context, or accept suffix-equivalent sockets. The same exact host is used by every `docker info`, `ps`, `inspect`, `start`, `stop`, and `restart` command. `DOCKER_HOST`, `DOCKER_CONTEXT`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`, and `DOCKER_CONFIG` are removed from every Docker child. Container commands run only after this exact daemon target has been verified. Docker command timeouts and spawn or inspection failures remain typed inspection failures and cannot prove that a container is missing or stopped.

Request, execution-plan, and planning-context Debug implementations expose only structural facts such as runtime ID, action, endpoint presence, location, variant, command count, and verification kind. They redact URLs, query and fragment data, home and socket paths, plist paths, tokens, raw arguments, and raw configuration by construction.

Docker container state is explicit: `running`, `exited`, `created`, `paused`, `restarting`, `removing`, `dead`, and unknown states are classified independently. Only `exited` and `created` are approved as stopped. Transitional, dead, and unknown states reject management plans. Readiness probes use a slower interval than child-process polling, and both remain governed by total elapsed-time deadlines. No-op plans revalidate readiness or stopped state before succeeding.

P9-M1B2A supplies internal planning, native execution, bounded verification, progress, dependency classification, and safe error primitives only. It does not register lifecycle IPC or managed Tauri state, emit events, execute through the operation manager, change legacy commands, or integrate with frontend code.

## M1B2B1 Admission and Managed-State Boundary

AI-OS manages exactly one `RuntimeExecutionState`, which contains exactly one shared `Arc<RuntimeOperationManager>`. The manager remains the only operation store; managed state contains no plans, task map, status cache, or duplicate snapshots.

Admission is one locked accepted/conflict/rejected decision. Every queued, running, or cancelling operation counts toward a global limit of 16, including open operations. Open does not reserve a lifecycle slot; start, stop, and restart reserve one slot per runtime. Conflict detection precedes capacity rejection and clones the exact existing canonical snapshot under the same lock. Missing, terminal, mismatched-runtime, and otherwise invalid lifecycle-slot references are removed under that lock before admission continues. Capacity rejection uses the distinct retryable `operation-capacity-exceeded` error.

Progress mutation explicitly returns `Applied` for a distinct update and `Unchanged` for a byte-for-byte duplicate. Only `Applied` changes `updatedAt` and increments revision. M1B2B2 may therefore emit only meaningful progress changes without inferring mutation from snapshot values.

M1B2B1 registers managed state only. It adds no lifecycle IPC, execution supervisor, plan wiring, runtime event emission, frontend service wrapper, or UI integration. Those remain M1B2B2 and M1B2B3 work.

## M1B2B2 Execution, IPC, and Event Boundary

Canonical lifecycle requests now pass through typed, non-native static preflight before admission. Preflight validates the registry-owned runtime ID, statically supported action, and any required explicit Ollama or Open WebUI endpoint. It freezes the parsed endpoint and location, rejects remote local-lifecycle requests before admission, and performs no native inspection. OpenClaw remains the exception: its mutable active profile is read and frozen once during post-admission preparation.

Static preflight also freezes the Registry-owned `RuntimeAdapterKind`. Lifecycle support, endpoint requirements, context collection, and plan/context matching dispatch exhaustively through that adapter kind rather than a duplicate Runtime-ID list. A Registry-valid adapter without an approved lifecycle path is rejected as unsupported before admission.

An accepted operation returns its revision-1 queued snapshot immediately after best-effort queued emission and caller-independent Supervisor scheduling. There is no start gate. The sequential Supervisor owns the validated request and operation ID, prepares one frozen plan within a 30-second absolute deadline, requests the queued-to-running transition, and executes only after that transition succeeds. Preparation failure or panic terminalizes the queued operation without native execution. Execution success, normalized failure, or panic produces at most one Manager-accepted terminal transition.

Every accepted queued, running, distinct-progress, and terminal snapshot is emitted best-effort as `runtime://operation` with `RuntimeOperationEvent { version: 1, operation }`. Events always contain the full canonical snapshot, use its revision as the only ordering field, and are emitted after Manager methods release their lock. Duplicate or late progress, conflicts, capacity rejection, lookup, and rejected cancellation emit nothing. Event delivery never changes operation truth.

Cancellation and terminalization expose explicit mutation outcomes. An accepted cancellation mutation emits the same canonical event; an idempotent unchanged cancellation does not. Failure terminalization distinguishes an applied failed transition from an already-terminal winner, and only the applied transition emits. Scheduling rejection or panic must return an accepted terminal snapshot or a sanitized Manager error and can never fall back to an orphaned queued snapshot.

All canonical event emission is isolated from both ordinary emitter errors and emitter panics. Neither can revise state, fail successful execution, or trigger another terminal transition. Panic payloads are excluded from canonical state, IPC, and events. B2 does not alter or claim suppression of the process-global Rust panic hook or all process diagnostics.

The additive native IPC boundary contains exactly `start_runtime_operation`, `get_runtime_operation`, and `cancel_runtime_operation`. All current adapters remain non-cancellable, so active cancellation returns `cancellation-unsupported` and terminal cancellation returns `cancellation-too-late` without mutation or emission. Lifecycle completion updates only operation state; it does not refresh runtime status, health, readiness, or OpenClaw Gateway connectivity.

M1B2B2 itself does not add frontend wrappers or listeners, migrate legacy commands or UI, persist operations, or implement status reconciliation. The typed client boundary is implemented separately in M1B2B3; M1C remains unimplemented.

## M1B2B3 Typed Client Boundary

The unused TypeScript Runtime service now exposes typed wrappers for `start_runtime_operation`, `get_runtime_operation`, and `cancel_runtime_operation` without rewriting normalized backend errors or delegating to legacy commands. The start request retains the canonical `{ runtimeId, action, endpointUrl? }` shape and returns the accepted/conflict/rejected admission union.

One listener wrapper subscribes to exactly `runtime://operation`, accepts only version-1 payloads with a present operation object, and passes the full canonical snapshot to its caller. Unknown versions and malformed payloads are ignored. The wrapper returns Tauri's exact unlisten function and retains no global subscription state; future consumers own listener lifecycle.

Snapshot reconciliation uses only operation ID and revision. A null current value accepts the incoming snapshot, higher revisions replace current state, and equal or lower revisions preserve it. Mismatched operation IDs produce a fixed payload-free error. The helper does not merge fields, mutate inputs, or use timestamps or arrival order.

M1B2B3 adds no React consumer, UI migration, runtime-status refresh, persistence, or legacy delegation. It was implemented separately from B1 and B2. Complete M1B2B approval and merge remain pending final review and manual integration validation; M1C remains unimplemented.

## Consequences

- Native operation state, not event delivery, will be the source of truth.
- Late subscribers will recover state through operation lookup once IPC is added.
- Operations are lost on application exit by design.
- Initial runtime adapters do not advertise genuine cancellation.
- Legacy native lifecycle commands and bulk UI behavior remain available during the staged frontend migration.

## M1C1 Canonical UI State and Individual Controls

The Dashboard and Services individual Start, Stop, and Open controls consume the canonical typed Runtime client through one App-owned `useRuntimeOperations` hook. Canonical identity comes directly from `RuntimeDefinition.id` and `RuntimeStatus.id`; display labels never derive identity, and only Ollama and Open WebUI select configured endpoints by canonical ID. No frontend OpenClaw endpoint or token is sent to lifecycle IPC.

The hook owns one `operationsById` full-Snapshot store, synchronous per-runtime lifecycle/Open submission channels, one listener registration state, terminal-refresh deduplication, and coalesced refresh coordination. Per-runtime active lifecycle, active Open, and latest terminal views are derived rather than stored as duplicate Snapshots. One `runtime://operation` listener is registered at the App boundary, gates individual admission until ready, reconciles responses, lookups, and events only by operation ID and Revision, and calls the exact unlisten function during cleanup, including late listener registration.

Conflicts attach to the returned canonical existing Snapshot without retry. Terminal Start, Stop, and Restart outcomes schedule a separate best-effort canonical Runtime-status refresh, while Open never refreshes status. Operation completion never directly changes Runtime lifecycle, health, readiness, or OpenClaw connectivity. Fixed action/code-based notifications exclude operation IDs, revisions, endpoints, raw native details, and arbitrary progress messages.

Start All and Stop All remain isolated legacy compatibility behavior pending a separate approved bulk design. No Restart or Cancel control was added. Backend legacy commands, unused legacy frontend individual wrappers/hooks, persistence, operation history, and active/recent list IPC remain unchanged for M1C2/M1C3 review.

M1C1 code-review hardening separates Runtime-definition loading from status loading so either successful result is preserved independently. `useRuntimes` is the sole global status-query coordinator: one request may be active, overlapping callers share at most one trailing query, and stale responses cannot overwrite later status. It exposes definition-loading and status-refreshing state separately for the existing manual refresh UX.

The canonical operation listener is mount-once and reads current ingestion and notification behavior through narrow refs, so endpoint, refresh, or callback changes cannot create a listener gap. Terminal feedback and lifecycle refresh are transition-based and bounded by the retained Snapshot store rather than an unbounded handled-ID set. The operation hook rejects a locally known active lifecycle/Open channel before admission and derives a nonterminal `hasCanonicalActivity` flag.

Legacy Bulk and canonical work are mutually exclusive at both UI and handler boundaries. Lifecycle toggles require a stable canonical `running` or `stopped` status; unknown, transitional, and failed statuses cannot be interpreted as Start. Admission rejections and thrown IPC failures are classified only through the canonical error-code allowlist and fixed safe messages. M1C2 and M1C3 remain unimplemented.

Final M1C1 review hardening extends Legacy Bulk isolation through its existing stabilization window: Start All releases only after the 45-second refresh settles, and Stop All releases only after the 8-second refresh settles. Owned timers are cleared on unmount. Ref-backed `isCanonicalActivityActive` and `isBulkIsolationActive` guards close both pre-render cross-boundary races while rendered state continues to disable the controls.

Configured Ollama or Open WebUI endpoint changes schedule one refresh through the same global coordinator without reloading definitions. Each query captures an endpoint generation; an old-generation response is not applied, and the coordinator runs one trailing query against the latest endpoints. Safe invoke-error classification accepts allowlisted codes from objects, bounded JSON strings, or exact code strings without reading or exposing backend messages or raw values. M1C2 and M1C3 remain unimplemented.

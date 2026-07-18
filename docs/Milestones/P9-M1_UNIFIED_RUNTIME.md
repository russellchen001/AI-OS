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

Binding M1B2 follow-ups:

1. Operation-conflict IPC rejection must atomically include the existing operation ID or snapshot.
2. Canonical IPC exposure must apply a bounded active-operation acceptance policy, especially to open operations that do not reserve lifecycle slots.

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

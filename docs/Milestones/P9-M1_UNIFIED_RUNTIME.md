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

## M1B — Lifecycle Operations, Progress, and Cancellation

Deferred scope:

- Typed start, stop, restart, and open operations
- Operation IDs and terminal operation states
- Progress events
- Idempotent cancellation where supported
- Runtime-scoped normalized lifecycle errors
- macOS process-management isolation
- Compatibility façades for legacy lifecycle commands

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

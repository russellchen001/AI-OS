# ADR-002: Runtime Lifecycle Operations

- **Status:** Accepted for P9-M1B1
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

Operations are process-local and are not persisted. Active operations are never evicted. Terminal operations are retained for 30 minutes and capped at 200 entries, with expired terminal entries removed before enforcing the cap. Cleanup occurs during creation, lookup, cancellation requests, and terminal transitions.

Operation IDs use the project's existing UUID v4 dependency and contain no runtime configuration or user data.

## Concurrency

The manager uses a mutex around short in-memory state mutations. No lock may be held while awaiting, sleeping, spawning processes, emitting events, or performing filesystem or network work. Poisoned locks return a sanitized operation-task error.

## M1B1 Boundary

This decision defines contracts and state management only. M1B1 does not register managed state or lifecycle IPC, emit events, execute runtime actions, delegate legacy commands, or integrate with the UI. Those integrations require later independently reviewable M1B increments.

## Consequences

- Native operation state, not event delivery, will be the source of truth.
- Late subscribers will recover state through operation lookup once IPC is added.
- Operations are lost on application exit by design.
- Initial runtime adapters do not advertise genuine cancellation.
- The current legacy lifecycle implementation and UI remain unchanged.

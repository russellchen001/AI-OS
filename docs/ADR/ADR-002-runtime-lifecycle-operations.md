# ADR-002: Runtime Lifecycle Operations

- **Status:** Accepted through P9-M1B2A
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

Ollama and Open WebUI require explicit HTTP or HTTPS endpoints. Localhost and loopback addresses are local; other valid hosts are remote. Embedded credentials, hostless URLs, and non-network or executable schemes are rejected. OpenClaw derives its endpoint exclusively from the active native profile rather than accepting a request override. Existing OpenClaw status classification remains compatible with WebSocket profiles, while native open actions require a separately validated HTTP or HTTPS endpoint.

Remote endpoints support only `open`. Local lifecycle commands use fixed executable paths and argument arrays: the OpenClaw launch service identifier, a verified Homebrew Ollama service, Docker Desktop's fixed application and CLI paths, a confirmed Open WebUI container ID, and Cherry Studio's fixed application name and static graceful-quit script. No adapter supports cancellation in this increment, and broad process-name killing is forbidden.

Open WebUI planning distinguishes Docker installation, process, daemon inspection, container existence, container running state, and endpoint readiness. A missing container can be concluded only after a successful daemon inspection. Docker is never started automatically for an Open WebUI operation, and ambiguous container candidates are rejected.

P9-M1B2A supplies internal planning, native execution, bounded verification, progress, dependency classification, and safe error primitives only. It does not register lifecycle IPC or managed Tauri state, emit events, execute through the operation manager, change legacy commands, or integrate with frontend code.

## Consequences

- Native operation state, not event delivery, will be the source of truth.
- Late subscribers will recover state through operation lookup once IPC is added.
- Operations are lost on application exit by design.
- Initial runtime adapters do not advertise genuine cancellation.
- The current legacy lifecycle implementation and UI remain unchanged.

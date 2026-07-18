# ADR-002: Runtime Lifecycle Operations

- **Status:** Accepted through P9-M1B2B1
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

## Consequences

- Native operation state, not event delivery, will be the source of truth.
- Late subscribers will recover state through operation lookup once IPC is added.
- Operations are lost on application exit by design.
- Initial runtime adapters do not advertise genuine cancellation.
- The current legacy lifecycle implementation and UI remain unchanged.

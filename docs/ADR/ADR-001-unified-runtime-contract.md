# ADR-001: Unified Runtime Contract

- **Status:** Accepted for P9-M1A
- **Date:** 2026-07-18
- **Decision owners:** AI-OS

## Context

AI-OS currently manages OpenClaw, Ollama, Docker Desktop, Open WebUI, and Cherry Studio through service-specific native logic and a small frontend service model. Health is returned as formatted text, while OpenClaw and provider integrations maintain separate richer status models.

P9-M1 requires an AI-OS-owned Runtime contract without breaking the existing Dashboard, Services, Settings, Tauri commands, or stored configuration.

## Decision

AI-OS owns a canonical Runtime contract with separate fields for:

- Stable identity and adapter kind
- Supported platform
- Runtime location
- Dependencies and capabilities
- Availability
- Lifecycle
- Health
- Readiness
- Observation time
- Safe normalized error

Runtime location is one of `local`, `remote`, or `hybrid`. Location is not a capability. Capabilities describe supported operations and are represented separately.

The initial registry is static and contains exactly:

- `openclaw`
- `ollama`
- `docker-desktop`
- `open-webui`
- `cherry-studio`

Native adapters translate each integration into the canonical contract. They do not expose arbitrary JSON metadata. OpenClaw remains an adapter: its active profile supplies location and cached connection status, while its protocol and storage schema remain internal to the existing OpenClaw module.

P9-M1A adds only read-only `list_runtimes` and `get_runtime_statuses` commands. Existing commands remain registered and unchanged.

## Compatibility

- Existing lifecycle and health IPC remains available.
- Existing UI continues using `useServices`.
- Existing `ai-os-settings` data remains unchanged.
- Existing OpenClaw configuration, migration, and export formats remain unchanged.
- A failure in the OpenClaw adapter produces an OpenClaw error record and does not prevent other runtime records from returning.

## Consequences

The project temporarily has both legacy service contracts and the canonical Runtime contract. This is intentional until the frontend migration in P9-M1C. Lifecycle operations and operation events are deferred to P9-M1B.

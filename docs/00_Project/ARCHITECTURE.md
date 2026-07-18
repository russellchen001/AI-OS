# AI-OS Architecture

## Overall Architecture

AI-OS is a Tauri desktop application with a React/TypeScript webview and a Rust native backend.

```text
React pages and components
        |
Hooks and frontend domain services
        |
Typed Tauri commands and events
        |
Rust runtime and integration modules
        |
Local services, provider APIs, OpenClaw, and the filesystem
```

The frontend owns presentation and user-driven orchestration. The native layer owns privileged operations such as process management, filesystem access, backups, system metrics, native configuration, and streaming network connections.

AI-OS is evolving toward four unified domain layers—Runtime, Session, Tool, and Workspace—with providers supplying model capabilities across them.

## Canonical Ownership

AI-OS owns the canonical domain models and configuration for the Runtime, Provider, Session, Tool, and Workspace layers. These contracts express AI-OS product behavior independently from any external system.

External runtimes and gateways integrate through adapters that translate their protocols and capabilities into AI-OS contracts. OpenClaw therefore maps into the AI-OS Runtime, Provider, Session, Tool, and Workspace contracts; it does not define them.

Core AI-OS modules must continue operating when OpenClaw is unavailable. An OpenClaw connection failure may limit OpenClaw-backed capabilities, but must not prevent independent providers, runtimes, sessions, tools, or workspace functions from operating.

## Runtime Layer

The Runtime Layer represents where and how AI work executes.

Responsibilities:

- Discover and report runtime availability and health
- Start, stop, open, and monitor supported local services
- Execute and cancel long-running operations
- Stream progress and model output to the frontend
- Normalize runtime status and failures
- Isolate operating-system-specific process behavior

Current runtime integrations include Ollama, OpenClaw, Docker Desktop, Open WebUI, and Cherry Studio. Tauri commands provide the native boundary, while React hooks translate command results and events into UI state.

The Unified Runtime planned for Phase 9 should provide one runtime contract instead of requiring features to understand each native integration independently.

## Provider Layer

The Provider Layer describes model vendors and endpoints independently from the runtime that connects to them.

The current provider baseline is:

- OpenAI
- Anthropic Claude
- Google Gemini
- xAI Grok
- Ollama

Responsibilities:

- Provider identity and display metadata
- Endpoint, authentication, model, and token-limit configuration
- Request adaptation for OpenAI-compatible, Anthropic, Ollama, and future protocols
- Provider health and readiness classification
- Streaming response normalization
- Usage, latency, cost, and error metadata

Provider credentials are sensitive and must not be exposed in logs, analytics, or default exports. Provider-specific request formats should terminate at an adapter boundary; pages and sessions should consume normalized results.

Phase 10 adds Doubao and Meta AI by extending the existing Provider Layer contracts and adapters rather than redesigning the layer.

## Session Layer

The Session Layer represents durable AI interactions and workflows.

Examples include:

- MultiLLM comparison conversations
- Routed single-provider conversations
- AI Council runs and individual role steps
- OpenClaw conversations
- Future AI Arena matches

A unified session should carry a stable identifier, timestamps, mode, prompt/messages, participants, provider/model/runtime references, status, outputs, errors, usage data, and links to generated artifacts. Session persistence must be versioned and migratable.

The Unified Session planned for Phase 9 should allow history, analytics, export, search, and restoration to work consistently across AI-OS features.

## Tool Layer

The Tool Layer represents capabilities an AI session or user can invoke beyond plain model completion.

Responsibilities:

- Stable tool identity and descriptive metadata
- Typed or schema-defined inputs and outputs
- Availability and permission state
- Invocation lifecycle, cancellation, timeout, and error handling
- Audit-friendly results without leaking secrets
- MCP and OpenClaw tool discovery and invocation

MCP servers are one source of tools, while OpenClaw may expose another catalog through its gateway. The Unified Tool planned for Phase 9 should normalize these sources without erasing provider-specific capabilities.

## Workspace Layer

The Workspace Layer owns durable user-created and AI-created material.

It includes:

- Artifact projects and artifacts
- Prompt templates
- Conversation and council history
- Imports and exports
- Tags, favorites, source attribution, and provider metadata
- Backup and restore boundaries

Workspace formats must be versioned, validated, and backward compatible. Imported HTML, SVG, Markdown, archives, and JSON are untrusted input and must be handled accordingly. The Unified Workspace planned for Phase 9 should establish shared storage, migration, search, linkage, and export conventions.

## OpenClaw Integration Position

OpenClaw is a first-class external runtime and gateway within AI-OS. It provides remote/local server profiles, authenticated WebSocket connectivity, status information, sessions, messages, tools, MCP visibility, and workspace-related gateway operations.

OpenClaw sits across several architectural layers:

- **Runtime:** connection state, health, and gateway execution
- **Provider:** access to model-backed capabilities exposed by OpenClaw
- **Session:** OpenClaw conversations and messages
- **Tool:** OpenClaw and MCP tool catalogs and invocations
- **Workspace:** data and artifacts surfaced through the gateway

OpenClaw must remain behind explicit integration contracts. AI-OS core models must support OpenClaw deeply without becoming dependent on its protocol, storage format, or availability. Protocol adaptation, authentication, and connection management belong in the native OpenClaw integration module.

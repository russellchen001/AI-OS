# AI-OS Roadmap

This roadmap defines the path to the AI-OS v1.0 production baseline. Phase contents may be refined through architecture review, but phase outcomes should remain stable unless the roadmap is deliberately revised.

## Completed

### Dashboard

Local service status, system metrics, operational summaries, analytics, recent activity, and quick actions.

### MultiLLM Hub

Parallel provider comparison, prompt routing, provider configuration, streaming responses, history, and usage analytics.

### Prompt Library

Reusable prompt templates with categorization, search, editing, import/export, and hand-off to MultiLLM workflows.

### AI Council

Role-based multi-model deliberation with planner, engineer, researcher, critic, and judge stages, session history, and export.

### Artifacts Workspace

Project-based artifact management, rich previews, metadata, bulk operations, and workspace import/export.

### Settings

Application preferences for themes, refresh behavior, logs, backups, and local service endpoints.

## Phase 9 — OpenClaw Deep Integration & Unified Foundation

### P9-M1 — Unified Runtime

Create a common runtime contract for discovery, health, lifecycle operations, streaming, cancellation, and normalized errors.

### P9-M2 — Unified Session

Create a durable session model shared by MultiLLM, AI Council, OpenClaw, and future conversational workflows.

### P9-M3 — Unified Tool & MCP

Normalize tool catalogs and invocation across MCP, OpenClaw, and future runtime/provider integrations.

### P9-M4 — Unified Workspace

Unify persistence, migrations, search, relationships, metadata, import/export, backup, and recovery across workspace data.

## Phase 10 — Provider Expansion

### P10-M1 — Doubao

Add Doubao provider configuration, health checks, request adaptation, streaming, model selection, and usage metadata.

### P10-M2 — Meta AI

Add Meta AI support through a documented provider/runtime path with normalized streaming, errors, and session metadata.

## Phase 11 — AI Arena

Create an evaluation workspace where multiple models can respond to common tasks and be compared using explicit criteria, reproducible configurations, recorded results, and human or model-assisted judging.

## Phase 12 — Beta & Polish

- Complete end-to-end workflow validation
- Resolve release-blocking reliability and usability issues
- Harden credential handling and native permissions
- Validate data migrations, backup, restore, import, and export
- Improve accessibility, empty states, error recovery, and performance
- Complete production documentation and release checklists
- Run a controlled beta and incorporate prioritized feedback

## AI-OS v1.0

AI-OS v1.0 is reached when the completed product surfaces and Phases 9–12 operate as one stable, documented, backward-compatible desktop product with no known release-blocking defects.

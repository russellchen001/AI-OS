# AI-OS Coding Assistant Instructions

- AI-OS is production software built with React, TypeScript, Tauri, and Rust.
- Favor stability, readability, maintainability, security, and backward compatibility over cleverness.
- When a task is marked PLAN ONLY, do not create, modify, rename, or delete files. Return analysis and a proposed plan only.
- Read `docs/00_Project/AI_OS_CONSTITUTION.md`, `ARCHITECTURE.md`, and `DEVELOPMENT_WORKFLOW.md` before proposing cross-cutting changes.
- Preserve existing application behavior, stored data, exports, Tauri IPC contracts, and provider integrations unless a change explicitly requires otherwise.
- Keep React presentation, frontend orchestration, native operations, and provider protocol adaptation behind clear boundaries.
- Do not place credentials in source, logs, analytics, examples, or default exports.
- Treat external responses, imported files, paths, HTML/SVG, and command inputs as untrusted.
- Prefer small, focused changes. Do not combine unrelated refactors with features or fixes.
- Avoid duplicating types, constants, provider metadata, or domain rules; extend the canonical definition.
- Isolate macOS-specific behavior and do not imply cross-platform support without implementation and validation.
- Use typed commands/events and actionable errors for Tauri operations. Long-running work should expose progress, cancellation, and terminal state.
- Add or update tests and documentation in proportion to the risk. Never claim a build or test passed unless it was run.
- Record significant architectural decisions in `docs/ADR/` and milestone scope in `docs/Milestones/`.
- Follow: PLAN → Architecture Review → Implementation → Build → Manual Test → Code Review → Merge.

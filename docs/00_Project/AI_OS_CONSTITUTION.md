# AI-OS Constitution

## Project Vision

AI-OS is a local-first desktop AI operating system and control plane that orchestrates AI providers, runtimes, sessions, tools, workflows, artifacts, and collaborative intelligence through one unified desktop experience.

The project should make complex AI workflows understandable and operable without hiding important state from the user. AI-OS v1.0 is intended to be a stable foundation that can evolve without repeatedly replacing its core concepts or breaking existing workflows.

## Core Principles

1. **Local-first control.** Prefer local execution, local ownership of data, and explicit user control. Remote integrations must be visible and intentional.
2. **Reliability before novelty.** Stable, recoverable behavior is more valuable than clever abstractions or premature features.
3. **Clear boundaries.** Runtime, provider, session, tool, workspace, and presentation concerns must have explicit contracts.
4. **Backward compatibility.** Preserve user data, stored settings, IPC contracts, exports, and established workflows whenever practical.
5. **Security and privacy by design.** Minimize privileges, protect credentials, validate external data, and avoid exposing secrets in logs or exports.
6. **Observable operations.** Long-running or failure-prone operations must communicate progress, completion, cancellation, and actionable errors.
7. **Human-readable systems.** Favor straightforward code, explicit state, documented decisions, and predictable naming.

## Architecture Rules

- AI-OS is the primary application and system owner.
- AI-OS owns the canonical Runtime, Provider, Session, Tool, and Workspace contracts.
- React and TypeScript own presentation, user interaction, and feature composition.
- Rust and Tauri own privileged operating-system access, native persistence, process control, and network operations that must not execute in the webview.
- Frontend-to-native communication must use typed, documented Tauri commands and events.
- Domain logic should live outside large page components when it can be expressed as a reusable service, hook, reducer, or native module.
- Provider-specific behavior must be isolated behind provider contracts. Core session and workspace models must not depend on a single vendor.
- Sessions must retain enough metadata to identify their runtime, provider, model, mode, and related artifacts.
- Tools must declare stable identities, inputs, outputs, availability, and failure behavior.
- Workspace data must use versioned formats and explicit migrations. Existing stored data must not be silently discarded.
- OpenClaw is an integrated runtime and gateway adapter.
- OpenClaw capabilities must map into AI-OS contracts.
- AI-OS core contracts must not be derived from OpenClaw protocols, storage formats, or internal architecture.
- New cross-cutting architectural decisions must be recorded in `docs/ADR/`.
- Platform-specific behavior must be isolated and clearly documented.

## Development Principles

- Plan changes before implementation and identify affected contracts and stored data.
- Prefer small, reviewable changes with one clear responsibility.
- Do not mix unrelated refactors with feature or defect work.
- Treat TypeScript and Rust warnings, build failures, and unexplained runtime errors as defects.
- Validate inputs at trust boundaries and return useful, non-secret-bearing errors.
- Preserve public behavior unless a deliberate migration or breaking change has been approved and documented.
- Add tests around domain logic, migrations, protocol handling, and regressions as the test foundation grows.
- Update documentation and ADRs in the same change as the behavior they describe.
- Avoid new dependencies unless their ownership cost and security impact are justified.
- Prefer removal of duplication and clarification of ownership over additional abstraction layers.

## Milestone Workflow

Each milestone follows this lifecycle:

1. Define the outcome, scope, exclusions, and acceptance criteria in `docs/Milestones/`.
2. Review architecture impact, compatibility risk, data migration, security, and recovery behavior.
3. Record significant decisions in `docs/ADR/`.
4. Implement in small, independently verifiable changes.
5. Complete the required build and manual test workflow.
6. Review code, documentation, and acceptance criteria together.
7. Merge only when the milestone increment is stable and leaves the main branch usable.
8. Update the roadmap and milestone status after validation.

## Version Target: v1.0

AI-OS v1.0 is the first production-quality baseline. It includes the completed product surfaces, the unified runtime/session/tool/workspace foundation, planned provider expansion, AI Arena, and beta polish described in the roadmap.

The v1.0 target requires:

- Stable core workflows and documented architecture
- Backward-compatible user data and configuration handling
- Clear provider and OpenClaw integration boundaries
- Reliable cancellation, error reporting, backup, and recovery behavior
- Security review of credentials, native permissions, imported content, and external operations
- Repeatable builds and documented manual release validation
- No known release-blocking defects

# AI-OS UI Specification

**Version:** 1.0
**Status:** Product and design specification
**Date:** 2026-08-01

## 1. Document Authority

This document owns AI-OS information architecture, page behavior, and interaction requirements. `AI_OS_MASTER_GUIDE.md` remains authoritative for product direction, architecture, roadmap, and scope. `AI_OS_PRODUCT_VISION.md` owns vision and brand; `AI_OS_DESIGN_SYSTEM.md` owns visual and component rules; `AI_OS_FIGMA_BLUEPRINT.md` owns design-file delivery. The Master Guide prevails on conflict.

This specification describes a target experience. “Current implementation” statements are observations, not completion claims.

## 2. Design Philosophy

**AI-OS is a workspace, not a developer dashboard.** The primary journey is objective → conversation → task/plan → execution → approval → result/artifact. Operational detail is available when needed but does not dominate ordinary use.

The interface must feel bright, calm, understandable, local-first, and trustworthy. The light theme is white-first: the main workspace uses white as its dominant field, while soft neutral surfaces and borders create hierarchy without making the product feel grey or heavy. It progressively discloses complexity and reports what AI-OS is doing in plain language.

## 3. UX Principles

1. Start from user intent, not provider or tool selection.
2. Make system status, external effects, cost, and uncertainty visible.
3. Ask for approval immediately before consequential action.
4. Preserve user control: pause, cancel, retry, inspect, and recover.
5. Prefer one shared pattern over page-specific controls.
6. Never imply that mock, unavailable, or planned behavior is working.
7. Provide keyboard, screen-reader, reduced-motion, and responsive access.

## 4. Information Architecture

Primary navigation:

- Workspace / Chat
- My AI
- Agents
- AI Council / AI 智囊团
- AI Arena / AI 竞技场
- Artifacts
- Settings

Conversation history belongs beside Workspace. Task and Artifact panels are contextual workspace surfaces, not unrelated destinations. Dashboard, Services, Runtime, OpenClaw, Models, Logs, Backup, MultiLLM, Prompt Library, and similar operational pages migrate into clearly labeled Settings sections; they may remain reachable during transition.

## 5. Global Navigation

Desktop uses a persistent sidebar with product identity, New conversation, primary destinations, recent conversations, search/command entry, connection state, and Settings. Tablet may collapse labels. Mobile uses a drawer or bottom-level destination switcher while preserving New conversation and current task access.

Navigation must preserve unsent input or ask before discarding it. Deep links restore the destination and selected entity. Hidden legacy pages must remain discoverable in Settings during migration.

## 6. Workspace and Chat

Target desktop layout: navigation | conversation workspace | optional contextual panel. The center contains conversation title, message timeline, composer, attachments, active AI/agent summary, and task state. The contextual panel switches between Task and Artifacts without losing state.

Chat accepts Ask and Do requests. It displays streaming output, citations or sources where available, tool/task events summarized in human language, recoverable errors, and final outcomes. The composer supports attachments, stop, retry, and explicit mode or agent selection only when useful.

**Current implementation:** `ChatPage`, sidebar conversation controls, local conversation services, and backend conversation/provider work exist on `feature/ui-refactor`. Their compilation does not prove persistence, full Task Engine routing, approvals, or target panel behavior.

## 7. Conversation History

Provide create, select, rename, search, archive, and delete. Show title, updated time, task-state marker, and optional project grouping. Destructive deletion requires confirmation and explains artifact/task retention. Empty history offers a useful first prompt. Loading uses stable skeletons; failures preserve local drafts and offer retry.

## 8. Task Panel

The Task panel shows objective, Ask/Do classification, status, plan steps, current execution, approvals, progress, result, errors, and safe actions. Supported states include Created, Understanding, Planning, Ready, Executing, Waiting for approval, Verifying, Recovering, Completed, Failed, and Cancelled.

The panel reads Task Engine/Planner/Runtime truth. It never fabricates progress or directly executes tools. Approve and deny actions must identify the effect, target, permissions, and reversibility.

## 9. Artifacts

Artifacts include documents, code, images, spreadsheets, presentations, reports, data, and imported files. Users can preview, inspect provenance, rename, export, reuse, and relate an artifact to its conversation/task. Generated outputs enter the artifact model rather than becoming anonymous chat attachments. Permission and storage location are visible.

## 10. My AI

My AI manages providers, accounts or API credentials, local/cloud models, capabilities, connection status, routing preferences, usage, cost, latency, and privacy. Provider cards distinguish configured, connected, degraded, unavailable, and unsupported states. Secret values are never re-displayed or logged.

**Current implementation:** `MyAiPage` and provider services establish a UI direction. Connection, discovery, OAuth, durable secret storage, and full AI Center ownership remain subject to implementation and verification.

## 11. Agents

Agents presents Runtime-controlled executors such as OpenClaw and future adapters through a shared registry. Each entry shows capabilities, health, location, permissions, current operations, logs, and configuration. “Installed,” “configured,” “connected,” and “healthy” are distinct truths. Agent UI cannot invoke adapters around Runtime or permission gates.

## 12. AI Council / AI 智囊团

Council helps users improve decision quality. Flow: define decision → Chief of Staff clarifies objective → proposed dynamic expert team → user review where needed → structured expert discussion → dissent/consensus → final report with assumptions, confidence, risks, and next actions.

Council is not model comparison. Experts represent useful perspectives, not necessarily distinct providers. All model calls pass through AI Center. Council does not own Task state, planning, or execution. The UI distinguishes team proposal, discussion, synthesis, and report; it never presents a static list as the complete Council product.

## 13. AI Arena / AI 竞技场

Arena supports Compare, Debate, Collaboration, Competition, Role Play, Game, and Simulation. Setup defines mode, objective, participants, roles, models, rules, rounds, judging/voting, user or audience participation, limits, cost budget, and stopping conditions. Live view shows turns, state, events, participation controls, scores, and moderation. Results include replay/history and a reproducible record.

Werewolf is a representative extensible Game/Role Play template, not an architectural special case. Compare retains blind review, side-by-side outputs, scoring, cost, and latency. Arena serves controlled interaction, experimentation, entertainment, and research; Council serves user decisions. Every model call passes through AI Center.

**Current implementation:** `AiArenaPage` is an early surface and must not be described as implementing the complete mode engine.

## 14. Settings and Legacy Operations

Settings groups: My AI; Agents and Runtime; Skills and MCP; Permissions and approvals; Memory and privacy; Devices/integrations; Appearance/accessibility; Data, backup, and export; Logs/diagnostics; Advanced. Operational pages are nested here with plain-language summaries and an advanced-details disclosure.

Migration must preserve existing functionality and deep links until replacement sections reach parity. No legacy capability is silently deleted.

## 15. Responsive Behavior

- Desktop ≥1200px: persistent navigation, main workspace, optional 320–400px context panel.
- Tablet 768–1199px: compact navigation; context panel overlays or replaces secondary content.
- Mobile <768px: single primary pane; drawers/sheets for history, tasks, artifacts, and setup.

Touch targets are at least 44px. Composer, approval action, active task, and Arena turn state remain reachable without horizontal scrolling.

## 16. Accessibility

Meet WCAG 2.2 AA intent: semantic landmarks, logical headings, visible focus, keyboard operation, accessible names, contrast-compliant tokens, non-color state cues, live-region restraint, error association, caption/transcript support, 200% zoom, and reduced motion. Focus returns predictably after dialogs and route changes.

## 17. System States

Every major surface defines: empty, loading, partial, success, recoverable error, terminal error, offline, permission denied, approval required, cancelled, and unsupported. Skeletons preserve geometry. Error text states what happened, what was not changed, and the safe next action. Offline mode distinguishes cached/local capability from unavailable cloud capability.

## 18. Interaction and Architecture Boundaries

The UI must not bypass:

- Task Engine for request and Task ownership
- Planner for multi-step planning
- AI Center for provider/model access, including Council and Arena
- Skill Framework and permission checks for capabilities
- Runtime for execution, scheduling, supervision, cancellation, and recovery

UI services may adapt IPC and view state, but must not duplicate domain truth, call providers directly, invoke OpenClaw directly, or claim success before backend confirmation.

## 19. Component Reuse

Use shared primitives and domain components from the Design System. Conversation, task, provider, agent, Council participant, Arena participant, status, approval, empty state, error state, and artifact provenance patterns must not be independently reimplemented without an approved variant.

## 20. Acceptance Criteria

- Primary navigation matches this information architecture and legacy operations remain accessible in Settings.
- Ask/Do, task, approval, error, offline, and completion states are truthful and accessible.
- Workspace supports conversation plus contextual Task/Artifacts without data loss.
- My AI and Agents distinguish configuration, connection, and health.
- Council demonstrates Chief of Staff dynamic assembly and decision report boundaries.
- Arena designs cover all seven modes, reproducibility, participation, replay, voting/scoring, cost, and latency.
- Responsive and keyboard flows pass documented checks.
- No UI path bypasses Task Engine, Planner, AI Center, Skill/permission boundaries, or Runtime.

# AI-OS Figma Blueprint

**Version:** 1.0
**Status:** Design-file and screen-delivery blueprint
**Date:** 2026-08-01

## 1. Purpose and Authority

This is not a `.fig` file. It is the implementation-ready blueprint for a designer, Figma AI, or Codex to create one. It owns Figma organization, frames, prototypes, annotations, and handoff. The Master Guide prevails; UI behavior follows the UI Spec; tokens/components follow the Design System.

## 2. Figma File Structure

Pages, in order:

1. `00 Cover & Readme`
2. `01 Foundations`
3. `02 Variables`
4. `03 Components`
5. `04 Patterns`
6. `10 Workspace`
7. `11 My AI`
8. `12 Agents`
9. `13 AI Council`
10. `14 AI Arena`
11. `15 Artifacts`
12. `16 Settings`
13. `20 Responsive`
14. `30 Prototype Flows`
15. `90 Archive`

Cover records version, owner, status, linked source specifications, last review, and implementation status legend: Proposed, Approved, Implemented, Verified. Never label a frame Implemented without repository evidence.

## 3. Foundations and Variables

Create variable collections: `Color` (Light/Dark/High Contrast), `Spacing`, `Size`, `Radius`, `Border`, `Elevation`, `Typography`, `Motion`, and `Breakpoint`. Names mirror Design System tokens (`color/bg/canvas`, not hex names). Bind every component property to variables where Figma supports it.

The Light mode canvas for all primary desktop, tablet, and mobile frames is white. Navigation and nested surfaces use the Design System's near-white semantic tokens only for necessary separation. Foundation examples must demonstrate that hierarchy comes primarily from whitespace, type, borders, and restrained elevation rather than broad grey backgrounds.

Foundations contain swatches, typography specimens, icon rules, grid examples, motion notes, contrast results, and do/don't examples. No detached local styles.

## 4. Components and Variants

Component sets include Button, IconButton, Input, Textarea, Select, Search, Checkbox, Radio, Switch, Tabs, Badge, Tooltip, Menu, Card, ListRow, Table, Sidebar, TopBar, Panel, Drawer, Dialog, Toast, Skeleton, Progress, EmptyState, ErrorState, OfflineBanner, and ApprovalCard.

Domain sets include ConversationRow, MessageBlock, Composer, TaskCard/Panel/Step, ArtifactCard/Preview, ProviderCard, ModelRow, AgentCard, CouncilTeamProposal, CouncilExpert, CouncilDiscussionTurn, CouncilReport, ArenaModeCard, ArenaParticipant, ArenaRound, ArenaStage, ArenaVote/Score, ReplayTimeline, and CostLatencySummary.

Variant properties use `Type`, `Size`, `State`, `Theme`, and domain-specific properties. Avoid duplicated components named “Copy”. Use exposed instance swaps and boolean/text properties.

## 5. Auto Layout and Naming

All production frames and components use Auto Layout, constraints, min/max sizing, and realistic overflow. Naming pattern: `Domain/Component/Variant`; frames: `Area / View / Breakpoint / State`. Layers use meaningful names such as `Task status`, never `Frame 392`.

## 6. Frames and Breakpoints

- Desktop wide: 1440×1024
- Desktop compact: 1200×900
- Tablet landscape: 1024×768
- Tablet portrait: 768×1024
- Mobile: 390×844 and 360×800

Every key page has desktop, tablet, and mobile frames plus empty, loading, error/offline, permission, and approval states where applicable.

## 7. Required Page Frames

### Workspace / Chat

Frames: first run; populated conversation; streaming Ask; Do planning; executing; waiting approval; verifying; completed with artifacts; recoverable failure; offline/local-only; conversation search; mobile composer. Desktop regions: Sidebar 238px, flexible conversation, contextual Task/Artifacts panel 360px. Annotate persistence, draft, keyboard, focus, scroll, and panel-resize behavior.

### Task Panel

Frames cover every Task lifecycle state, plan step dependency, progress, cancellation, recovery, partial success, approval, and failure detail. The compact version fits a workspace panel; expanded detail is a full sheet/page. Backend truth and permitted actions are annotated.

### Artifacts

Frames: library grid/list, filtered/search, preview, provenance, related task/conversation, export, unavailable local file, permission denied, and mobile preview. Cover document, image, code, spreadsheet, presentation, report, and imported file examples.

### My AI

Frames: provider overview; add provider; API key; OAuth handoff; model discovery; local models; routing/fallback; usage/cost; connected/degraded/unavailable; secret error. Separate configured, connected, and healthy visually and in annotations.

### Agents

Frames: registry; detail; install/configure; permissions; health; active operation; logs; unavailable; remote/local; OpenClaw example and generic future adapter. All execution controls annotate Runtime and permission boundaries.

### AI Council / AI 智囊团

Frames: Council home; decision brief; Chief of Staff clarification; proposed dynamic team; edit/approve team; expert discussion; dissent and consensus; final report; save/share artifact; insufficient evidence/error. The Chief of Staff is facilitator, not a provider selector. Expert cards show role and rationale; provider/model detail is optional metadata from AI Center. Report includes recommendation, options, assumptions, evidence, dissent, confidence, risks, and next actions.

### AI Arena / AI 竞技场

Home and setup frames cover all modes: Compare, Debate, Collaboration, Competition, Role Play, Game, Simulation. For each mode show objective, participant/role/model setup, rules, rounds, judging/voting, audience/user participation, cost limit, and stopping conditions.

Live frames cover stage, participant state, current turn, event/timeline, private-information treatment, moderation, user/audience action, vote/score, pause/stop, and connection failure. Result frames cover winner/outcome where applicable, score rubric, disagreement, cost/latency, replay, reproducible configuration, clone/rematch, and history. Include Werewolf as one social-deduction template; its role secrecy and phase flow demonstrate extensibility but do not define the underlying architecture.

### Settings

Frames: Settings home; My AI; Agents/Runtime; Skills/MCP; Permissions/approvals; Memory/privacy; Devices/integrations; Appearance/accessibility; Data/backup/export; Logs/diagnostics; Advanced. Show migration entry points for legacy Dashboard, Services, OpenClaw, Models, Logs, Backup, MultiLLM, and Prompt Library without claiming removed parity.

## 8. Prototype Flows

Create connected, keyboard-annotated flows:

1. First launch → connect provider/local model → first Ask.
2. Do request → plan → permission/approval → execution → verification → artifact.
3. Failure → explanation → retry/recover/cancel.
4. Council brief → Chief of Staff team → discussion → report.
5. Arena mode → setup → live interaction → vote/score → replay/history.
6. Offline transition → available local capability → cloud restoration.
7. Agent permission change → impacted operation state.

Prototype transitions follow motion tokens and reduced-motion notes. Modal focus entry/return and mobile back behavior are annotated.

## 9. State Matrix

For every feature, document: Empty, Loading, Ready, Partial, Streaming/Live, Success, Recoverable error, Terminal error, Offline, Permission denied, Approval required, Cancelled, Disabled, Unsupported. Matrix columns identify data source, user action, system response, retained data, focus target, announcement, and next route.

## 10. Approval, Permission, Error, and Offline

Approval frames state effect, target, data, permission, risk, cost where relevant, reversibility, and approve/deny. Permission denial explains the missing capability and safe route to Settings. Errors disclose no secret or raw stack. Offline frames distinguish local, cached, queued, and unavailable actions; do not show false connected states.

## 11. Annotation and Handoff

Each approved frame includes numbered annotations for behavior, architecture owner, data/state source, responsive rule, accessibility, empty/error behavior, and unresolved decision. Dev Mode descriptions link component/token names and acceptance criteria. Redlines are generated only where variables/Auto Layout do not express intent.

Handoff checklist: variables bound; component instances clean; all states represented; contrast checked; keyboard order noted; responsive frames complete; copy reviewed; sample data non-sensitive; architecture boundaries noted; implementation status honest; open decisions assigned.

## 12. Acceptance Criteria

- File structure, variables, components, naming, and Auto Layout match this blueprint.
- All key pages and required states exist at desktop/tablet/mobile breakpoints.
- Council shows dynamic Chief of Staff team assembly, discussion, and final report.
- Arena covers all seven modes, setup, live interaction, participation, scoring/voting, replay/history, cost/latency, and reproducibility.
- Approval, permission, error, and offline flows are prototyped and accessible.
- Handoff annotations identify Task Engine, Planner, AI Center, Skill/permission, and Runtime boundaries.

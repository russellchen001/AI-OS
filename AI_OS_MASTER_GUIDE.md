# AI-OS Master Guide

**Edition:** Foundation Edition  
**Version:** 2026.3
**Status:** Active Development  
**Project:** AI-OS

---

## Document Authority

This document defines the current product direction, target architecture, and development roadmap for AI-OS.

Where this guide conflicts with older roadmap or architecture documents, this guide takes precedence.

Companion documents have narrower authority: `AI_OS_PRODUCT_VISION.md` owns brand and vision; `AI_OS_UI_SPEC.md` owns information architecture and experience; `AI_OS_DESIGN_SYSTEM.md` owns visual and component rules; and `AI_OS_FIGMA_BLUEPRINT.md` owns design-file and screen-delivery specifications. If any companion document conflicts with this guide, this guide prevails.

`HANDOFF.md` owns current repository state, completed work, rejected approaches, and the immediate next step. This guide never describes current status; where this guide appears to state a status, `HANDOFF.md` prevails.

`AGENTS.md` is the entry point for any implementation agent and defines the reading order and delivery format.

Existing source code represents the current implementation baseline. It must be preserved and evolved incrementally toward the architecture defined here.

Do not discard completed work merely because the target architecture has changed.

The approved AI-OS v1.0 roadmap runs through P17. Phases P1 through P8 predate this guide; their outcomes are part of the current implementation baseline and their historical records live in `docs/archive/`. Phases P9 through P17 are defined in section 11 of this guide. AI Council and AI Arena are both mandatory v1.0 capabilities. Development sequencing may be refined, but removing either capability from v1.0 requires an explicit approved roadmap revision. Implementation advice must not silently change this frozen product scope.

---

## Table of Contents

1. [Project Snapshot](#1-project-snapshot)
2. [Product Definition](#2-product-definition)
3. [Product Philosophy](#3-product-philosophy)
4. [System Architecture](#4-system-architecture)
5. [Core Systems](#5-core-systems)
6. [Task Model](#6-task-model)
7. [Skill Framework](#7-skill-framework)
8. [Security and Permissions](#8-security-and-permissions)
9. [Development Rules](#9-development-rules)
10. [Implementation Agent Instructions](#10-implementation-agent-instructions)
11. [Roadmap](#11-roadmap)
12. [Non-Goals](#12-non-goals)
13. [Glossary](#13-glossary)

---

# 1. Project Snapshot

## Mission

AI-OS is a personal AI operating system designed to help ordinary users complete real-world digital tasks through natural language.

The user describes the desired outcome. AI-OS understands the intent, creates a plan, selects the required intelligence and capabilities, executes the task, verifies the outcome, and reports the result.

## Target Users

AI-OS is designed for ordinary users.

Users should not need:

- Programming knowledge
- AI knowledge
- Automation knowledge
- API knowledge
- Knowledge of the internal application or tool being used

## Current Development Status

Current status, completed phases, the active milestone, and the next work item
are recorded in `HANDOFF.md`. This guide deliberately does not duplicate them,
because two copies of a status will eventually disagree.

## Current Repository Implementation

AI-OS is a Tauri application. The Rust backend lives in `src-tauri/`, and the
React and TypeScript frontend lives in `src/`.

The runtime implementation baseline is `src-tauri/src/runtime/`, covering
lifecycle, executor, operations, scheduler, and recovery.

Future development must build on the existing implementation rather than replace it without a demonstrated architectural need.

---

# 2. Product Definition

AI-OS is not merely a chatbot, an LLM wrapper, or a collection of automation scripts.

AI-OS is an execution-oriented personal AI operating system.

Conversation is the primary user interface. Task completion is the product.

## Core Experience

The user should be able to say:

> Reply to this email and tell John I will arrive tomorrow.

AI-OS should then:

1. Understand the request.
2. Locate the relevant email.
3. Generate an appropriate reply.
4. Request confirmation when required.
5. Send the reply.
6. Verify and report completion.

The user should not need to manually choose an AI model, application, API, workflow, or execution method.

## Ask and Do

AI-OS supports two fundamental request modes.

### Ask

An Ask request returns information and normally does not create an external side effect.

Examples:

- Explain a concept.
- Summarize a PDF.
- Compare several AI answers.
- Answer a factual question.

Typical flow:

```text
User
  ↓
Task Engine
  ↓
AI Center
  ↓
Response
```

### Do

A Do request performs an action and may change external state.

Examples:

- Send an email.
- Organize files.
- Download a file.
- Manage a NAS.
- Control a device.

Typical flow:

```text
User
  ↓
Task Engine
  ↓
Planner
  ↓
Skill Framework
  ↓
Runtime
  ↓
OpenClaw or another executor
  ↓
External result
```

## AI-OS v1.0 Product Boundary

AI-OS reaches v1.0 only when the roadmap through P17 is complete and integrated.

The v1.0 product boundary includes, at minimum:

- Reliable Runtime foundation
- Task Engine and Planner
- OpenClaw integration
- Skill Framework
- AI Center
- Memory
- Core Skills
- AI Council
- AI Arena

Individual capabilities may exist earlier, but the product is not considered v1.0 until the integrated P17 acceptance boundary is met.


## Platform Identity — Multi-Model, Multi-Agent, Multi-Skill

AI-OS is a **multi-model, multi-Agent, multi-Skill AI application platform**.

Models, Agents, and Skills are three independent extension dimensions:

- **Models** provide replaceable intelligence through AI Center.
- **Agents** provide replaceable agentic execution through Runtime-to-Agent
  adapters.
- **Skills** provide replaceable domain capabilities through the Skill
  Framework.

No single model, Agent, Skill, provider, application, or external project is
the AI-OS architecture.

AI-OS owns:

- user intent and Task lifecycle;
- planning and orchestration;
- Runtime lifecycle and execution policy;
- permissions and confirmations;
- model, Agent, and Skill selection boundaries;
- normalized contracts and results;
- user-facing product experience.

Specialized implementation should be delegated to mature, compatible external
products whenever practical.

```text
AI-OS Platform
├── Multi-Model
│   └── AI Center
├── Multi-Agent
│   └── Runtime-to-Agent adapters
└── Multi-Skill
    └── Skill / Provider capability ecosystem
```

The current v1.0 implementation boundary has one operational general execution
Agent: OpenClaw.

That is a v1.0 release-scope decision, not a definition of AI-OS as a
single-Agent product.

---

---

# 3. Product Philosophy

## Task First

AI-OS optimizes for task completion, not conversation length or message count.

## Execution First

When AI-OS can safely complete a task, it should execute rather than only explain how the user could do it manually.

## Human Simplicity

Internal complexity belongs inside AI-OS, not with the user.

## Local First

Local execution is preferred when practical because it can improve privacy, speed, reliability, offline capability, and user ownership.

AI-OS is local-first, not local-only. Cloud AI and external services may be used when they provide required capability.

## AI Independence

AI-OS must not depend on one AI provider or model.

AI providers and models are replaceable components managed by AI Center.

## Modular Expansion

New external capabilities should normally be added as Skills rather than by modifying the core architecture.


## Reuse First / GitHub First

AI-OS should write the **smallest amount of custom implementation necessary**.

Before implementing a substantial new Skill, Provider, Agent capability,
execution engine, model-management feature, workflow system, connector, or
technical subsystem, development must first investigate whether a mature
reusable implementation already exists.

Preferred sequence:

```text
Requirement
  ↓
Check existing AI-OS capability
  ↓
Search GitHub / OSS / mature products
  ↓
Evaluate license / security / maintenance / architecture / UX
  ↓
Reuse / integrate / adapt / wrap
  ↓
Build only the missing AI-OS-specific layer
```

Evaluation must consider:

- commercial-use and license compatibility;
- project maturity and maintenance;
- security and credential handling;
- supported operating systems and hardware;
- architecture fit;
- installation and runtime burden;
- ordinary-user experience;
- whether AI-OS already owns the responsibility;
- whether the dependency can remain replaceable behind a stable Adapter,
  Provider, Skill, MCP, or equivalent boundary.

AI-OS must not recreate mature functionality merely to keep implementation
in-house.

When a mature product already solves most of a requirement, AI-OS should
normally integrate it and implement only the remaining AI-OS-specific
orchestration, policy, permission, normalization, and user-experience layer.

A project with an incompatible license or operating model may still be retained
as an architecture or research reference, but restricted implementation must
not be copied into AI-OS.

The value of AI-OS is measured by:

- capability;
- reliability;
- integration quality;
- security;
- replaceability;
- user simplicity;
- successful real-world task completion;

not by the number of lines written internally.

---

---

# 4. System Architecture

## Architecture Overview

```text
User
  ↓
Chat Interface
  ↓
Task Engine
  ↓
Planner
  ↓
AI Center + Memory
  ↓
Skill Framework
  ↓
Runtime
  ↓
OpenClaw Adapter / Other Executors
  ↓
Computer / Internet / Devices / Services
```

AI Council and AI Arena are separate capabilities built on AI Center:

```text
AI Center
  ├──→ AI Council
  └──→ AI Arena
```

## Architectural Responsibilities

- **Task Engine** determines what the user wants and owns task state.
- **Planner** determines how the objective should be achieved.
- **AI Center** owns providers, models, routing, fallback, model invocation, and shared multi-model infrastructure.
- **AI Council** dynamically assembles and coordinates an expert organization to improve user decision quality and synthesize recommendations.
- **AI Arena** provides controlled multi-AI interaction for comparison, debate, collaboration, competition, role play, games, simulation, entertainment, and research.
- **Memory** provides relevant context.
- **Skill Framework** exposes executable capabilities.
- **Runtime** manages execution lifecycle, sessions, scheduling, retries, and recovery.
- **OpenClaw** performs local agent actions when selected by Runtime.
- **Settings Center** provides user-facing configuration and control.

## Critical Boundary: Runtime and OpenClaw

Runtime and OpenClaw are not the same component.

```text
AI-OS
  ↓
Runtime
  ↓
OpenClaw Adapter
  ↓
OpenClaw Agent
  ↓
Local actions
```

Runtime is the AI-OS execution manager.

OpenClaw is one execution agent controlled through Runtime.

OpenClaw must not replace Task Engine, Planner, AI Center, Memory, or the broader product architecture.

## Agent Boundary and Future Agents

AI-OS is architecturally a multi-Agent platform.

AI-OS v1.0 currently has one operational general execution Agent: OpenClaw.
Hermes and Custom Agent records exist in the Registry as non-operational
placeholders.

This is a v1.0 implementation boundary, not a definition of AI-OS as a
single-Agent product.

The intended v2.0 arrangement is that Hermes becomes the primary Agent, as a
growth-type agent that accumulates understanding of the user, with OpenClaw
acting as its assistant for connecting to and operating external systems.
Further agents such as WorkBuddy may follow.

This places a constraint on all v1.0 work: the Runtime-to-Agent boundary must
remain a general adapter contract, not an OpenClaw-shaped one.

Adding a second Agent must not require changes to Task Engine, Planner, or
Runtime. Agent-specific behaviour belongs behind the adapter.

---

# 5. Core Systems

## 5.1 Chat Interface

The primary user interface should be conversational and approachable for ordinary users.

Supporting interfaces may include:

- Task status and history
- Settings
- AI provider and model management
- Skill management
- Permission management
- Device and integration management

The interface must not contain execution or business logic that belongs to backend systems.

## 5.2 Task Engine

The Task Engine is the central entry point for every user request.

Responsibilities:

- Receive requests
- Understand intent
- Classify Ask and Do requests
- Create Tasks
- Assign stable Task IDs
- Own and update Task state
- Route work to the correct system
- Return final status and results

The Task Engine must not:

- Directly operate external services
- Directly control the computer
- Directly call Skills for complex workflows
- Select individual AI providers outside AI Center

## 5.3 Planner

The Planner converts a Task objective into an executable plan.

Responsibilities:

- Decompose complex tasks
- Select Skills
- Order steps
- Identify dependencies
- Define confirmation and verification points

The Planner creates plans. It does not directly execute them.

## 5.4 Runtime

Runtime is responsible for reliable execution management.

Responsibilities:

- Manage execution sessions
- Execute plan steps
- Invoke Skills and execution adapters
- Track execution progress
- Handle cancellation
- Schedule background or delayed work
- Retry recoverable failures
- Preserve failure context

Runtime decides how to execute safely. It does not decide the user's goal.

## 5.5 AI Center

AI Center is the single entry point for AI intelligence.

Responsibilities:

- Provider management
- Model management
- Local model management
- Cloud model management
- Model routing
- Fallback and failover
- Cost and latency policies
- Shared multi-model invocation infrastructure

No module should directly call an AI provider when the request belongs through AI Center.

AI Council and AI Arena must use AI Center for provider and model access rather than create unrelated direct integrations.

### Execution Layer

AI Center executes in the Rust backend. Provider invocation, Auto ordering,
fallback selection, and credential use are backend responsibilities.

The frontend submits a request and renders the result. It must not select
providers, order candidates, decide fallback, or hold credentials in memory.

This is required because background and scheduled execution cannot depend on an
open application window, because credentials must remain in the native security
layer, and because Runtime, Memory, AI Council, and AI Arena all need backend
access to AI Center.

## 5.6 AI Council

AI Council（AI 智囊团）is AI-OS's multi-AI collaborative decision system. It is not a fixed expert list. It dynamically assembles a professional advisory team from the user's objective, organizes discussion, synthesizes perspectives, and produces a final recommendation. The Chief of Staff（战略幕僚）understands the need, assembles the team, facilitates discussion, and delivers the conclusion.

Potential uses:

- Strategic analysis
- Complex planning
- Research synthesis
- High-uncertainty comparison
- Independent critique

AI Council improves decision quality for the user. It is not horizontal model comparison and does not replace Task Engine, Planner, AI Center, or Runtime. It obtains all model access through AI Center; its product logic does not belong to Runtime.

### Council to Execution

A Council recommendation is intended to become executed work, not to stop at a
report. The intended path is:

```text
AI Council → Task Engine → Planner → user confirmation → Runtime → Agent
```

Council produces the recommendation. Task Engine creates a Do task carrying it
as context. Planner decomposes it. The user confirms before execution, because a
Council plan may contain irreversible actions. Runtime and the selected Agent
execute.

This path is not yet specified in detail. It must be specified before P16
implementation begins, or P16 will ship as a report-writing feature that leaves
the user to restate the plan manually.

Open points for that specification: which component converts a prose
recommendation into a task objective; the granularity at which the user
confirms; whether a Council plan can be saved and re-run; and failure handling
partway through a Council-originated plan.

## 5.7 AI Arena

AI Arena（AI 竞技场）is AI-OS's multi-AI interaction platform. Under explicit, shared rules, multiple AIs can compare, debate, collaborate, compete, role-play, play games, and run simulations. Model comparison is one Arena mode, not the whole product. Arena explores AI behavior, capability, and collaboration as well as evaluation.

Responsibilities:

- Support at least Compare, Debate, Collaboration, Competition, Role Play, Game, and Simulation modes
- Configure an arbitrary bounded number of participants, their models, roles, teams or independent sides, rules, rounds, audience or user participation, and stopping conditions
- Support live interaction, replay, and history
- Use social-deduction games such as Werewolf as representative Game/Role Play cases without hard-coding one game into the architecture
- Run the same request or evaluation case across multiple models when using Compare mode
- Provide side-by-side result comparison where appropriate
- Support blind evaluation where appropriate
- Support user voting and structured scoring
- Apply configurable evaluation criteria
- Preserve prompts, model configuration, outputs, latency, cost, and evaluation metadata for reproducibility
- Explain meaningful model disagreements
- Support comparison without changing Task Engine, Planner, or Runtime ownership

Every reproducible Arena record preserves applicable rules, roles, rounds, prompts, model configuration, outputs, scores or votes, cost, latency, and evaluation metadata. Arena must use AI Center for all model access.

AI Arena does not replace AI Council, Planner, Task Engine, AI Center, or Runtime. Council helps the user make better decisions; Arena enables controlled multi-AI interaction, experimentation, entertainment, and research.

## 5.8 Memory

Memory stores useful long-term context.

Potential content:

- User preferences
- Past tasks
- Project context
- Frequently used information
- Approved routines

Memory provides context only. It does not execute tasks or own task state.

## 5.9 Settings Center

Settings Center is the user's control panel.

It should allow users to manage:

- AI providers
- AI models
- Local models
- Skills
- Permissions
- OpenClaw integration
- NAS and devices
- Privacy and cloud-use preferences
- Memory preferences

The experience should be simple and understandable without knowledge of internal architecture.

---

# 6. Task Model

## Task Structure

A Task represents one user objective and its execution lifecycle.

Illustrative structure:

```json
{
  "id": "task_001",
  "type": "DO",
  "intent": "reply_email",
  "status": "planning",
  "priority": "normal",
  "context": {},
  "plan": [],
  "result": null,
  "error": null
}
```

The exact schema must follow the implementation produced during P10. This document defines responsibilities, not an immutable serialization format.

## Task States

Recommended lifecycle:

```text
Created
  ↓
Understanding
  ↓
Planning
  ↓
Ready
  ↓
Executing
  ↓
Verifying
  ↓
Completed
```

Failure and recovery path:

```text
Executing
  ↓
Recovering
  ↓
Retrying
  ↓
Executing
```

If recovery fails:

```text
Recovering
  ↓
Failed
```

## State Ownership

Task Engine owns task state transitions.

Runtime reports execution events and outcomes to Task Engine.

Skills return structured results and errors but do not independently modify global Task state.

---

# 7. Skill Framework

## Purpose

The Skill Framework is the capability expansion system of AI-OS.

Examples:

- Email Skill
- Browser Skill
- Calendar Skill
- File Skill
- NAS Skill
- Download Skill
- Smart Home Skill
- Local Model Management Skill

## Skill Responsibilities

A Skill is responsible for:

- Performing capability-specific actions
- Communicating with an external service or device
- Validating capability-specific inputs
- Returning structured results and errors

A Skill is not responsible for:

- Understanding broad user intent
- Creating multi-step plans
- Selecting AI providers
- Managing global Task state
- Directly coordinating unrelated Skills

## Skill Structure

Recommended structure:

```text
skill-name/
├── manifest
├── interface
├── executor
├── configuration
├── permissions
└── tests
```

The exact file layout may follow the repository's language and conventions.

## Skill Manifest

Every Skill should expose machine-readable metadata.

Illustrative example:

```json
{
  "id": "email",
  "name": "Email Skill",
  "version": "1.0.0",
  "description": "Manage email operations",
  "permissions": [
    "email.read",
    "email.send"
  ],
  "capabilities": [
    "read_email",
    "search_email",
    "send_email"
  ]
}
```

## Skill Lifecycle

```text
Discovery
  ↓
Installation
  ↓
Configuration
  ↓
Available
  ↓
Execution
  ↓
Update or Removal
```

## Coordination Rule

Skills should not directly coordinate other Skills.

Incorrect:

```text
Email Skill → Browser Skill
```

Correct:

```text
Planner → Email Skill
        → Browser Skill
```

---

# 8. Security and Permissions

Every Skill and execution adapter must declare required permissions.

Examples:

- `email.read`
- `email.send`
- `filesystem.read`
- `filesystem.write`
- `network.access`
- `device.read`
- `device.control`

## Confirmation Policy

Sensitive actions should require explicit user confirmation unless the user has intentionally configured a trusted automation policy.

Examples that normally require confirmation:

- Sending sensitive or consequential messages
- Financial transactions
- Purchases
- Destructive file operations
- Changing security settings
- Sharing private information

Permission checks and confirmations must occur before irreversible external effects.

Secrets must never be hard-coded or committed to source control.

---

# 9. Development Rules

## Core Rules

1. Understand existing code before changing it.
2. Work on one active milestone at a time.
3. Make the smallest correct change.
4. Preserve completed architecture unless a demonstrated limitation requires change.
5. Keep module ownership clear.
6. Avoid duplicate systems and overlapping responsibilities.
7. Prefer readable code over clever code.
8. Add tests for significant behavior and regressions. Every user-facing feature also ships an acceptance script at `verify/verify_<feature>.sh` that verifies actual behavior rather than the existence of a function.
9. Update this guide only when product direction, architecture, status, or development rules materially change.
10. Development-order recommendations do not modify the approved product roadmap.
11. Product scope or phase changes require an explicit update to this guide approved by the project owner.
12. Codex or another implementation agent must never silently remove, postpone beyond v1.0, rename, or renumber a frozen v1.0 capability.
13. Apply Reuse First / GitHub First before substantial new implementation: inspect existing AI-OS capabilities first, then mature external products, and prefer a reviewed Adapter / Provider / Skill / MCP integration over rebuilding equivalent functionality.
14. Third-party integrations must remain replaceable. AI-OS owns orchestration and product policy; external products own only the specialized implementation delegated to them.
15. Reuse shared Provider, account, credential, model, Agent, Skill, and Runtime infrastructure whenever an existing contract already satisfies the requirement.
16. Do not create duplicate login systems, credential stores, Provider Instances, Agent registries, model registries, account systems, or execution systems merely because another capability consumes the same underlying service.

## Forbidden Patterns

Do not bypass the architecture for convenience.

Incorrect:

```text
UI → External Service
```

Correct:

```text
UI → Task Engine → Planner → Runtime → Skill → External Service
```

Do not call AI providers directly from unrelated modules.

Incorrect:

```text
Skill → OpenAI API
```

Correct:

```text
Skill or Planner → AI Center → Provider
```

Do not place AI Center routing, provider ordering, fallback selection, or credential handling in the frontend.

Incorrect:

```text
Frontend service → provider selection → provider HTTP call
```

Correct:

```text
Frontend → AI Center (Rust backend) → Provider
```

Do not place product planning logic inside Runtime or external execution logic inside the UI.

## Definition of Done

A change is complete when:

- The requested behavior works
- Relevant tests pass
- Errors and edge cases are handled appropriately
- Architecture boundaries remain intact
- No secrets or unrelated changes are included
- Documentation is updated when required

---

# 10. Implementation Agent Instructions

Any implementation agent — Codex, Claude, Gemini, Cursor, or another — is responsible for continuing AI-OS, not restarting or redesigning it.

## Before Coding

1. Read `AGENTS.md`, then `HANDOFF.md`, then this guide.
2. Confirm the active milestone from `HANDOFF.md`, and check its Completed and Rejected sections before proposing anything.
3. Inspect the relevant existing implementation.
4. Identify the module that owns the responsibility.
5. Propose the smallest viable change when the change is significant.

## During Coding

- Follow existing repository conventions.
- Preserve stable interfaces where practical.
- Keep changes focused.
- Add or update tests.
- Do not introduce speculative future features.
- Do not silently change architecture.

## When Requirements Are Unclear

1. Identify the uncertainty.
2. Explain the available approaches.
3. Recommend the safest approach.
4. Request confirmation before an architectural change.

## Completion Report

Every completed development task should report:

- **Summary:** What changed
- **Reason:** Why it was required
- **Files:** Files added or modified
- **Validation:** Checks and tests performed
- **Risks:** Known limitations or unresolved concerns
- **Next:** Recommended next step

## Most Important Rule

> Continue the project. Do not restart the project.

---

# 11. Roadmap

Phase completion status is recorded in `HANDOFF.md`, not here. This section defines only the goals and boundary of each phase.

## P9 — Runtime Foundation

Purpose: provide reliable execution infrastructure.

## P10 — Task Engine

Goals:

- Convert user requests into structured Tasks
- Implement Task lifecycle and state ownership
- Track results and failures
- Connect the Chat Interface to the execution architecture
- Provide stable interfaces for later Planner and Skill integration

P10 does not include the full Skill ecosystem, AI Council, advanced Memory, or marketplace features.

## P11 — OpenClaw Integration

Goals:

- Create the Runtime-to-OpenClaw adapter
- Enable controlled local execution
- Handle permissions and execution events
- Preserve Runtime lifecycle ownership

## P12 — Skill Framework

Goals:

- Skill registry
- Skill discovery
- Skill manifests
- Permission model
- Skill lifecycle

## P13 — AI Center

Goals:

- Provider and model management
- Local and cloud model support
- Routing and fallback
- Shared multi-model invocation for AI Council and AI Arena
- Provider-independent routing, observability, cost, and latency metadata

## P14 — Memory

Goals:

- User preferences
- Historical context
- Project memory
- Long-term knowledge retrieval

## P15 — Core Skills

Purpose: expand AI-OS from an execution framework into a practical personal AI operating system with real-world capabilities.

P15 v1.0 capability areas (10 Skills):

1. Email and Calendar
2. Browser and Search
3. File Management
4. Downloads
5. NAS Management
6. Document / Spreadsheet / Presentation Workflows
7. Local Model Management
8. Computer Control
9. Local Generative Media
10. Cognitive Distillation Foundation

### Local Generative Media Skill

Goals:

- Enable local-first image and video generation workflows
- Integrate with user-controlled local generation environments such as Draw Things and ComfyUI
- Manage generation orchestration including prompts, models, LoRA selection, and refinement loops
- Prefer local generation resources before cloud generation providers
- Preserve user ownership and privacy by avoiding unnecessary cloud uploads


#### Generative Media Reuse Architecture

Generative Media follows Reuse First.

##### ComfyUI-Agent-Kit

ComfyUI-Agent-Kit is the preferred reusable foundation for ComfyUI-oriented:

- hardware and runtime inspection where applicable;
- model and workflow knowledge;
- model acquisition;
- model-management assistance;
- ComfyUI Agent / MCP tooling.

AI-OS should not build a duplicate large ComfyUI model-advisor system when
reviewed reusable components already provide the required capability.

##### ComfyUI-Mac-Silicon

ComfyUI-Mac-Silicon is retained as Apple Silicon-specific knowledge for:

- MPS;
- unified memory;
- Apple Silicon compatibility;
- Mac-oriented model recommendation;
- model precision and memory constraints;
- performance guidance.

External recommendation knowledge is advisory.

AI-OS **GM-2 Ready First** remains authoritative for whether the current
machine, workflow, model, nodes, runtime, and output path are genuinely usable.

Local model acquisition should follow:

```text
Inspect actual computer hardware
  ↓
Inspect actual local media runtime
  ↓
Determine compatible models / quantizations
  ↓
Rank for the user's objective
  ↓
Confirm acquisition when required
  ↓
Download / install
  ↓
GM-2 Ready First
  ↓
Real smoke generation and output retrieval
  ↓
Ready
```

AI-OS should not download a large model first and only afterward determine that
the user's computer cannot reasonably use it.

##### Cloud Generative Media Account Reuse

GM-4 must reuse Provider accounts and Provider Instances already connected
through P13 AI Center / My AI.

GM-4 must not create:

- a second xAI login;
- a second OpenAI login;
- a Generative-Media-specific Provider Instance;
- duplicate OAuth implementations;
- duplicate Device Code implementations;
- duplicate Keychain credential storage.

Provider account identity is shared infrastructure.

`Connected` and `Media Executable` are different concepts.

An account may be connected through My AI while lacking a legitimate
third-party media execution entitlement.

##### Prompt, Reference and Quality Intelligence Boundaries

Generative Media intelligence remains a Skill/backend domain behind the
selected Agent, Agent Skill Transport, `SkillInvocationGateway`, and Runtime
governance. `MediaRouter` is a backend router, not a Planner, Runtime, Agent, or
Task executor.

Prompt compilation begins only after the exact Provider and Provider Instance
are selected. It may adapt dialect/model parameters within that identity, but
must preserve explicit Provider, account, model, and profile choices. A
deliberate expert prompt remains verbatim unless structured constraints,
reference intelligence, or explicit enhancement requires compilation.

Reference analysis uses replaceable adapters. Local references are never
silently uploaded; ordinary analysis/generation never installs packages or
downloads models. Local VLM Setup/Repair is a separate, explicit-confirmation
operation, and Ready requires compatible schemas, complete local model assets,
a real workflow smoke, and successful normalized `ReferenceSpec` output.

The quality loop is a bounded correction step inside the already selected
Provider execution. The default maximum is one automatic correction retry.
Cancellation, permission, policy, Provider/account identity, and cost budget
remain hard boundaries; Local First never means Local then Cloud. Attempt
metadata may record normalized assessments/corrections, but not raw user media
or unrestricted full prompts in long-term memory.

### Cognitive Distillation Foundation

Goals:

- Build the foundation for evidence-based cognitive models
- Extract decision patterns, reasoning frameworks, value priorities, and trade-off preferences from authorized data sources
- Create reusable cognitive representations for future strategic simulation
- Avoid simple personality imitation or prompt-based role play
- Provide explainable foundations for future AI Council and Strategic Intelligence capabilities


### P15 Computer Control boundary

P15 does not implement Smart Home / Device Control.

Computer Control fills deterministic system-level gaps that are not a good fit
for Computer Use or general Agent execution.

It does not duplicate GUI automation.

Its v1.0 scope is deterministic system state and direct system operations such
as:

- storage;
- CPU;
- memory;
- network;
- processes;
- applications;
- clipboard;
- audio;
- power;
- notifications;
- permissions.

Visual GUI understanding and interaction belong to Computer Use / GUI
intelligence providers rather than Computer Control.


### Accepted v1.0 Reuse Integrations After Generative Media

The following mature external projects are accepted integration candidates
after the current Generative Media sequence.

They are specialized implementations behind AI-OS boundaries, not replacement
architectures.

#### Mano-P / Mano-CUA

Role:

- GUI / Computer Use intelligence;
- visual grounding;
- visual action reasoning;
- multi-step GUI interaction.

Preferred boundary:

```text
AI-OS Task / Plan
  ↓
Runtime
  ↓
selected Agent / Skill execution path
  ↓
Mano-CUA Adapter
  ↓
GUI
```

Mano-P / Mano-CUA does not replace:

- Task Engine;
- Planner;
- Runtime;
- OpenClaw architecture;
- Computer Control.

Local execution is preferred.

Cloud screenshot or task transmission must not be used as a silent fallback.

#### Local Model Optimization — llmfit

Primary reusable component:

- `AlexsJones/llmfit`

Role:

- Local Model hardware profiling and compatibility analysis for AI Center /
  My AI;
- model-fit and quantization recommendation;
- memory, context-window and expected-speed estimation;
- benchmark-informed recommendation;
- installed-model discovery and model-acquisition assistance where
  appropriate.

Architecture:

```text
AI-OS
  ↓
Local Model Optimization Adapter
  ↓
llmfit
  ↓
hardware / model-fit / quantization / benchmark recommendation
  ↓
AI-OS Provider selection and lifecycle orchestration
  ↓
oMLX / Ollama / future replaceable Local Model Providers
```

llmfit does not replace:

- AI Center;
- Provider Registry;
- Local First routing;
- model/account identity;
- permissions or policy;
- oMLX;
- Ollama.

AI-OS retains Provider identity, routing, policy, permissions, account
management, model lifecycle orchestration, and product experience.

Magnitude is not a default planned v1.0 integration. It remains a future
optional replaceable Local Inference Provider and should only be integrated if
a concrete execution capability gap remains after llmfit, oMLX and Ollama.

AI-OS must not add Magnitude merely to duplicate hardware profiling, model
recommendation, model lifecycle, or inference capabilities already adequately
covered by the existing stack.

---

## P16 — Strategic Intelligence and AI Council

Goals:

- Dynamic expert-team assembly from the user's objective
- Chief of Staff facilitation and expert discussion
- Independent critique, consensus, and synthesized recommendations
- Decision-support reports for complex decisions and planning
- A specified and implemented path from recommendation to executed work through Task Engine, Planner, user confirmation, Runtime, and an Agent
- Provider-independent model access through AI Center

Strategic Intelligence expansion:

- Cognitive Simulation Engine based on P15 Cognitive Distillation outputs
- Scenario simulation and strategic reasoning
- Second-order reasoning: modelling how another decision-maker may respond when aware of the current strategy
- Evidence-based prediction of possible decision patterns
- Multi-perspective strategic analysis through AI Council

Agent Connectivity Layer:

- Integrate external connectivity architectures such as Linco Bridge as future reference and implementation candidates
- Support remote client access, Agent session continuity, event streaming, and multi-device interaction
- Keep Agent Connectivity separate from Runtime execution architecture
- Do not make external bridge systems a required Runtime dependency


## P17 — AI Arena

Goals:

- Multi-AI interaction platform with Compare, Debate, Collaboration, Competition, Role Play, Game, and Simulation modes
- Flexible participant-count and formation configuration, including independent multi-party and team-based arrangements rather than a fixed 1v1 structure
- Rule, role, model, round, audience, participation, and stopping-condition configuration
- Live interaction, user participation, voting, scoring, replay, and history
- Representative extensible game templates, including social-deduction play, without hard-coded game architecture
- Reproducible records of prompts, rules, roles, rounds, model configuration, outputs, votes or scores, cost, and latency
- Blind testing, configurable evaluation criteria, and disagreement analysis where applicable
- Provider-independent integration through AI Center and optional use of Memory
- Clear separation from AI Council decision-support workflows

## AI-OS v1.0 Completion Boundary

AI-OS v1.0 requires completion and integration of P9 through P17 while preserving all accepted work from earlier phases.

Minimum acceptance characteristics:

- Users can submit Ask and Do requests through the normal interface
- Task Engine and Planner manage the task and plan lifecycle
- Runtime can execute through OpenClaw and Skills under permissions
- AI Center supports local and cloud models through replaceable providers
- Memory supplies relevant context without owning execution
- Core Skills complete useful real-world workflows
- AI Council dynamically assembles expert teams under a Chief of Staff and produces decision-support synthesis
- AI Arena supports controlled, reproducible multi-AI interaction across its required modes while retaining comparison and evaluation records
- Architecture, permissions, failure handling, and user-facing reporting work as an integrated product


## v2.0 Deferred — Social / Community Intelligence

Social / Community Intelligence is explicitly deferred to AI-OS v2.0.

It is not part of the current P15 or v1.0 implementation boundary.

Potential future sources may include:

- Xiaohongshu;
- Douyin;
- Bilibili;
- Weibo;
- Zhihu;
- Kuaishou;
- similar social and community services.

MediaCrawler, Pachong, social-media-copilot, and other reviewed projects remain
future architecture and reuse references only.

Do not implement this capability during v1.0 unless the project owner
explicitly changes the roadmap.

---

---

# 12. Non-Goals

AI-OS is not intended to become:

- A replacement for Windows or macOS
- A social network
- A cloud-only AI service
- A conventional coding IDE replacement
- A collection of unrelated automation scripts
- A complex workflow editor that ordinary users must manually configure

The focus is personal AI task execution through natural language.

---

# 13. Glossary

**AI-OS**  
The personal AI operating system described by this guide.

**Task**  
A structured unit of work created from user intent.

**Task Engine**  
The system that creates Tasks, owns Task state, and routes requests.

**Planner**  
The system that converts a Task objective into an execution plan.

**Runtime**  
The execution management layer responsible for sessions, scheduling, retries, recovery, and execution lifecycle.

**OpenClaw**  
A local agent executor controlled through Runtime.

**Skill**  
A modular executable capability.

**AI Center**  
The intelligence management layer responsible for providers, models, routing, and multi-model systems.

**AI Council**  
AI 智囊团: a dynamic multi-AI expert organization for collaborative decision support, facilitated by a Chief of Staff and powered through AI Center.

**AI Arena**  
AI 竞技场: a controlled multi-AI interaction platform for comparison, debate, collaboration, competition, role play, games, simulation, entertainment, experimentation, and research, powered through AI Center and preserving reproducible records.

**Memory**  
The long-term context system. Memory supplies context but does not execute tasks.

**Settings Center**  
The user-facing interface for managing AI providers, models, Skills, integrations, permissions, privacy, and preferences.

**Milestone**  
A focused development phase with defined goals and boundaries.

---

# Change Log

## 2026-09-09 — llmfit Local Model Optimization Direction

- Selected `AlexsJones/llmfit` as the preferred v1.0 reusable Local Model
  optimization / recommendation foundation.
- Assigned llmfit hardware profiling, model-fit analysis, quantization
  recommendation, expected performance estimation, benchmark evidence, and
  model-acquisition assistance.
- Kept AI Center, Provider Registry, Local First routing, policy and model
  lifecycle orchestration under AI-OS ownership.
- Kept oMLX and Ollama as the current v1.0 Local Model execution Providers.
- Superseded the earlier plan to integrate Magnitude as the primary Local Model
  optimization component.
- Retained Magnitude only as a future optional replaceable Local Inference
  Provider if a demonstrated execution gap justifies it.

## 2026-09-08 — Multi-Model / Multi-Agent / Multi-Skill Platform

- Defined AI-OS explicitly as a multi-model, multi-Agent, multi-Skill AI
  application platform.
- Clarified that OpenClaw-only operational execution in the current v1.0
  baseline is a release-scope boundary rather than a single-Agent product
  architecture.
- Added Reuse First / GitHub First as a mandatory development principle.
- Required mature compatible products to be investigated before substantial
  custom implementation.
- Required shared Provider, account, credential, model, Agent, Skill, and
  Runtime infrastructure to be reused rather than recreated per capability.
- Accepted ComfyUI-Agent-Kit and ComfyUI-Mac-Silicon as Generative Media reuse
  foundations while retaining GM-2 Ready First as execution authority.
- Required GM-4 to reuse existing P13 / My AI Provider connections.
- Accepted Mano-P / Mano-CUA as a post-Generative-Media GUI / Computer Use
  intelligence integration candidate.
- Accepted Magnitude as a post-Generative-Media Local Model optimization /
  inference integration candidate.
- Deferred Social / Community Intelligence to v2.0.
- Reduced the active P15 v1.0 boundary to exactly 10 Skills.
- Removed Vehicle Control from AI-OS v1.0 and P15. Earlier v1 Vehicle Control
  roadmap decisions are superseded.


## 2026-08-02 — Version 2026.3

- Removed all current-status content from this guide; `HANDOFF.md` is now the sole owner of repository state, and the document-authority section says so explicitly.
- Removed per-phase status markers from the roadmap for the same reason.
- Recorded the decision that AI Center executes in the Rust backend, and added a matching forbidden pattern for frontend routing.
- Added the Council-to-execution path as an explicit P16 goal and flagged it as requiring specification before implementation.
- Added the Agent boundary section: v1.0 runs OpenClaw only, v2.0 intends Hermes as primary with OpenClaw assisting, and the Runtime-to-Agent contract must stay general.
- Renamed section 10 to Implementation Agent Instructions and made the reading order start at `AGENTS.md`.
- Clarified that P1–P8 predate this guide and that section 11 defines P9–P17.
- Added the acceptance-script requirement to the development rules.

## 2026-08-01 — Version 2026.2

- Formalized AI Council as a dynamic expert organization led by a Chief of Staff for user decision support.
- Expanded AI Arena from evaluation-only framing to a controlled multi-AI interaction platform while retaining reproducible evaluation capabilities.
- Clarified companion-document authority and preserved the frozen P1–P17 v1.0 scope.

---

# Final Principle

> AI-OS exists to help ordinary users complete real-world tasks through natural language.

When making decisions, prefer:

- Simplicity
- Stability
- Clear architecture
- User value
- Reuse of mature, compatible products before custom implementation
- Incremental progress

---

# Future Architecture Extensions

## Cognitive Intelligence Layer

AI-OS may build evidence-based cognitive models representing how a person,
role, or decision framework evaluates situations.

Cognitive Distillation extracts:

- decision principles
- value priorities
- risk preferences
- reasoning patterns
- trade-off preferences
- communication patterns

Cognitive Models are not personality clones. They represent evidence-based
decision patterns with confidence and supporting evidence.

Future use cases:

- strategic simulation
- AI Council reasoning
- decision support
- scenario analysis


## Local Generative Media Layer

AI-OS supports local-first image and video generation workflows.

Priority:

1. User local providers
   - Draw Things
   - ComfyUI
   - other local generation engines

2. Configured cloud providers

AI-OS manages:

- prompt generation
- model selection
- LoRA selection
- generation workflow
- iterative refinement

AI-OS should not silently upload local generation tasks to cloud services
when suitable local capability exists.


## Strategic Intelligence Layer

Future Strategic Intelligence combines:

- Cognitive Distillation
- Cognitive Simulation
- AI Council
- scenario analysis
- second-order reasoning

The goal is not reading minds. The goal is evidence-based simulation of
possible decision patterns under different scenarios.


## Agent Connectivity Layer

External Agent connectivity architectures such as Linco Bridge may provide:

- remote client access
- Agent session continuity
- multi-device interaction
- event streaming
- external channel connectivity

Agent Connectivity remains separate from Runtime execution architecture.

<!-- AI_OS_MINIMAL_KERNEL_DEFINITION_START -->

## Canonical Product Identity — AI-Native Operating System

AI-OS is an AI-native operating system.

AI-OS is not:

- an Agent;
- an LLM;
- a Skill;
- a collection of hard-coded domain executors.

Its role is to use the minimum stable Kernel necessary to connect, orchestrate
and govern multiple Agents, multiple LLMs and multiple Skills in order to
fulfill user intent.

### Minimal Kernel

The Kernel owns:

- Task Engine;
- Planner;
- Runtime;
- Agent Registry;
- LLM / Provider Registry;
- Skill Registry;
- Memory;
- permissions and policy;
- lifecycle;
- compatibility negotiation;
- observability and recovery.

The Kernel should stay small.

Concrete user capabilities should be reused or adapted from mature existing
implementations whenever possible.

Preferred order:

`reuse -> adapt -> wrap -> delegate -> build`

### Canonical execution architecture

User Do-task control plane:

`User -> Task Engine -> Planner -> Runtime -> selected Agent`

Capability invocation plane:

`Agent -> Agent Skill Transport Adapter -> AI-OS Skill Invocation Gateway -> Skill backend`

Backend examples may include:

- MCP;
- native OS integration;
- external APIs;
- local CLIs;
- Providers;
- mature open-source components.

Agents execute tasks.

LLMs provide intelligence.

Skills provide reusable capabilities.

AI-OS connects and governs all three.

### Agent compatibility

Agent compatibility is capability-based and determined by capability discovery and contract negotiation, never by an exact Agent version.


AI-OS does not bind its execution architecture to one exact OpenClaw, Hermes,
or future Agent version.

Compatibility is capability-based.

Agent versions are metadata.

Runtime determines support through probing and contract negotiation.

Agent-specific protocol and version differences remain isolated behind Agent
and Transport Adapters.

Version alone must never be the reason a Skill becomes available or
unavailable.

### Autonomous Skill expansion

The long-term capability acquisition model is:

`Need -> Discover -> Evaluate -> Adapt -> Validate -> Register -> Expose -> Use`

AI-OS should eventually be able to search external capability ecosystems such
as GitHub, MCP servers, Agent Skills, libraries, CLIs and APIs when an existing
Skill cannot satisfy the user's request.

AI-OS may automatically adapt validated capabilities for compatible Agents.

This autonomous expansion applies to the capability ecosystem, not unrestricted
rewriting of the Kernel or security model.

### Intelligence Layer

The strategic Intelligence Layer is:

`P15 Cognitive Distillation -> P16 AI Council Simulation -> AI Arena Evaluation`

#### Cognitive Distillation

P15 produces structured cognitive data that can represent:

- a person;
- an expert;
- a decision style;
- an Agent behavior pattern.

The data should preserve evidence, provenance, uncertainty and contradictions
where possible.

#### AI Council

P16 supplies the multi-Agent / multi-model Council runtime.

Paperclip and Agency Agents are external Council / Agent-operation integration
candidates and should be directly reused where appropriate instead of
reimplemented.

They are not the Cognitive Distillation engine.

AI Council consumes P15 distilled cognitive data and can run persona / expert
simulation against a new situation.

The key capability is:

`distilled person + new situation -> multi-model Council simulation`

The purpose is to estimate how the modeled person may:

- interpret;
- reason;
- decide;
- handle;
- act.

This simulated anticipation is the primary meaning of "prediction" for this
feature.

It is not defined as a separate generic probability forecasting subsystem.

#### AI Arena

AI Arena compares Agents, LLMs, Council configurations and simulation outputs.

Its evidence can later improve routing, Council composition and distilled
profiles.

Canonical learning loop:

`Distill -> Simulate -> Compare -> Validate -> Refine`

### External integration boundary

External projects may be directly integrated when that is better than
reimplementation.

They remain replaceable components.

AI-OS retains ownership of:

- Kernel contracts;
- governance;
- permission model;
- Task lifecycle;
- Memory;
- Agent / LLM / Skill registries;
- interoperability.

### Development constraint

Do not add domain-specific execution logic to AI-OS Core merely because a new
user capability is requested.

First determine whether that capability can be reused, adapted, wrapped or
delegated.

AR-1 is therefore limited to restoring the correct generic execution boundary.

<!-- AI_OS_MINIMAL_KERNEL_DEFINITION_END -->

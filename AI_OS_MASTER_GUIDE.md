# AI-OS Master Guide

**Edition:** Foundation Edition  
**Version:** 2026.2
**Status:** Active Development  
**Project:** AI-OS

---

## Document Authority

This document defines the current product direction, target architecture, and development roadmap for AI-OS.

Where this guide conflicts with older roadmap or architecture documents, this guide takes precedence.

Companion documents have narrower authority: `AI_OS_PRODUCT_VISION.md` owns brand and vision; `AI_OS_UI_SPEC.md` owns information architecture and experience; `AI_OS_DESIGN_SYSTEM.md` owns visual and component rules; and `AI_OS_FIGMA_BLUEPRINT.md` owns design-file and screen-delivery specifications. If any companion document conflicts with this guide, this guide prevails.

Existing source code represents the current implementation baseline. It must be preserved and evolved incrementally toward the architecture defined here.

Do not discard completed work merely because the target architecture has changed.

The approved AI-OS v1.0 roadmap contains 17 phases, P1 through P17. AI Council and AI Arena are both mandatory v1.0 capabilities. Development sequencing may be refined, but removing either capability from v1.0 requires an explicit approved roadmap revision. Implementation advice must not silently change this frozen product scope.

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
10. [Codex Instructions](#10-codex-instructions)
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

**Completed:**

- P9 Runtime Foundation
- Runtime lifecycle management
- Runtime recovery
- Runtime operation management
- Runtime scheduling foundation
- P10 Task Engine and Planner
- P11 OpenClaw Integration

**Current priority:**

- Complete and validate the approved UI Refactor documentation and implementation without starting P12

**Next milestones:**

- P12 Skill Framework
- P13 AI Center
- P14 Memory
- P15 Core Skills
- P16 AI Council
- P17 AI Arena

## Current Repository Implementation

The current runtime implementation is located at:

`src-tauri/src/runtime/`

Known runtime areas include:

- Lifecycle
- Executor
- Operations
- Scheduler
- Recovery

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

## 5.6 AI Council

AI Council（AI 智囊团）is AI-OS's multi-AI collaborative decision system. It is not a fixed expert list. It dynamically assembles a professional advisory team from the user's objective, organizes discussion, synthesizes perspectives, and produces a final recommendation. The Chief of Staff（战略幕僚）understands the need, assembles the team, facilitates discussion, and delivers the conclusion.

Potential uses:

- Strategic analysis
- Complex planning
- Research synthesis
- High-uncertainty comparison
- Independent critique

AI Council improves decision quality for the user. It is not horizontal model comparison and does not replace Task Engine, Planner, AI Center, or Runtime. It obtains all model access through AI Center; its product logic does not belong to Runtime.

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
8. Add tests for significant behavior and regressions.
9. Update this guide only when product direction, architecture, status, or development rules materially change.
10. Development-order recommendations do not modify the approved product roadmap.
11. Product scope or phase changes require an explicit update to this guide approved by the project owner.
12. Codex or another implementation agent must never silently remove, postpone beyond v1.0, rename, or renumber a frozen v1.0 capability.

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

# 10. Codex Instructions

Codex is responsible for continuing AI-OS, not restarting or redesigning it.

## Before Coding

1. Read `AI_OS_MASTER_GUIDE.md`.
2. Confirm the active milestone.
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

## P9 — Runtime Foundation

**Status:** Completed

Purpose: provide reliable execution infrastructure.

## P10 — Task Engine

**Status:** Current priority

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

Initial capability areas:

- Email and calendar
- Browser and search
- File management
- Downloads
- NAS management
- Document, spreadsheet, and presentation workflows
- Local model management
- Smart home and device control

## P16 — AI Council

Goals:

- Dynamic expert-team assembly from the user's objective
- Chief of Staff facilitation and expert discussion
- Independent critique, consensus, and synthesized recommendations
- Decision-support reports for complex decisions and planning
- Provider-independent model access through AI Center

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
- Incremental progress

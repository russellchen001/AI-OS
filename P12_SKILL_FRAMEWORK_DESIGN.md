# AI-OS P12 Skill Framework Design Freeze v1.0

Date:
2026-08-01

Status:
Architecture Design

Phase:
P12 — Skill Framework

---

# 1. Purpose

Skill Framework introduces a modular capability system into AI-OS.

Before P12:

- Runtime management
- Task execution
- OpenClaw integration
- AI Provider connection


After P12:

- Capability discovery
- Skill selection
- Permission validation
- Safe execution
- Modular expansion


Skill Framework is the capability layer between Planner and Runtime.


Architecture:

User
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
OpenClaw / Executor
↓
External Result


---

# 2. Skill Definition

A Skill is a reusable capability module that AI-OS can select and execute.

A Skill is NOT:

- Prompt
- Agent
- Model
- MCP Server
- Workflow


A Skill may use:

- MCP tools
- OpenClaw tools
- Local executors
- Cloud services


---

# 3. Core Principles

1. Skills are replaceable.
2. Runtime remains stable.
3. Permissions are explicit.
4. Capabilities are discoverable.
5. Providers are replaceable.
6. New capabilities should not require core architecture changes.


---

# 4. Skill Model

Each Skill contains:

- id
- name
- version
- description
- capabilities
- permissions
- executor
- manifest metadata


Example:

email.send

filesystem.organize

nas.backup


Naming convention:

domain.action


---

# 5. Capability Model

Capabilities describe what a Skill can do.

Example:

email skill:

[
 "email.read",
 "email.compose",
 "email.send"
]


Planner selects Skills through capability matching.


---

# 6. Permission Model

Skills must declare required permissions.

Example:

email.send requires:

[
 "email.read",
 "email.send"
]


Permission flow:

Skill
↓
Permission Check
↓
Approval / Denial
↓
Runtime Execution


P12 extends the existing OpenClaw permission architecture.

Existing permission systems must not be replaced.


---

# 7. Skill Manifest

Manifest describes:

- Skill identity
- Capabilities
- Permissions
- Executor requirements


Example:

{
"id":"email",
"name":"Email Assistant",
"version":"1.0.0",
"capabilities":[
 "email.read",
 "email.send"
],
"permissions":[
 "email.read",
 "email.send"
],
"executor":{
 "type":"openclaw",
 "handler":"email"
}
}


---

# 8. Skill Registry

Skill Registry manages installed Skills.

Responsibilities:

- register
- unregister
- enable
- disable
- list
- search by capability


Registry becomes the source of truth for available capabilities.


---

# 9. Runtime Relationship

Runtime remains responsible for:

- lifecycle
- execution
- recovery
- scheduling


Skill Framework is responsible for:

- capability organization
- discovery
- selection
- permission declaration


Skill Framework does not replace Runtime.


---

# 10. P12 Scope

Included:

- Skill Contract
- Manifest Schema
- Registry
- Permission Binding
- Runtime Adapter


Not included:

- Email Skill
- Browser Skill
- NAS Skill
- Automation workflows


Those belong to P15 Core Skills.


---

# 11. Implementation Order

P12-M1

Skill Contract


↓

P12-M2

Manifest


↓

P12-M3

Registry


↓

P12-M4

Permission Binding


↓

P12-M5

Runtime Integration


---


# AI-OS Development Workflow

All production changes follow the workflow below. Documentation-only changes may scale individual validation steps to their risk, but must preserve the same review sequence.

```text
PLAN
  ↓
Architecture Review
  ↓
Implementation
  ↓
Build
  ↓
Manual Test
  ↓
Code Review
  ↓
Merge
```

## 1. PLAN

Define the problem, desired outcome, scope, exclusions, acceptance criteria, compatibility requirements, risks, and validation approach. Identify affected user data, IPC contracts, provider protocols, and external services.

When a task is marked **PLAN ONLY**, do not create, modify, rename, or delete files. Stop after returning the analysis, risks, affected files, implementation steps, and validation criteria. Do not proceed to architecture changes, implementation, build, or merge activities unless a later task explicitly authorizes them.

## 2. Architecture Review

Confirm ownership and boundaries across the frontend, native backend, Runtime, Provider, Session, Tool, and Workspace layers. Review security, migration, recovery, platform, dependency, and backward-compatibility implications. Record significant decisions in `docs/ADR/` before implementation.

## 3. Implementation

Implement the smallest maintainable change that satisfies the approved plan. Keep unrelated refactors separate, preserve established contracts, validate trust boundaries, and update relevant documentation with the change.

## 4. Build

Run the checks appropriate to the affected surface. At minimum, production application changes should complete the TypeScript/Vite build and the relevant Rust checks. Resolve warnings or document why they are safe and temporary.

## 5. Manual Test

Test acceptance criteria in the real desktop application where practical. Exercise success, failure, cancellation, restart, and data-preservation paths relevant to the change. Verify that unaffected critical workflows still operate.

Record the environment, steps, and result so another contributor can reproduce the validation.

## 6. Code Review

Review implementation, tests, documentation, architecture impact, security, compatibility, and manual-test evidence together. Address findings before approval; do not treat review as a formatting-only gate.

## 7. Merge

Merge only after required checks and review pass, the branch is current enough to integrate safely, and the main branch will remain buildable and usable. Update milestone and roadmap status when the merged change completes an accepted outcome.

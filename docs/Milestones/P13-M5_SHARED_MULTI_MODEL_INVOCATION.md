# P13-M5 — Shared Multi-Model Invocation Infrastructure

## Objective

Provide one Provider-independent AI Center interface that can invoke multiple explicitly selected models concurrently for later use by AI Council and AI Arena.

P13-M5 completes the final remaining P13 goal from `AI_OS_MASTER_GUIDE.md`: shared multi-model invocation infrastructure.

## Scope

- Ordered multi-model participant input
- Duplicate participant and model removal
- Explicit Provider instance and model selection
- Independent operation ID for every participant
- Concurrent execution
- Independent success, failure, and cancellation results
- Failure isolation between participants
- Whole-invocation cancellation
- Stable aggregate result preserving participant order
- Aggregate timing and outcome counts
- Reuse of P13-M4 invocation metadata for successful results
- Safe normalized error categories

## Routing Rules

1. Every participant names one explicit Provider instance and model.
2. Multi-model invocation never uses Auto routing.
3. A participant never silently falls back to another Provider or model.
4. Participants execute concurrently.
5. One participant failure must not reject or discard other participant results.
6. Aggregate output preserves normalized input order.
7. Whole-invocation cancellation attempts to stop every active child invocation.
8. One child cancellation failure must not prevent cancellation of other children.

## Canonical Result

The aggregate result contains:

- Multi-invocation ID
- Start and completion timestamps
- Total latency
- Participant count
- Success count
- Failure count
- Cancellation count
- Ordered participant results

Each successful participant includes the existing `AiCenterResponse` and its P13-M4 metadata.

Each failed or cancelled participant includes only a safe normalized error category.

## Non-Scope

- AI Council expert-team assembly
- AI Council synthesis
- AI Arena rounds, roles, teams, rules, scoring, voting or replay
- Provider ranking
- Auto-routing policy changes
- New Provider integrations
- Agent execution
- Hermes or Custom Agent adapters

## Agent Boundary

AI-OS v1.0 has one operational execution Agent: OpenClaw.

Hermes and Custom Agent registry records remain interface placeholders for possible AI-OS v2.0 development. They do not have operational adapters and are not part of P13-M5.

## Acceptance Checklist

- [x] Multi-model invocation accepts explicit participants.
- [x] Duplicate participant IDs are removed.
- [x] Duplicate Provider-instance/model combinations are removed.
- [x] Remaining participants preserve input order.
- [x] Each participant owns an independent operation ID.
- [x] Participants execute concurrently.
- [x] One participant failure does not reject the aggregate invocation.
- [x] Successful results preserve P13-M4 metadata.
- [x] Failed and cancelled results expose safe error categories.
- [x] Aggregate results include timing and outcome counts.
- [x] Whole-invocation cancellation targets every active child.
- [x] Cancellation uses failure isolation.
- [x] Explicit participants do not use Auto fallback.
- [x] Frontend production build passes.
- [x] Full Rust library tests pass.
- [x] Cargo check and formatting pass.
- [x] Git difference validation passes.

## Completion Status

- **P13-M5:** Completed
- **P13 AI Center:** Completed
- **Completion date:** 2026-08-02
- **Architecture owner:** AI Center
- **Consumers:** P16 AI Council and P17 AI Arena

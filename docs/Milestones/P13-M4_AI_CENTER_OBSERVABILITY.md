# P13-M4 — AI Center Invocation Observability

## Objective

Make every AI Center invocation explainable without exposing credentials or provider-specific internals. AI-OS must report which Provider and model actually answered, whether execution was local or cloud, whether Auto routing fell back, how long the request took, and whether token and cost values are exact, estimated, free-local, or unavailable.

This milestone implements the provider-independent observability, cost, and latency metadata required by `AI_OS_MASTER_GUIDE.md`. AI Center owns the invocation record. Chat and future AI Council/AI Arena consumers render or persist the shared record without reconstructing routing decisions themselves.

## Scope

- One canonical TypeScript invocation-metadata contract returned by normal and streaming AI Center calls
- Explicit `local` or `cloud` execution source
- Explicit `auto` or `manual` route mode
- Ordered attempt records for Auto routing
- Safe success, failure, and cancellation outcomes
- End-to-end latency for each attempt and the completed invocation
- Clearly labelled token estimates when exact Provider usage is unavailable
- Estimated USD cost only when the pricing table has a verified match
- Truthful zero-dollar local Ollama pricing through the existing local wildcard price
- Conversation persistence of successful invocation metadata
- A compact, ordinary-user-readable response provenance row in Workspace
- Analytics recording through the existing analytics store
- Reusable metadata for later AI Council and AI Arena integration

## Non-Scope

- Provider billing reconciliation or claiming estimates are invoice-accurate
- New pricing entries without a verified source
- Persistent backend telemetry, remote analytics, or credential logging
- Prompt or response content inside routing-attempt records
- Provider ranking, quality scoring, budgets, or user-configurable routing policy
- AI Council or AI Arena execution logic
- Redesigning Task Engine, Runtime, conversation storage, or Provider adapters

## Canonical Contract

Each completed invocation exposes:

- Invocation ID
- Route mode: `auto` or `manual`
- Selected Provider instance, Provider, and model
- Execution source: `local` or `cloud`
- Start and completion timestamps
- Total latency in milliseconds
- Input and output token counts plus an `estimated` accuracy marker
- Estimated USD cost, or no cost value when pricing is unknown
- Whether pricing matched a verified table entry
- Whether fallback occurred
- Ordered attempts containing Provider, model, source, duration, and outcome

Attempt errors must be normalized to safe categories. Credentials, tokens, authorization URLs, raw response bodies, prompts, and generated text must never enter attempt metadata.

## Routing Rules

1. Manual selection produces exactly one attempt and never falls back.
2. Auto preserves the M3 Local First order: available Ollama models precede cloud candidates.
3. Auto may continue only when an attempt fails before emitting output.
4. Once an attempt emits output, AI-OS must not silently switch Provider or model.
5. `fallbackOccurred` is true only when the successful or terminal attempt is not the first attempt.
6. Cancellation is a terminal observable outcome and must not be reported as Provider failure.

## Cost and Token Truthfulness

- Until a Provider returns normalized usage through the AI Center contract, token counts are estimates based on bounded conversation text and output text.
- Estimated counts must be marked `estimated`; they must never be presented as exact usage.
- Cost is calculated with the existing pricing registry.
- Unknown pricing produces `pricingMatched: false` and no displayed dollar amount.
- Ollama matches the existing zero-cost local price entry and may display `$0.00 estimated`.

## Acceptance Checklist

- [x] Manual local selection returns metadata naming Ollama, the exact model, `local`, and `manual`.
- [x] Manual cloud selection returns metadata naming the exact Provider/model, `cloud`, and `manual`.
- [x] Auto success on the first local candidate reports one successful local attempt and no fallback.
- [x] Auto pre-output failure records the failed attempt and the next attempted candidate in order.
- [x] Auto never retries after output has started.
- [x] Cancellation is represented without inventing a successful response or a Provider failure.
- [x] Total and per-attempt latency values are finite non-negative integers.
- [x] Token values are finite non-negative integers and labelled estimated.
- [x] Known pricing produces an estimated USD value; unknown pricing displays unavailable rather than `$0.00`.
- [x] Successful Workspace assistant messages persist invocation metadata across reload.
- [x] Workspace shows a compact provenance row with source, Provider/model, latency, and truthful cost state.
- [x] Existing Analytics receives one success event per completed invocation with the same Provider, model, latency, token estimates, cost state, route mode, and fallback flag.
- [x] No prompt, response text, access token, credential, authorization URL, or raw Provider error is stored in attempt metadata.
- [x] Existing explicit selection, Auto fallback, stop response, Task completion, and conversation behavior remain functional.
- [x] Frontend production build, Rust formatting, full Rust tests, `cargo check`, and `git diff --check` pass.
- [x] Real desktop validation covers one manual local request and one Auto Local First request.

## Completion Status

- **P13-M4:** Completed
- **Specification date:** 2026-08-02
- **Completion date:** 2026-08-02
- **Architecture owner:** AI Center
- **Dependencies:** Accepted P13 M3 local/cloud model routing and Local First Auto behavior

## Completion Evidence

- Pure observability test script passed for local/cloud source, token estimation, verified and unknown pricing, safe error categories, ordered fallback attempts, cancellation/no-fallback policy, no fallback after emitted output, and Analytics field parity.
- Frontend production build passed with 2,674 transformed modules.
- Rust formatting check passed; all 394 Rust library tests passed; `cargo check` passed with four pre-existing warnings.
- `git diff --check` passed.
- Fresh isolated desktop application build passed.
- Manual Ollama request returned the exact `M4 Manual verified` response and displayed `On this Mac`, `ollama · qwen2.5:7b`, measured latency, and `$0.00 estimated`.
- Reload preserved the response provenance row.
- Auto returned the exact `M4 Auto verified` response through `qwen2.5:7b` with local provenance and no fallback marker.
- Manual xAI request returned the exact `M4 Cloud verified` response and displayed `Cloud`, `grok · grok-4.5`, measured latency, and a nonzero estimated cost.
- A long Ollama response exposed Stop response; cancellation preserved partial text and canonical cancelled invocation metadata without recording a success Analytics event.

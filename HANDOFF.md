# AI-OS — Current State

> Updated: 2026-08-11
> This file is the single source of truth for current repository state.
> Historical detail lives in `docs/archive/HANDOFF_HISTORY.md`.

---

## What this product is

AI-OS is a local-first personal AI operating system. The user says what they
want in plain language; AI-OS works out how to do it and gets it done.

Work is carried out by **Agents** — local executors that can actually touch the
computer, the internet, and connected services. Agents are the hands. The AI
models behind them, managed by AI Center, are the brain. Models are replaceable;
the product does not depend on any single provider.

Two capabilities are the reason this product exists, and neither is a chatbot
feature:

**AI Council（AI 智囊团）** — for any objective, a Chief of Staff works out which
models should be involved, assembles them into an advisory team, runs the
discussion, and produces a recommendation. Other products stop at the report.
Here the recommendation is meant to become work: the conclusion is handed to
Task Engine as a Do task, Planner turns it into steps, the user confirms, and an
Agent executes it. **Advice that turns into action is the differentiator.**

**AI Arena（AI 竞技场）** — multiple AIs interact under shared rules: debate,
social-deduction games such as Werewolf, collaboration, competition, role play,
simulation, or just talking to each other. Model comparison is one mode, not the
point. Arena is where the user watches AI behave, not where the user gets work
done.

Everything else in the roadmap exists to make these two possible and reliable.

---

## Agent roadmap

**v1.0 — OpenClaw only.**

OpenClaw is the single operational execution Agent for v1.0. Hermes and Custom
Agent records exist in the Registry but are non-operational placeholders. All
non-built-in agents are deletable; OpenClaw is built-in protected.

**v2.0 — Hermes leads, OpenClaw assists.**

The intended v2.0 arrangement, recorded now so the architecture does not close
it off:

- **Hermes** becomes the primary Agent. It is a growth-type agent — the more it
  is used, the better it understands the user. It owns continuity and judgement.
- **OpenClaw** becomes Hermes's assistant, used for what it is best at:
  connecting to and operating external systems and services.
- **WorkBuddy** and other agents may follow.

Design constraint this places on v1.0 work: the Runtime-to-Agent boundary must
stay a general adapter contract, not an OpenClaw-shaped one. Adding a second
Agent must not require changes to Task Engine, Planner, or Runtime.

---

## Open design gap — Council to execution

**Status: not yet specified. Decide before P16 implementation begins.**

AI Council currently ends at a recommendation. The path from recommendation to
executed work is undefined, which means P16 as specified would produce a
report-writing feature and the user would have to restate the plan manually to
get it done.

The required v1.0 path:

```text
AI Council
  ↓  recommendation
Task Engine        creates a Do task with the recommendation as context
  ↓
Planner            decomposes it into executable steps
  ↓
User confirmation  required — Council plans may include irreversible actions
  ↓
Runtime → OpenClaw executes
```

Things to settle when this is specified:

- Who converts a prose recommendation into a task objective — Council, Task
  Engine, or Planner
- What the confirmation surface looks like, and which step granularity the user
  approves
- Whether a Council plan can be saved and re-run later
- What happens when a step fails midway through a Council-originated plan

---

## AC-EXEC-MODEL — completed 2026-08-23

**Status: completed.**

### Final architecture

Download execution uses dedicated OpenClaw execution agents selected by AI Center.

Current execution agents:

| Agent | Model | num_ctx | Skills | Tools | Used for |
| --- | --- | ---: | --- | --- | --- |
| `ai-os-files` | `omlx/Qwen3.5-9B-4bit` on Apple Silicon; existing Ollama model elsewhere | 65536 | baidu-drive | exec, read | Filesystem scan/read/write/move |
| `ai-os-exec-standard` | `omlx/Qwen3.5-9B-4bit` on Apple Silicon; existing Ollama model elsewhere | 65536 | baidu-drive | exec, read | Download execution |

Filesystem operations remain on `ai-os-files`.

Download execution asks AI Center for eligible OpenClaw execution-agent ids and
uses them in preference order.

### Selection policy

1. Capability requirements are admission gates.
2. Each execution capability declares its own required context window.
3. `download.start` currently requires 65536 context, based on measured
   `baidu-drive` Skill execution requirements.
4. AI Center compares the requirement against the execution agent's configured
   effective context, not the model's theoretical maximum.
5. Agents that cannot satisfy the requirement are excluded.
6. Eligible candidates follow AI Center preference order.
7. Local First applies among eligible candidates.
8. Eligible cloud execution agents may follow local candidates.

Context requirements are capability-scoped rather than one global constant.
Future Skills may declare different context requirements without changing the
routing rule.

### Runtime fallback policy

Download Runtime consumes the ordered execution-agent candidate list.

A download may execute on at most two agents:

- preferred eligible agent;
- one fallback agent.

Fallback is allowed only when Runtime file verification rejects an otherwise
completed execution with the retryable file-verification `ProtocolFailure`.

Malformed requests, denied permissions, authentication failures, and other
non-file-verification failures stop immediately.

Each execution-agent attempt receives a distinct idempotency key containing the
execution id and agent id.

Runtime file verification remains authoritative. Agent self-report alone is
never sufficient for download success.

### Implementation

AI Center execution-agent selection is implemented in:

`src-tauri/src/providers.rs`

Responsibilities:

- capability-specific context requirements;
- effective context-window admission;
- execution-agent/model matching;
- AI Center preference ordering;
- returning agent ids without leaking routing internals.

Download fallback is implemented in:

`src-tauri/src/runtime/openclaw_gateway_adapter.rs`

Responsibilities:

- obtain ordered execution-agent candidates;
- preserve existing download routing;
- execute candidates sequentially;
- cap attempts at two;
- retry only the approved file-verification failure;
- preserve destination file verification.

Adding another eligible execution agent does not require Task Engine, Planner,
or capability-contract changes.

### Verification

Deterministic verification:

- `verify/verify_ac_exec_model_step1.sh`
- `verify/verify_ac_exec_model_step3a.sh`
- `verify/verify_ac_exec_model_step3b1.sh`
- `verify/verify_ac_exec_model_step3b2.sh`
- `verify/verify_ac_exec_model_step3b3.sh`
- `verify/verify_ac_exec_model_complete.sh`

Verified behavior:

- capability-specific context admission;
- AI Center candidate ordering;
- filesystem execution remains separate;
- downloads consume AI Center candidates;
- per-agent idempotency;
- retryable file-verification failure;
- sequential fallback;
- maximum two execution attempts;
- non-file-verification failures stop immediately;
- OpenClaw adapter tests pass;
- Rust compilation passes.

Real E2E verified 2026-08-23:

A Baidu share-link download ran through the app using
`ai-os-exec-standard`. The agent executed the provider Skill workflow and a
9.8 MB file landed in the selected destination.

### Technical decisions

Execution routing uses agent granularity rather than per-run model override.

An execution agent binds:

- model;
- configured context window;
- Skill allowlist;
- tool permissions;
- workspace.

Runtime does not hard-code provider selection by cloud-drive domain.

Runtime file verification must remain in place because model self-report cannot
be trusted as proof that a download actually completed.

### Constraints to preserve

- Filesystem operations stay on `ai-os-files`.
- Execution agents retain minimal Skill and tool exposure.
- Runtime file verification must not be removed.
- Runtime must not hard-code cloud-drive provider selection by domain.
- Do not reintroduce prose substring matching for machine error classification.
- Task Engine and Planner must not know individual execution-agent ids.

### Known non-blocking issues

- `bdpan download` may create a directory named after the downloaded file.
- Local 8B execution at 64K can be slow.
- Probabilistic real-model behavior remains a manual E2E confirmation rather
  than a deterministic CI assertion.

---
### oMLX migration — 2026-08-23

Ollama was replaced by oMLX. Both execution agents now run
`omlx/Qwen3.5-9B-4bit`. Three separate failures surfaced and each had a
different cause; the symptom in every case looked like "the model ignores the
Skill and just runs curl".

1. **Out of memory.** 9B at `num_ctx` 65536 exceeded the Metal watermark
   (11.5 GB against an 11.2 GB abort threshold) and the request was aborted
   before the agent ran. Lowered to 32768 for both agents.

2. **Skill hunting.** The prompt said to inspect `$HOME/.agents/skills`, which
   the model read as an instruction to search the filesystem. It spent six execs
   on `find`, `ls`, and `which`, looked in the wrong directory, then invented a
   `bdpan --url` flag that does not exist. Fixed by giving the exact path
   (`$HOME/.agents/skills/<name>/SKILL.md`), forbidding search commands, and
   forbidding invented flags.

3. **Stopping at preparation.** `baidu-drive` requires a generated
   `--session-id`. The model computed it, echoed it, and ended its turn. Fixed
   by stating that preparing a value is not a step and must not end the turn.

**Context requirement correction.** AC-EXEC-MODEL previously declared 64K for
Download, measured on `qwen3:8b`. Under `omlx/Qwen3.5-9B-4bit` 32K is
sufficient and 64K is actively harmful because it exhausts GPU memory. This is
why the requirement is declared per capability and validated against the
agent's real configuration rather than fixed as a global constant — a model
swap changes the number.

Verified end to end through the app on 2026-08-23 at 20:25: a Baidu share was
transferred and both files landed in the selected directory.

**Observation, not a defect:** the app run downloaded both files in the share
because the request named no specific item. The prompt already forbids
downloading a whole share when the user identifies one file; behaviour with an
explicit filename has not been re-tested since the model change.

---

## P15 Real-World Task Closure — decided 2026-08-29

The unified real-world task closure is:

Research → Verify → Ask → Compare → Decide → Timing → Confirm → Execute → Validate → Distill

Chinese product meaning:

查 → 验 → 问 → 比 → 决 → 择时 → 确认 → 执 → 验收 → 沉淀

- This is the general AI-OS real-world task closure, not a Commerce or Shopping-specific capability.
- Planner orchestrates the stages; Skills provide concrete capabilities; Runtime performs real execution and does not own business decisions.
- Memory / Cognitive Distillation consumes structured results that are eligible for distillation under its own policy.
- Real-time data must distinguish search discovery, verified data, account-state truth, and the final executable result. Freshness is a required decision input.
- When external information is insufficient, Ask may use Browser, Email, customer service, contacts, or other capabilities to fill the gap.
- Compare must apply the user's real constraints instead of comparing surface price alone.
- Timing must decide whether now is the best execution time, including billing and payment cycles, exchange rates, offer expiry, inventory, shipping, booking windows, maintenance windows, and market volatility.
- Confirm is required for actions needing user authorization, including payment, submission, deletion, sending, and signing.
- Execute must be followed by Validate; a submitted request is not proof of completion.
- Distill does not automatically write every task to long-term Memory. It produces structured results for Memory / Cognitive Distillation to consume according to their policies.

Design source: a hard-drive purchase exposed conflicts between stale or sold-out public-search prices and the live product page. AI-OS must prefer current, verifiable, executable information and account for the user's shipping, warranty value, exchange rate, and payment cycle.

### P15 Skill implementation alignment — verified 2026-08-29

Browser/Search, Downloads, File Management, Local Model Management,
Email/Calendar, Document, and the implemented Spreadsheet capabilities now
provide structured evidence and real-result validation inputs for the Planner
closure. These Skills provide capabilities only; none owns or reimplements the
Planner's full closure.

Evidence records source, provider, observation time, freshness, and a stable
state. Search discovery is not the same as verified evidence, authenticated
account state, an executable result, or a validated completed action. Timing
remains Planner metadata and may delay Execute without moving business rules
into Skills or Runtime. Execute must be followed by Validate; submission or a
successful command alone is not completion. Distill produces structured output
for later policy-controlled consumption and does not automatically write
long-term Memory.

Spreadsheet Read and Create real Excel E2E passed on 2026-08-29. Create now
saves the workbook, reopens the generated workbook, reads the first worksheet
used range, and requires TSV read-back equality before reporting success.

## P15 Provider Selection & Authorization Policy — decided 2026-08-29

Default provider priority is:

Official API / OAuth → Native Structured Interface → Authenticated Session →
Deterministic Automation → Computer Use

- Capability support, resource compatibility/locality, availability, authorization state, interface priority, and explicit user preference govern selection.
- Resource locality can override generic interface priority. Local files are never uploaded merely to use Microsoft Graph.
- OAuth authorization and per-action User Confirmation are separate contracts.
- Secrets, raw tokens, passwords, and cookies never enter Planner input/output, Memory, or Evidence. Providers receive only opaque authorization references.
- An unavailable or incomplete Official API may fall back only when policy allows it. Providers must never hide internal fallback.
- Microsoft Graph is the Official API/OAuth provider for Microsoft 365, OneDrive, and SharePoint resources. Authorization Code + PKCE uses the existing single-use ephemeral loopback callback and Keychain token store; startup and post-connect state require a real `/me` identity request.
- Local Structured File is the preferred foundation for supported local resources. Native Excel remains the deterministic local provider for Excel-specific behavior.
- Native Excel sessions are externally owned by default; AI-OS must not close a user-owned Excel application session.
- macOS SystemPermission state distinguishes not determined, granted, and denied; granted permission is detected and reused.
- Authenticated Browser session is a fallback provider, not an Official API, and stores only an opaque profile/session reference.
- Commerce capabilities add platform-specific Official API providers when supported without changing the Skill contract.

### P15 Official Provider real integration status — 2026-08-29

- Provider Selection & Authorization Foundation: **Completed**.
- Microsoft Graph code: **Implemented; Entra App Registration status is NotConfigured.** The remaining external step is creating the user-owned public-client registration and completing Microsoft login/consent. Configure the public client id with `VITE_AI_OS_MICROSOFT_OAUTH_CLIENT_ID` or the collapsed Advanced developer settings; optional tenant strategy is `VITE_AI_OS_MICROSOFT_OAUTH_TENANT` (`common` by default). No client secret is used.
- Microsoft Graph executable scope: `/me`, drive discovery, XLSX discovery, workbook session creation, used-range/range read, explicit-range write, and mandatory Graph read-back validation. Graph resources use only drive/item/worksheet/range references; local XLS/XLSX never route to Graph.
- Google Workspace Official API/OAuth integration is **Implemented; real E2E PASS on 2026-08-29.** The verified path covers OAuth/PKCE, macOS Keychain authorization, real Google identity, Drive listing, Docs create/read-back, Sheets create/write/read-back, Slides create/read-back, and automatic deletion of dedicated E2E resources. Sheets reads use `UNFORMATTED_VALUE` so numeric and boolean values retain structured API types.
- WPS is **Backend Broker Required**, not Connected or Completed. A confidential WPS APPKEY must remain server-side; it is forbidden in the desktop binary, Planner, Memory, Evidence, logs, or frontend storage. No browser fallback may be labelled WPS Official API.
- Apple iWork was **App Not Installed** on the 2026-08-29 acceptance machine. Connections now rescans Pages, Numbers, and Keynote independently at runtime and reports partial installation accurately; Native real E2E remains SKIP until an application is installed. Native sessions remain externally owned and AI-OS must never quit a user-owned app.
- Disconnect is local: My AI removes the Keychain credential and local authorization mapping. AI-OS does not claim a remote Microsoft revoke when the public-client flow provides none.
- Browser authenticated sessions: lifecycle/evidence contract and persistent opaque profile metadata are implemented. Profile metadata is stored atomically in the app-data directory and contains no cookies, passwords, or bearer tokens. Authentication still requires a verified account marker on an HTTPS platform origin; a public page is never authenticated. No browser runtime with a safely callable account-state detector is currently connected, so real session E2E remains SKIP on user login/profile handoff rather than fabricating Connected.
- eBay is the first built-in External Connector instance. Its main Connections row uses the reviewed `ebay-buy` manifest and shared Broker contract; it is no longer a fixed Developer Approval Required placeholder and cannot be added a second time through Add Other Provider. Browse can be available while Cart/Checkout/Order remain approval-gated. Real external integration remains blocked on a deployed Broker, eBay developer application/real account, and any required Production Buy API approval.
- Amazon Consumer: Product Advertising API is limited to product advertising; no official ordinary-buyer account/cart/checkout/order API is registered. Authenticated Browser is required for those consumer actions. Selling Partner API must not be used as a buyer API.
- Taobao / JD / Pinduoduo Consumer: their open platforms are merchant/service-provider oriented or approval-limited; AI-OS has no approved ordinary-consumer cart/checkout/order API credential. Authenticated Browser is the declared fallback and must verify the signed-in account before authenticated evidence.
- Commerce capability facts were reviewed against official platform documentation on 2026-08-29. Unsupported official consumer APIs are an accurate boundary, not a failed E2E.
- P15 progress remains **5 of 11 complete** because Presentation and the complete Office capability area remain unfinished.

### P15 Office Provider matrix — 2026-08-29

| Provider | Document | Spreadsheet | Presentation | Real E2E / boundary |
|---|---|---|---|---|
| Microsoft Graph | Implemented, OAuth | Implemented, OAuth | Unsupported in current adapter | SKIP — user app registration/consent |
| Google Workspace | Implemented, OAuth | Implemented, OAuth | Implemented, OAuth | **PASS — real identity + Drive + Docs + Sheets + Slides E2E, 2026-08-29** |
| WPS | Backend-broker contract | Backend-broker contract | Backend-broker contract | SKIP — server APPKEY provisioning |
| Apple iWork | Native Pages provider declared | Native Numbers provider declared | Native Keynote provider declared | SKIP — apps not installed |
| Local Structured | Implemented | Implemented foundation | Unsupported | Foundation/local fallback |
| Native Microsoft Office | Implemented | Implemented, real E2E | Provider declared; read/create unfinished | Excel E2E PASS; Presentation incomplete |

Presentation remains **In progress**. Google Slides read/create and real Google external OAuth E2E now pass; Keynote cannot run because the app is not installed, and the provider-neutral Presentation runtime acceptance is not complete.

### P15 Unified Connections & Account Onboarding — decided 2026-08-29

- External Connector Framework is the shared connection architecture for reviewed website/API integrations. Built-in and Custom Provider instances use the same manifest validation, capability state, Broker safety, authorization-reference isolation, disconnect, and removal contracts; adding a future site requires a reviewed manifest, capability mapping, and behavior tests rather than another OAuth/Broker state machine.
- `Add Other Provider` supports Local Application, Website Login, trusted External API Connector, and Unsupported Provider Request. Ordinary users select a type and enter only public application configuration. Arbitrary URLs plus arbitrary JSON are never executable Connectors; unknown or untrusted Providers receive no high-risk capability.
- Provider Definition and Provider Instance identities are separate. Custom Providers persist atomically and survive restart; malformed persisted configuration fails closed. Built-in definitions may be disconnected but not removed. Custom Providers may be disconnected and then removed after confirmation.
- Backend Broker is the only owner of client secrets, certificates, raw tokens, private keys, and authorization codes. Desktop storage is limited to public configuration, Broker URL, opaque environment-scoped authorization reference, capability state, and non-sensitive evidence reference. Production Broker URLs require HTTPS; HTTP is limited to localhost development.
- `Disconnect Account` performs remote Broker revoke/delete before clearing the local opaque reference and capability authorization state. A remote failure retains local tracking and is not reported as fully disconnected. `Remove Provider` is a separate custom-only action that first requires successful disconnect, then atomically deletes its definition, instance, public configuration, session/reference, and non-sensitive cached state.
- eBay Buy is the first reviewed Connector manifest and built-in Provider instance. The single main eBay row provides Configure → Save and Validate → Test → Connect → OAuth status verification → Connected/Partially Available → Disconnect Account. Browse, Cart, Checkout, and Order capabilities have independent authorization/approval state; `ebay.checkout.confirm` always requires User Confirmation. Production Buy API remains SKIP/WAITING_FOR_USER until the developer application and approval are real.
- eBay supports Managed by AI-OS and Self-hosted / Bring Your Own App Broker modes. Managed Broker status is **Not Provisioned** until `AI_OS_MANAGED_EBAY_BROKER_URL` points to a real deployment; no fake address is supplied. Self-hosted desktop configuration contains only Environment, public App ID, RuName, and Broker URL. The administrator configures `EBAY_CLIENT_SECRET`/Cert ID and `BROKER_ENCRYPTION_KEY` only on the independent Broker service.
- The deployable Broker lives at `services/external-connector-broker`. It implements health, reviewed manifest discovery, configuration validation, authorization begin/status/callback, encrypted token storage and refresh, capability availability/execute, and token deletion on disconnect. eBay exposes no general remote token revoke endpoint in this flow, so Disconnect truthfully reports remote revoke unsupported while deleting the Broker token and desktop opaque reference.
- Broker unit tests and local mock eBay HTTP tests are local behavior evidence only; they are not a real eBay account or Production Buy API E2E PASS.
- Rescan Apps and Apple iWork Connect are pre-existing behavior and remain on their original runtime/Tauri paths; this framework only carries regression coverage for them.
- Connections in My AI is the single onboarding surface for Microsoft 365, Google Workspace, WPS, eBay, Amazon, Taobao, JD, Pinduoduo, and Apple iWork. It extends the existing Provider and Keychain systems; it is not a second Provider registry or credential store.
- `Connect All` advances in the fixed order Microsoft → Google Workspace → WPS → eBay → Amazon → Taobao → JD → Pinduoduo → Apple iWork. Already connected providers are skipped, configuration/approval/app blockers are retained in the summary, and Skip or a later failure never removes earlier successful connections.
- AI-OS owns provider detection, official authorization URL construction, ephemeral callback port, PKCE/state, callback processing, secure token storage, opaque browser profile reference, connection verification, reuse, and reconnect.
- The user enters credentials only on the platform's official page and personally completes 2FA, CAPTCHA, OAuth consent, developer terms, identity verification, or administrator approval. AI-OS never reads or stores a third-party password.
- OAuth, Backend Broker, Native Application, and Authenticated Browser share one status surface. It includes Not Configured, Disconnected, Connecting, Waiting for User, Connected, Expired, Login Required, Authorization Required, Developer Approval Required, Backend Broker Required, App Not Installed, and Error; their authorization mechanisms remain distinct.
- Connected is evidence-gated. OAuth requires a successful identity API request. Browser login requires a provider-specific account marker or authenticated endpoint from the persistent profile; opening a login page is only Waiting for User.
- The current MCP Browser bridge does not expose a safe persistent-profile account verifier. Amazon, Taobao, JD, and Pinduoduo onboarding therefore opens only the official login page and remains Waiting for User rather than fabricating Connected. Completing real Browser E2E requires a browser provider that can return a safe account marker without exposing password fields, raw cookies, or bearer tokens.
- Microsoft application identity is application-level configuration. The public Client ID input is inside Advanced developer settings, which is collapsed by default; the ordinary Connections surface contains only provider status and connection controls. Development builds show Configuration Required until `VITE_AI_OS_MICROSOFT_OAUTH_CLIENT_ID` exists or the public application id is entered there. End users must not be required to create their own Entra application in a production distribution. The public client id is not a credential and no client secret is accepted.
- Runtime local-application availability is owned by the Connections backend command, not frontend persistence. Page entry and Rescan Apps share the same fresh detector across `/Applications` and the current user's `Applications` folder. It identifies Pages, Numbers, Keynote, WPS Office, and Microsoft Excel by bundle ID, with standard bundle paths as fallback, so renamed app bundles are detected. Installing or removing an app changes the next result without recompiling; iWork aggregation is recalculated from the latest component results. Manual rescans show progress/success beside the button and surface failures explicitly.
- Apple iWork `Authorization Required` is actionable: Connect invokes the registered native command and requests Automation access from each installed iWork application by bundle ID. The UI becomes Connected only after every installed iWork authorization probe succeeds; denial or command failure is shown as Error rather than silently treated as connected.
- Technical decision: Microsoft desktop OAuth uses PKCE with a dynamic loopback port and the stable registered redirect `http://localhost/oauth/callback`; OAuth, SystemPermission, AuthenticatedSession, and UserConfirmation remain separate, and no client secret is accepted or stored.
- Technical decision: Google Workspace desktop OAuth retains the dynamic `127.0.0.1` loopback root callback used by the successful real OAuth/E2E path. The Google OAuth client secret required by the token endpoint is stored only in macOS Keychain through the OAuth-client service and must never enter frontend persistence, Planner, Evidence, Memory, logs, or the repository.
- eBay capability approval is per capability. Production checkout/order remain Developer Approval Required when the Broker reports missing Buy API approval; Browse can remain available. AI-OS does not route ordinary consumer checkout through seller APIs.

## P15 progress

This table is the single answer to "how far along is the current P15
implementation scope". It reflects the latest roadmap amendments in this
HANDOFF, including deferred and replacement capabilities.

| # | Capability area | Status | Acceptance |
|---|---|---|---|
| 1 | Downloads | Done | `verify_p15_download_*`, `verify_p15_baidu_official` |
| 2 | File management | Done | `verify_p15_file_*`, `verify_p15_filesystem_provider_*` |
| 3 | Browser and search | Done | `verify_p15_browser_*` |
| 4 | Local model management | Done | `verify_p15_local_model_*` |
| 5 | Email and calendar | Done | `verify_p15_email_calendar_*` |
| 6 | NAS management | Not started | — |
| 7 | Document, spreadsheet, presentation | In progress | `verify_p15_document_*`, `verify_p15_spreadsheet_*`, `verify_p15_office_provider_registry` |
| 8 | Computer Control | Not started | — |
| 9 | Vehicle Control | Not started | — |
| 10 | Local generative media | Not started | — |
| 11 | Cognitive distillation foundation | Not started | — |

**5 of 11 complete.**

### Historical remaining P15 order — decided 2026-08-23, superseded 2026-08-28

This ordering is retained as history and is not the current implementation status.

1. Email and calendar
2. Document, spreadsheet, presentation
3. Local generative media
4. Cognitive distillation foundation
5. NAS management — blocked on hardware
6. Smart home and device control — blocked on hardware

NAS and smart home are last because the hardware has not arrived. Writing them
without a device to test against produces code nobody can verify, which is how
the CSS regressions survived for months.

Cognitive distillation sits second to last: it is the prerequisite for P16 AI
Council, but it is closer to research than to a shippable capability, and the
first three areas are what make AI-OS usable day to day.



Supporting infrastructure, not a capability area in its own right:
`verify_p15_mcp_*` (MCP tool and skill execution) and
`verify_p15_openclaw_history_compat`. Every capability above depends on these,
so a regression there breaks several areas at once.

Also complete and shared by all execution work: AC-EXEC-MODEL — AI Center
selects the execution agent, with capability and context window as hard
admission gates, Local First second, and one fallback attempt. See
`verify_ac_exec_model_*`.

Naming note: today's Download work appears as AC-EXEC-MODEL in this file, as
`p15_download_*` in acceptance, and as one word ("Downloads") in the Master
Guide. When adding an area, record all three names here so the mapping stays
findable.

---

## P15 roadmap amendment — 2026-08-28

**Status: decided. Do not reopen this scope during implementation unless the
roadmap is explicitly revised.**

Smart Home / Device Control is removed from the current P15 implementation
scope and deferred. There is no current smart-home hardware available for real
end-to-end validation, so P15 must not ship a mock-only implementation merely
to satisfy the roadmap.

Two capabilities are added to P15:

1. Computer Control
2. Vehicle Control

### Computer Control v1

Purpose: fill deterministic macOS system-level gaps that Computer Use and
OpenClaw do not handle cleanly or reliably.

Computer Control does not replace Computer Use and does not become another
general-purpose Agent.

Fixed ownership:

- Computer Use: visual GUI interaction, clicking, typing, dragging, and other
  screen-driven operations
- OpenClaw: general Agent execution and tool-driven work
- Computer Control: deterministic macOS system state and direct system
  operations
- Domain Skills: Office, Vehicle, Downloads, NAS, generative media, and other
  specialist capabilities

Computer Control v1 may cover:

- system.storage
- system.cpu
- system.memory
- system.network
- system.process.*
- system.app.*
- system.clipboard.*
- system.audio.*
- system.power.*
- system.notification.*
- system.permissions.*

Do not expand Computer Control into GUI automation already owned by Computer
Use.

### Vehicle Control v1

Vehicle Control is a separate domain Skill. v1 supports Tesla only.

Architecture:

Vehicle Control → Tesla Provider → official Tesla Fleet API

The official Tesla App may also be used as a handoff / authorization surface
when Tesla requires its own UI, pairing flow, user confirmation, or active
supervision.

Vehicle Control v1 should include official capabilities where available for:

- vehicle state
- battery and charging state
- climate
- lock / unlock
- charging start / stop and charging limits
- navigation destinations and waypoints
- other supported non-driving remote commands

Navigation is part of Vehicle Control v1.

Vehicle Control must not:

- reverse-engineer unsupported Tesla private APIs
- treat the Tesla App as an unofficial programmable API
- use Computer Use to bypass Tesla safety controls
- directly control steering, acceleration, braking, FSD driving, or Actually
  Smart Summon when Tesla has not exposed an appropriate official third-party
  interface

FSD / Actually Smart Summon remain future extension areas. Reserve conceptual
space for `vehicle.autonomy.*` and `vehicle.summon.*`, but do not implement
them until Tesla exposes an official supported interface.

### Updated remaining P15 execution order

After the current Office capability:

1. Computer Control v1
2. Vehicle Control v1 — Tesla Provider
3. Local generative media
4. Cognitive distillation foundation
5. NAS management foundation — hardware E2E remains blocked until storage is
   installed

Computer Control and Vehicle Control are implemented serially, not in
parallel.

NAS remains in P15, but hardware-dependent storage behavior must not be marked
complete without real hardware E2E.


## P15-1 Email and calendar — completed

Status: completed 2026-08-23

Implemented:
- macOS permission detection foundation
- Native Mail.app read/search foundation
- Native Calendar EventKit foundation
- Calendar event creation foundation
- Mail draft creation foundation
- Mail send confirmation foundation

Architecture validation:
- Native First decision validated.
- AppleScript Mail capability evaluation completed.
- Mail.app access works through native macOS integration.
- No current capability gap requires Gmail API or Microsoft Graph.
- Cloud API remains optional future enhancement only.

Evaluation result:
- Mail.app available: yes
- AppleScript access: yes
- Account count: 1
- Mailbox scan: completed successfully
- No Cloud API integration required at this stage.

Next:
Continue remaining P15 Core Skills according to capability ordering.

## P15-2 Document Read, Create and Convert — completed

Status: completed 2026-08-26

Implemented:
- `document.read` Skill registry entry
- Office Provider Registry with Local First ordering
- macOS Native provider resolution
- one-time permission confirmation
- Runtime → OpenClaw Gateway execution
- Native `/usr/bin/textutil` DOC/DOCX text extraction
- absolute-path and supported-extension fail-closed validation
- bounded text output
- `document.create` one-time permission confirmation
- Native `/usr/bin/textutil` DOC/DOCX creation
- absolute-path, extension and 4096-byte input validation
- atomic create-only behavior with no overwrite
- `document.convert` one-time permission confirmation
- Native DOC ↔ DOCX conversion through `/usr/bin/textutil`
- absolute source/destination validation and atomic no-overwrite output

Acceptance:
- `verify/verify_p15_document_skill_registry.sh`
- `verify/verify_p15_office_provider_registry.sh`
- `verify/verify_p15_document_read.sh`
- `verify/verify_p15_document_create.sh`
- `verify/verify_p15_document_convert.sh`

The broader Document, Spreadsheet and Presentation capability remains in
progress. Spreadsheet Create and presentation work have not started.

## P15-3 Spreadsheet Read and Create — completed

Status: completed 2026-08-28

Implemented:
- `spreadsheet.read` and `spreadsheet.create` Skill Registry capabilities
- one-time permission confirmation for read and create
- Local First Office Provider resolution
- Microsoft Excel for Mac AppleScript adapter
- absolute XLS/XLSX path validation
- Spreadsheet Read first-worksheet used-range reading
- sheet name, row count, column count and bounded TSV output
- Spreadsheet Read workbook close without saving
- Spreadsheet Create bounded TSV input
- Excel-container workbook generation
- XLS and XLSX output selection
- fail-closed unsupported Provider behavior
- no-overwrite target movement
- Runtime → OpenClaw Gateway execution
- success only from a real exec tool result

Acceptance:
- `verify/verify_p15_spreadsheet_read.sh`
- `verify/verify_p15_spreadsheet_create.sh`
- `verify/fixtures/p15-spreadsheet-read.xlsx`

Presentation is in progress: Google Slides read/create, read-back validation, OAuth authorization, and real external Google E2E pass; provider-neutral Presentation runtime acceptance remains outstanding, and Apple Keynote is not installed on the acceptance machine.

## P15-1 Email and calendar — historical architecture decision, implementation completed

This pre-implementation status was superseded by the completed milestone above on 2026-08-23.

**Decided 2026-08-23. These choices are made; do not reopen them at implementation time.**

### Native first, cloud as an optional supplement

Read and everyday operations go through the system apps — Mail.app and
Calendar.app via AppleScript and EventKit. Cloud APIs (Gmail API, Microsoft
Graph) are added only for what native cannot do, and only as an opt-in
enhancement in Settings.

This is a product decision, not a technical one. The Master Guide's first
principle is that the user does not configure anything and does not need to know
which tool AI-OS used. The user already signed into Gmail, iCloud, or Outlook in
macOS. Reading Calendar.app needs no OAuth, no API key, no developer app
registration — the capability works the moment it ships.

If the first use of email opened an OAuth consent screen and asked the user to
register a Google Cloud project, that principle would be broken on day one.

Layering:

- Native: list mail, search, read, list and create calendar events, draft and
  send replies
- Cloud API: only where native genuinely cannot deliver — complex server-side
  search, bulk archive, cross-account operations
- Cloud connection lives in Settings as an enhancement, never as a first-run
  requirement

Accepted trade-offs: AppleScript access to Mail is awkward (slow search, clumsy
attachment handling) and this path is macOS only. Both are acceptable because
AI-OS is a macOS product today and zero-configuration matters more than search
latency. Cloud APIs also conflict with Local First: message bodies would travel
over the network even under the user's own account. Native keeps the data on
the machine.

### Permissions are step one, not an afterthought

macOS gates Mail and Calendar behind privacy consent. Under Tauri dev the
consent dialog frequently does not appear and the call simply fails — exactly
what happened with the Downloads directory, where a permission failure surfaced
as "must be an existing absolute directory" and sent debugging in the wrong
direction.

The first implementation step is therefore permission detection and guidance,
before any feature work:

- Detect whether AI-OS holds Automation and Calendar access
- If not, name the exact System Settings pane the user must open
- Never let a permission failure surface as a generic capability error

### Sending requires confirmation

Reading is automatic. Sending is not.

"Reply for me" means AI-OS sends mail as the user. This is the same class of
risk as the destructive-operation confirmation already required for downloads,
and worse — a sent message cannot be recalled.

Fixed rule: read automatically, always confirm before sending. The confirmation
must show recipient, subject, and body exactly as they will be sent. This
applies to replies, forwards, new messages, and calendar invitations that
involve other people.

### Implementation order

1. Permission detection and guidance
2. Read: list and search mail, list calendar events
3. Create: calendar events with no external attendees (low risk)
4. Draft: compose replies without sending
5. Send: with mandatory confirmation surface
6. Cloud API supplement, only if steps 2-5 expose a real gap

Each step ships its own acceptance script. Steps 1-4 are automatable; step 5
needs manual confirmation of the confirmation surface itself.

### Open question, answer during step 2

Which native path for mail — AppleScript against Mail.app, or reading the local
mail store directly? AppleScript is the supported interface but slow; direct
store access is faster but undocumented and breaks on macOS updates. Decide
after measuring AppleScript search on a real mailbox, not before.

Calendar has no equivalent question: EventKit is the correct supported interface.

---

## Repository state

| | |
|---|---|
| Branch | `feature/p15-core-skills` |
| HEAD | P15 Provider/Office integration active — Google Workspace real E2E verified 2026-08-29 |
| Latest tag | `p13-m5-complete` |
| Baseline state | Working tree was clean at the stable baseline before this handoff update |
| Active phase | P15 Core Skills — 5 of 11 complete; Office remains in progress. Google Workspace real E2E is complete; provider-neutral Presentation acceptance remains pending. |

---

## Completed

| Phase | Scope | Evidence |
|---|---|---|
| P9 | Runtime foundation: lifecycle, executor, operations, scheduler, recovery | `src-tauri/src/runtime/` |
| P10 | Task Engine and Planner, through P10-M13 | `1d173be`, tag `v0.11.0` |
| P11 | OpenClaw integration: execution contract, gateway adapter, permission boundary, event forwarding | `1d173be`, tag `v0.11.0` |
| P12 | Skill Framework: registry, discovery, manifests, permission model, lifecycle | Completed; recorded retroactively |
| P13-M1 | Canonical Provider domain and adapter registry | |
| P13-M2 | Provider credential and account connection infrastructure | |
| P13-M3 | Local/cloud execution, model discovery, Local First routing | |
| P13-M4 | Provider-independent observability, cost and latency metadata | tag `p13-m4-complete` |
| P13-M5 | Shared multi-model invocation | `1e01cd2`, tag `p13-m5-complete` |
| AC-BACKEND-0 | Legacy MultiLLM removal; provider HTTP invocation moved to Rust | `verify/verify_p13_ai_center_migration.sh` |
| AC-BACKEND-1 | Auto ordering, Local First preference, and fallback selection moved to Rust | `verify/verify_ac_backend_1_routing.sh` |
| AC-BACKEND-2 | Canonical AI Center invocation metadata moved to Rust | `verify/verify_ac_backend_2_observability.sh` |
| Provider setup dialog | Restored setup/manage dialog styles removed during P13 migration | `verify/verify_provider_setup_dialog.sh` |
| UI Refactor | White-first workspace across Chat, Sidebar, My AI, Agents, Arena, Council, Artifacts, Settings | |
| Agent Registry | Non-built-in agents (including Hermes and Custom Agents) are deletable; OpenClaw stays built-in protected | `c88f7b9`, `9b54873` |
| Legacy UI Step1 | Removed obsolete page entries from App routing while preserving Models and MCP entries | `verify/verify_legacy_ui_step1.sh` |
| Legacy UI Step2 | Removed obsolete PageName and Settings navigation entries while preserving new workspace structure | `verify/verify_legacy_ui_step2.sh` |
| Ollama streaming fix | Increased local Ollama generation capacity and timeout handling for long AI Center streaming responses | `43da32f` |
| Chat workspace layout fix | Adjusted Chat message container width and spacing so long responses stay inside the workspace boundary | `6192b6a` |
| P14 Memory Retrieval | User memories are injected as hidden system context for ordinary Chat requests without entering conversation history | `verify/verify_p14_memory_retrieval.sh` |
| P14 General Memory Policy | Structured language, response detail, currency, and budget defaults with request-only overrides | `verify/verify_p14_general_memory_policy.sh` |
| AI Center End-to-End QA | Auto/Local First, manual selection, multi-model, streaming, cancellation, fallback, and analytics verified | `verify/verify_ai_center_e2e_qa.sh` |
| oMLX AI Center | Native oMLX provider, Keychain-backed Bearer auth, model discovery, My AI setup, Local First routing, and OpenAI-compatible streaming/non-streaming Chat execution | `verify/verify_omlx_ai_center_step1.sh`; automated provider suite and full Rust regression passed; real manual oMLX Chat UI E2E passed 2026-08-23 |
| oMLX local service UX | Connected oMLX instances move into On this Mac with runtime health, models, default model, refresh, start, and connection management; connected Apple Silicon installations auto-start when AI-OS opens | `verify/verify_omlx_ai_center_step1.sh`; 442 Rust tests and frontend production build passed; real Stopped → automatic Ready UI E2E passed 2026-08-23 |
| oMLX local model management | My AI exposes Details, Show in Finder, Delete, Pull model, and Refresh; oMLX admin authentication remains backend-only through the saved Keychain API key, Finder uses the server-returned model path, destructive deletion requires confirmation, and downloads report success only after the oMLX task completes | `verify/verify_omlx_ai_center_step1.sh`; Provider behavior tests and frontend production build passed 2026-08-23; Details UI E2E passed, Finder/Delete/Pull E2E pending |
| P15 Filesystem scan | Explicit PlanStep execution, one-time confirmation, permission enforcement, real OpenClaw `exec` scan, readable result rendering, Work-specific errors, and stable local message times | `verify/verify_p15_file_execution_contract.sh`, `verify/verify_p15_file_scan_confirmation.sh`; real UI E2E passed 2026-08-15 with `.DS_Store`, `Lable_副本.docx`, and `__副本.jpeg` |
| P15 Filesystem read | Explicit file picker and confirmation, real OpenClaw text read, MIME and size detection, 1 MB read limit, 64 KiB output limit, binary/unsupported handling, and readable Chat rendering | `verify/verify_p15_file_read.sh`; real UI E2E passed 2026-08-15 with repository `README.md` content |
| P15 Filesystem write | Explicit save-path selection and confirmation, real OpenClaw text creation, 4 KiB input limit, private 0600 permissions, and create-only/no-overwrite failure handling | `verify/verify_p15_file_write.sh`; isolated real OpenClaw create/no-overwrite smoke and real UI E2E passed 2026-08-15 with a 27-byte text file |
| P15 Filesystem move | Explicit source/destination selection and one-time confirmation, real OpenClaw move execution, no-overwrite behavior, absolute/different-path validation, fail-closed source/destination checks, and readable Chat rendering | `verify/verify_p15_file_move.sh`; isolated real OpenClaw smoke and real UI E2E passed 2026-08-15 moving `/private/tmp/ai-os-p15-ui-write-20260815.txt` to `/private/tmp/ai-os-p15-ui-move-20260815.txt` |
| P15 Local Model Core Skill | Ollama local model management through Runtime, including model list, inspect, pull, delete capabilities, Chat execution flow, My AI management UI, and unified Dialog interaction | `verify_p15_local_model_core_skill.sh`, `verify_p15_local_model_step1.sh`, `verify_p15_local_model_step2.sh`, `verify_p15_local_model_step3.sh`, `verify_p15_local_model_step4.sh`; completed 2026-08-16 |
| P15 Download Skill | Task/Plan/Runtime/OpenClaw execution; Direct HTTP, Web page, Thunder submission, aria2 tool routing, cloud-drive extension point, destination verification, safe errors, and readable Chat results | `verify/verify_p15_download_complete.sh`; real OpenClaw E2E passed 2026-08-21 |

P14 General Memory Policy behavioral QA passed:

- Language: long-term Chinese, current English, then restored Chinese
- Response detail: long-term concise, current detailed, then restored concise
- Budget and currency: long-term AUD 500, current AUD 1000, then restored AUD 500

AI Center End-to-End QA passed:

- Auto routing selected local Ollama `qwen2.5:7b` when available, preserving Local First
- Manual OpenAI `gpt-5.6-sol` selection executed without silent fallback
- P13-M5 shared multi-model invocation evidence remains valid
- Real Ollama streaming completed successfully
- Cancelling a long OpenAI response stopped output immediately with no later continuation or fallback, and restored input readiness
- Auto fallback selected cloud Anthropic `sonnet` only while Ollama was unavailable, then returned to local routing after recovery
- Analytics records included invocation ID, auto route mode, cloud source, Anthropic provider/model, four attempts, and fallback state

### Connected AI providers

| Provider | Method | Status |
|---|---|---|
| Google Gemini | Native PKCE OAuth | Verified — 50 models |
| OpenAI Codex / ChatGPT | Official Codex OAuth, port 1455 | Verified — 7 models, GPT-5.6-Sol |
| xAI Grok | RFC 8628 device auth + API key | Verified — grok-4.5 |
| Anthropic Claude Code | Official `claude` CLI as local proxy | Verified — Claude Pro |
| Ollama (local) | Local runtime | Verified — qwen2.5:7b, qwen3:8b, deepseek-r1:8b |
| oMLX (local, Apple Silicon) | Local OpenAI-compatible API + API key | Verified — DeepSeek-R1-Distill-Qwen-7B-4bit discovered; authenticated SSE and real AI-OS Chat UI E2E passed 2026-08-23 |
| DeepSeek | API key | Implemented, key not yet entered |
| OpenRouter | PKCE + API key | Implemented, no account yet |
| Kimi Code | RFC 8628 device auth | Implemented, no account yet |
| Meta Model API | Developer key | Implemented, no account yet |

Credentials are stored in macOS Keychain only. Never in browser state, Provider
config, or the repository.

---

## In progress

P15 Core Skills remains active. File management, Local Model Management, and
Download Skill are complete. Download execution now remains inside the v1.0
Agent boundary: Task Engine → Planner → Runtime → Skill → OpenClaw → tool/Web.

All four Filesystem capabilities enter through Task Engine and Planner, resolve through the
Skill registry, execute through Runtime and the dedicated `ai-os-files`
OpenClaw worker, enforce one-time confirmation where required, normalize real
tool results, and render readable Chat output without exposing raw Gateway
history.

---

## Next

P15 Core Skills implementation order:

Completed:
1. File Management Skill
2. Local Model Management Skill
3. Download Skill

Download Skill architecture:
- Chat supplies only `source` and `destination`; it never selects a provider.
- Skill registry resolves Download to the common OpenClaw executor.
- Direct HTTP files use an OpenClaw `exec` tool workflow.
- HTML download pages use an OpenClaw Web workflow that reads the page, resolves
  its download link, and downloads the linked file.
- Thunder/ED2K use OpenClaw to submit to the installed Thunder application by
  bundle identity and URL scheme; submission does not imply completion.
- Magnet uses Thunder when available and otherwise routes to aria2; torrent and
  FTP route to the aria2 tool adapter. qBittorrent is not installed or required.
- Baidu links route through OpenClaw to the installed Baidu Netdisk application.
  Future cloud tools extend Download routing, not Task Engine or Planner.
- AI-OS verifies new files in the selected destination before reporting completed.

Next capability order:
4. Browser and Search Skill
5. NAS Management Skill
6. Document, Spreadsheet and Presentation Workflow Skill
7. Email and Calendar Skill
8. Smart Home and Device Control Skill
9. Local Generative Media Skill
10. Cognitive Distillation Foundation

Do not begin P16 until P15 capability foundations are implemented or explicitly deferred.

---

## Rejected — do not propose again

| Decision | Reason |
|---|---|
| Third-party Anthropic OAuth client | Anthropic publishes no public third-party client registration. Use the official `claude` CLI as a local authorization proxy instead. |
| Reading or persisting Claude Code's OAuth token | Claude Code owns its own credential. AI-OS stores only the local connection and model preference. |
| Consumer Meta AI login for third-party inference | Not authorized by Meta. Only the Meta Model API developer key is supported. |
| Top-right `Add AI` button | Duplicated the `Other AI` catalog entry. The catalog entry is the single generic Provider path. |
| Treating every Ollama model-discovery failure as "no models installed" | Misleading. The card now distinguishes not-installed, stopped, starting, running-without-models, and ready. |
| Auto fallback after output has started | Fallback only fires when a candidate fails *before* producing output. Explicit model selection never silently switches. |
| Hard-coding one game into Arena architecture | Werewolf and similar are representative cases, not the architecture. |

---

## Technical decisions

**AI Center**

- Shared Multi-Model Invocation is the canonical execution layer
- Each participant owns an independent `operationId`
- Multi-model execution is fully concurrent; participants never use Auto fallback
- Participant ordering is deterministic after normalization; duplicates removed pre-execution
- Aggregated results preserve per-model P13-M4 metadata
- Auto routing is Local First: connected oMLX models are attempted first, then local Ollama models, then connected cloud defaults
- oMLX is the preferred Apple Silicon local engine; machines without a connected oMLX instance naturally continue through Ollama without a separate platform-specific route
- oMLX uses the independent `omlx-local` Provider instance, Keychain-backed Bearer authentication, `/v1/models` discovery, and OpenAI-compatible Chat endpoints
- AI-OS auto-starts the installed oMLX macOS app only when an `omlx-local` instance is connected and the machine is Apple Silicon; unsupported machines retain Ollama without attempting oMLX startup
- My AI treats connected oMLX as a local service under On this Mac; it is removed from Add another Provider until disconnected
- On Apple Silicon, a connected oMLX instance replaces Ollama in My AI and both OpenClaw execution agents. Ollama remains an optional Add another Provider entry and remains the default local engine on unsupported computers
- OpenClaw reads the oMLX credential through a private 0600 file SecretRef exported from the existing Keychain credential, because the separate OpenClaw process cannot directly read AI-OS's Keychain item
- The canonical AI Center streaming command owns oMLX candidate attempts, chunk emission, cancellation, and fallback decisions; the UI does not call oMLX directly
- OpenClaw execution agents use `omlx/Qwen3.5-9B-4bit` on Apple Silicon; oMLX model management has Details, Show in Finder, Delete, Pull model, and Refresh parity
- Explicit manual model selection never silently falls back
- Fallback occurs only before output has started
- Cancellation stops the current stream without later output continuation

**Memory**

- Ordinary Chat requests load long-term `user` memories and inject them as outbound message zero
- Memory Policy is separate from Memory Storage; `src/services/memoryPolicy.ts` owns parsing, resolution, and outbound constraints while SQLite CRUD remains unchanged
- ChatPage only retrieves Memory, calls the Memory Policy resolver, and assembles outbound context; it does not own concrete Policy rules
- Policy priority is current request override, then long-term Memory, then system default
- Resolved runtime policy enters only the outbound request and is also applied to a temporary clone of the current outbound user message for model compatibility
- UI and saved conversation user content always retain the original text; request overrides are never written into conversation history or long-term Memory and do not affect the next request
- Ordinary factual memories remain available as background context without forced structured parsing
- Explicit “记住…” commands remain a local save-and-confirm path and do not call AI Center
- Initial structured policies are `language`, `response_detail`, `currency`, and `budget`
- Conversation-scoped persistent overrides are not implemented
- Streaming Provider input accepts `system` messages; Anthropic combines them into the Messages API top-level `system` field and excludes them from `messages`


**Office Workflow Skill**

- Office capabilities resolve through a provider-neutral Office Provider Registry.
- Local First applies: macOS `document.read` resolves to the Native provider.
- Native DOC/DOCX reading, creation and conversion use `/usr/bin/textutil` through the existing OpenClaw Gateway execution boundary.
- Unsupported providers, relative paths, and unsupported extensions fail closed.
- `document.read`, `document.create` and `document.convert` require one-time user confirmation.
- `document.create` accepts bounded text input and creates atomically without overwriting an existing target.
- `document.convert` supports DOC ↔ DOCX only; source and destination must be distinct absolute paths, and the destination is never overwritten.
- Spreadsheet Read skips `MacosNative`, which does not claim spreadsheet capabilities, and resolves to the first available Office Provider.
- The current executable Spreadsheet Read and Create adapter is Microsoft Excel for Mac through AppleScript; other resolved providers fail closed until their adapters exist.
- Spreadsheet Read opens an absolute XLS/XLSX path, reads the first worksheet used range as bounded TSV, and closes without saving.
- Spreadsheet Create accepts bounded TSV input and requires an absolute XLS/XLSX target path.
- Because sandboxed Excel cannot save directly to arbitrary temporary paths, Spreadsheet Create generates inside Excel's container cache, then uses a no-overwrite move to the requested target.
- `spreadsheet.read` and `spreadsheet.create` require one-time user confirmation.
- Successful output must come from the real exec tool result; Gateway submission alone is not success.

**Download Skill**

- Download capability enters through Task Engine and Planner.
- Runtime resolves Download Skill through the Skill registry and the general
  Runtime-to-OpenClaw execution contract.
- Technical decision: Download execution belongs to OpenClaw; AI-OS owns
  Task/Plan/permissions/capability routing, not an independent downloader
  execution path.
- The former Runtime → aria2/Thunder executor and independent download-task store
  were removed. aria2 remains the non-interactive magnet/torrent/FTP tool OpenClaw
  can invoke; the GUI-only Thunder launcher was removed.
- Direct file URLs, Web pages, Thunder wrappers, magnet/torrent/FTP, and known
  cloud-drive sources are classified inside Download Skill, never ChatPage.
- A resource name without a URL is delegated to the main OpenClaw Agent, which
  searches for a working download source and must continue through download and
  destination-file verification without content, copyright, filename, or NSFW
  classification.
- Download execution uses the existing `ai-os-files` worker and its proven
  `ollama-ai-os/qwen3:4b-instruct` tool-calling model. The default main
  `qwen3:8b` produced no tokens or tool calls and hit OpenClaw's 180-second idle
  timeout in the first real Baidu E2E. Runtime model override was also rejected
  by OpenClaw policy, so Download never depends on that override.
- Direct and Web downloads must create a new file in the selected existing
  absolute destination before AI-OS returns `completed`.
- Technical decision: a Download result may only be `completed`; opening an
  external application or submitting a URL is never success. Every route must
  create a new file in the requested directory or fail.
- Download execution is content-neutral after the user confirms the PlanStep.
  The Agent performs file transfer and destination verification rather than
  inferring legality from filenames or replacing the user's decision. AI-OS does
  not implement adult-content recognition, copyright judgment, or content
  scanning in P15.
- Standard `thunder://` wrappers are decoded before routing. Magnet, torrent,
  and ED2K prefer an installed non-interactive Thunder Skill, CLI, or local API;
  opening the Thunder GUI is never accepted. If Thunder automation is absent,
  magnet/torrent may fall back to aria2 and ED2K may use another installed cloud
  offline-download Skill; otherwise ED2K fails with the real unsupported-tool
  reason. FTP continues through aria2.
- Cloud providers are not hard-coded in Runtime. OpenClaw first discovers an
  installed Download/cloud Skill that declares support for the supplied source;
  otherwise it falls back to the provider-neutral Browser flow. Provider login,
  OAuth, VIP sessions, and extraction codes belong to the installed Skill or
  persistent browser profile. Adding 115, Google Drive, or another provider must
  not require changes to Task, Planner, Runtime, or Download routing.
- The currently installed Baidu `baidu-drive` Skill and `bdpan` CLI remain the
  proven example of this contract. OAuth is completed once; normal downloads
  require no GUI interaction, and Runtime still requires a destination file.
- Web and cloud-share downloads use one provider-neutral OpenClaw Browser flow,
  not one adapter per website. It reuses an existing authenticated browser
  session, selects an authorized VIP/fast option when available, otherwise uses
  the free option, and handles JavaScript waits, buttons, hidden forms, and
  redirects without user clicks. Missing login is reported as authentication
  required; credentials are never requested or stored in Chat. Long transfers
  may run for up to four hours; only a completed destination file is success.
- Real official-CLI E2E on 2026-08-22 automatically resolved a public share,
  applied its extraction code, transferred it into the authorized app scope, and
  downloaded `EhViewer-2.0.2.2.apk` (27,747,451 bytes) to a temporary local
  destination without GUI interaction.
- Web E2E currently proves a normal HTML download link. JavaScript-heavy,
  authenticated, CAPTCHA, and interactive sites still depend on available
  OpenClaw Browser/cloud tools and remain unverified.
- Real E2E on 2026-08-21 proved Direct HTTP and HTML-link downloads through
  Task → Plan → Runtime → OpenClaw with byte-matching destination files. The
  earlier GUI-launch acceptance for Thunder/Baidu was invalidated and removed.
- `verify/verify_p15_download_complete.sh` also proves failure classification,
  readable Chat results, full Rust regression tests, frontend build, and cargo check.

**P15 Core Skills**

- Explicit Core Skill actions enter through Task Engine and Planner, resolve through the existing Skill registry, and execute through Runtime and OpenClaw
- One-time user confirmation is PlanStep-scoped and permits only the exact implemented `filesystem.scan`, `filesystem.read`, `filesystem.write`, `filesystem.move`, and `download.start` actions; it does not modify trusted automation
- Work/Core Skill errors use safe OpenClaw/Runtime reporting, while ASK errors retain AI Center/provider semantics
- `filesystem.scan` is an AI-OS capability identifier, not an OpenClaw Gateway RPC or tool id; the adapter translates it to the official `agent` / `agent.wait` path and reads the resulting session history
- Real `filesystem.scan` E2E passed through Task, Planner, Runtime, permission, OpenClaw agent, read-only `exec`, normalized output, and Chat UI on 2026-08-15
- `filesystem.read` always requires one-time confirmation, uses the same dedicated worker, detects MIME and size before reading, never sends unsupported binary content to Chat, and bounds text reads to 1 MB with at most 64 KiB returned
- `filesystem.write` always requires one-time confirmation, passes selected path and current text through the existing Plan/Runtime chain, limits UTF-8 content to 4 KiB, creates with 0600 permissions, and uses shell noclobber plus an existence check so overwrite attempts fail closed
- Real `filesystem.write` E2E passed through Task, Planner, Runtime, permission, OpenClaw agent, `exec`, normalized output, and Chat UI on 2026-08-15; the UI-created file contained the exact 27-byte requested text
- `filesystem.move` always requires one-time confirmation, requires absolute and different source/destination paths, forbids overwrite, verifies the source disappears and destination appears after `mv`, and reports source-missing, destination-existing, or execution failure without claiming success
- Real `filesystem.move` E2E passed through Task, Planner, Runtime, permission, OpenClaw agent, `exec`, normalized output, and Chat UI on 2026-08-15
- File execution uses the dedicated `ai-os-files` OpenClaw worker with no inherited skills or workspace bootstrap context and an isolated Ollama provider; `main` remains the default personal agent
- OpenClaw permission and confirmation remain authoritative; the dedicated worker does not enable trusted automation or bypass tool policy

**Migration**

- Legacy MultiLLM service has been removed
- Provider Registry is the single source of provider identity
- Council and My AI consume AI Center provider models
- No new feature should introduce MultiLLM-specific storage keys or execution paths

**Removing legacy code**

- CSS class names have no compile-time link to TypeScript, so deleting a
  feature can silently remove styles the surviving UI still uses; the build
  will still pass
- When removing a module, grep `App.css` for its class names before and after
  and confirm every affected screen renders

**Credentials**

- All API keys and OAuth tokens pass to the native security layer and live in macOS Keychain
- A newly stored credential is removed automatically if live verification fails
- Claude Code subscription execution stays routed separately from Anthropic API-key execution

**Observability**

- AI Center owns the canonical invocation record; Workspace renders it but must
  not reconstruct routing decisions
- Records include route mode, execution source, latency, ordered attempts, and fallback state
- Records exclude prompts, outputs, raw Provider responses, credentials, and tokens

---

## Decided — 2026-08-02

**AI Center routing moves to the Rust backend.**

Provider invocation, Auto ordering, fallback selection, and credential use
belong in Rust. The frontend submits a request and renders the result. It must
not select providers, order candidates, decide fallback, or hold credentials in
memory.

Rationale: background and scheduled execution cannot depend on an open
application window; credentials must stay in the native security layer;
Runtime, Memory (P14), AI Council (P16), and AI Arena (P17) all need backend
access to AI Center.

Migration is incremental, not a rewrite. Each step ships its own acceptance
script and must not change observable behaviour:

This is architectural correction, not a P13 milestone. P13 is defined by the
Master Guide as M1-M5 and is complete. These steps are tracked as AC-BACKEND,
outside the phase numbering.

Provider HTTP invocation already moved to Rust as part of the legacy MultiLLM
removal (AC-BACKEND-0).

1. **AC-BACKEND-1 — Completed.** Rust owns Auto ordering, Local First
   preference, and fallback selection. The frontend sends an Auto-or-manual
   request and renders the selected result. Observability remains in the
   frontend until AC-BACKEND-2.
2. **AC-BACKEND-2 — Completed.** Rust supplies canonical route mode, source,
   ordered attempts, outcomes, safe error categories, latency, token estimates,
   and fallback state. The frontend is a call-and-render layer that enriches
   pricing from the existing `modelPricing` configuration and persists
   Analytics to localStorage. Invocation records exclude prompt, output,
   credential, API key, OAuth token, access token, and refresh token contents.
   Multi-model concurrency, explicit choices, deterministic ordering, and
   per-participant operation IDs remain unchanged.

Constraints carried into the migration:

- Local First ordering, per-participant `operationId`, concurrent multi-model
  execution, and no-fallback-for-multi-model all survive unchanged
- Claude Code subscription execution stays routed separately from Anthropic
  API-key execution
- Fallback still fires only before output has started
- Invocation records still exclude prompts, outputs, credentials, and tokens

---

## Environment notes

- Repository is a Tauri app: Rust backend in `src-tauri/`, React/TypeScript frontend in `src/`
- Latest full validation: 394 Rust tests passing, `cargo check` passing with 4 pre-existing warnings, frontend production build passing
- Owner's machine is macOS; do not hard-code absolute user paths in this file

---

## Change log

<!-- ./done.sh appends here automatically -->

## 变更日志
- 2026-08-09 04:09  fix(myai): restore provider setup dialog styles lost in P13 migration
- 2026-08-09 05:23  fix(ui): restore chat, sidebar and markdown styles lost in P13 migration


## External Skill Architecture

### Agency Agents Skill Reference

Repository:
https://github.com/msitarzewski/agency-agents

Purpose:
- Use external Agent Skill packages to provide professional role definitions for AI Council.
- AI-OS should not maintain a duplicate internal agent talent library.
- Agent roles, expertise descriptions and workflow templates should come from installable Skills.
- Preserve Agency Agents as a candidate source and design reference for the P16
  AI Council expert Profile/Role Library and dynamic expert-team assembly.
- Do not integrate Agency Agents during P15 unless the Master Guide roadmap is
  explicitly revised.

Architecture direction:

AI-OS owns:
- Skill discovery
- Skill loading
- Role selection
- Council orchestration
- Memory tracking
- Model assignment

Skills own:
- Agent role definitions
- Professional personas
- Expertise descriptions
- Workflow instructions
- Output standards

Initial reference Skill:
- Agency Agents

Important boundary:
- Agency Agents is a Skill resource, not a Runtime.
- Before P16 implementation begins, specify the full Council-to-execution
  contract: Council recommendation → Task Engine → Planner → user confirmation
  → Runtime → OpenClaw.
- AI-OS v1.0 execution remains OpenClaw-only.
- External Agent Skills provide Council roles only and do not introduce additional execution adapters.

Future Skill model:

AI-OS
 |
 Skill Runtime
 |
 Installed Skills
 |
 Agent Role Providers
 |
 AI Council
 |
 OpenClaw Execution

## External Orchestration Architecture Reference

### Paperclip Reference

Repository:
https://github.com/paperclipai/paperclip

Local reference checkout:
- `../paperclip`
- Reference commit when recorded: `f0e6c0f54`

Purpose:
- Preserve Paperclip as an external architecture and implementation reference
  for AI-OS task orchestration and execution governance.
- Study its goal hierarchy, task lifecycle, delegation, approvals, budget
  controls, heartbeat scheduling, audit trail, persistent execution state, and
  runtime adapter boundaries before P16 implementation begins.
- Use Paperclip to inform the unresolved Council-to-execution contract rather
  than introducing a second AI-OS orchestration system.
- Do not integrate or run Paperclip as part of P15 Core Skills.
- Do not make Paperclip an AI-OS Runtime or execution Agent.

Architecture boundary:

AI-OS owns:
- AI Council
- Task Engine
- Planner
- User confirmation and approvals
- Runtime policy
- Memory
- AI Center
- OpenClaw execution integration

Paperclip is reference material for:
- Goal-to-task decomposition patterns
- Task ownership and delegation
- Approval and governance flows
- Budget and execution limits
- Heartbeat / resumable work patterns
- Persistent task state
- Audit and observability patterns
- Runtime adapter separation

Important boundary:
- Paperclip is an orchestration reference, not a dependency decision.
- AI-OS must not duplicate Paperclip wholesale or introduce its runtime model
  without an explicit architecture decision.
- AI-OS v1.0 execution remains OpenClaw-only.
- Paperclip support for other agent runtimes does not expand the AI-OS v1.0
  Agent scope.
- Before P16 coding begins, compare the Paperclip reference against the required
  AI-OS contract:
  Council recommendation → Task Engine → Planner → user confirmation
  → Runtime → OpenClaw.
- Adopt only mechanisms that fit the AI-OS architecture and Master Guide.

### Local external reference repositories

These repositories are intentionally kept outside the `dashboard` repository
and must not be committed into the AI-OS application source tree:

- `../agency-agents` — Agency Agents role/Profile reference, commit `ebe9c99`
- `../paperclip` — orchestration architecture reference, commit `f0e6c0f54`

They are source references only. P15 must not depend on either repository at
runtime.
- 2026-08-13 22:49  docs: record external agent architecture references
- 2026-08-14 01:29  feat: add p15 core skill execution contract
- 2026-08-14 01:53  feat: add explicit folder scan work action


## P15 Browser/Search

Status:
- Completed.

Implemented:
- Browser Skill contract (`browser.search`, `browser.control`).
- MCP runtime execution path.
- Browser Provider abstraction layer.
- Browser Provider Registry.
- Browser Runtime Dispatcher.
- MCP Browser Provider bridge.

Architecture decision:
- Browser capability does not directly depend on a specific browser tool.
- MCP Browser is the first provider implementation.
- Future browser integrations (Chrome DevTools MCP, BrowserSkill, etc.) must be added as providers without changing Browser Skill contracts.

Execution path:

Planner
→ Skill Resolver
→ Browser Skill
→ Browser Runtime Dispatcher
→ Browser Provider Registry
→ MCP Browser Provider
→ MCP Runtime
→ tools/call

Verification:
- verify_p15_browser_complete.sh


---

## Future roadmap additions

### P15 additions

**Local Generative Media Skill**

- Local-first image/video generation capability
- Draw Things and ComfyUI style local provider support
- Model and LoRA orchestration
- Prompt generation and iterative refinement
- Cloud generation only when local capability is unavailable

**Cognitive Distillation Foundation**

- Evidence-based cognitive model extraction
- Decision pattern modeling
- Reasoning framework representation
- Foundation for future Strategic Intelligence


### P16 direction

**Strategic Intelligence and Agent Expansion**

Planned capabilities:

- Cognitive Simulation Engine
- Strategic Council enhancement
- Second-order prediction
- Scenario simulation
- Linco Bridge style Agent Connectivity Layer
- Multi-device Agent access


## Additional technical decisions

- Cognitive Distillation is not personality cloning.
- Cognitive Models must expose evidence and confidence.
- Local Generative Media follows Local First routing.
- Agent Connectivity must not become a Runtime dependency.

## External Connectivity Architecture Reference

### Linco Bridge Reference

Repository:
https://github.com/lincotalk/linco-bridge

Reference commit:
27688263dc33d07a5a06b7613329705552373dc1

Purpose:
- Agent Connectivity architecture reference
- Remote client access pattern
- Agent session continuity
- Event streaming design
- OpenClaw and Hermes connector architecture reference

Boundary:
- Linco Bridge is reference architecture only during P15
- AI-OS Runtime must not depend on Linco Bridge
- Candidate input for P16 Agent Connectivity Layer

Future P16 consideration:

AI-OS Agent Connectivity Layer may provide:
- multi-device Agent access
- remote sessions
- channel adapters
- event synchronization
- secure external client communication

---

## AC-EXEC-MODEL — Agent execution model ownership

**Decided 2026-08-22. Not yet implemented.**

### What was wrong

`ai-os-files` is pinned to `ollama-ai-os/qwen3:4b-instruct`. AI-OS passes only
`agentId` to the gateway, so OpenClaw decides which model executes. AI Center's
Auto routing, Local First, and fallback cover conversation but not execution.

The 4B model reads a SKILL.md and then stops without running the commands it
just read. Verified 2026-08-22: the same Baidu Drive download failed repeatedly
under 4B (5 reads, 2 unrelated execs) and succeeded under `ollama/qwen3:8b`
(8 execs: `bdpan transfer` -> `transfer list` -> `transfer select` -> `download`,
10.3 MB file landed on disk).

Nothing else in the chain was broken. AI-OS, gateway, agent, skill, `bdpan`,
Baidu login, destination directory, and the Skill-first prompt were all correct.

### Why model override does not work

The gateway rejects a per-run model override from this caller:
`provider/model overrides are not authorized for this caller`. The CLI accepts
`--model`; the gateway `agent` method does not. This is OpenClaw's restriction,
not ours.

### Decision: select at agent granularity

`openclaw agents add --model <id>` exists, so AI-OS creates several execution
agents with fixed models, and AI Center chooses between agents rather than
models.

This is better than a model override anyway. An execution agent binds model,
context window, skill allowlist, tool permissions, and workspace together.
Swapping only the model breaks that pairing — verified: `qwen3:8b` is smarter
but has a 32K window, while `qwen3:4b-instruct` has 64K, and a long SKILL.md
plus history overflows 32K. Capability alone is not a sufficient selection
criterion.

Proposed agents (names not final):
- light — 4B, 64K window, read-only file tools
- standard — 8B, shell and browser, download skills
- heavy — cloud model, long-document and multi-step work

### Implementation order

1. Create the execution agents with fixed models
2. AI Center selects the agent; the adapter passes the chosen `agentId`
3. On Runtime file-verification failure, retry down the candidate list

Selection must consider context window, not just capability.

### Also found

- The model reported success while its `exec` had failed. Runtime file
  verification caught it. Never trust an agent's self-report.
- Frontend error mapping in `src/services/tasks.ts` matches on substrings, so
  `destination is unavailable` renders as "OpenClaw is unavailable". This false
  lead cost hours. Rust already returns typed error kinds
  (`AuthenticationRequired`, `PairingRequired`, `ConnectionUnavailable`,
  `ProtocolFailure`, `ExecutionFailed`) — pass the kind through and switch on it
  instead of guessing from text.
- Diagnosis method that worked: read the agent trajectory and count which tools
  it actually called:
  `grep -o '"name":"[a-z_]*"' <session>.trajectory.jsonl | sort | uniq -c`
  Do this before theorising about any agent execution failure.

### Cloud-drive download scope

Cloud drives stay in v1.0 as one download route among HTTP, FTP, BT, ED2K,
magnet, and Thunder, which are already implemented. Chinese-internet resources
are often only distributed through drives, so this is a real gap, not an
optional extra.

Do not evaluate new drive tooling until AC-EXEC-MODEL lands — any Skill-based
tool will fail the same way under a 4B model. Candidates found on ClawHub for
later evaluation: `pansou` (search across 12 drive services), `baidupcs-go`
(mature CLI, supports offline download), `baidu-netdisk-storage`. Note that
`baidu-drive` confines operations to `/apps/bdpan/`, which does not suit
downloading arbitrary shared links.

Drive services are private APIs, not open protocols. Any tool is a reverse
engineering effort, needs the user's credentials, and can break when the
provider changes. Expect maintenance; do not treat a working tool as permanent.

---

## P15 Download Skill — handoff 2026-08-22

### Current status

- P15 Download Skill must NOT currently be treated as fully completed.
- The 2026-08-21 Direct HTTP and ordinary HTML-link OpenClaw E2E results remain valid historical verification.
- Provider-neutral Skill-first Web/cloud-share routing is now implemented and covered by focused Rust and deterministic verification.
- A real authenticated cloud-share download remains the final manual E2E gate before Downloads can be marked fully complete.

### Current implementation

The Download capability already has substantial implementation in the current working tree:

- `download.start` is registered as a Core Skill capability.
- Task → Plan → Runtime → OpenClaw execution exists.
- `download.start` requires one-time user confirmation.
- Direct HTTP routing exists.
- Web/cloud-share routing exists.
- Search routing exists.
- aria2 routing exists.
- P2P / Thunder-preferred routing exists.
- Runtime verifies that a real file appears in the selected destination before reporting success.
- Download verification scans nested directories and returns relative file paths because cloud-drive tools may create a bundle directory under the selected destination.
- Download verification rejects zero-byte artifacts and HTML login/extraction pages; third-party Skills are discovered from `$HOME/.agents/skills` before generic curl fallback.
- The download dialog preserves the user's complete item description as `selectionHint`; for multi-item shares OpenClaw must list items and download only the filename, type, or approximate size requested instead of downloading the whole share.
- After a matching download Skill is read, its documented share-link command must be the next tool call; browser Skills and curl are forbidden, and supported isolated target-folder options use a unique execution-scoped name.
- Real OpenClaw E2E test entry exists in `src-tauri/src/task_execution.rs`:
  `real_download_runs_through_task_plan_runtime_and_openclaw`.
- The real E2E test is intentionally ignored unless the OpenClaw gateway and P15 download fixture are available.

### Skill-first Web/cloud-share decision

The current Web route in:

`src-tauri/src/runtime/openclaw_gateway_adapter.rs`

function:

`execute_download_start()`

now uses this provider-neutral Skill-first contract:

1. Inspect the installed OpenClaw Skills already available in OpenClaw context.
2. If a download/cloud-drive Skill declares support for the source, use that Skill first.
3. A Skill is an instruction document, not a callable tool.
4. Read the matching `SKILL.md` using OpenClaw's existing read capability.
5. Continue immediately after reading it and execute its instructions using existing tools such as `exec`.
6. Reading `SKILL.md` alone is NOT successful completion.
7. Preserve the source URL exactly when passing it to CLI commands; do not rewrite it as Markdown link syntax.
8. Continue until AI-OS verifies that a real downloaded file exists in the selected destination.
9. Only when no installed Skill supports the source may OpenClaw fall back to generic curl/browser handling.
10. AI-OS Runtime must remain provider-neutral and must not select a cloud provider by hard-coded domain.
11. Do not repeat the same failed command or browser wait more than once; return the real Skill error instead of entering an unrelated fallback loop.

### Implemented Web prompt behavior

The current prompt explicitly requires:

- Skill-first selection.
- `SKILL.md` read followed by actual execution.
- No attempt to call the Skill name as though it were an OpenClaw tool.
- Exact preservation and shell quoting of the source URL.
- Generic curl/browser only as fallback.
- A direct share-link command documented by a matching Skill bypasses curl and browser automation.
- The same failed command or browser wait is not repeated indefinitely.
- No success after merely reading a Skill, opening a page, resolving a URL, or announcing a next action.
- Success only after a complete file exists in the destination.

### Next developer action

Continue from:

`src-tauri/src/runtime/openclaw_gateway_adapter.rs`

→ `execute_download_start()`

→ `DownloadExecutionRoute::Web`

Focused Rust tests, `verify/verify_p15_download_complete.sh`, and
`verify/verify_p15_download_exec_agent.sh` pass. Next:

1. run the real OpenClaw download E2E with an authenticated cloud-share fixture;
2. verify that OpenClaw reads the matching Skill instructions and executes its documented command;
3. verify that the source URL is passed unchanged to the provider CLI/tool;
4. verify that AI-OS reports success only after the destination contains the downloaded file.

Do not mark P15 Downloads complete solely because the existing Direct HTTP or ordinary HTML-link tests pass.

### Architecture boundary

Keep the existing architecture:

User request
→ Task Engine
→ Plan
→ user confirmation
→ Runtime
→ OpenClaw
→ installed Skill / existing OpenClaw tools
→ Runtime file verification

AI-OS Runtime owns Task/Plan/permission/capability routing and completion verification.

OpenClaw owns execution.

Installed provider Skills describe provider-specific execution.

Do not move provider-specific cloud-drive logic into the AI-OS Runtime and do not hard-code provider selection by domain.
- 2026-08-23 09:35  feat(p15): complete download skill with dedicated 8B execution agent
- 2026-08-23 09:59  fix(errors): classify runtime failures by kind instead of matching prose
- 2026-08-23 12:10  complete AC execution model
- 2026-08-23 15:54  完成 oMLX 自动启动与 My AI 本地服务卡片
- 2026-08-23 17:16  完成 oMLX 全面替代 Ollama及本地模型管理对齐
- 2026-08-23 20:33  fix(p15): adapt download execution to oMLX Qwen3.5-9B
- 2026-08-26 23:23  完成 P15-1 Email Calendar 与 P15-2 Document Read
- 2026-08-27 00:07  完成 P15-2 Document Create
- 2026-08-27 20:46  完成 P15-2 Document Convert
- 2026-08-27 21:52  完成 P15-3 Spreadsheet Read
- 2026-08-28 00:44  完成 P15-3 Spreadsheet Create 并修复 AC-EXEC 验收

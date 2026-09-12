# AI-OS — Current State

<!-- COMPUTER_CONTROL_STATE_START -->

## CURRENT COMPUTER CONTROL STATE — 2026-09-06

> Authoritative for Computer Control. Where it is, what is stopping it, and what
> comes next.

### Where it is

- P15 is **7 / 10**. Office and Computer Control v1 are Complete; Vehicle Control is Deferred / out of v1.
- Computer Control **Phase A is committed** (`80d1d01`):
  `system.storage`, `system.cpu`, `system.memory`, `system.network`,
  `system.process.list`, `system.process.info`.
- Computer Control **Phase B is complete and committed** (`4ca692f`):
  `system.app.list`, `system.app.running`, `system.app.launch`,
  `system.app.quit`.
- Computer Control **Phase C is complete**. C1 clipboard, C2 audio, and C3 power are complete.
- Computer Control **Phase D is complete for v1**. Native notification delivery is Deferred / out of v1; D2 permission inspection is complete.
- Computer Control **Phase E is complete**. E1 confirmed process termination and E2 final cross-capability workflow are complete.
- **Computer Control v1 is Complete.**
- Current stable baseline: `f010543` (`feat: complete zero-admin Computer Control v1`).

### Phase B accepted state

Phase B passed both the Computer Control phase gate and the Office phase gate
before commit.

Application ownership remains intentionally narrow:

- listing installed/running applications belongs to Computer Control
- launching or requesting quit of an application belongs to Computer Control
- interacting with controls inside an application belongs to Computer Use
- multi-step judgement and orchestration belongs to OpenClaw / Planner

Application addressing does not infer identity from display-name shape.
The adapter only validates that an identifier is safe to hand to the platform;
the operating system determines whether an installed application answers to it.

Launch and quit results are observation-based rather than trusting command
success alone. User work is preserved: an application that refuses to quit
because it has unsaved work remains running rather than being force-closed.

### Phase C accepted state

C1 clipboard is complete:

- `system.clipboard.read`
- `system.clipboard.write`
- plain UTF-8 text only
- bounded to 65,536 bytes
- macOS uses `pbpaste` / `pbcopy`
- write data is passed through stdin, never through a shell
- no screen reading, keystrokes, GUI automation or application control
- both operations remain behind one-time user confirmation
- non-macOS builds fail closed until a platform adapter exists

Clipboard ownership remains narrow: Computer Control can read or replace the
clipboard value. Deciding where to paste it or interacting with a destination
application remains Computer Use / Planner work.

C2 audio is complete:

- `system.audio.volume.get`
- `system.audio.volume.set`
- `system.audio.mute.get`
- `system.audio.mute.set`
- output volume is an integer from 0 through 100
- mute state is an explicit boolean
- macOS uses deterministic Standard Additions through `/usr/bin/osascript`
- no shell, screen driving, keystrokes, media-player control, recording or
  speech interpretation
- real E2E changes system state, observes the change and restores the person's
  exact original output volume and mute state
- all four operations remain behind one-time user confirmation
- non-macOS builds fail closed until a platform adapter exists

Audio ownership remains narrow: Computer Control can read or directly set
system output volume and mute state. Media selection/control, recording,
speech interpretation and deciding what state is appropriate remain outside
Computer Control.

C3 power is complete:

- `system.power.sleep`
- `system.power.restart`
- `system.power.shutdown`
- all three require confirmation on the current execution request
- Trusted Automation cannot bypass Power confirmation
- unconfirmed Power returns `PermissionRequired`
- macOS uses normal System Events AppleScript power requests
- no privilege escalation, forced power path, process kill, GUI click, or
  keyboard simulation is used
- Power accepts no force, delay, or other behavior parameters
- restart/shutdown are deliberately not real-E2E tested because successful
  execution would terminate the test runner and may interrupt user work
- acceptance compile-checks the exact AppleScript without executing it
- System capability discovery is now separated from execution, so generic
  routing tests cannot accidentally invoke destructive capabilities

Power ownership remains narrow: Computer Control may request a normal OS sleep,
restart, or shutdown only after current user confirmation. If macOS refuses,
AI-OS reports the refusal and does not escalate to a force path.

D1 native notifications are complete:

- `system.notification.send`
- macOS uses UserNotifications.framework directly in the AI-OS process
- title is required and bounded to 128 characters
- body is optional and bounded to 4,096 characters
- unknown input fields are rejected
- the native authorization state is read before submission
- a first explicit send may invoke Apple's own notification authorization
  prompt only while the state is `notDetermined`
- denied or unknown authorization fails closed
- authorized, provisional, and ephemeral states may submit notifications
- no AppleScript, shell, keyboard event, Notification Center GUI automation,
  or alternate application-identity shim is used
- D1 is confirmable through the normal Runtime permission gate; unlike Power,
  it may intentionally be enabled in Trusted Automation
- automated acceptance does not post a real notification from a Rust test
  binary because that would validate the test binary's identity rather than
  the AI-OS application identity; real app identity is validated in the final
  Computer Control workflow

Notification ownership remains narrow: Computer Control can submit one native
AI-OS notification. Deciding when notification is useful remains Planner work;
reminders and calendar events remain their own domain capabilities.

D2 permissions are complete:

- `system.permission.list`
- `system.permission.open_settings`
- permission state is read through native macOS framework APIs in-process
- Notifications preserve Apple's not-determined, denied, authorized,
  provisional, ephemeral, and unknown distinctions
- Camera and Microphone preserve AVFoundation not-determined, restricted,
  denied, authorized, and unknown distinctions
- Accessibility and Screen Recording use Apple's boolean preflight APIs; when
  false AI-OS reports `notGranted` rather than inventing whether the state is
  denied or never requested
- Automation is reported as `unavailable` as a global state because macOS
  authorization is target-application-specific
- Full Disk Access is reported as `unavailable` because macOS exposes no public
  global authorization-status API
- `system.permission.open_settings` uses native NSWorkspace and a fixed
  allowlist of System Settings destinations
- settings handoff is supported for Notifications, Accessibility, Screen
  Recording, Camera, Microphone, Automation, Calendar, and Full Disk Access
- D2 never calls permission-request APIs, never modifies TCC, never edits a TCC
  database, and never clicks an authorization UI
- no shell, AppleScript, System Events, or Computer Use is used
- the existing `macos_permissions.rs` Mail/Calendar command remains a legacy
  domain-specific compatibility surface and is not the Computer Control D2
  permission authority

Permission ownership remains narrow: Computer Control can observe public
authorization state and take the person to the correct settings pane. The
person grants or revokes permission; AI-OS does not make that decision or
operate the System Settings UI.

### Phase E accepted state

E1 confirmed process termination is complete:

- `system.process.terminate`
- request requires exactly `pid` and `expectedStartTimeUnixSeconds`
- the expected start time comes from the existing detailed
  `system.process.info` result
- a PID alone is never treated as process identity because operating systems
  reuse PIDs
- immediately before signalling, AI-OS refreshes the PID and requires its
  current `start_time()` to equal the previously observed value
- a changed identity fails closed before any signal is sent
- PID 0, PID 1, and the AI-OS process executing the request are refused
- no caller-selectable signal, force flag, or delay exists
- v1 sends only `sysinfo::Signal::Term`; there is no `Signal::Kill`, SIGKILL,
  shell command, `pkill`, `killall`, or force escalation
- successful signal delivery alone is not completion; AI-OS observes that the
  original process became absent, reached a terminal state, or that the PID was
  reused by a different process identity
- a process that remains active after the bounded observation window reports
  failure and is not force-killed
- process termination is in `ALWAYS_CONFIRM_CAPABILITIES`: current
  `user_confirmed` is mandatory and Trusted Automation cannot bypass it
- the real E2E spawns its own disposable `/bin/sleep` child, proves a stale
  start-time identity does not signal it, terminates it with the exact identity,
  observes termination, and reaps only that test-owned child

Process-control ownership remains narrow: Computer Control may terminate one
explicitly identified process after current user confirmation. Selecting which
process should be terminated, deciding whether termination is appropriate, and
multi-step recovery remain Planner / OpenClaw work.

### Computer Control v1 scope correction — native notification deferred

This section supersedes the earlier D1 completion record for current v1 scope.

`system.notification.send` is removed from Computer Control v1. Its earlier
implementation remains historical engineering evidence, but the capability is
no longer declared, routed, authorisable or part of the v1 acceptance gate.

Real acceptance exposed platform-side developer administration outside the
owner's accepted product boundary. The correct product decision is to defer
the capability rather than require the owner to administer that platform
infrastructure or pay for access.

D2 permission inspection remains in v1.

### Phase E2 final workflow accepted state

Phase E2 adds no new capability.

The final workflow crosses the real permission-enforcing Runtime and Gateway
for:

- `system.storage`
- `system.app.running`
- `system.audio.volume.get`
- `system.process.info`
- `system.process.terminate`

It proves unconfirmed `system.power.shutdown` returns `PermissionRequired`
before destructive execution.

The only process modified by E2 is a disposable `/bin/sleep` child created by
the verifier. AI-OS reads its PID and `startedAtUnixSeconds`, proves an
unconfirmed termination leaves it alive, then confirms the exact same process
identity, observes normal termination and reaps only that verifier-owned child.

E2 does not change clipboard or audio state, open System Settings, execute
sleep/restart/shutdown or terminate user-owned processes.

Computer Control v1 is Complete with this scope:

- Phase A — machine/process reads
- Phase B — applications
- Phase C — clipboard, output audio and always-confirmed Power
- Phase D — native permission inspection and settings handoff
- Phase E — always-confirmed process termination and final workflow
- Native notification delivery — Deferred / out of v1

### Capability admission rule — zero cost / zero owner-admin

Standing rule for P15 and later capability development:

Every new capability must pass a feasibility gate BEFORE implementation.

A v1 path is not admitted when developing or normally using it requires the
owner to:

- purchase developer membership, API access, API credits, an extra
  subscription or dedicated paid infrastructure
- manually manage signing certificates, private keys, provisioning profiles or
  application-signing identities
- manually modify Keychain, TCC/security databases or perform specialist
  operating-system administration
- maintain developer-only infrastructure an ordinary AI-OS user would not
  reasonably possess

If such a requirement is discovered later, stop and defer that path instead of
asking the owner to solve the platform administration.

Normal operating-system APIs remain acceptable when AI-OS can use them without
imposing that administration on the owner.

### Vehicle Control feasibility gate — Deferred / out of v1

Vehicle Control v1 was evaluated before adapter implementation, as required by
the zero-cost / zero-owner-admin capability admission rule.

Decision: **Deferred / out of P15 v1.**

The evaluated Tesla paths did not justify a v1 implementation:

- Tesla Fleet API is not an acceptable v1 foundation because the official
  remote path introduces paid / billing-dependent API infrastructure and
  developer-side setup that violates the zero-cost / zero-owner-admin rule.
- Tesla's official local BLE vehicle-command path can provide a subset of
  deterministic nearby controls without the Fleet API.
- That BLE-only subset does not provide enough incremental product value over
  the existing Tesla app to justify a full AI-OS Vehicle provider, key
  lifecycle, permission model, Runtime integration and acceptance surface.
- Local-only lock/unlock, climate and charging controls are not sufficient on
  their own to justify maintaining a separate Vehicle Control skill.
- Remote orchestration, destination delivery and broader vehicle workflows are
  the capabilities that could make Vehicle Control materially valuable to
  AI-OS; the currently acceptable zero-cost official path does not provide
  enough of that product surface.

No Vehicle Control adapter was implemented, so this decision creates no
Vehicle implementation debt and requires no code rollback.

Vehicle Control may be reconsidered later if an official path provides enough
incremental AI-OS value without violating the standing capability-admission
rule. Examples include:

- a genuinely free official remote-control interface suitable for local AI-OS
  use
- an official Tesla app automation or handoff surface that supports useful
  Planner workflows
- an already-existing AI-OS infrastructure layer that makes vehicle
  integration possible without adding a vehicle-specific paid dependency
- a materially useful cross-skill workflow involving Calendar, charging,
  navigation, climate or other vehicle state that cannot be reasonably
  achieved through the Tesla app alone

The safety boundary remains permanent if Vehicle Control is revisited:

- official interfaces only
- no reverse-engineered/private Tesla APIs
- no driving or vehicle-motion control
- no FSD control

Removing Vehicle Control from P15 v1 reduces the active P15 scope from eleven
capability areas to ten. Current completion is therefore **7 / 10**.

### Generative Media v1 product contract — GM-0

Generative Media is an **AI-OS Generative Media Orchestrator**, not a
ComfyUI controller and not a single-provider image-generation feature.

Its job is to turn user intent and references into generated media while
hiding implementation complexity such as models, LoRAs, workflow graphs,
samplers, nodes, provider-specific prompt syntax and local runtime setup.

The standing product principles are:

- Local First, but not Local Only.
- Local availability is based on **Ready**, not merely Installed.
- ComfyUI is the default local execution engine.
- Cloud providers are interchangeable through a Provider Registry.
- No cloud provider or model is hardcoded as the permanent backend.
- Prompt engineering is an implementation detail, not a user prerequisite.
- Reference / reverse-prompt analysis should reuse strong OSS components rather
  than reimplementing mature visual-analysis systems.
- AI-OS manages local models, LoRAs, nodes and workflows for ordinary users.
- Cloud generation uses the user's own provider account / credential; AI-OS
  does not resell generation credits or provide a unified paid quota.
- Local automatic refinement is controlled by user-selected retry limits.
- Cloud automatic refinement is controlled by a hard user-defined budget.
- Provider policy rejection must not trigger an automatic provider switch for
  the purpose of bypassing that provider's policy.
- Every provider, model, workflow and dependency remains replaceable.

#### Platform scope — macOS v1, cross-platform-safe architecture

AI-OS v1.0 supports **macOS only**.

Windows, Linux and HarmonyOS are future target platforms. v1.0 must not
implement, validate or claim production support for those platforms.

The architecture must nevertheless remain safe for future cross-platform
implementation.

The standing platform rule is:

**Mac-first implementation, cross-platform-safe architecture.**

Shared Generative Media core contracts must not encode macOS-specific
assumptions that would force a later rewrite for Windows, Linux or HarmonyOS.

Platform-neutral core boundaries include:

- provider contracts
- media capability contracts
- `CreativeIntent`
- `ReferenceSpec`
- Generation Profile metadata
- asset identity and provenance
- workflow identity
- provider health
- local-engine state
- normalized output/result contracts
- retry / budget policy
- cancellation semantics

Platform-specific details belong behind platform adapters, including:

- local application discovery
- application installation
- local engine launch / stop
- filesystem paths
- executable locations
- OS package format
- GPU / acceleration detection
- environment preparation
- platform process behavior
- local security / permission handling

The platform model must reserve at least:

- macOS
- Windows
- Linux
- HarmonyOS

Only macOS receives a real v1 implementation.

Windows, Linux and HarmonyOS may have reserved enum values, interfaces,
capability declarations or explicit unsupported states, but v1 must not contain
fake implementations that claim those platforms work.

**Architecture-ready is not Product-supported.**

ComfyUI execution over its local API should remain as platform-independent as
practical. Platform-specific discovery, installation, startup and filesystem
handling must not leak into the shared ComfyUI Provider contract.

Hardware profiling must also remain platform-neutral.

A common `HardwareProfile` should be able to represent fields such as:

- operating system
- CPU architecture
- system memory
- GPU vendor
- GPU model
- GPU memory
- unified-memory availability
- acceleration backend
- supported numeric precision
- available disk space

The macOS v1 adapter may currently map this to Apple Silicon, Metal / MPS and
Unified Memory.

Future Windows and Linux adapters may map the same core fields to CUDA, ROCm,
DirectML or other backends as appropriate.

HarmonyOS implementation details are intentionally not assumed in v1. Only the
platform boundary is reserved; actual runtime, packaging, hardware acceleration
and local-model strategy must be determined when HarmonyOS work begins.

Core asset and output contracts must not expose Unix-only absolute-path
semantics.

Use platform-neutral asset / output location abstractions in shared contracts,
with the macOS adapter resolving them to actual macOS paths.

#### Local environment state — Ready First

AI-OS must not treat the existence of a ComfyUI application or folder as proof
that local generation is usable.

The local environment has at least these stable states:

- `not_installed`
- `installed_not_configured`
- `installed_broken`
- `ready`

Transient setup states may include:

- `installing`
- `configuring`
- `repairing`
- `starting`
- `testing`

A local ComfyUI environment is `ready` only when all requirements needed for
the requested generation profile have passed:

- ComfyUI exists
- ComfyUI can start or is already running
- the local API is reachable
- the required workflow exists and is compatible
- required models are present and complete
- required VAE / text encoders / auxiliary model assets are present
- required custom nodes are present and version-compatible
- managed assets pass integrity checks
- a minimal smoke-generation path succeeds
- AI-OS can retrieve the produced output file

**Installed is not Ready.**

If local generation is not `ready`, AI-OS must not silently execute against an
incomplete environment.

#### First-use local / cloud decision

When the requested local environment is `ready`, Local First means AI-OS uses
it directly unless the user explicitly selected another provider.

When ComfyUI is `not_installed`, the user receives two product-level choices:

1. Set up free local generation.
2. Use a cloud provider.

When ComfyUI is `installed_not_configured`, the choices are:

1. Complete local setup automatically.
2. Use a cloud provider.

When ComfyUI is `installed_broken`, the choices are:

1. Repair the local environment automatically.
2. Use a cloud provider.

The user must not be turned into the ComfyUI administrator.

#### One-click local setup

"Set up local generation" means delivery of a usable generation capability,
not merely installation of an empty ComfyUI application.

AI-OS owns the setup chain:

1. inspect OS, GPU, VRAM / unified memory, RAM and available disk space
2. select a compatible Generation Profile
3. download and install ComfyUI when needed
4. prepare required model assets
5. prepare required VAE / encoders / auxiliary assets
6. prepare only the necessary custom nodes
7. install a verified workflow
8. configure all paths and bindings
9. start the local runtime
10. run health checks
11. run a minimal smoke generation
12. declare the profile `ready` only after output retrieval succeeds

Ordinary users should not need to understand checkpoints, diffusion models,
LoRAs, VAE, CLIP / text encoders, samplers, schedulers, CFG, steps, custom
nodes or workflow JSON.

Advanced users may later opt into direct model / workflow control, but this is
not the default product path.

#### Generation Profile Registry

Local capability is represented by stable user-facing profiles rather than
specific model names.

Initial profile classes may include:

- `image.general.standard`
- `image.general.quality`
- `image.photorealistic`
- `image.anime`
- `image.reference`
- `video.t2v.standard`
- `video.t2v.quality`
- `video.i2v.standard`
- `video.i2v.quality`

A profile manifest owns implementation metadata such as:

- current recommended model
- precision / quantization
- asset source
- checksum
- license
- minimum hardware
- recommended hardware
- required disk space
- workflow
- required nodes
- capabilities
- content compatibility
- last verified version / date

Model names are implementation choices beneath the profile and may change
without changing the Generative Media product contract.

The selection rule is:

**stable completion on the user's hardware is more important than theoretical
maximum model quality.**

#### Verified assets and nodes

Automatic installation uses controlled asset provenance.

Preferred asset tiers:

1. AI-OS Verified
2. Trusted upstream
3. Community / user-requested

AI-OS Verified assets must have known source, version, checksum, license,
hardware requirements and tested workflow compatibility.

Trusted upstream assets originate from official model / project publishers and
must still be validated before managed installation.

Community assets such as Civitai models and LoRAs may be supported when useful,
but AI-OS must not silently download arbitrary community artifacts without
provenance and compatibility checks.

Custom nodes follow the same rule through a Verified Node Registry containing
repository, version / commit, license, dependencies, supported ComfyUI version,
security / compatibility status and last verification.

Default workflows should prefer ComfyUI core nodes where practical.

Managed profiles are version-pinned by default; AI-OS must not blindly follow
latest model or node releases and destabilize a working environment.

#### Cloud Provider Registry

Cloud generation uses a replaceable Provider Registry.

Initial provider classes may include:

- xAI
- OpenAI
- Google
- future providers

The currently recommended cloud provider may be xAI Imagine when current
capability, cost, availability and lawful-content compatibility make it the
best default, but this is recommendation metadata, not a hardcoded routing
dependency.

A user who explicitly selects a provider must have that choice respected.

Provider metadata should describe capability rather than fame, including:

- image generation
- image editing
- text-to-video
- image-to-video
- reference support
- resolution / duration limits
- latency
- price
- regional availability
- lawful-content compatibility
- real-person restrictions
- health / availability
- last policy verification

Ordinary users should receive a sensible recommended provider instead of being
required to research provider moderation systems themselves.

Recommendation metadata may change as providers, policies, prices and model
capabilities change.

#### Cloud credentials and cost

AI-OS does not provide a unified paid generation balance and does not resell
cloud generation credits.

Generative Media reuses the user's configured provider identity / credential
where that credential actually grants media API access.

Before cloud execution AI-OS must validate, as applicable:

- provider credential exists
- requested media capability exists
- requested model is available
- required API entitlement exists
- billing / quota state permits the request

A chat-model login or configured LLM provider must not be assumed to imply
media-generation entitlement.

Cloud cost belongs directly to the user's provider account.

#### Cloud budget

Paid automatic generation / refinement is bounded by user-defined budget.

AI-OS should support:

- task budget
- optional default per-task cloud budget

Before every paid generation or retry, AI-OS must estimate whether the next
operation fits inside the remaining budget.

If it would exceed the remaining budget, AI-OS stops before issuing the
request.

Budget protection is proactive, not post-spend reporting.

#### Prompt intelligence

Prompt engineering is not a prerequisite for using Generative Media.

AI-OS first represents the user's goal as a provider-neutral
`CreativeIntent`, which may include:

- subject
- scene
- style
- composition
- camera
- lighting
- colour
- mood
- motion
- constraints
- references

A provider / model-specific Prompt Compiler then translates that intent into
the representation appropriate for the selected local model or cloud provider.

A prompt supplied by an advanced user is accepted, but ordinary users may
express only the desired result.

#### Reference Analysis / reverse prompting

AI-OS should not reimplement mature reverse-prompt and visual-analysis models
when strong open-source implementations already exist.

Reference understanding is exposed through a replaceable
Reference Analysis Adapter Registry.

Initial OSS candidates include classes represented by:

- JoyCaption for diffusion-oriented image caption / prompt reconstruction
- Florence2-style structured visual analysis / PromptGen
- Qwen2.5-VL / ShotVL-class video understanding for motion and shot analysis

The exact OSS dependency remains replaceable and must be reviewed for license,
maintenance, hardware compatibility and integration safety before adoption.

The product-level result is a normalized `ReferenceSpec`, not a promise to
recover the unknowable original generation prompt.

`ReferenceSpec` may include:

- subject
- scene
- composition
- camera
- lighting
- palette
- style
- pose
- objects
- motion
- shot structure
- constraints
- confidence

Supported user intents include:

- analyze a reference
- generate something visually similar
- preserve selected reference characteristics while changing others
- use provider-native reference conditioning when available

When provider-native reference conditioning is available, AI-OS may combine
the original reference with the generated prompt / intent rather than relying
only on textual reconstruction.

#### Local automatic refinement

Local result inspection and automatic refinement may be enabled by the user.

The user controls the maximum local retry count.

Suggested UI choices may include:

- Off
- 1 retry
- 2 retries
- 3 retries

A reasonable initial default is 1 retry, but the setting belongs to the user.

Local retries remain bounded because they consume time, electricity and
machine resources even when they have no API charge.

#### Cloud automatic refinement

Cloud refinement is budget-controlled rather than merely retry-count
controlled.

The loop may continue only while:

- the user has enabled paid automatic refinement
- the next estimated request remains inside the remaining task budget
- provider policy permits the request
- retry / time / cancellation limits have not been reached

A provider content-policy rejection must not automatically route the same
request to another provider to evade that provider's policy.

The user may explicitly choose a different provider afterward.

#### Self-correction / quality loop

Generative Media v1 includes bounded quality inspection.

The intended loop is:

`Generate`
→ inspect generated result
→ compare against `CreativeIntent`
→ identify material mismatch
→ adjust prompt / parameters / workflow where appropriate
→ retry within the user's local retry or cloud budget limits

This is a bounded execution loop, not an autonomous unlimited search.

#### Generative Media v1 capability target

The v1 product target includes:

Generation:
- Text → Image
- Image Edit
- Text → Video
- Image → Video

Reference:
- Image Analysis
- Video Analysis
- Similar Image
- Similar Video
- Reference-conditioned generation where the provider supports it

Intelligence:
- intent interpretation
- automatic prompt composition
- provider / model-specific prompt compilation
- result inspection
- bounded automatic refinement

Infrastructure:
- provider discovery
- local Ready detection
- one-click install / configure / repair
- Generation Profile management
- model / LoRA / node / workflow management
- cloud Provider Registry
- budget enforcement
- cancellation
- output-file handling

#### Generative Media implementation roadmap

Generative Media is developed in this order:

**GM-0 — Product Contract**

- product boundary
- platform boundary
- Ready First state model
- local / cloud first-use flow
- profile / asset / provider rules
- prompt / reference contract
- retry / budget contract

**GM-1 — Core / Provider Router**

- provider-neutral media capability contract
- `CreativeIntent`
- normalized media request / result
- provider discovery / health
- Runtime / permission integration
- replaceable Provider adapters
- platform-neutral core interfaces

**GM-2 — ComfyUI Ready Detection + Local Provider**

- detect all Ready states, not merely installation
- platform-neutral ComfyUI execution contract
- macOS discovery / start implementation
- ComfyUI health / API execution
- workflow submission
- progress
- cancellation
- history
- output retrieval
- local failure normalization

**GM-3 — One-click Install / Configure / Repair**

- platform-neutral installer interface
- macOS installer implementation
- hardware profiling
- Generation Profile selection
- ComfyUI installation
- model / LoRA / asset management
- verified node management
- workflow installation
- automatic repair
- smoke generation
- Ready promotion only after successful output

**GM-4 — Cloud Providers**

- xAI provider
- OpenAI provider
- additional providers through the same registry
- provider capability / policy metadata
- credential reuse
- price estimation
- task budget enforcement
- async result handling

**GM-5 — Prompt Intelligence + OSS Reference Analysis**

- CreativeIntent interpreter
- provider / model-specific Prompt Compiler
- Reference Analysis Adapter Registry
- OSS image / video reference analysis
- normalized ReferenceSpec
- similar / reference-conditioned generation

**GM-6 — Self-correction / Quality Loop**

- generated-result inspection
- intent comparison
- bounded prompt / workflow refinement
- user-controlled local retry
- budget-controlled cloud retry
- cancellation / time limits
- no automatic policy-refusal provider bypass

GM phases are implemented in this order unless a later acceptance result proves
the sequence wrong.

GM-0 is a product-contract phase and does not itself install ComfyUI, download
models, invoke a cloud provider or add a production media capability.

### What comes next

1. Local generative media.
2. Cognitive distillation foundation.
3. NAS foundation remains hardware-blocked for real hardware E2E.

Computer Control must remain complementary to the other execution layers:

- Computer Use owns visual GUI interaction
- OpenClaw owns general Agent execution
- Computer Control owns deterministic system state and direct system operations

Nothing in Computer Control should become a second general-purpose Agent or a
screen-driving automation layer.

<!-- COMPUTER_CONTROL_STATE_END -->

<!-- OFFICE_TAKEOVER_2026-09-02_START -->

## CURRENT P15 OFFICE TAKEOVER STATE — 2026-09-02

> **This section is the authoritative current Office implementation state for the next developer.**
> If an older Office / Excel note elsewhere in this file conflicts with this section, use this section.

### Current repository baseline

- Branch: `feature/p15-core-skills`
- Stable takeover baseline before this documentation commit:
  `633aeb42258a248ed80dd0fa760525a6207ed9d3`
- Working tree was clean before this HANDOFF update.
- Office is **Complete** as of 2026-09-05.
- P15 is **6 / 11**.
- Checked against the twenty items under "Office completion requirements" one
  by one rather than by impression. Nineteen were already accepted. The
  twentieth, "permission / confirmation behavior", was NOT: twelve capabilities
  the resolver could route were absent from the permission gate's list and were
  therefore denied in production -- implemented, routed, gate-proven and
  uncallable. They are now permitted, and
  `every_capability_the_resolver_can_route_is_one_a_person_can_authorise`
  fails the suite if it happens again.

### Microsoft Word — COMPLETE

Microsoft Word Common Capability is complete.

Accepted deterministic Word workflows include:

- document read
- document create
- document edit
- paragraph / heading changes
- basic formatting
- deterministic 2 x 2 table insertion and read-back
- bounded inline-image insertion and reopen validation
- save-copy
- PDF export
- no-overwrite
- original-file preservation
- operation-owned document lifecycle
- realistic combined document workflow

Important ownership invariant:

Word can return a stale document proxy after opening. The accepted path uses bounded document-count waiting and a fresh active-document reference. Never quit Word and never close a user-owned document.

DOC -> DOCX remains a non-blocking SKIP because no safe legacy DOC fixture was available.

Do not reopen Word work unless an actual regression is observed.

### Microsoft PowerPoint — COMPLETE

Microsoft PowerPoint Common Capability is complete.

Accepted deterministic PowerPoint workflows include:

- presentation read
- presentation create
- title/body edit
- slide add
- slide delete
- slide reorder
- image insertion
- text insertion
- table insertion
- basic-shape insertion
- save-copy
- PDF export
- original-file preservation
- operation-owned presentation lifecycle
- realistic combined presentation workflow

PPT -> PPTX remains a non-blocking SKIP because no safe legacy PPT fixture was available.

Do not reopen PowerPoint work unless an actual regression is observed.

### Microsoft Excel — IN PROGRESS

#### Phase A — COMPLETE

Accepted capabilities include:

- `set_cell`
- `set_formula`
- `clear_cell`
- typed cell values
- formula read-back
- calculated-value verification
- unique XLSX save-copy
- no-overwrite
- original preservation
- fresh post-Save-As workbook ownership
- output reopen validation
- workbook/window restoration
- Excel-process preservation
- user-workbook preservation

Critical invariant:

After Excel Save As, the old workbook proxy may no longer be authoritative. Reacquire the fresh active workbook and verify its full path equals the expected unique output path before continuing.

#### Phase B Mutation — COMPLETE

Accepted capabilities include:

- `add_worksheet`
- `rename_worksheet`
- `delete_worksheet`
- named-sheet `set_cell`
- named-sheet `set_formula`

Important worksheet identity invariant:

Excel 16.78 does not guarantee the insertion index of a newly created worksheet.

Therefore AddWorksheet must:

1. snapshot worksheet identities before creation
2. create the worksheet
3. reacquire the fresh workbook reference
4. snapshot identities after creation
5. calculate the unique set difference
6. use that discovered worksheet identity

Never assume the newly added worksheet is at a particular index.

Deleting the final worksheet fails closed.

#### Phase B Multi-Sheet Read — COMPLETE

Accepted capabilities include:

- worksheet list
- worksheet count
- legacy no-selector first-sheet behavior
- explicit named-sheet read
- bounded selected multi-sheet read
- bounded `allSheets`
- missing worksheet fail closed
- per-sheet truncation metadata
- global bounded read protocol

Current bounds:

- maximum 16 worksheets
- maximum 200 rows per worksheet
- maximum 64 columns per worksheet
- maximum 256 rendered characters per cell
- roughly 60k total protocol-character budget

Phase B real Excel E2E passed.

Do not reopen Excel Phase A or Phase B unless a regression is observed.

### Excel Phase C — COMPLETE

Landed operations, all through the existing `edit_excel_workbook()` adapter on
the existing provider-neutral `spreadsheet.edit` Runtime route. No second Excel
adapter was added.

- `insert_row`
- `delete_row`
- `insert_column`
- `delete_column`

Contract: explicit worksheet identity, 1-based bounded indexes (row 1-10000,
column 1-256), one line per operation, no `count` parameter, existing
save-copy / reopen ownership model, real shifted-content validation.

#### The previous attempt's hypothesis was wrong

The earlier failure was blamed on `delete range range "A:A" shift shift to left`.
An isolated real Excel 16.78 probe (`verify/probe_excel_structural_semantics.sh`)
ran all twelve candidate forms -- plain, `shift`, and `entire row`/`entire column`
-- each in its own `osascript` invocation against a fresh workbook. **All twelve
shifted content correctly and identically.** The `shift` parameter changes
nothing.

The accepted form is therefore the plain one, with no `shift` parameter:

```applescript
insert into range (range lineReference of structuralSheet)
delete range (range lineReference of structuralSheet)
```

The real cause of the earlier failure was in adapter integration, not AppleScript
semantics. Two integration defects were found and fixed:

1. **Reopen validation read addresses the change had moved.** A structural change
   moves every cell after it, so an address written earlier on that worksheet is
   stale by the time the saved copy is reopened. The validation asserted the old
   address anyway and reported a working shift as
   `SetCell string validation mismatch`. Worksheets touched by a structural
   operation are now collected before the validation pass, and cell operations on
   them report `displaced-by-structural-change` instead of asserting a stale
   address. Excel's shift arithmetic is deliberately **not** reimplemented in
   AppleScript to predict the new address -- that is the same looks-equivalent
   assumption that cost the previous attempt.

2. **Validation reads the saved copy once, at the end.** Every probe therefore
   sees the final state of its worksheet, not the state right after its own
   operation. Each structural operation now gets a worksheet to itself in the
   E2E, so the final state is the post-operation state and each read-back proves
   exactly one operation.

#### Accepted real Excel 16.78 evidence

`document::excel::tests::excel_phase_c_structural_real_e2e`, all read out of the
reopened saved copy:

| worksheet | before | operation | probe |
| --- | --- | --- | --- |
| InsCol | `A1=R1 B1=C1 C1=C2` | insert column B | `B1= , C1=C1` |
| DelCol | `A1=R1 B1=C1 C1=C2` | delete column A | `A1=C1, B1=C2` |
| InsRow | `A1=X1 A2=R2 A3=R3` | insert row 2 | `A2= , A3=R2` |
| DelRow | `A1=X1 A2=R2 A3=R3` | delete row 1 | `A1=R2, A2=R3` |
| Workflow | `A1=R1 B1=C1 C1=C2` | insert column B, delete column A | `A1= , B1=C1` |

`Workflow` is exactly the sequence the previous attempt failed on.

Used-range snapshots are recorded per operation and reported, not asserted --
Excel does not always shrink a used range on deletion.

#### Verifiers

- `verify/verify_p15_excel_phase_c_structural.sh` -- compiles the embedded
  AppleScript on its own first, so a syntax mistake fails in a second rather than
  minutes into the real E2E; then the real structural E2E, then Phase A, Phase B
  Mutation and Phase B Read regressions, then Excel workbook/window state
  restoration.
- `verify/gate_p15_excel_phase_c.sh` -- one-pass acceptance gate: the above plus
  Spreadsheet Create, Spreadsheet Read, the full Rust suite, the frontend build
  and `git diff --check`. One line per step.

Full gate result on real Excel 16.78: **PASS** on every step.

`verify/verify_p15_excel_phase_a.sh` asserts how many tests the
`document::excel::tests::` filter matches, so that a filter which silently stops
matching cannot pass as a clean run. Phase C moved that count from 8 to 11.
**Anyone adding or removing a test in that module must update it.**

### Microsoft Excel Common Capability — COMPLETE

Every item below is landed and proven on real Excel 16.78. The list is kept as
the record of what was required.

1. ~~basic formatting~~ — **COMPLETE**, see Excel Phase D below
2. ~~sort/filter~~ — **COMPLETE**, see Excel Phase E below
3. ~~common chart integration~~ — **COMPLETE**, see Excel Phase F below
4. ~~formula-aware spreadsheet read~~ — **COMPLETE**, see Excel Phase G below
5. ~~final realistic spreadsheet workflow~~ — **COMPLETE**, see Excel Phase H below
6. ~~final Excel Common Capability acceptance~~ — **COMPLETE**

Existing isolated chart probes previously demonstrated that column / line / pie chart creation is technically possible in real Excel. Do not treat those probes alone as integrated Common Capability completion.

### Excel Phase D — Basic formatting — COMPLETE

Landed operations, again through the existing `edit_excel_workbook()` adapter on
the existing provider-neutral `spreadsheet.edit` route. No second Excel adapter.

- `format_cells` — one rectangular range, any combination of `bold`, `italic`,
  `fontSize`, `fontName`, `fillColorIndex`, `numberFormat`
- `set_column_width`
- `set_row_height`

#### What was probed before anything was written

`verify/probe_excel_formatting_semantics.sh` answered the two questions the
phase could not be written without:

1. **Do these property names exist?** `italic`, `font size` and font `name` had
   never been probed; only bold, number format, colour index, column width and
   row height had.
2. **Does each attribute survive save-as-xlsx, close and reopen?** The adapter
   validates by reading the reopened saved copy, so an attribute that is correct
   live but lost on save could not be validated that way at all.

All eight passed both. Proven forms, used verbatim:

```applescript
set bold of font object of range rangeReference to true
set italic of font object of range rangeReference to true
set font size of font object of range rangeReference to 18
set name of font object of range rangeReference to "Courier New"
set number format of range rangeReference to "0.00"
set color index of interior object of range rangeReference to 6
set column width of column 1 of targetSheet to 24
set row height of row 1 of targetSheet to 30
```

Read-back text forms matter and are not uniform: bold/italic return `true`,
font size returns `18` (not `18.0`), colour index returns `6`, but column width
and row height return `24.0` and `30.0`. **Width and height must be compared
numerically, not as strings**, and the adapter allows half a unit of drift
because Excel stores them as reals and may settle a fraction away from a
requested whole number; the observed value is reported, not the requested one.

#### Contract

- one range per operation, bounded: rows 1-10000, columns 1-256, at most 65536
  cells, and an end that may not precede its start
- attributes are optional but an operation carrying none is **refused** -- it
  would change nothing and then validate clean
- attribute bounds are Excel's own: font size 1-409, palette index 1-56, column
  width 1-255, row height 1-409. Zero is excluded on width and height because it
  hides the line rather than resizing it, which is not what the caller asked for
- encoding is fixed arity with an empty field per absent attribute and an `end`
  terminator, so no optional value is ever the trailing field
- the Phase C displacement rule applies here too: formatting addresses on a
  worksheet that a structural operation touched are reported as
  `displaced-by-structural-change` rather than asserted stale

#### Accepted real Excel 16.78 evidence

`document::excel::tests::excel_phase_d_formatting_real_e2e`, read out of the
reopened saved copy at the top-left cell of each range:

| range | requested | read back |
| --- | --- | --- |
| `A1:B1` | bold, italic, size 14, Courier New, fill 6 | `bold=true;italic=true;size=14;font=Courier New;fill=6;` |
| `B2:B2` | number format `0.00` | `number=0.00;` |
| column 1 | width 24 | within half a unit of 24 |
| row 1 | height 30 | within half a unit of 30 |

`B2` deliberately asks for a number format and nothing else, so an
implementation that painted the whole sheet would be caught.

#### Verifiers

- `verify/probe_excel_formatting_semantics.sh` — the isolated probe above
- `verify/verify_p15_excel_phase_d_formatting.sh` — AppleScript compile, input
  validation, encoding, the real E2E, then Phase A / Phase B Mutation /
  Phase B Read / Phase C regressions, then Excel state restoration
- `verify/gate_p15_excel.sh` — one-pass gate (renamed from
  `gate_p15_excel_phase_c.sh`; the Phase D verifier subsumes the Phase C step)

Full gate result on real Excel 16.78: **PASS** on every step.

The `document::excel::tests::` count guard in
`verify/verify_p15_excel_phase_a.sh` moved from 11 to **14**.

### Excel Phase E — sort/filter — COMPLETE

Landed operations, again through the existing `edit_excel_workbook()` adapter on
the existing provider-neutral `spreadsheet.edit` route.

- `sort_range` — one bounded range, a key column that must fall inside it,
  explicit `order` and explicit `hasHeader`
- `apply_filter` — one bounded range, a `field` bounded by the range's own
  width, and a criteria string
- `clear_filter` — unhides the rows a filter hid

`hasHeader` and `order` are **required, not defaulted**. Guessing `hasHeader`
wrong sorts the caller's header row into their data, and no later operation
undoes that.

#### What the probe established

`verify/probe_excel_sort_filter_semantics.sh`. Sort already had a proven form
and a scalar read-back; autofilter had neither, so the probe existed to find
what a filter CAN be judged by and whether that evidence survives save-as-xlsx,
close and reopen.

- all four sort variants work and survive the round trip: ascending,
  descending, header yes, header no, and a non-first key column
- `autofilter mode of <sheet>` reads back `true` and survives the round trip
- `hidden of row N of <sheet>` reflects the criteria and survives the round trip
  — filtering `Score > 1` over `Bravo 2 / Alpha 1` leaves row 2 visible and row
  3 hidden, in the reopened copy
- `show all data <sheet>` unhides the rows and leaves `autofilter mode` true
- `range of autofilter object` does **not** answer `get address` (error -1708);
  it is not used

So a filter is validated by `autofilter mode` plus the hidden state of the
range's first data row and last row. `apply_filter` asserts the mode is on;
`clear_filter` asserts neither probe row is hidden. Both report all three
values.

The AppleScript enumeration constants `sort ascending` and `header yes` cannot
be interpolated from a field, so the adapter writes out the four probed forms
literally and selects between them.

#### The displacement rule had to grow, and this cost a real run

Phase C established that an operation which moves cells invalidates addresses
written earlier on that worksheet. That was implemented for structural
operations only. **Sorting also moves cells**, so the first real Phase E run
failed with `SetCell string validation mismatch` for exactly the Phase C reason.

There are now two sets, because the two kinds of movement invalidate different
things:

| | `structurallyChangedSheets` | `reorderedSheets` |
| --- | --- | --- |
| filled by | insert/delete row/column | `sort_range` |
| moves values | yes | yes |
| moves cell formatting | yes | yes (it travels with the row) |
| moves row/column **indexes** | yes | no |
| invalidates `set_cell` / `set_formula` / `clear_cell` / `format_cells` | yes | yes |
| invalidates `set_column_width` / `set_row_height` / `sort_range` probes / filter row probes | yes | **no** |

The marker is now `displaced-by-moved-content` rather than
`displaced-by-structural-change`.

A filter hides rows without moving their contents, so **ordinary
address-based validation still applies on a filtered worksheet**. The Phase E
E2E asserts that directly (`set_cell:Filtered!A2=Bravo`), which is what keeps
the displacement rule from quietly widening into "skip validation whenever
anything happened".

#### Accepted real Excel 16.78 evidence

`document::excel::tests::excel_phase_e_sort_filter_real_e2e`. Three worksheets,
each `Name/Score` with `Bravo 2` and `Alpha 1`, all read out of the reopened
saved copy:

| worksheet | operation | read back |
| --- | --- | --- |
| Sorted | sort by Name ascending, header | `A2=Alpha,A3=Bravo` |
| Filtered | filter Score `>1` | `mode=true,row2hidden=false,row3hidden=true` |
| Cleared | same filter, then clear | `mode=true,row2hidden=false,row3hidden=false` |

Plus `set_cell:Sorted!A2=displaced-by-moved-content` and
`set_cell:Filtered!A2=Bravo`.

#### Verifiers

- `verify/probe_excel_sort_filter_semantics.sh`
- `verify/verify_p15_excel_phase_e_sort_filter.sh` — AppleScript compile, input
  validation, encoding, the real E2E, then Phase A / B Mutation / B Read /
  C / D regressions, then Excel state restoration
- `verify/gate_p15_excel.sh` now runs the Phase E verifier, which subsumes the
  earlier phase steps

Full gate result on real Excel 16.78: **PASS** on every step.

The `document::excel::tests::` count guard moved from 14 to **17**.

### Excel Phase F — common chart integration — COMPLETE

`add_chart` creates one chart from a bounded source range, through the existing
`edit_excel_workbook()` adapter on the existing provider-neutral
`spreadsheet.edit` route.

Public chart types are `column`, `bar`, `line`, `pie`. An explicit `name` is
**required** rather than accepting Excel's own `Chart 1`: it is the handle the
reopened copy is searched by, and two charts sharing a name would make that
search ambiguous.

#### What the probes established

`verify/probe_excel_chart_semantics.sh`, two rounds:

| form | result |
| --- | --- |
| `make new chart object at <sheet>` | works |
| `make new chart at end of <workbook>` | fails, -50 |
| `set (source data of chart of X) to <range>` | fails, -10006 |
| `set source data chart of X source (<range>) plot by columns` | works |
| `name of <chart object>` | settable, survives reopen |
| `chart type of chart of <chart object>` | readable, survives reopen |
| `count of series` / `formula of series 1` | readable, survives reopen |

**Excel does not always store the constant that was set.** Asking for
`line chart` produces a chart that reads back as `line markers`; `pie exploded`
reads back as `pie chart`; `pie` and `line` alone are not valid constants at
all, and `xy scatter` does not compile. The encoded record therefore carries
**both** constants: one selects the AppleScript branch, the other is what the
reopened copy is compared against.

| public type | set | stored |
| --- | --- | --- |
| `column` | `column clustered` | `column clustered` |
| `bar` | `bar clustered` | `bar clustered` |
| `line` | `line markers` | `line markers` |
| `pie` | `pie chart` | `pie chart` |

The strongest evidence available is the **series formula**, which survives the
reopen and names the ranges the chart actually plots:
`=SERIES(Charted!$B$1,Charted!$A$2:$A$3,Charted!$B$2:$B$3,1)`. A chart that
exists but plots nothing cannot pass that.

#### An Excel deadlock that costs half an hour if you hit it

**`repeat with x in chart objects of <sheet>` hangs Excel indefinitely once a
chart exists on that sheet.** `with timeout` does not rescue it; the osascript
process has to be killed. The first Phase F run used exactly that loop to check
for a duplicate chart name and stalled for 30 minutes.

`verify/probe_excel_chart_multi.sh` found working alternatives, all answering in
under a second:

- `count of chart objects of <sheet>`
- `name of chart object <index> of <sheet>`
- `name of every chart object of <sheet>`
- `exists chart object "<name>" of <sheet>`  ← what the adapter uses

The hanging form is kept in that probe, unrun, so the finding stays
reproducible. **Do not enumerate `chart objects` with `repeat with x in`
anywhere.**

#### Accepted real Excel 16.78 evidence

`document::excel::tests::excel_phase_f_chart_real_e2e`. Four charts on one
worksheet — charts are found by name, not position, and nothing here moves
cells — each read out of the reopened saved copy:

`add_chart:Charted!A1:B3:name=ByColumn,type=column clustered,series=1,f1==SERIES(Charted!$B$1,Charted!$A$2:$A$3,Charted!$B$2:$B$3,1)`
and the same for `ByBar` (`bar clustered`), `ByLine` (`line markers`) and
`ByPie` (`pie chart`).

`add_chart` carries **no displacement guard**: a chart is found by name rather
than by address, so moving the cells around it does not make the probe stale.

#### Verifiers

- `verify/probe_excel_chart_semantics.sh`, `verify/probe_excel_chart_multi.sh`
- `verify/verify_p15_excel_phase_f_charts.sh`
- `verify/gate_p15_excel.sh`

The `document::excel::tests::` count guard moved from 17 to **20**.

### The verification harness was rebuilt during Phase F

Two harness faults cost more time than any code defect, and both are fixed:

1. **The phase verifiers ran each other.** Phase F ran E, which ran D, which ran
   C, which ran A and B. One gate cost **32 real Excel cycles**. Phase verifiers
   no longer run their predecessors; `verify/gate_p15_excel.sh` runs every phase
   once, in order. A full gate now takes about **2.5 minutes**.
2. **A hang was invisible and unbounded.** Step logs went to `mktemp`, and there
   was no time limit. `run_gate.command` now enforces `AIOS_TASK_TIMEOUT`
   (default 1800s) by running the task in its own process group so the whole
   tree can be killed, writes step logs to `.gate-logs/` inside the repository,
   and the gate prints a `....` line before each step so an in-progress step is
   visible.

`run_gate.command`, `.aios-task.sh`, `.gate-run.log` and `.gate-logs/` are
gitignored developer-machine scaffolding, not part of the product.

**If a run is killed mid-flight, Excel keeps the operation workbook open and
shows a document-recovery banner on the next launch.** Close the leftover
`operation-*` workbooks without saving before trusting the next run's
before/after workbook-state comparison.

### Excel Phase G — formula-aware read — COMPLETE

`spreadsheet.read` takes an optional `includeFormulas` boolean on the existing
provider-neutral read route. It is **opt-in and strictly typed**: absent, null
and `false` all produce byte-for-byte the output the route produced before, and
a truthy string is refused rather than silently changing the response shape for
a caller who did not ask.

Two keys are added per worksheet:

- `usedRange` — always present now, so the returned grid maps to real addresses
- `formulas` — present **only when requested**, as a sparse list of
  `{row, column, formula}`

Sparse, because a cell holding a constant reports that constant as its formula:
the only thing that marks a real formula is the leading `=`, and reporting every
cell would double the payload to say nothing. An empty list means "asked, none
present"; an absent key means "never asked" -- different answers.

Bounds: at most 512 formula entries per worksheet, the existing 256-character
cell cap, and the existing ~60k protocol budget. Exceeding any of them sets the
truncation flag rather than silently dropping entries. The parser also compares
the arrived count against the declared `AIOS_FORMULA_COUNT` and reports
truncation on a mismatch, so a payload cut on the way out cannot read as
complete.

#### What the probe established

`verify/probe_excel_formula_read_semantics.sh`:

- `formula of used range` returns the **same 2D list shape** as
  `value of used range`, so it needs no separate traversal
- a constant cell reports the constant (`B2formula=2`, `A2formula=Bravo`)
- `formula` comes back in **English on a Chinese-locale Excel**
  (`=SUM(B2:B3)`), so `formula` is used and `formula local` is not
- all of it survives save-as-xlsx, close and reopen

#### Two real-Excel traps this phase hit

1. **`tab` is Excel terminology inside a `tell` block.** The separator was
   written as `& tab &` and came out as the literal text `tab` --
   `4tab2tab=SUM(B2:B3)`. Inside `tell application "Microsoft Excel"` the
   application's dictionary wins over AppleScript's constant. The separator is
   now `(ASCII character 9)`. The `joinRow` helpers can keep using `tab` only
   because they live **outside** the tell block. This is the same class of bug
   as `line` in the Phase C probe.

2. **A read E2E must not open a workbook from a temp directory.** A read runs
   its own osascript invocation, so it does not inherit the implicit access
   macOS grants an app for a file that app just wrote. Excel then raises a
   "please locate this file" sandbox prompt that no automated run can answer --
   and that modal **blocks every later Excel automation** until a person
   dismisses it, which is what made a whole gate collapse with unrelated
   failures. Read E2Es now build their workbook under Excel's own cache
   directory via `excel_readable_workspace()`. The edit-side E2Es never hit
   this because they reopen their output inside the same invocation that saved
   it.

#### Verifiers

- `verify/probe_excel_formula_read_semantics.sh`
- `verify/verify_p15_excel_phase_g_formula_read.sh`
- `verify/gate_p15_excel.sh`

Two verifiers assert how many tests their filter matched, so that a filter which
silently stops matching cannot pass as a clean run. **Adding or removing a test
in either module means updating the count:**

| verifier | filter | count |
| --- | --- | --- |
| `verify_p15_excel_phase_a.sh` | `document::excel::tests::` | 20 |
| `verify_p15_spreadsheet_read.sh` | `openclaw_gateway_adapter::tests::spreadsheet_read_` | 11 |

### A Phase B regression the Phase G work exposed

`delete_worksheet` raises Excel's "will permanently delete this sheet" modal
unless `display alerts` is off. It had never surfaced because this machine's
Excel happened to have alerts off; when that flipped back to `true`, an
unattended run sat on the modal until it was killed, and the modal then blocked
every later Excel automation.

The adapter now turns `display alerts` off immediately around the delete and
**restores the previous value on both the success and the error path** --
leaving a user's Excel with alerts off is not acceptable. Proven by
`verify/probe_excel_alert_suppression.sh`, which also proves the restore
survives an erroring delete.

**Nothing else in the adapter raises a modal**: `close ... saving no` does not,
and save targets are always unique paths.

### Operating notes for whoever runs these gates

- If a run is killed mid-flight, Excel keeps the `operation-*` workbook open and
  shows a document-recovery banner. Close those workbooks without saving before
  trusting the next run's before/after workbook-state comparison.
- A modal in Excel is a hard stop for every later run, not just the one that
  raised it. When several unrelated phases start failing with AppleEvent
  timeout (-1712), look at Excel's screen before looking at the code.

### Excel Phase H — realistic workflow — COMPLETE

`runtime::openclaw_gateway_adapter::tests::excel_phase_h_realistic_workflow_real_e2e`

One workbook built the way someone actually would, in one
`spreadsheet.edit` call: typed cells, totals by formula, a **descending sort
on the computed total**, a bold filled header, a number format, a wider label
column, a chart, and a filter. Then read back through **`spreadsheet.read` in
its own osascript invocation**.

This is the only test that crosses both halves of the Excel work. Every
per-phase E2E judges the edit adapter against its own reopened copy; this one
judges the file as a caller actually receives it, and asserts three things no
per-phase test can:

- the rows come back in sorted order (East 60, South 55, North 30)
- a filtered row is **hidden, not deleted** — North is still in the file
- **Excel rewrote each total's relative references as the sort moved its row**,
  so row 3 holds `=SUM(B3:C3)` even though that formula was written on row 2

#### It immediately caught a real defect in the displacement rule

The rule was answered **per worksheet** — "was this sheet ever sorted". So a
format applied *after* the sort was reported as displaced and its check
silently skipped, even though its address was perfectly valid. That is exactly
the quiet widening into "skip validation whenever anything happened" that
earlier phases warned this rule must not become, and only a workflow mixing
moving and non-moving operations on one worksheet could expose it.

Displacement is now answered **per operation**: does any *later* operation move
content on this worksheet (`movesAfter` in the adapter's AppleScript). The two
kinds of movement still invalidate different things:

| | structural (`insert_row` etc.) | reorder (`sort_range`) |
| --- | --- | --- |
| moves values | yes | yes |
| moves cell formatting | yes | yes (travels with the row) |
| moves row/column **indexes** | yes | no |
| invalidates `set_cell` / `set_formula` / `clear_cell` / `format_cells` | yes | yes |
| invalidates `set_column_width` / `set_row_height` / `sort_range` probes / filter row probes | yes | **no** |

Two assertions now pin the rule from both sides: Phase E asserts a *filtered*
worksheet still validates normally (a filter moves nothing), and Phase H
asserts formatting *after* a sort validates normally.

#### Verifier

`verify/verify_p15_excel_phase_h_workflow.sh`, and the gate step "Phase H
workflow".

### Microsoft Excel — what the gate covers

`verify/gate_p15_excel.sh`, 14 steps, about 3 minutes, all green on real Excel
16.78:

Phase A cells and formulas · Phase B worksheets · Phase B multi-sheet read ·
Phase C row/column structure · Phase D formatting · Phase E sort and filter ·
Phase F charts · Phase G formula-aware read · Phase H realistic workflow ·
Spreadsheet Create · Spreadsheet Read · full Rust suite · frontend build ·
`git diff --check`

Every phase preserves the source workbook, saves to a unique path without
overwriting, restores Excel's workbook and window state, never quits Excel, and
leaves the user's own workbooks untouched.

### Apple iWork — COMPLETE

Pages, Numbers and Keynote 15.3.1 are installed on the acceptance Mac and all
three answer AppleScript.

#### The honest starting position

The Provider Registry declares **all eight** Office capabilities for Apple
iWork. What is actually executable is much less:

| capability | iWork adapter | evidence |
| --- | --- | --- |
| `presentation.read` / `presentation.create` | `document/keynote.rs` | real E2E |
| everything else | **none** | — |

`verify/verify_p15_iwork_real_e2e.sh` checks only that the three apps exist and
answer `get version`. The handoff is explicit that application detection and
metadata do not count as capability, so **Pages and Numbers currently have no
proven capability at all**, and the registry declaration overstates what iWork
can do. A Provider Registry declaration is not executable evidence.

Note the bundle identifiers are `com.apple.Pages` / `com.apple.Numbers` /
`com.apple.Keynote` — **not** `com.apple.iWork.*`, which does not resolve.

#### What the probe established

`verify/probe_iwork_pages_numbers_semantics.sh`, two rounds.

**Pages — enough for `document.read`, `document.create` and `document.convert`:**

| form | result |
| --- | --- |
| `make new document`, `set body text`, `save in POSIX file <path>` | works |
| close, `open POSIX file <path>`, read `body text` | works, content intact |
| `count of paragraphs` / `words` / `characters of body text` | works |
| `export <doc> to POSIX file (<path> & ".pdf") as PDF` | works |
| `export <doc> to POSIX file <path-without-extension> as PDF` | **fails, error 6** |

**The export target must carry its extension.** Without one Pages treats the
path as a folder and errors, leaving its document open.

**Numbers — enough for `spreadsheet.read` and `spreadsheet.create`, with one
hard limit:**

| form | result |
| --- | --- |
| `make new document`, `set value of cell "A1" of table 1 of sheet 1` | works |
| save, close, reopen, read cells back | works (`A1=Region`, `B2=42.0`) |
| `value of every cell of range "A1:B1"` / `of row 1` | list, as expected |
| a `"=SUM(A1:A2)"` string really becomes a formula | value `30.0`, `formula` reads back |
| `export ... as CSV` / `as Microsoft Excel` | works, with the extension on the path |
| **`make new sheet at end of sheets`** | **FAILS, -10000** |

Three consequences the adapter has to live with:

1. **A new Numbers table is already 22 rows x 7 columns.** There is no "used
   range" the way Excel has one, so a read must trim trailing empty rows and
   columns itself or it will return a wall of blanks.
2. **An empty cell reads back as `missing value`**, not as an empty string.
3. **Sheet and table names are localized** — `工作表 1`, `表格 1` on this
   machine. Address sheets and tables **by index**; read the name only to
   report it, never to find anything.

Numbers cannot add a sheet through AppleScript, so a Numbers
`spreadsheet.create` is single-sheet. That is a real provider limit and must be
declared as one rather than worked around or hidden.

#### Pages and Numbers — COMPLETE

`document/pages.rs` — `document.read`, `document.create`, `document.convert`.
`document/numbers.rs` — `spreadsheet.read`, `spreadsheet.create`.

Both are proven by real E2Es that assert **content**, not `get version`:
`verify/verify_p15_iwork_pages.sh` and `verify/verify_p15_iwork_numbers.sh`,
both in the Office gate.

Shared behaviour, matching the Keynote adapter:

- a document the caller already has open is read where it is and **left open**;
  only documents the adapter opened are closed
- creating over an existing file is refused, never replaced
- the E2Es write under a uniquely named directory in the user's Documents
  folder and remove it. Pages and Numbers are sandboxed; the probe proved that
  location works, an arbitrary temp directory is not known to, and a sandbox
  prompt in an unattended run blocks every later automation

#### The registry no longer overstates iWork

Apple iWork declared `ALL_OFFICE_CAPABILITIES` — all eight — while only Keynote
had an adapter. It now declares `IWORK_CAPABILITIES`, the seven it can actually
execute. **`spreadsheet.edit` is deliberately absent**: Numbers has read and
create adapters but no edit one.

`iwork_declares_only_what_it_can_execute` pins this, including the absence.

#### Routing: dispatch by format, not by priority

The registry is ordered by priority, so Microsoft Office wins every capability
it declares for as long as it is installed. A `.pages` file was therefore handed
to an adapter that could not open it, and `document.read` rejected the extension
outright. **The Pages and Numbers adapters existed but nothing could reach
them.**

`document.read`, `document.create`, `document.convert`, `spreadsheet.read` and
`spreadsheet.create` now dispatch on the file's own extension before the
priority list gets a say. Everything else falls through unchanged, so no
Microsoft path moved.

**`document/resolver.rs` already contains a full format-aware provider
resolver** — candidates, native and import format ranking, fidelity warnings,
an `executable` flag — and **nothing in production calls it**. Only its own
tests do. Wiring it in is the right end state and belongs with the final
Provider Matrix; the extension dispatch is the narrow, testable part of the same
idea, chosen so that eight proven Excel phases did not have to move to make
iWork reachable.

#### Keynote — COMPLETE, and it was already stronger than assumed

`verify/verify_p15_presentation_real_e2e.sh` runs
`document::keynote::tests::keynote_real_e2e`, which was assumed to be another
`get version` check and is not. It already asserts content, and one thing
Pages and Numbers do not:

- create, then read back and assert the title and body actually appear
- **a document the user already had open stays open** — the adapter reads it in
  place and the test proves the document survives
- creating over an existing target is refused, and the existing content is
  re-read afterwards to prove nothing was replaced
- the source is scanned to prove the adapter contains no `quit`

It also writes to `std::env::temp_dir()` and passes, which is worth knowing:
**Keynote reads back from a temp directory without a sandbox prompt**, unlike
Excel. Pages and Numbers were given the more conservative Documents-folder
workspace before this was known; moving them to a temp directory would be a
small, provable improvement, not a necessity.

The gap was never Keynote's evidence. It was that **the verifier belonged to no
gate**. It is now a step in `verify/gate_p15_office.sh`.

#### The presentation route had the `.pages` defect in reverse

`presentation.read` / `presentation.create` resolved straight to Apple iWork
through `resolve_local_presentation_provider`, regardless of the format in
front of them. So a `.pptx` was handed to the Keynote adapter and refused for
not being a `.key`.

Meanwhile **`document/powerpoint.rs` has proven read, create, edit and export
adapters** — `verify/verify_p15_powerpoint_common_capability.sh` passes all of
them — and **no production path called any of them**.

Both presentation entry points now dispatch on the file's own extension, with
`.key` still falling through to Keynote exactly as before. That verifier is
also now a gate step.

#### Three adapters were written, proven, and unreachable

This kept happening, and it is the thing to watch for in the remaining Office
work:

| adapter | state before | reachable? |
| --- | --- | --- |
| `document/pages.rs`, `document/numbers.rs` | did not exist | — |
| `document/powerpoint.rs` | proven by its own verifier | **no** |
| `document/resolver.rs` (format-aware provider resolution) | proven by its own tests | **no** |

A passing verifier proves an adapter works. It does not prove anything can
call it. When closing out the Provider Matrix, check the production call path
for every capability the matrix claims, not just that a test exists.

#### Remaining iWork work

None. Pages, Numbers and Keynote are complete and all three are gate steps.

`presentation.edit` and `presentation.convert` are not Office capabilities at
all, yet `powerpoint.rs` implements both. Either declare and route them or
record why they stay unexposed — that belongs with the final Provider Matrix.

#### An unresolved cleanup for the machine owner

Round 2 of the probe left **two Numbers documents open** before its cleanup was
fixed:

- `未命名` — saved at
  `~/Library/Mobile Documents/com~apple~Numbers/Documents/未命名.numbers`
- `未命名2` — unsaved

They are almost certainly the probe's own (the counts line up exactly), but that
is not certain enough to close or delete someone's unsaved document, so they
were left alone. The probe now closes exactly the documents each candidate
created, by identity rather than by name — an errored candidate leaves an
*unsaved* document, which no name prefix matches, and closing every document
would take the user's own work with it.

### The Office skill is a capability, not six application integrations

This is the correction that reframes all the remaining Office work, and it came
from the owner: *"办公技能是一个通用技能，不能因为遇到某个软件就不能用了"* — the
Office skill is a general capability; it must not stop working because of which
application happens to be installed.

Everything before this was built entirely on desktop application automation. No
Excel meant no spreadsheet capability. No iWork meant no iWork capability. No
WPS meant a sentence explaining that WPS was not installed. That is six
integrations wearing a capability's name.

#### Three layers, chosen by what is available

1. **Application automation** — Excel, Word, PowerPoint, Pages, Numbers,
   Keynote. Highest fidelity: charts, formulas, formatting, sorting. Requires
   the application.
2. **Structured file access** — `document/structured.rs`. `.xlsx`, `.docx` and
   `.pptx` are ZIP archives of XML, so they are read from the file itself with
   **no application at all**. Available on every machine. This is the floor that
   keeps a capability from disappearing.
3. **Cloud fallback** — Google Workspace, for cloud-native resources.

The registry now ranks the structured layer at priority 900, below every
installed application, and `spreadsheet.read` falls to it only when Microsoft
Office is absent. An installed application still reads its own format better;
the point is that its absence is no longer fatal.

#### What proves it is interchangeable rather than merely similar

`document::structured::tests::structured_and_excel_agree_on_the_same_workbook`:
a workbook **Excel itself wrote** is read by Excel and read without Excel, and
the two must agree — escaping (`&`, `<tag>`), booleans, numbers and worksheet
identity included. Anything weaker and "works without the application" is a
claim rather than a fact.

### WPS Office — supported through its files, not through its UI

The earlier instruction was to classify WPS as `APP_NOT_INSTALLED` or
`UNSUPPORTED_BY_PROVIDER_AUTOMATION` and move on. **That answer is wrong for a
general capability**, and the owner rejected it: *"不能因为本机没安装 wps 就写一句
没安装不能做"*.

The right answer follows from what WPS actually is: **WPS's formats are the
Microsoft formats.** A `.xlsx` written by WPS is the same ZIP of XML as one
written by Excel. So:

- **The skill supports WPS files whether or not WPS is installed**, through the
  structured layer. That is real support, not a classification.
- WPS's *desktop automation* still has no published deterministic AppleScript
  contract on macOS, and WPS AirScript is a Kingsoft **cloud** API that must
  never masquerade as a local desktop adapter. So the WPS *provider* declares no
  executable capability — that part of the earlier instruction stands.

The distinction to hold onto: **a provider having no automation adapter is not
the same as a format being unsupported.** The first is about an application; the
second is about the capability, and the capability is what the user asked for.

### Remaining work to finish the general capability

In order, each one widening what works with no application present:

1. `spreadsheet.create` in the structured layer — write `.xlsx` directly
2. `document.read` in the structured layer — `.docx` is `word/document.xml`
3. `presentation.read` in the structured layer — `.pptx` slide text
4. Wire `document/resolver.rs` in. It is a complete format-aware provider
   resolver, with native/import format ranking, fidelity warnings and an
   `executable` flag, **that no production code calls**. The extension dispatch
   now in the gateway is the narrow, testable part of the same idea; the
   resolver is the general form, and it is what lets a request that names no
   format pick the best available provider.
5. Then the final Provider Matrix, built from what is reachable rather than what
   is declared.

#### The pattern that keeps recurring — check for it

Four things in this stretch were found written, plausible, and callable from
nowhere:

| | state found |
| --- | --- |
| `document/powerpoint.rs` | proven by its own verifier, unreachable |
| `document/resolver.rs` | proven by its own tests, unreachable |
| `LocalStructured` provider | four declared capabilities, no implementation, `available: false` |
| Apple iWork declaration | all eight capabilities, one adapter |

**A passing verifier proves an adapter works. It does not prove anything can
call it.** Build the Provider Matrix from traced production call paths.

### Google Workspace — FALLBACK REVIEW REQUIRED

Retain the existing Google Workspace OAuth/API foundation.

Do not rewrite the stable Keychain / OAuth recovery architecture.

Review Docs / Sheets / Slides against the same Office Common Capability contract.

Google Workspace is the cloud fallback when a suitable local provider is unavailable or when the resource is a Google-native cloud resource.

Important boundary:

- local paths are local filesystem resources
- Google resource IDs are cloud resources
- never treat one as the other

### Office provider architecture — KEEP

Retain:

`Office Common Capability Layer -> Provider Resolver -> Concrete Provider Adapter`

Provider resolution must consider:

- requested capability
- resource location
- file format
- installed applications
- provider authorization
- actual executable adapter support
- local-first preference
- compatibility
- native format
- explicit user preference where supplied

A Provider Registry declaration does **not** prove an executable capability.

Unsupported provider-specific functionality must fail closed.

### Office completion requirements

Office must remain **In Progress** until all of the following are accepted:

- provider-neutral Common Capability Layer
- format-aware Provider Resolver
- Microsoft Word common capability
- Microsoft Excel common capability
- Microsoft PowerPoint common capability
- executable Pages common capability
- executable Numbers common capability
- executable Keynote common capability
- WPS deterministic-support classification
- Google Docs fallback coverage
- Google Sheets fallback coverage
- Google Slides fallback coverage
- Provider Matrix verifier
- realistic document workflow
- realistic spreadsheet workflow
- realistic presentation workflow
- permission / confirmation behavior
- regression suite
- frontend build
- scoped diff verification

Only after those gates pass:

- Office -> **Complete**
- P15 -> **6 / 11**

### Recommended remaining Office order

1. ~~Excel Phase C — row/column structural mutation~~ — **COMPLETE**
2. ~~Excel basic formatting~~ — **COMPLETE**
3. ~~Excel sort/filter~~ — **COMPLETE**
4. ~~Excel common chart integration~~ — **COMPLETE**
5. ~~Excel formula-aware read~~ — **COMPLETE**
6. ~~Excel final realistic workflow~~ — **COMPLETE**
7. ~~close Excel Common Capability~~ — **COMPLETE**
8. ~~Pages Common Capability~~ — **COMPLETE**
9. ~~Numbers Common Capability~~ — **COMPLETE**
10. ~~Keynote Common Capability~~ — **COMPLETE**
11. WPS capability classification — NEXT
12. Google Docs/Sheets/Slides fallback review/completion
13. final Office Provider Matrix
14. final provider-neutral realistic workflow acceptance
15. mark Office Complete only after all gates pass

### Safety / implementation rules for the next developer

- Do not touch stable Browser / Keychain / OAuth recovery unless an actual regression is demonstrated.
- Do not reopen Word or PowerPoint without a demonstrated regression.
- Do not reopen Excel Phase A/B without a demonstrated regression.
- Never quit Excel, Word, or PowerPoint as part of an Office operation.
- Close only operation-owned resources.
- Preserve user-owned Office documents/workbooks/presentations.
- Preserve no-overwrite behavior.
- Preserve source files.
- Fail closed on ambiguous identity or unsupported provider automation.
- Use scoped `rustfmt`; do not run global `cargo fmt`.
- Use explicit `git add`; do not use `./done.sh` for scoped Office commits.
- Do not push unless explicitly requested.
- HANDOFF.md remains the authoritative current-status source.

<!-- OFFICE_TAKEOVER_2026-09-02_END -->


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
- Apple iWork native automation is **Implemented; real E2E PASS on 2026-08-31** for Pages, Numbers, and Keynote 15.3.1. Connections rescans each application independently at runtime and reports partial installation accurately. Native sessions remain externally owned; reads close only documents opened by AI-OS and never quit a user-owned application.
- Disconnect is local: My AI removes the Keychain credential and local authorization mapping. AI-OS does not claim a remote Microsoft revoke when the public-client flow provides none.
- Browser authenticated sessions: lifecycle/evidence contract and persistent opaque profile metadata are implemented. Profile metadata is stored atomically in the app-data directory and contains no cookies, passwords, or bearer tokens. Authentication still requires a verified account marker on an HTTPS platform origin; a public page is never authenticated. No browser runtime with a safely callable account-state detector is currently connected, so real session E2E remains SKIP on user login/profile handoff rather than fabricating Connected.
- Consumer eBay is a built-in **Authenticated Browser** provider. Its main Connections row uses the same AI-OS-owned managed browser runtime as Amazon, Taobao, JD, and Pinduoduo; it does not expose eBay developer credentials, RuName, scopes, callback, or Broker configuration. The official OAuth/API connector remains an isolated deferred prototype for future structured seller/enterprise capabilities and is not the P15 consumer shopping connection path.
- eBay Production keyset is **Created**, the Marketplace Account Deletion exemption is **Granted**, and the Privacy Policy remains published at `https://russellchen001.github.io/AI-OS/privacy/` with public contact `aios.privacy@gmail.com`. This infrastructure is retained for the optional Official OAuth/API prototype; public Broker deployment and Production OAuth E2E are explicitly deferred and are not blockers for consumer eBay.
- Amazon Consumer: Product Advertising API is limited to product advertising; no official ordinary-buyer account/cart/checkout/order API is registered. Authenticated Browser is required for those consumer actions. Selling Partner API must not be used as a buyer API.
- Taobao / JD / Pinduoduo Consumer: their open platforms are merchant/service-provider oriented or approval-limited; AI-OS has no approved ordinary-consumer cart/checkout/order API credential. Authenticated Browser is the declared fallback and must verify the signed-in account before authenticated evidence.
- Commerce capability facts were reviewed against official platform documentation on 2026-08-29. Unsupported official consumer APIs are an accurate boundary, not a failed E2E.
- P15 progress is **5 of 11 complete**. The combined Document, Spreadsheet, and Presentation capability area remains in progress under the corrected provider-coverage acceptance rule.

### P15 Office Provider matrix — 2026-08-29

| Provider | Document | Spreadsheet | Presentation | Real E2E / boundary |
|---|---|---|---|---|
| Microsoft Graph | Implemented, OAuth | Implemented, OAuth | Unsupported in current adapter | SKIP — user app registration/consent |
| Google Workspace | Implemented, OAuth | Implemented, OAuth | Implemented, OAuth | **PASS — real identity + Drive + Docs + Sheets + Slides E2E, 2026-08-29** |
| WPS | Backend-broker contract | Backend-broker contract | Backend-broker contract | SKIP — server APPKEY provisioning |
| Apple iWork | Native Pages implemented | Native Numbers implemented | Native Keynote read/create implemented | **PASS — Pages + Numbers + Keynote real E2E, 2026-08-31** |
| Local Structured | Implemented | Implemented foundation | Unsupported | Foundation/local fallback |
| Native Microsoft Office | Implemented | Multi-sheet read + edit foundation, real E2E | Implemented, real E2E | Word/PowerPoint common capability complete; Excel Phase A + Phase B complete |

Office is **In Progress**. The 2026-08-31 closure validated capability categories but did not satisfy required provider coverage. Native Word and PowerPoint common capability are complete and Excel Phase A edit is complete, while Excel row-column/format/sort-filter/chart/realistic workflows, executable Pages/Numbers workflows, WPS automation coverage, format-aware resolution, and realistic cross-provider workflows still require implementation and acceptance. OAuth, SystemPermission, AuthenticatedSession, and UserConfirmation remain separate.

Current executable inventory (2026-09-01): generic macOS `textutil` provides bounded DOC/DOCX read/create/convert; Microsoft Word 16.78 provides real DOCX read/create/edit/table/image/save-copy/PDF-export workflows; Microsoft PowerPoint 16.78 provides real PPTX read/create/edit, slide add/delete/reorder, text-box/image/table/basic-shape, save-copy, and PDF-export workflows; Microsoft Excel provides real XLS/XLSX read/create plus XLSX set-cell/set-formula/clear-cell/save-copy/read-back Phase A and deterministic add/rename/delete/named-sheet mutation; Apple Keynote provides real `.key` read/create; Google Workspace provides official API Docs/Sheets/Slides read/create plus Sheets write/read-back. Pages and Numbers currently have only installation/Automation probes, and WPS is not installed and has no accepted deterministic adapter. Registry declarations and availability probes are not executable evidence.

Technical decision: the Office common capability layer resolves an `OfficeRouteRequest` by concrete application, capability, resource location, file format, installed/authorized state, executable-adapter state, Local First policy, native/import compatibility, and explicit user preference. Cloud resource identifiers can route only to Google Workspace; local paths can route only to local executable adapters. Native format compatibility outranks generic suite priority, and import routes return a fidelity warning. The legacy aggregate `OfficeProvider` description remains temporarily for compatibility but is not completion evidence.

Technical decision: Excel 16.78 `save workbook as` may invalidate the pre-save workbook proxy. Every Excel save-copy must write a unique absolute output, reacquire `active workbook`, verify its fresh `full name` equals that output, and use only this fresh reference for close/read; identity mismatch fails closed. The adapter stages an operation-owned input copy, restores workbook/window counts, never closes user-owned workbooks, never quits Excel, preserves the original, and refuses overwrite.

WPS investigation (2026-09-01): WPS Office is not installed on the acceptance machine. Official WPS macOS material documents the supported Office file formats and interactive shortcuts but does not publish a desktop AppleScript, CLI, or local automation contract. WPS Open Platform AirScript is a separate Kingsoft cloud-document API requiring file/script identifiers and an API token; it is not a deterministic local WPS Writer/Spreadsheet/Presentation adapter. Local WPS common capabilities therefore remain `APP_NOT_INSTALLED` and `UNSUPPORTED_BY_PROVIDER_AUTOMATION`; file-level DOCX/XLSX/PPTX compatibility may be evaluated as a future import route with an explicit fidelity warning, never as fake executable support.

Microsoft Word Common Capability (2026-09-01): real Word 16.78 E2E passes read, create, paragraph/heading edit, 2x2 table write/read-back, bounded inline-image insertion/reopen presence, save-copy, DOCX-to-PDF export, no-overwrite, original preservation, and the combined realistic workflow. `make table at selection` was rejected after real `-10024` evidence; the selected deterministic table path is `make table at text object of selection`. Every operation stages a disposable copy in Word's container, waits for a fresh active document owned by that operation, closes only that document, restores the prior document count, and never quits Word. PDF export uses the scripting dictionary's `format PDF`; DOC-to-DOCX remains SKIP because no safe legacy DOC fixture was available for real validation. `document.edit` and `document.convert` remain one-time-confirmation operations. Office remains **In Progress** and P15 remains **5 of 11** because the full Office Provider Matrix is not complete.

Microsoft PowerPoint Common Capability (2026-09-01): real PowerPoint 16.78 build 01008 E2E passes bounded PPTX read, three-slide create/reopen, title/body edit, slide add/delete/reorder, deterministic text-box/picture/2x2 shape-table/autoshape insertion, save-copy, original preservation, PPTX-to-PDF export, no-overwrite, and the combined realistic workflow. Selected SDEF paths are `make new slide at end of active presentation`, `make new text box/picture/shape/shape table at slide`, table cell access through `table object` → row → cell → shape → text frame, Standard Suite `delete`/`move`, `save ... in (POSIX file ...) as save as Open XML presentation`, and PDF `save as PDF`. Each open waits for exactly one fresh active presentation whose full path matches the staged operation copy; AI-OS closes only that operation-owned presentation, restores the prior presentation count, and never quits PowerPoint. Legacy PPT-to-PPTX remains SKIP because no safe legacy PPT fixture was available. `presentation.read`, `presentation.create`, `presentation.edit`, and `presentation.export` retain one-time confirmation. Office remains **In Progress** and P15 remains **5 of 11**.

Microsoft Excel Phase A + Phase B (2026-09-01): provider-neutral `spreadsheet.edit` routes to the deterministic Microsoft Excel adapter with one-time confirmation. Phase A real landed-XLSX E2E passes bounded typed `set_cell`, bounded `set_formula` with formula/calculated-value read-back, `clear_cell`, unique XLSX save-copy, no-overwrite, original preservation, fresh post-save ownership, output reopen validation, workbook/window restoration, Excel process preservation, and user-workbook preservation. Phase B mutation adds deterministic `add_worksheet`, `rename_worksheet`, `delete_worksheet`, and explicit named-sheet edits. Excel 16.78 does not guarantee a new worksheet's insertion index, so AddWorksheet uses pre/post worksheet identity snapshots plus a unique set difference and never assumes an index. Phase B read extends the existing provider-neutral `spreadsheet.read` Runtime path with worksheet list/count, explicit `sheet`, bounded `sheets`, and bounded `allSheets` selection while preserving the legacy no-selector first-sheet behavior. Named or selected missing worksheets fail closed instead of falling back. Multi-sheet output is bounded to 16 worksheets, 200 rows x 64 columns per worksheet, 256 characters per rendered cell, and a roughly 60k-character protocol budget with explicit truncation metadata. Real Excel E2E passes worksheet list/count, first-sheet compatibility, specific-sheet read, selected multi-sheet read, all-sheet read, missing-sheet failure, workbook/window restoration, Excel-process preservation, and user-workbook preservation. Excel Phase B is **Complete**. Excel Common Capability remains **In Progress** pending row/column, formatting, sort/filter, chart integration, and the final realistic workflow. Office remains **In Progress** and P15 remains **5 of 11**.

### P15 Unified Connections & Account Onboarding — decided 2026-08-29

#### Consumer Account Connection Friction Rule — decided 2026-08-31

- AI-OS must not require an end user to obtain a Developer Account, API key, OAuth application, RuName, callback infrastructure, or deployed backend merely to connect an ordinary consumer website account. Developer credentials never appear in the consumer connection UX.
- Shopping, consumer marketplace, retail, travel booking, and similar account-backed web services should prefer Authenticated Browser when one normal user login can safely provide the required account-state evidence. Official OAuth/API is preferred when AI-OS or the platform can preconfigure one-click OAuth, or for structured/high-frequency API workloads, seller or enterprise workflows, writes/automation, and capabilities that are materially more reliable through an official API.
- eBay consumer therefore uses the shared managed Authenticated Browser runtime. The deterministic Official OAuth/API foundation is preserved but deferred as an optional structured provider; it is not registered as the default consumer eBay connection.

- External Connector Framework is the shared connection architecture for reviewed website/API integrations. Built-in and Custom Provider instances use the same manifest validation, capability state, Broker safety, authorization-reference isolation, disconnect, and removal contracts; adding a future site requires a reviewed manifest, capability mapping, and behavior tests rather than another OAuth/Broker state machine.
- `Add Other Provider` supports Local Application, Website Login, trusted External API Connector, and Unsupported Provider Request. Ordinary users select a type and enter only public application configuration. Arbitrary URLs plus arbitrary JSON are never executable Connectors; unknown or untrusted Providers receive no high-risk capability.
- Provider Definition and Provider Instance identities are separate. Custom Providers persist atomically and survive restart; malformed persisted configuration fails closed. Built-in definitions may be disconnected but not removed. Custom Providers may be disconnected and then removed after confirmation.
- Backend Broker is the only owner of client secrets, certificates, raw tokens, private keys, and authorization codes. Desktop storage is limited to public configuration, Broker URL, opaque environment-scoped authorization reference, capability state, and non-sensitive evidence reference. Production Broker URLs require HTTPS; HTTP is limited to localhost development.
- `Disconnect Account` performs remote Broker revoke/delete before clearing the local opaque reference and capability authorization state. A remote failure retains local tracking and is not reported as fully disconnected. `Remove Provider` is a separate custom-only action that first requires successful disconnect, then atomically deletes its definition, instance, public configuration, session/reference, and non-sensitive cached state.
- The deferred eBay Official OAuth/API prototype retains the reviewed Connector manifest, secure Broker token lifecycle, and independent Browse/Cart/Checkout/Order approval contracts. It is hidden from the ordinary Connections path and does not determine consumer eBay state. Any future purchase execution still requires User Confirmation.
- eBay supports Managed by AI-OS and Self-hosted / Bring Your Own App Broker modes. Managed Broker status is **Not Provisioned** until `AI_OS_MANAGED_EBAY_BROKER_URL` points to a real deployment; no fake address is supplied. Self-hosted desktop configuration contains only Environment, public App ID, RuName, and Broker URL. The administrator configures `EBAY_CLIENT_SECRET`/Cert ID and `BROKER_ENCRYPTION_KEY` only on the independent Broker service.
- The deployable Broker lives at `services/external-connector-broker`. It implements health, reviewed manifest discovery, configuration validation, authorization begin/status/callback, encrypted persistent token storage, refresh and restart re-verification through the Identity API, capability availability/execute, official token revocation, and local token deletion on disconnect. The desktop stores only public application configuration and an opaque authorization reference; raw authorization codes, access/refresh tokens, client secrets, and raw eBay account identifiers do not enter frontend persistence, ordinary logs, Planner, Evidence, Memory, or HANDOFF. Identity verification retains only a stable irreversible account marker inside encrypted Broker storage.
- Broker unit tests and local mock eBay HTTP tests cover exact approved scopes, RuName/state validation and single-use state, failed Identity verification, encrypted restart recovery, expired-token refresh/read-back, and official revocation. They are deterministic prototype evidence only; public Broker deployment and real OAuth E2E are deferred by product decision.
- eBay Browser status is **REAL MAC E2E PASS**. Connect opens `https://signin.ebay.com/signin/` in an AI-OS-owned managed profile. Account verification navigates to the official signed-in-only `https://accountsettings.ebay.com/uas` page and accepts only exact HTTPS eBay origins plus structural account-page evidence; it reads no page text, password, cookie, storage, token, authorization header, or personal account value. Restart reopens the managed profile headlessly and re-verifies rather than trusting persisted state; Disconnect closes only the AI-OS-owned process and deletes its managed profile/session.
- eBay Browser real E2E on 2026-08-31 passed login, manual `I've signed in` account-page verification, restart recovery without a visible browser, Disconnect profile removal, Reconnect, and Connected verification. Automatic post-login detection remains a convenience rather than an acceptance authority; the manual trigger is shown only while Waiting for user and still requires backend evidence.
- Taobao real recovery evidence on 2026-08-31 exposed an intermittent false Expired on the public homepage when signed-in and signed-out navigation structures appeared together. Recovery now uses the signed-in-only `https://i.taobao.com/my_taobao.htm` destination observed in the managed session; deterministic verification remains fail-closed. Post-fix real restart recovery passed and retained **Connected**.
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
| 7 | Document, spreadsheet, presentation | Done | `verify_p15_document_*`, `verify_p15_spreadsheet_*`, `verify_p15_office_provider_registry`, `verify_p15_iwork_real_e2e`, `verify_p15_presentation_real_e2e` |
| 8 | Computer Control | Not started | — |
| 9 | Vehicle Control | Not started | — |
| 10 | Local generative media | Not started | — |
| 11 | Cognitive distillation foundation | Not started | — |

**6 of 11 complete.**

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

Presentation completed on 2026-08-31: Google Slides read/create, read-back validation, OAuth authorization, and real external Google E2E pass; native Keynote read/create, read-back validation, no-overwrite, and existing-document ownership real E2E also pass.

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
| HEAD | `3520e4e` — Generative Media GM-1 complete: Core Domain, execution target, Provider Registry, routing, Runtime bridge, capability-boundary repair, and Shared Runtime Permission Admission |
| Latest tag | `p15-computer-control-v1-complete` |
| Baseline state | Working tree was clean at the stable baseline before this handoff update |
| Active phase | P15 Core Skills — 7 of 10 active capability areas complete. Generative Media — Local First is In Progress; GM-1 is complete and GM-2 is next. Vehicle Control is Deferred / out of v1. |

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
| P15 Computer Control v1 | Deterministic system reads, application lifecycle, clipboard, output audio, native permission inspection/settings handoff, always-confirmed power actions, and PID-reuse-safe process termination. Native notification delivery Deferred / out of v1 after feasibility acceptance. | `f010543`, `96979b9`, tag `p15-computer-control-v1-complete` |
| P15 Generative Media GM-1 | Provider-neutral Generative Media foundation: CreativeIntent/MediaRequest domain, execution targets, replaceable MediaProvider Registry, Local First routing, Runtime-owned execution bridge, stable media capability IDs, restored Download/Media Skill boundary, and Shared Runtime Permission Admission. Production provider registry intentionally remains empty until GM-2. | `346a684`, `a649a83`, `b40467d`, `ffd396e`, `2b2035c`, `00d97ab`, `5f9a6a5`, `3520e4e` |

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

P15 Core Skills remains active at **8 / 10 active capability areas complete**.

Current capability area:

**Generative Media — Complete**

GM-6 is complete. User Do-task execution remains Agent-owned:

`Task Engine → Planner → Runtime → selected Agent → Agent Skill Transport → SkillInvocationGateway → Generative Media backend → MediaRouter → MediaProvider`

`MediaRouter` is a backend router only. Runtime governance and the
`SkillInvocationGateway` permission boundary remain authoritative.

Current Generative Media milestone status:

- GM-0 — Product Contract: Complete
- GM-1A — Core Domain: Complete
- GM-1B1 — Execution Target: Complete
- GM-1B2 — Provider Contract + Registry: Complete
- GM-1B3 — Routing Policy: Complete
- GM-1C — Runtime / execution bridge: Complete
- GM-1C1 — Download / Generative Media capability-boundary repair: Complete
- GM-1D — Shared Runtime Permission Admission: Complete
- GM-2 — ComfyUI Ready Detection + Local Provider: Complete
- GM-3 — One-click Setup / Repair foundation: Complete
- GM-4 — Cloud Providers: Complete
- GM-5 — Prompt / Reference Intelligence: Complete
- GM-6 — Quality Loop: Complete
- Generative Media Closure: Complete

Vehicle Control remains Deferred / out of v1 after its feasibility gate.
NAS Management foundation remains hardware-blocked for real hardware E2E.

---

## Next

P15 active capability status:

Completed:
1. File Management Skill
2. Browser / Search
3. Local Model Management Skill
4. Download Skill
5. Email and Calendar
6. Office workflows
7. Computer Control v1
8. Generative Media

Deferred / removed from active v1 scope:
- Vehicle Control v1 — Deferred after feasibility gate; no adapter implemented

Next P15 Skill:
- **Mano-P / Mano-CUA**

After Mano-P / Mano-CUA:
1. llmfit Local Model Optimization / Recommendation integration
2. Cognitive Distillation completion
3. NAS Management foundation — real hardware E2E remains blocked until NAS storage is installed

Generative Media implementation order:

1. GM-0 — Product Contract — Complete
2. GM-1 — Core / Provider Router / Runtime Permission Foundation — Complete
3. GM-2 — ComfyUI Ready Detection + Local Provider — Complete
4. GM-3 — One-click Install / Configure / Repair — Complete
5. GM-4 — Cloud Providers, credentials, entitlement and budget enforcement — Complete
6. GM-5 — Prompt Intelligence + OSS Reference Analysis — Complete
7. GM-6 — Self-correction / Quality Loop — Complete

Do not begin P16 until the remaining P15 capability foundations are implemented
or explicitly deferred.

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

**Public privacy pages**

- Public legal/integration pages use standalone static files under `public/` and the repository's GitHub Pages workflow. This keeps them independent from the Tauri application and avoids changing Authenticated Browser or runtime connection behavior.

**macOS Provider credentials**

- macOS Keychain remains the only persistent credential authority. Provider secrets use the stable Generic Password service `com.ai-os.provider` and the validated Provider instance ID as the account; existing items require no namespace migration.
- The Rust backend keeps one process-global, process-lifetime, per-account credential cache. Normal startup, My AI/Connections rendering, passive status display, and cached model metadata recovery do not read Provider secrets. The first actual operation requiring an account loads that Keychain item once; later operations in the same process use the shared backend value. Successful writes replace the cached value and successful deletes remove only that account. Secrets never enter frontend state, ordinary JSON, logs, Planner, Evidence, Memory, or HANDOFF.
- `security-framework` 3.7 updates an existing service/account item with `SecItemUpdate`; credential refresh does not delete and recreate the item or change its ACL.
- The previous cache-only mitigation passed deterministic tests but failed real E2E: one dev validation cycle showed 4–5 `com.ai-os.provider` prompts. The machine has six configured Keychain-backed Provider accounts, so a newly rebuilt ad-hoc binary can trigger one authorization per distinct item even though each account is cached after its first read. This disproves the earlier global “at most one prompt” claim.
- Development uses ordinary ad-hoc-signed `tauri:dev`; rebuilds may change the binary `cdhash`. AI-OS does not install a local certificate, alter Keychain ACLs, or preload every Provider credential to hide that behavior. Normal startup and ordinary UI navigation should therefore produce zero Provider Keychain prompts; the first real use of one credential after a rebuild may require authorization for that item, and unused Provider items are not read.
- Stable Apple code signing is a **PRE-RELEASE REQUIREMENT**. A production build must preserve a stable designated requirement so repeated Keychain prompts are not accepted after the necessary first authorization or compatible application update. It is not part of the P15 development workflow.
- Debug builds emit safe `[KEYCHAIN_TRACE]` records only when `AI_OS_KEYCHAIN_TRACE=1`, with service, opaque account hash, caller tag, PID, executable path, timestamp, and cache HIT/MISS. Release builds compile the trace to a no-op. No secret, token, API key, authorization code, username, or email is logged.
- Credential/session recovery has three separate lifecycles. Provider API keys and official Microsoft/Google OAuth credentials are not read during normal startup; their persisted non-sensitive configured/connected metadata remains authoritative until an actual operation lazily reads the corresponding secure item and obtains explicit invalidation evidence. Authenticated Browser providers independently recover through their AI-OS-owned profile, live DevTools channel, and structural account verifier without accessing Provider/OAuth Keychain items. Backend runtime results outrank persisted metadata, which outranks frontend placeholders; a snapshot started before a newer per-provider recovery result cannot overwrite that result.

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

- Explicit Core Skill actions enter through Task Engine and Planner and resolve through the existing Skill Registry. OpenClaw-backed skills continue through Runtime → OpenClaw, while Generative Media is a Runtime-owned Core Skill and executes through its own Generative Media Executor / MediaRouter path without entering OpenClaw Gateway.
- PlanStep `user_confirmed` remains current-execution approval and does not modify Trusted Automation. Runtime now owns the shared capability-permission decision used by both OpenClaw-backed skills and Generative Media. Existing configured Trusted Automation capability strings preserve their previous behavior; Generative Media `media.*` capabilities are always-confirm during GM-1 and cannot be silently authorized by persistent Trusted Automation.
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

**Generative Media — GM-1**

- Product identity is **Generative Media — Local First**. AI-OS is a Generative Media Orchestrator, not a ComfyUI controller and not a single-provider integration.
- v1.0 product support remains macOS-only. Shared Generative Media contracts remain platform-neutral for future Windows, Linux and HarmonyOS adapters. Architecture-ready does not mean Product-supported.
- Stable execution boundary: `Planner → Skill Resolver → Shared Runtime Permission Admission → Generative Media Executor → MediaRouter → MediaProvider`.
- Generative Media is Runtime-owned and must not be implemented inside `OpenClawGatewayExecutionAdapter`.
- Provider identity/authentication infrastructure may be shared where appropriate, but media execution routing remains separate from ordinary AI Center LLM routing: account identity may be shared; execution is not shared.
- Stable external capability IDs:
  - `media.text-to-image`
  - `media.image-edit`
  - `media.text-to-video`
  - `media.image-to-video`
  - `media.reference.image.analyze`
  - `media.reference.video.analyze`
  - `media.reference.generate`
- Generative Media Skill id is `generative-media`; executor kind is `media`; handler is `generative-media`.
- `SkillManifest.permissions` remains descriptive metadata in the current architecture. Real execution admission is enforced by the Runtime capability-permission boundary, not by merely declaring `permissions: ["media.execute"]`.
- GM-1 media capabilities are always-confirm. Persistent `trustedAutomationCapabilities` cannot silently authorize them even if a media capability string is present. GM-4 owns later paid-cloud budget and spending authorization semantics.
- Permission denial occurs before Generative Media execution admission. Unknown media capabilities fail closed.
- Manual Provider selection is exact and has no fallback. Local First does not silently fall from local to paid cloud. A Provider policy rejection terminates that route and must not trigger automatic Provider switching to evade policy.
- Local First routing and provider selection live in the Generative Media domain.
- The production `MediaProviderRegistry` is intentionally empty at GM-1 completion. The first real local Provider belongs to GM-2.
- Output and reference handles remain opaque and path-neutral. Shared contracts do not expose Unix-only absolute-path assumptions.
- Download and Generative Media namespaces are separated by regression coverage: Downloads owns `download.*`; Generative Media owns `media.*`.
- GM-1 closure commits:
  - GM-0 Contract: `346a684`
  - GM-1A Core Domain: `a649a83`
  - GM-1B1 Execution Target: `b40467d`
  - GM-1B2 Provider Registry: `ffd396e`
  - GM-1B3 Routing Policy: `2b2035c`
  - GM-1C Runtime Bridge: `00d97ab`
  - GM-1C1 capability-boundary repair: `5f9a6a5`
  - GM-1D Shared Runtime Permission Admission: `3520e4e`
- Next milestone is **GM-2 — ComfyUI Ready Detection + Local Provider**. GM-2 implements Ready First state detection and the first real local execution Provider; GM-3 owns one-click installation/configuration/repair and GM-4 owns cloud execution.


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
- 2026-09-06  Generative Media GM-1 formally completed at `3520e4e`. GM-1 establishes the provider-neutral media domain, execution targets, replaceable Provider Registry, Local First routing, Runtime-owned execution bridge, stable `media.*` capabilities, Download/Media namespace separation, and Shared Runtime Permission Admission. Generative Media remains outside OpenClaw Gateway. `SkillManifest.permissions` remains descriptive metadata; real admission is enforced by the Runtime capability-permission boundary. All seven media capabilities require confirmation attached to the current execution during GM-1, and persistent Trusted Automation cannot silently authorize them. The production MediaProviderRegistry intentionally remains empty, so this milestone does not claim real image/video generation. P15 remains 7/10 active capability areas complete; Generative Media remains In Progress. Next is GM-2 — ComfyUI Ready Detection + Local Provider.
- 2026-09-06  Generative Media GM-0 product contract established. AI-OS v1.0 remains macOS-only while shared Generative Media contracts are required to stay cross-platform-safe for future Windows, Linux and HarmonyOS adapters. Generative Media is Local First / Ready First with ComfyUI as the default local execution engine but not a hardcoded product dependency. Local readiness requires a usable API, compatible workflow and assets, integrity checks and successful smoke output rather than mere installation. AI-OS owns one-click install/configure/repair plus model, LoRA, node and workflow management for ordinary users. Cloud generation uses a replaceable Provider Registry and the user's own provider credentials, with current recommendation metadata allowed to prefer xAI while preserving explicit user selection. Prompt engineering is an implementation detail; CreativeIntent and provider-specific Prompt Compilers sit above execution. Reference analysis is adapter-based and should reuse strong OSS rather than reimplement mature reverse-prompt systems. Local refinement follows user retry limits; cloud refinement follows hard user-defined budget. Implementation order is GM-0 Contract -> GM-1 Core Router -> GM-2 ComfyUI Ready/Provider -> GM-3 One-click Setup/Repair -> GM-4 Cloud Providers -> GM-5 Prompt/Reference Intelligence -> GM-6 Quality Loop.
- 2026-09-06  Vehicle Control v1 feasibility gate completed before implementation. Tesla Fleet API is rejected as a v1 dependency under the zero-cost / zero-owner-admin rule; the official local BLE path was also not selected because its nearby-control subset does not provide enough incremental AI-OS product value over the Tesla app to justify a full Vehicle skill. Vehicle Control is Deferred / out of v1 with no adapter code written. P15 active scope changes from 11 capability areas to 10, so current progress is 7/10. Local Generative Media is next.
- 2026-09-06  Computer Control v1 formally closed at `f010543`. Final scope: deterministic machine/process reads, application lifecycle, clipboard, output audio, always-confirmed Power, native permission inspection/settings handoff, always-confirmed PID-reuse-safe process termination, and final cross-capability Runtime workflow. Native notification delivery is Deferred / out of v1 after real acceptance exposed developer-side signing/certificate administration outside the owner's product constraints. P15 was 7/11 at Computer Control closure; Vehicle Control was subsequently removed from v1 scope, making the active P15 scope 7/10. Standing admission rule: new capabilities must pass a zero-cost / zero-owner-admin feasibility gate before implementation; paid developer/API dependencies, owner-managed certificates/signing, and specialist OS administration are not acceptable v1 requirements. Vehicle Control subsequently completed that feasibility gate and was Deferred / out of v1 before adapter implementation; Local Generative Media is next.
- 2026-09-05  Computer Control Phase B: applications, addressed rather than operated. `system.app.list`, `system.app.running`, `system.app.launch` and `system.app.quit`. Nothing in the module sends a keystroke or looks at a window, so starting an application belongs to it and pressing a button inside one does not. THREE defects in this phase were the same mistake wearing different clothes, and that is the lesson worth keeping: each matched an incidental detail instead of what is actually guaranteed. The identifier check listed the ways an input could be BAD -- a slash, a space -- and `Safari浏览器` walked through, having neither. The `mdfind` parser split on a run of four spaces, a width nobody had measured; it now splits on the attribute NAMES, which is the part `mdfind` promises. And the identifier check then required a dot, on the theory that identifiers are reverse-domain names -- until this machine turned out to have an application whose identifier is `MacNetPlayer`, which made a real, launchable application unaddressable. That last one is the instructive one: shape CANNOT tell an identifier from a display name, because `MacNetPlayer` is both shapes at once. The check now establishes only that a value is safe to hand to the platform -- ASCII, no whitespace, no slash, no leading or trailing dot -- and whether anything answers to it is settled by the machine, which `open -b` does by exiting non-zero for an identifier nothing is installed under. All three were caught by their own tests on the gate rather than by review. Two probes were needed for the same reason: the FIRST measured its own bugs on the two questions that mattered most -- an exit status read at the end of a pipeline, so it was `sed`'s and not `open`'s, and an AppleScript with a C-style ternary that failed to compile and said nothing about permission. What they established: `open -b` exits 0 and prints nothing when the application was ALREADY running, so a launch cannot say whether it started anything, and `alreadyRunning` comes from looking before and after; and `quit` REPORTS SUCCESS for an application that is not running at all, so its return value is worth nothing and every outcome is decided by observation instead -- exactly the rule the Office adapters follow when they count documents. Quitting something not running is refused rather than reported as a successful quit. Quit ASKS: an application showing a "save your changes?" sheet is left showing it and reported as still running, because unsaved work is the person's. Listing running applications has two paths: System Events is documented and needs Accessibility, which this machine grants and another will not, while `lsappinfo` needs nothing and is undocumented. Preferring the first and falling back to the second means the capability does not vanish on an ungranted machine, and refusing only when both fail means it never reports an empty desktop that is not empty. The documented permission-free route is `NSWorkspace.runningApplications`, in-process work belonging to Phase D; the contract will not change when the implementation does. Seven of this machine's 499 installed bundles declare no identifier at all -- counted in the warnings rather than listed, because nothing could address them.
- 2026-09-05  Computer Control begun, Phase A: reading the machine. The owner's boundary is what the module is written against and it is sharper than it sounds -- Computer Use owns the screen, OpenClaw owns agency, and Computer Control owns neither: each capability takes a defined input, does ONE thing, and returns a defined result. "Set the output volume to 30" belongs here; "put the machine into a state suitable for a meeting" is a plan and does not. Listing running applications belongs here; pressing a button inside one does not, whatever it would accomplish. Nothing in the module reads the screen and nothing in it loops. Phase A is `system.storage`, `system.cpu`, `system.memory`, `system.network`, `system.process.list` and `system.process.info`, all reads, all through `sysinfo` -- one pure-Rust crate covering macOS, Windows and Linux, and therefore HarmonyOS, which is Linux underneath. So this capability area starts on the right side of the release rule rather than owing the gap list a line, and the tests prove it by passing on Linux, which is not the system they were written on. `verify/probe_computer_control_semantics.sh` was run before any of it was written and found four things that would have shaped it wrongly. First, APFS reports one container through several volumes: this machine lists `/` and `/System/Volumes/Data` at 460 GiB EACH, so adding them claims 920 GiB of disk that does not exist -- figures are reported per mount point, never summed, and `sharesContainerWith` names the mounts a volume's numbers are indistinguishable from. Second, a mounted installer volume reported zero total space, so `measured` is a field and an unmeasured volume carries NO size numbers at all rather than a zero it never had. Third, `sysinfo` on an unsupported system does not fail -- it returns empty values, a machine with no disks and no memory -- so every entry point refuses outright rather than passing that off as a reading. Fourth, processor use is a difference between two samples, so the capability takes both itself and says so, and is deliberately the slow one. The probe also settled two things NOT in Phase A. `display notification` through osascript posted nothing and reported no error on this machine: a silent failure, so notifications will be posted by the application itself, which also fixes the identity a person would see. And TCC permissions cannot be read this way at all -- `CGPreflightScreenCaptureAccess` is a C function the JXA bridge does not expose and `AVCaptureDevice` did not load -- but the deeper problem is that TCC is per-binary, so a shell probe reads the SHELL's permissions and says nothing about AI-OS's. That area must be in-process Rust. Worth knowing while it is: `check_macos_mail_calendar_permissions()` currently returns a hardcoded `NotDetermined` while describing itself as a detection foundation, so there is no working precedent to copy. One structural decision, taken because of what the Office work cost: the capability list, the dispatcher and the permission gate are wired together by tests from the first commit rather than after the sixth accident -- `every_declared_system_capability_is_dispatched` walks the module's own list, and the authorisation guard now walks it too.
- 2026-09-05  Office marked Complete and P15 moved to 6 / 11 -- but only after checking the twenty written completion requirements one at a time, which is the whole reason to have written them down. Nineteen were already accepted. The twentieth, "permission / confirmation behavior", was not, and finding out how it was not is the entry worth reading. `ConfiguredCapabilityPermissionGate` carries a hardcoded list of what a person may authorise for a single run; anything absent from it is DENIED, and the configurable allow-list beside it defaults to empty. TWELVE capabilities the resolver could route were missing from that list: `document.merge`, `document.split`, `document.rotate`, `document.encrypt`, `document.decrypt`, `document.annotate`, `document.fill`, `document.redact`, `document.stamp`, `document.replace`, `spreadsheet.convert` and `presentation.convert`. Every one of them had a working adapter, a route through the resolver, a row in the traced matrix and a passing gate step -- and none of them could run, because nothing could say yes to them. That is the SIXTH time in this work that a working adapter has been reachable from nowhere, and the first five were each found by hand. Ten of the twelve were added in the past few days by the author of this entry, who wired them into the gateway, the resolver, the registry, the matrix and the gate, and not into the one place that decides whether a thing may run at all. The existing tests could not see it: they exercise adapters and routing, and "may this run" is asked somewhere else entirely. The list is now a named constant and `every_capability_the_resolver_can_route_is_one_a_person_can_authorise` walks every capability the candidate table declares and fails if the gate would deny it -- verified to actually fail by removing one entry and watching it go red, because a test that has never been red has proven nothing. The check is deliberately one-directional: the permission list also carries filesystem and download capabilities the Office resolver knows nothing about, and demanding an exact match would assert something false. The general lesson, now instrumented rather than remembered: a capability is not reachable because its adapter works, or because it routes, or because a gate step passes. It is reachable when every layer between the caller and the adapter admits it exists, and the only cheap way to know that is a test that walks the layers rather than a person who remembers them all.
- 2026-09-05  A test that had been passing on luck for weeks brought the gate down, and it was this repository's own documented trap. `word_realistic_workflow_real_e2e` wrote its image fixture into `std::env::temp_dir()` and handed that path to Word. A sandboxed Office application has implicit access to what it wrote itself and to its own container and to NOTHING else, so a file passed to it from a temp directory raises a "grant access to this file" panel that no automated run can answer -- and that panel then blocks every later automation of that application until a person dismisses it. `structured.rs` carries a comment saying exactly this, written when the same trap cost an Excel timeout and a PowerPoint crash; the Word test simply never got moved. It surfaced as `AppleEvent timed out (-1712)` after 123 seconds, on the run immediately after three green ones, which is the useful part: three passes proved nothing, because the failure needs the panel to actually appear rather than the grant to be remembered. Both fixtures now live in their application's own container, the one place that never asks -- Word's through `word_cache_output`, PowerPoint's through `cache_path`, the same helpers the real inputs already use. PowerPoint's was the same latent bomb and was still passing; it is fixed in the same pass rather than left to go off later. If a run has already raised the panel, quitting the application clears it -- the code change stops it being raised again, it cannot dismiss one that is already up.
- 2026-09-05  The interfaces for a cross-platform release, with the cross-platform work itself deliberately left to a later version. The owner's decision: v1.0 ships macOS only, but the seams are shaped now so that Windows, Linux and HarmonyOS are a continuation rather than a rewrite. Three things. First, `Platform` is a declared field on every candidate instead of a fact hidden inside `app_exists("/Applications/Microsoft Word.app")` -- which is false on Windows for the same reason it is false on a Mac with no Word, and those are not the same fact. `available` (installed here), `executable` (this build has an adapter) and `platforms` (can exist on this system at all) are now three questions instead of one. Checked rather than assumed: HarmonyOS reports `target_os = "linux"` with `target_env = "ohos"`, confirmed with `rustc --print cfg --target aarch64-unknown-linux-ohos`, so the operating system alone cannot tell it from Linux and `Platform::current()` looks at the environment too. Second, the gap is written into a test rather than remembered: eight capabilities have NO portable floor today -- `document.create`, `document.edit`, `document.convert`, `presentation.create`, `presentation.edit`, `presentation.convert`, `spreadsheet.edit`, `spreadsheet.convert` -- and the test asserts exactly that list, in both directions. Adding a ninth means deliberately adding a line; building a floor means DELETING one. It is not "assert everything is portable", because a test that fails from the day it is written teaches nobody anything. Third, and this is the part that earned its keep: the Windows adapter was designed on paper and checked against the interface, because an interface with only one implementation is usually wrong. Four findings held. Every public adapter signature is already platform-neutral -- `fn(&Value) -> Result<Value, XError>` -- and every line of AppleScript is private to a function body, so a Windows adapter is a body swap and not an interface change. Application detection is a one-line `cfg` swap (a registry lookup rather than a path). The sandbox staging into `~/Library/Containers/...`, which exists because a file handed to a sandboxed Office application from a temp directory raises a grant panel no automated run can answer, is private to each adapter and simply becomes a temp directory on Windows. And `structured.rs` was checked to be genuinely portable: every macOS reference in it is inside `#[cfg(target_os = "macos")]` TEST code, so the floor really is a floor. Two findings did not hold, and both are recorded as work rather than fixed here. `run_osascript` is duplicated SEVEN times, once per module, each with its own error type -- on Windows that becomes fourteen, so a later version wants one automation runner with a per-platform body and one error mapping. And `OfficeApplication::MacosNative` names a platform in the routing vocabulary itself: it is `textutil`, which is a FALSE dependency, since `.docx` is a ZIP of XML this repository already reads. When the portable floor replaces it the variant should disappear rather than grow a Windows twin.
- 2026-09-05  A PDF can be turned back into an editable document, which closes the last thing the PDF capability could not do -- and it is the one part of it that genuinely needs an application. Everything else here is done to the file in Rust on any machine; re-typesetting cannot be, because a PDF has no paragraphs to reflow, so the page has to be laid out again. `verify/probe_pdf_to_editable.sh` established on this machine that Word opens a PDF under AppleScript, saves it as a real `.docx` (ZIP magic bytes) and returns the text whole. Word's adapter is now a general converter rather than a PDF exporter: `pdf→docx`, `doc→docx`, and `doc/docx→pdf`. That required a ROUTING decision the owner took: Word could not declare it writes `.docx` without also winning `doc→docx` from `textutil`, and the resolver has no way to say "only when the source is a PDF". The choice was to make Word genuinely perform every conversion it declares rather than to split the capability under a second name -- the same rule that removed six declared-but-unreachable adapters, applied in the mirror direction. `docx→doc` and `rtf→docx` stay with macOS's own conversion, because Word's adapter does not write the old binary format and does not read RTF, and the gateway test that covered the textutil path was pointed at `docx→doc` so it still tests what it was written to test. Two things are deliberately NOT claimed. The identity check that stops the adapter saving over whatever else the person had open is dropped for PDF sources only, because Word renames a document it converted from one and the copy is no longer identifiable by path -- the document count still has to move by exactly one. And converting out of a PDF returns a warning saying the words come back and the page does not, because a caller who is not told will believe the layout survived; the end-to-end asserts that warning is present, asserts the ZIP magic bytes, and asserts the sentence survives, but deliberately asserts nothing about layout. The probe's own fixture was a plain-text PDF, so it proved the MECHANISM and not the fidelity, and that distinction is why the warning exists rather than a fidelity claim.
- 2026-09-05  Recorded before it is worked on, because it changes the shape of everything after it: the owner intends to RELEASE AI-OS and it must not require a particular operating system -- macOS, Windows, Linux and HarmonyOS. Measured rather than assumed, the current state is not "mostly portable with gaps": the bundle targets are `['app', 'dmg']`, there are 41 `cfg(target_os = "macos")` sites and none for any other system, 14 `osascript` call sites, 6 `textutil` and one `cupsfilter`. Three KINDS of platform dependence need three different answers and must not be lumped together. A FALSE dependency uses a platform tool for something that is portable anyway -- `textutil` converting `.docx`, which is a ZIP of XML this repository already reads -- and must be eliminated. A REAL dependency drives an application the person installed, and should NOT be eliminated: a person with Word should get Word's fidelity, so it wants one adapter per platform (AppleScript on macOS, COM on Windows), which the resolver already has the shape for. And some things are NATURALLY exclusive -- iWork exists only on macOS, and its absence on Windows is a fact rather than a defect. The release rule that follows is one line: every capability must have a pure-Rust floor, and platforms and applications are enhancements on top of it. That is the shape `pdf.rs` and `structured.rs` already have; it is simply not spread across everything yet. On the four targets there are only three runtime shapes -- macOS drives applications, Windows can once its adapters exist, and Linux and HarmonyOS have NO office application to drive at all -- which turns the floor from a principle into a hard requirement and puts portable `docx`/`pptx` creation and editing on the critical path, with portable PDF export as the hardest piece because it needs real typesetting. Checked rather than remembered: Rust's OpenHarmony targets are Tier 2 WITH host tools, so the pure-Rust core compiles for HarmonyOS today, while Tauri's OpenHarmony support merged on 2026-06-08 into wry's `feat/open-harmony` branch and not into a stable release -- so the core can go there and the shell cannot yet, and that half is upstream's timing, not ours. The first thing to build is not code but a test: enumerate every capability and assert each has at least one candidate that is not macOS-only, so "it started depending on the system again" is caught by the gate instead of by whoever remembers. That is the same instrument as the traced provider matrix -- principles held by tests rather than by comments.
- 2026-09-05  Changing what a PDF says, which is the last thing the skill could not do, on the owner's choice of scope. `document.redact`, `document.stamp` and `document.replace` are new, and under them sits the piece that made them possible: `document/pdf_layout.rs`, which walks a page's content stream the way a reader does -- text matrices, spacing parameters, font widths -- and reports every run and every GLYPH with where it sits. `extract_text` gives words and not one thing about position, and covering a phrase, replacing one, or deciding whether replacement text fits all begin with its box. It was calibrated against `pdftotext -bbox` as an independent oracle: four lines including two of Chinese matched to three decimal places, and `"2024-0117"` was located at 163.745..224.407 where pdftotext reports 163.745118..224.407227. Redaction removes the drawing instructions for the glyphs and only THEN covers the place -- the usual way this is done wrong is a black box over live text, which any reader can still select and `pdftotext` prints, and that has leaked real documents. Letters that stay do not move: a removed glyph becomes a kerning step of exactly its own width, and any kerning the run already carried is put back with it. Replacement is set in the font the old words were set in, so it looks like the document rather than a patch; an embedded subset carries only the glyphs the document already used, so a character it lacks is refused BY NAME rather than drawn as a blank box. A longer replacement is refused by default with the number of points it did not fit by, because a PDF does not reflow -- `whenLonger` can ask for it to be shrunk to the old width or to push the rest of the line, and both say so in the warnings. Five things were learned the hard way. A base-14 font may declare no `/Widths` at all, so the position arithmetic is a GUESS, and a guess moves every letter that stays -- runs carry `measured` and rewriting refuses when it is false. The first Chinese document crashed the match scan outright: it advanced through the text one BYTE at a time and stepped into the middle of a character, which an English-only fixture can never reach; it is `match_indices` now, and a Chinese fixture guards the path permanently. That fixture was itself broken on the first run -- a CMap parser wants exactly `endcmap / CMapName currentdict /CMap defineresource pop`, and without it the text decoded to nothing -- caught only because the fixture asserts it reads back as the Chinese it was built from before anything relies on it. Emitting the replacement as its own `TJ` made the text layer read `Invoice  2026` with two spaces, so a search for `Invoice 2026` would no longer find it; at the same size the new letters now join the run already being built, and only the shrink case, which needs its own `Tf`, splits the run. And a correction to what this entry's author said out loud a day earlier: `ttf-parser` is NOT needed for any of this. The character-to-code direction comes from the font's own `/ToUnicode`, reversed; a character absent from that map is almost certainly absent from the embedded subset too, so parsing the font program would only confirm the bad news. `lopdf`'s `embed_image` feature was taken on so a signature or seal can be stamped -- a pure-Rust decoder, the same trade as `zip` and `lopdf` themselves. Everything was checked against `qpdf` and `pdftotext` on a reportlab-generated file with embedded subset fonts and mixed Chinese, not only against fixtures this repository builds. What is still NOT possible is re-typesetting -- changing a paragraph, changing the layout -- which needs the page laid out again by a word processor; `verify/probe_pdf_to_editable.sh` asks this machine whether Word can do it, so that path can be wired the way OCR is: used where the platform offers it, honestly absent where it does not.
- 2026-09-05  PDF finished as a working tool rather than a reader, on the owner's list: *"页面级补齐,批注/表单,pdf 加密/解密"*. `document.rotate`, `document.encrypt`, `document.decrypt`, `document.annotate` and `document.fill` are new, all in Rust on the file itself, so none of them asks what the person has installed or what system they run. Reordering pages and deleting them turned out to need nothing built at all -- `document.split` is "take these pages, in this order", so `3,1,2` reorders and leaving a page out deletes it -- which is why only rotation was missing from the page-level work. Rotation is RELATIVE, because that is what "turn this 90 degrees" means to someone looking at a page that is already sideways. Four things were learned the hard way and each would have shipped as a silent lie. First, `EncryptionState::try_from` refuses a document with no `/ID`, and plenty of files written by simple tools have none, so one is generated rather than the request being refused for a reason the person cannot act on. Second, and this is the dangerous one: an encrypted PDF hands `Document::load` an EMPTY document, not encrypted objects waiting to be unlocked, so `load` then `decrypt` produces a document with nothing in it AND NO ERROR -- the password has to go in at load time, through `load_with_password`, and the password is checked explicitly afterwards because a wrong one also merely looks empty. Third, opening with a password leaves `/Encrypt` in the trailer describing a state the document is no longer in, so it is dropped, and `max_id` is corrected with it or every reader warns that the object count is wrong. Fourth, a checkbox does not turn on to `/Yes`; it turns on to whatever name the document put in its appearance dictionary, so the on-state is read out of the file instead of guessed -- guessing produces a box that silently stays empty. Filling also sets `/NeedAppearances`, without which a filled form arrives looking untouched. Annotation is deliberately notes only: a `/Text` annotation is the one kind every reader draws for itself, and a highlight without a hand-built appearance stream is a mark some readers show and others do not. `document.read` now also returns what is written ON a document -- its annotations and its form fields with their current values -- and a `/Widget` is excluded from the annotation list, because the face of a form field is not a remark anyone made. The tests are ordinary tests, not macOS ones, and they build their own fixtures with `lopdf`: a module whose whole claim is that it needs no platform cannot prove that with a fixture only one platform can build. Everything was checked against `qpdf` and `pdftotext` as well as against itself -- the lock reports AESv2 to a third-party tool, `pdftotext` refuses the file without the password and reads it with one, and the filled form shows `/V (Ada Lovelace)`, `/AS /On` and `/NeedAppearances true` in the bytes.
- 2026-09-05  PDF made portable, on the owner's correction: *"你不能要求用户用什么系统"*. The first version drove macOS PDFKit and Vision, which made a whole capability depend on which operating system the person runs -- the same mistake as making it depend on which application they installed, and rejected for the same reason. PDF is a FORMAT, not an application, so it belongs in the same layer as `structured.rs`: the file read in Rust, on any machine. `document.read` on a .pdf, `document.merge` and `document.split` are now `lopdf` and nothing else, and the PDF candidate moved from the macOS-conversion provider to `LocalStructured`, which is what "needs no application AND no particular platform" means. Recognition is the one part that genuinely cannot be portable -- reading words out of a picture needs a trained model, and no such thing is guaranteed on an arbitrary machine -- so it is an ENHANCEMENT: where the platform offers one it is used, where it does not the page is reported as having no text of its own and everything else still works. Two real defects were found on the way, and the second is the one worth remembering. First, a page can INHERIT `Resources`, `MediaBox`, `CropBox` and `Rotate` from its page-tree node, so re-parenting it under a merged tree silently loses its fonts; `flatten_inherited` copies those down before a page leaves its own document. Second, and this is the one that nearly shipped as a corrupt merge: `extract_text_chunks` FLAT-MAPS, so one page can produce several chunks and the results do not line up one-to-one with the pages asked for. Indexing into a multi-page call attributes one page's text to another, which appeared as a merged page reading as empty while its content stream plainly contained the words -- pages are now extracted one at a time. It was found by asking, in order, whether each source read on its own, whether one source alone survived assembly, and what the merged page's content stream actually said; the answer that broke it open was that page two's stream literally began `(Brav`. Office gate is 27 steps, all green, and `lopdf` is the second dependency (after `zip`) taken on to keep a capability from depending on a platform.
- 2026-09-05  PDF, which the owner spotted was missing: the skill could WRITE a PDF from every application it drives and READ none, so handing it a PDF got nothing. `document/pdf.rs` closes that with no application and no installed dependency, because PDFKit and Vision are part of macOS. `document.read` accepts `.pdf` and returns text per page; `document.merge` and `document.split` are new and are PDF-only, since PDF is the one format no Office or iWork application on this machine reads. The part worth keeping is how a scan is handled. A page that is only an image returns ZERO characters from PDFKit, and returning that as an empty document would be a silent lie, so such a page goes to Vision -- and the result marks every page `text-layer`, `ocr` or `image-only`, because recognised text and text the file declares are not the same kind of evidence and a caller may need to know which it holds. Recognition is on by default (a caller asking to read a document means the words in it) and can be turned off, in which case the scan is honestly empty and says why. A locked PDF is refused rather than read as empty. Five probe findings shaped this and three of them contradicted what I was about to write. Vision runs SYNCHRONOUSLY through `performRequests`, so recognition fits the way every other adapter here works, with no callbacks. `cupsfilter` turns plain text into a real text-layer PDF with no application, which is what lets the end-to-end build its own fixtures. A PDFKit FreeText annotation produces NO text layer, so a fixture built that way would have made every text-layer assertion vacuous -- that one was caught only because the fixture asserts its own premise before the test uses it. `NSAttributedString.alloc.initWithStringAttributes` does not exist in JXA; `NSString.drawAtPointWithAttributes` does. And the JXA bridge hands back page counts that JSON renders as STRINGS, so they are coerced with `Number()` -- a caller doing arithmetic on `pageCount` would otherwise get `"2" + 1 = "21"`. The end-to-end proves recognition against text it generated itself, so what OCR must recover is known exactly rather than judged by eye. Office gate is 27 steps, all green.
- 2026-09-04  Word now writes the format the extension promises, which removes the last carve-out in Office routing. `create_word_document` saved every path as `format document default`, so a `.doc` request produced DOCX bytes under a `.doc` name -- a file that opens, and is a lie about itself. It had been worked around by excluding `.doc` from Word's `document.create` arm; a carve-out around a defect is not a fix, and the defect is now fixed. The format follows the extension, and the evidence is the magic bytes rather than the file name, because the name is exactly what was wrong: a `.docx` must start `504b` (ZIP) and a `.doc` `d0cf11e0a1b11ae1` (OLE2), both are then read back through Word, and a format Word does not write is refused rather than mislabelled. Two AppleScript lessons from `verify/probe_word_save_formats.sh`, each of which cost a round. The enumerator is `format document97` with NO space -- `format document 97` is a compile error, and because three candidates were being tried in ONE script that one error killed the other two branches and made all three look rejected, so a probe asks one thing per script. And `format document default` is a SINGLE enumerator name: reading it as `format document` plus a `default` parameter leaves a stray identifier and produces the same syntax error wearing a disguise, which is what made three correct enumerator names look wrong on the next round. Word's own dictionary settled it, and `sdef` needs Xcode -- the file is readable directly at `Contents/Resources/Word.sdef`. Office gate is 25 steps with one more Word step, all green.
- 2026-09-04  Correction, and the last route aligned. Two earlier entries and a comment in `document/resolver.rs` said `document.convert` was deliberately not routed through the resolver because the providers disagreed about what conversion meant. That stopped being true the moment the owner settled it -- the destination decides -- and `document.convert` has routed through the resolver since the conversion work landed, with eleven rows in the traced matrix covering .docx to .pdf through Word, .docx to and from .pages through Pages, and .doc to and from .docx through macOS conversion. The stale comment is corrected in place rather than deleted, because a comment outliving the code is the exact failure the traced matrix exists to prevent, and this one had me report the wrong thing to the owner twice. The capability that genuinely was NOT aligned is `document.create`, and it is now: it was the last Office entry point still dispatching on the file extension, which meant that on a machine with Word installed a `.docx` was still created by plain-text conversion while `document.read` on the same file went to Word. Creating follows reading, so Word answers for its own format whenever it is installed -- with one deliberate exception. Word's create adapter saves DOCX bytes whatever the path is called, so a `.doc` request would get a mislabelled file; macOS conversion picks its format from the extension and genuinely writes the old binary format, so `.doc` is left to it rather than quietly mislabelled. That asymmetry is in the code as a guard with the reason next to it, not left to be rediscovered. Every Office capability now routes through one resolver: document read/create/convert/edit, spreadsheet read/create/edit/convert, presentation read/create/edit/convert, local and cloud. Nothing dispatches on an extension by hand any more.
- 2026-09-04  NOT YET GATED, recorded so it is not lost: a routing decision must not read the Keychain. Making Google Workspace a routing candidate meant asking whether its credential existed, and that turned out to be wrong twice over. A routing decision should not perform a privileged read at all; and on macOS every Keychain read from a REBUILT binary raises an authorization panel, so merely deciding where to send a request began prompting the person on every compile. The consequence was worse than the nuisance: a run that died holding an unanswered panel blocked every later Keychain read on the machine invisibly -- the next run hung for nine minutes with no prompt displayed and no output, and a bare `security find-generic-password -w` hung identically. Nothing recovers from that state except killing the stuck processes, and while a panel is frontmost no other window can be clicked at all, so an unattended session cannot clear it and has to wait for the runner's own watchdog. `provider_credential_present` is therefore replaced by `provider_instance_connected`, which reads the provider configuration file: configuration says whether the account was set up, and whether the credential is still good is reported by the call that uses it, which is the only place that can know. An expired or refresh-needed instance still counts as a candidate, because the request is what finds out and its error says exactly what to do. The working tree holds this change plus the Sheets number-coercion fix; neither is committed, because the gate has not run since. Whoever picks this up runs `verify/gate_p15_office.sh` first and commits only if it is green.
- 2026-09-04  The cloud half is now proven against the real account, not only wired. The first live attempt failed and the failure was worth having: it got far enough to show the credential works, the network works, Google accepted the create and the sheet existed -- then the write's own read-back validation refused it. That validation was right and the bug was mine. Sheets writes with USER_ENTERED, so it parses what it is given: sending the text "42" makes Google store the number 42, and the read-back then compares a string against a number and fails. Tab-separated content is now converted the way the local structured writer already converts it, with one trap that cost a cycle -- serde_json prints every f64 with a trailing `.0`, so a faithfulness check through f64 rejects "42" and leaves it a string; integers need their own path. `007` and `1e3` stay text because the round trip has to be faithful, which does mean USER_ENTERED will coerce them and the write will be refused loudly rather than storing something the caller did not ask for. The second lesson is operational and cost far more than the first. That failed run left an orphan file in Drive, because cleanup only ran on success -- a cleanup that only runs on success is not a cleanup, and it now runs at the start as well, matching on the title prefix only this test writes. Worse, the failed process died holding a pending macOS keychain authorization, and every later keychain read queued behind it invisibly: the next run hung for nine minutes with no prompt displayed and no output, and a bare `security find-generic-password -w` hung identically. Nothing recovers from that except closing the stuck processes. Two things follow for the next owner. A live test must print step markers, because a hung network call inside a silent test is indistinguishable from a slow one -- the markers are what finally located it in one run instead of five. And piping a long test through `grep` hides everything until the pipeline ends, which is why the first nine-minute hang looked like a hang with no output at all; the runner now writes the log unpiped. With the stuck processes cleared the round trip passed in 27 seconds: it cleaned up the earlier orphan, created a Google Sheet through the provider-neutral capability with the same tab-separated body a local .xlsx takes, read it back, edited it, confirmed the edit in a separate read, and moved its own file to Drive's trash. Trash, never `files.delete`: the owner asked for the test file to be cleaned up and only that file, and a reversible move honours that without destroying anything irreversibly in their account. Office gate is 25 steps, all green, and the cloud step now has live evidence behind it rather than only routing tests.
- 2026-09-04  Conversion and the cloud both landed, which closes the two items the Office skill was waiting on. The owner settled that conversion is two things and both must work: cross-suite format conversion so an iWork application can be handed an Office file and an Office application an iWork file, and PDF export from any of them. The DESTINATION decides which one a request is, which is what lets one capability mean both without ambiguity. Routing therefore cannot look at the source alone -- the same .docx goes to Word to become a PDF and to Pages to become a .pages -- so a candidate now declares what it can WRITE separately from what it can read. That distinction caught a real defect: Word writes a .docx perfectly well but its adapter only exports PDF, so ranking on native formats sent DOC-to-DOCX to code that cannot do it. iWork does the cross-suite half because it has to: no Microsoft application reads or writes an iWork format, while each iWork application both reads and writes its Microsoft counterpart. Three probe findings shape `document/iwork_convert.rs`, and the first is the one to remember: a `POSIX file` specifier handed to an iWork application that is not frontmost is SILENTLY DISCARDED for a foreign format -- `open` returns success and no document ever appears, no error, no window, for as long as you wait. Asking the same file four ways settled that it is a specifier problem and not a file one (alias 1s, `open -a` 2s, activate-then-POSIX worked, bare POSIX never did), so everything sends an alias. Second, importing a foreign format makes a new unsaved document, so `open` returns `missing value` and the document's own `file` is `missing value` too; it is found by which id appeared, which is also what makes it safe to close, and a document the person already had open is reused and never closed. Third, `export` OVERWRITES without complaint, so no-overwrite is enforced before any application is asked. Numbers needs one thing only Numbers needs: its document appears before the import has finished, so `sheet 1` exists while `table 1 of sheet 1` is still an invalid index -- an earlier probe read that as a refusal to open, and the fix is to wait for the table, not the document. Four real E2Es prove both directions and the PDF for Pages, Numbers, Keynote and Excel, each checking the CONTENT arrived and reading it back with the structured layer so what is checked is the file rather than the producing application's opinion of it; Keynote makes its own .pptx fixture and Numbers is given a .xlsx written with no application at all, so neither needs Microsoft Office. Then the cloud: Google's Docs, Sheets and Slides adapters were real, proven, and reachable from no provider-neutral capability -- the fifth adapter in this work found working and callable from nowhere. Both blockers were decisions rather than omissions and both are now settled. A Drive file id is not a path, so a request is a cloud request when it carries `resource.fileId` or says `provider: "google-workspace"`, which is the only way a CREATE can say so; everything else stays local, so no existing caller changes behaviour. And the synchronous gateway reaches the async adapters through `tauri::async_runtime::block_on`, the bridge `download/registry.rs` already uses. The cloud envelope matches what the local providers return, and the same tab-separated body that creates a local .xlsx creates a Google Sheet. Google's availability is the stored credential, read synchronously, because a routing decision should not perform a network refresh. The matrix gained a cloud half asserting that every cloud capability reaches Docs, Sheets or Slides and never a local application, that what Google has no adapter for claims no cloud route, that a disconnected account has no cloud route at all, and that a local path still never reaches Google. A live round trip exists behind `AI_OS_GOOGLE_LIVE` and deliberately does not delete the file it creates: deleting someone's data to tidy up after a test is not this code's decision. WPS now declares all three of its applications as available-and-not-executable, and the test says all three fall to the structured layer, which is the true statement -- the files are supported, the application is not driven. One recurring mistake is now a gate step of its own: `sed -i` on macOS drops the executable bit, which has cost four scripts their +x during this work, and `verify/verify_verifiers_are_executable.sh` caught a fifth on its first run. Office gate is 25 steps, all green. What remains is one product question, not an implementation gap: `document.create` and `document.convert` still route by extension rather than through the resolver, because Pages converts to PDF, macOS conversion converts between DOC and DOCX, and Word exports PDF, so what `document.convert` means across providers is still undecided -- `presentation.convert` and `spreadsheet.convert` were given the single unambiguous meaning the owner chose, and `document.convert` can follow once that is settled.
- 2026-09-04  The Office adapters that nothing could call are now callable, and the registry stopped overstating three providers. On the owner's decision, `document.edit`, `presentation.edit` and `presentation.convert` are exposed: all three were implemented and proven by their own verifiers and declared by no capability at all, which is the fifth time in this work that a working adapter turned out to be reachable from nowhere. They route through the same resolver as everything else and are rows in the traced matrix, which is the point -- a capability that exists only in a verifier is not a capability. None of them edits a file in place: each reads a source and writes a separate destination, leaving the source byte-identical, and the capability is named for what the caller wants while the contract is a copy. `presentation.convert` means exactly one thing, PDF export, deliberately unlike `document.convert`, whose meaning still differs by provider (Pages exports PDF, macOS converts DOC/DOCX, Word exports PDF) and which is why that one capability is still not routed through the resolver and is deliberately absent from the matrix. Word also gains the gate step it never had, now that `document.read` and `document.edit` both reach it, so the gate is 21 steps, all green. Three declarations were corrected to what their provider can execute: Microsoft Office gets its own list, being the only provider with an editing adapter for all three document kinds; WPS gets an empty one, because it is detected when installed but macOS publishes no deterministic automation contract for it and AirScript is a cloud API, so there is nothing to route to while its FILES stay supported through the structured layer; and Google Workspace declares the seven it implements rather than all eight. Google is worth stating precisely because it is not the WPS case: its Docs, Sheets and Slides adapters are real and proven, but they are exposed as commands the frontend calls directly while the provider-neutral Office routes are local-only, so `document.read` cannot reach them. Closing that is the one substantial Office item left, and it is a decision rather than an omission: a Drive file id is not a path, so the request shape has to be settled, and an async adapter has to be reachable from the synchronous gateway. The other open item is what `document.convert` should mean. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  Routing is now one thing. `document/resolver.rs` held a complete, tested, format-aware resolver that no production code called -- the third of the four adapters found written and unreachable -- while the entry points routed by a chain of extension checks, a priority-ordered registry lookup, and an `is Office installed` test that disagreed with each other. The same .docx returned Word's shape or plain text depending on an application neither path used. `office_candidates()` now supplies that resolver with what a real machine offers, and `document.read`, `spreadsheet.read`, `spreadsheet.create`, `spreadsheet.edit`, `presentation.read` and `presentation.create` all dispatch through it. Two fields in a candidate mean different things and the distinction is the point: `available` is whether the application is installed, `executable` is whether this build has an adapter a call path reaches. WPS declares `available` and not `executable`, which is the true statement -- macOS publishes no deterministic automation contract for it, so there is nothing to route to, while its files are still supported through the structured layer, because a WPS .docx is a .docx. The fidelity ordering comes from the format lists rather than from priority: macOS `textutil` reads a .docx by converting it to plain text, which is an import and not a native read, and declaring it as one is exactly what let the lowest-priority generic path outrank both Word and the structured layer on their own format. On the owner's decision, `.docx` now reads through Word when Word is installed and through the structured layer when it is not, with the same fields either way; `.doc` stays with Word and falls to conversion without it; `.rtf` and `.txt` are the floor's own formats and are what the textutil unit test now uses, because a `.docx` there would assert something different on a machine with Word than on one without. `document.create` and `document.convert` deliberately do NOT route through the resolver yet and are deliberately absent from the matrix: Pages converts to PDF, macOS conversion converts between DOC and DOCX, and Word exports PDF, so what `document.convert` means is a product question, not a routing one. The Provider Matrix is now three tests rather than a table -- every capability and format on a fully equipped machine, the same table on a machine with no Office application at all, and WPS installed changing nothing -- so a row cannot be added until a call path actually reaches it. One more sandbox lesson, which cost an Excel timeout and then a PowerPoint crash: a sandboxed Office application has implicit access to what it wrote itself and to its own container and to nothing else, so a test that hands it a source document or an image from a temp directory raises a grant panel no automated run can answer, and that panel blocks every later automation of that application. All three cross-provider tests now stage into the relevant application's own container. Also worth knowing for the next owner: a failed E2E can leave documents open in Word or PowerPoint, and the next run then times out with -1712 until they are closed. Office gate is 20 steps, all green. Remaining: whether `presentation.edit` and `presentation.convert`, which `powerpoint.rs` implements but no capability list declares, should be exposed, and the same question for Word's `document.edit`. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  The structured layer now covers all three Office formats, and the capability no longer depends on any application being installed. `spreadsheet.create` writes a minimal but conformant XLSX package -- content types, package and workbook relationships, shared strings, styles -- built beside its destination and renamed into place so a failure cannot leave a half-written file at the caller's path; values that parse as finite numbers are written as numbers, everything else as shared strings, which keeps `007` text rather than turning it into 7. The proof is not that this reader can read it back, which would only make it a private format wearing an .xlsx extension: Excel itself opens the file and reads back what was written, escaping included, through the existing provider-neutral read path. `document.read` and `presentation.read` follow the same rule -- .docx and .pptx are ZIP archives of XML and are read directly when Microsoft Office is absent -- and both return the shape the application adapter returns, because a caller must not have to know which provider answered. Two new cross-provider tests establish that: Word writes a document with a heading, an inline image and a table and this layer reads back the same table cells, image count and run formatting; PowerPoint writes a deck and this layer reads back the same slide order, titles, bodies and table cells. One difference is real and is stated rather than hidden: Word's paragraph count is not the file's, because it adds a mark at the end of every table row and one around an inline drawing, neither of which the document declares as a paragraph; imitating that would mean modelling a word processor's editing surface, so this reports the paragraphs the file has and the test asserts the exact relationship between the two counts, which fails if either side changes. `LOCAL_STRUCTURED_CAPABILITIES` gains `document.read` and `presentation.read` and still claims neither creation nor editing of documents and decks, which this layer cannot do. Two operational notes for the next owner. First, a test that hands Excel a file Excel did not itself write must place it under Excel's own container (`~/Library/Containers/com.microsoft.Excel/.../Caches`), not a temp directory: a read runs its own osascript invocation and does not inherit the implicit access macOS grants an app for a file that app just wrote, so a temp path raises a 'locate this file' prompt no automated run can answer and that then blocks every later Excel automation. That was the cause of the -1712 timeout this stretch, and `excel_readable_workspace` is the remedy in both files that need it. Second, `sed -i` on macOS rewrites a file rather than editing it in place and so drops the executable bit -- it has now cost three verifier scripts their +x. Office gate is 20 steps, all green. Remaining: wiring `document/resolver.rs`, whose format-aware routing is complete and tested but which nothing builds candidates for, and then a Provider Matrix traced through production call paths rather than read off declarations -- plus deciding whether `presentation.edit` and `presentation.convert`, which `powerpoint.rs` implements but no capability list declares, should be exposed. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  Office reframed as a capability rather than six application integrations, on the owner's correction that the skill must not stop working because of which application is installed. `document/structured.rs` reads `.xlsx` from the file itself -- workbook order, worksheets resolved through relationships, shared strings with rich-text runs joined, inline strings, booleans, numbers and the five XML entities, with cells placed by their own reference rather than document order because a real sheet omits empty cells. It applies the same bounds as the application adapters and reports what the file holds when it truncates. The proof that it is interchangeable rather than merely similar is a cross-check: a workbook Excel itself wrote is read by Excel and read without Excel and the two must agree, escaping, booleans and worksheet identity included. The registry's `LocalStructured` provider -- previously four declared capabilities, no implementation and `available: false`, the fourth thing found written-but-unreachable in this stretch -- now declares the one capability it executes, is available on every machine because it needs no application, and is ranked at priority 900 below every installed application; `spreadsheet.read` falls to it only when Microsoft Office is absent. This also settles WPS correctly: WPS's formats are the Microsoft formats, so the skill supports WPS files whether or not WPS is installed, through the structured layer. The WPS provider still declares no executable capability because macOS has no published deterministic automation contract for it and AirScript is a cloud API, but a provider having no adapter is not the same as a format being unsupported. Office gate is 20 steps, all green. Remaining: structured `spreadsheet.create`, `document.read` and `presentation.read`; wiring `document/resolver.rs`; then a Provider Matrix built from traced production call paths rather than declarations. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  Apple iWork Common Capability COMPLETE, and a second unreachable adapter found and wired. Keynote needed no new evidence: `keynote_real_e2e` already asserted title and body content, proved a document the user had open stays open, proved a refused overwrite leaves the original content intact, and scanned the source for `quit`. Its gap was that the verifier belonged to no gate; it is now a step in `verify/gate_p15_office.sh`. Worth recording: that E2E writes to a temp directory and passes, so Keynote reads back from one without a sandbox prompt, unlike Excel -- Pages and Numbers were given the more conservative Documents-folder workspace before this was known. The presentation route turned out to have the `.pages` defect in reverse: `presentation.read`/`create` resolved straight to Apple iWork regardless of format, so a `.pptx` was handed to the Keynote adapter and refused for not being a `.key`, while `document/powerpoint.rs` -- with read, create, edit and export all proven by `verify_p15_powerpoint_common_capability.sh` -- was called from no production path at all. Both entry points now dispatch on the file's own extension, `.key` still falls through to Keynote, and the PowerPoint verifier is also now a gate step. That makes three adapters found written, proven and unreachable in this stretch: PowerPoint, the format-aware resolver in `document/resolver.rs`, and (before this work) any Pages/Numbers path. A passing verifier proves an adapter works; it does not prove anything can call it, and the final Provider Matrix must check the production call path for every capability it claims. The Office gate is 19 steps, all green. Remaining Office work: WPS classification, Google Workspace fallback review, final Provider Matrix -- plus deciding whether `presentation.edit` and `presentation.convert`, which `powerpoint.rs` implements but no capability list declares, should be exposed. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  Apple Pages and Apple Numbers Common Capability COMPLETE. Pages gains `document.read`, `document.create` and `document.convert`; Numbers gains `spreadsheet.read` and `spreadsheet.create`. Both were probed before either was written (`verify/probe_iwork_pages_numbers_semantics.sh`) and both are proven by real E2Es that assert content rather than `get version`. Pages: an export target must carry its extension or Pages treats the path as a folder, fails with error 6 and leaves its document open. Numbers: a new table is already 22x7 and there is no used range, so reads trim trailing blanks while still reporting what Numbers holds; an empty cell reads back as `missing value`; sheet and table names are localized and are addressed by index and only reported by name; `make new sheet` fails with -10000, so a Numbers create is single-sheet and says so. Rows and columns can be added, so a create grows the table to fit. Two structural corrections landed with them. First, the registry stopped overstating iWork: it had declared all eight Office capabilities while only Keynote had an adapter, and now declares the seven it can execute, with `spreadsheet.edit` deliberately absent because Numbers has no edit adapter. Second, routing: the registry is priority-ordered, so Microsoft Office won every capability it declared and a `.pages` file was handed to an adapter that could not open it -- the Pages and Numbers adapters existed but nothing could reach them. `document.read`/`create`/`convert` and `spreadsheet.read`/`create` now dispatch on the file's own extension before the priority list applies, with everything else falling through unchanged. Note that `document/resolver.rs` already contains a complete format-aware provider resolver that no production code calls; wiring it in belongs with the final Provider Matrix, and the extension dispatch was chosen so eight proven Excel phases did not have to move. The gate is renamed `verify/gate_p15_office.sh` and is 17 steps, all green on the acceptance Mac. Remaining Office work: Keynote review, WPS classification, Google Workspace fallback review, final Provider Matrix. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  Microsoft Excel Common Capability COMPLETE. Phase H landed the realistic workflow: one workbook built in a single `spreadsheet.edit` call with typed cells, formula totals, a descending sort on the computed total, a bold filled header, a number format, a wider label column, a chart and a filter, then read back through `spreadsheet.read` in its own osascript invocation. It is the only test crossing both halves of the Excel work, and it asserts three things no per-phase test can: the rows come back in sorted order, a filtered row is hidden rather than deleted, and Excel rewrote each total's relative references as the sort moved its row. It immediately caught a real defect in the displacement rule, now fixed: displacement had been answered per worksheet ("was this sheet ever sorted"), so a format applied AFTER a sort was reported as displaced and its check silently skipped even though its address was valid -- the quiet widening into "skip validation whenever anything happened" that earlier phases warned against. Displacement is now answered per operation via `movesAfter`: does any LATER operation move content on this worksheet, with structural changes invalidating index-based probes as well as address-based ones and sorts invalidating only the latter. Phase E and Phase H now pin the rule from both sides. The Excel gate is 14 steps, about 3 minutes, all green on real Excel 16.78: Phase A through Phase H, Spreadsheet Create, Spreadsheet Read, the full Rust suite, the frontend build and `git diff --check`. Every phase preserves the source workbook, saves without overwriting, restores Excel's workbook and window state, never quits Excel and leaves user workbooks untouched. Next Office work is Pages, Numbers and Keynote Common Capability. Office remains In Progress and P15 remains 5 of 11.
- 2026-09-03  Microsoft Excel Phase G formula-aware read completed: `spreadsheet.read` accepts an opt-in, strictly typed `includeFormulas` boolean on the existing provider-neutral read route; absent/null/false reproduce the previous output byte for byte and a truthy string is refused. Each worksheet now reports `usedRange` always, and `formulas` only when requested, as a sparse list of {row, column, formula} -- sparse because a constant cell reports the constant as its formula, so only a leading `=` marks a real one, and because an empty list ("asked, none present") and an absent key ("never asked") are different answers. Bounded to 512 formula entries per worksheet inside the existing cell and protocol budgets, with a declared-versus-arrived count check so a payload cut on the way out cannot read as complete. An isolated probe (`verify/probe_excel_formula_read_semantics.sh`) established that `formula of used range` returns the same 2D list shape as `value of used range`, that constants report themselves, that `formula` returns English function names on a Chinese-locale Excel, and that all of it survives save/close/reopen. Two real-Excel traps were hit and are recorded: inside a `tell application "Microsoft Excel"` block `tab` resolves to Excel terminology and emitted the literal text "tab", so the separator is now `ASCII character 9`; and a read E2E that opens a workbook from a temp directory raises a sandbox "locate this file" modal that no unattended run can answer and that then blocks every later Excel automation, so read E2Es now build under Excel's own cache directory. That investigation also exposed a latent Phase B regression: `delete_worksheet` raises a permanent-delete modal unless `display alerts` is off, which had been masked by this machine's Excel happening to have alerts off; the adapter now suppresses alerts immediately around the delete and restores the previous value on both the success and the error path, proven by `verify/probe_excel_alert_suppression.sh`. Excel Common Capability, Office and P15 remain In Progress / In Progress / 5 of 11; the only remaining Excel work is one realistic multi-capability workflow and final acceptance.
- 2026-09-02  Microsoft Excel Phase F chart integration completed: `add_chart` creates a column, bar, line or pie chart from a bounded source range with a required explicit name, through the existing `edit_excel_workbook()` adapter on the existing provider-neutral `spreadsheet.edit` route. Two probe rounds (`verify/probe_excel_chart_semantics.sh`) established that `make new chart object at <sheet>` works while `make new chart at end of <workbook>` fails (-50), that source data must be set by the command form because `set (source data of chart of X) to <range>` fails (-10006), and that a chart's name, chart type, series count and series formula all survive save-as-xlsx/close/reopen. Excel does not always store the constant that was set -- `line chart` stores as `line markers`, `pie exploded` as `pie chart`, and bare `line`/`pie` are not valid constants -- so the encoded record carries both the constant to set and the constant to compare against. The series formula is the accepted evidence because it names the ranges actually plotted, so a chart that exists but plots nothing cannot pass. The first real run hung for thirty minutes: `repeat with x in chart objects of <sheet>` deadlocks Excel once a chart exists on that sheet and `with timeout` does not rescue it, and that loop was how duplicate chart names were checked; `verify/probe_excel_chart_multi.sh` proved `exists chart object <name>`, `count of chart objects`, `name of chart object <index>` and `name of every chart object` all answer instantly, and the adapter now addresses charts by name and never enumerates them. The verification harness was also rebuilt: phase verifiers no longer run their predecessors (the chain had been quadratic, 32 real Excel cycles per gate), `verify/gate_p15_excel.sh` runs every phase once in order and a full gate now takes about 2.5 minutes, and the developer-machine runner enforces a timeout in its own process group and writes step logs inside the repository so a hang is bounded and visible. Real Excel 16.78 gate passes Phase A, Phase B mutation, Phase B read, Phase C structural, Phase D formatting, Phase E sort/filter, Phase F charts, Spreadsheet Create, Spreadsheet Read, the full Rust suite, the frontend build and `git diff --check`. The `document::excel::tests::` count guard moved from 17 to 20. Next Excel work is formula-aware read. Excel Common Capability, Office and P15 remain In Progress / In Progress / 5 of 11.
- 2026-09-02  Microsoft Excel Phase E sort/filter completed: `sort_range` (bounded range, key column required to fall inside it, explicit `order` and explicit `hasHeader`), `apply_filter` (bounded range, field bounded by the range's own width, criteria string) and `clear_filter` landed through the existing `edit_excel_workbook()` adapter on the existing provider-neutral `spreadsheet.edit` route. An isolated probe (`verify/probe_excel_sort_filter_semantics.sh`) established that all four sort variants survive save-as-xlsx/close/reopen, that a filter can be judged by `autofilter mode` plus each row's `hidden` state and that both survive the round trip, that `show all data` unhides without removing the filter, and that `range of autofilter object` does not answer `get address` (-1708) and is therefore unused. `hasHeader` and `order` are required rather than defaulted because guessing `hasHeader` wrong sorts the caller's header row into their data irreversibly. The first real run failed with `SetCell string validation mismatch` because Phase C's displacement rule had been implemented for structural operations only and sorting also moves cells; there are now two sets -- `structurallyChangedSheets` (insert/delete row/column, which also moves row and column indexes) and `reorderedSheets` (sort, which moves values and the formatting that travels with them but not indexes) -- and the marker is now `displaced-by-moved-content`. A filter hides rows without moving contents, so ordinary address-based validation still applies on a filtered worksheet, and the E2E asserts that directly so the displacement rule cannot quietly widen into skipping validation whenever anything happened. Real Excel 16.78 gate passes Phase E, Phase A, Phase B mutation, Phase B read, Phase C structural, Phase D formatting, Spreadsheet Create, Spreadsheet Read, the full Rust suite, the frontend build and `git diff --check`. The `document::excel::tests::` count guard moved from 14 to 17. Next Excel work is common chart integration, which must be probed for save/reopen-survivable evidence before the operation is designed. Excel Common Capability, Office and P15 remain In Progress / In Progress / 5 of 11.
- 2026-09-02  Microsoft Excel Phase D basic formatting completed: `format_cells` (one bounded rectangular range with any combination of bold, italic, font size, font name, fill colour index and number format), `set_column_width` and `set_row_height` landed through the existing `edit_excel_workbook()` adapter on the existing provider-neutral `spreadsheet.edit` route. An isolated probe (`verify/probe_excel_formatting_semantics.sh`) was run first and established both that the previously unprobed `italic`, `font size` and font `name` properties exist and that all eight attributes survive save-as-xlsx, close and reopen -- the precondition for validating them by reading the reopened saved copy. Read-back text forms are not uniform: bold/italic return `true`, font size returns `18`, colour index returns `6`, but column width and row height return `24.0` and `30.0`, so line measures are compared numerically with half a unit of tolerance and the observed value is reported rather than the requested one. An operation carrying no attribute is refused rather than silently validating clean. Encoding is fixed arity with an `end` terminator so no optional value is ever the trailing field. The Phase C displacement rule extends to formatting addresses. Real Excel 16.78 gate passes Phase D formatting, Phase A, Phase B mutation, Phase B read, Phase C structural, Spreadsheet Create, Spreadsheet Read, the full Rust suite, the frontend build and `git diff --check`. The `document::excel::tests::` count guard moved from 11 to 14, and `verify/gate_p15_excel_phase_c.sh` was renamed `verify/gate_p15_excel.sh`. Next Excel work is sort/filter, whose sort form is already proven; autofilter still needs a save/reopen survival probe before its validation is designed. Excel Common Capability, Office and P15 remain In Progress / In Progress / 5 of 11.
- 2026-09-02  Microsoft Excel Phase C completed: `insert_row`, `delete_row`, `insert_column` and `delete_column` landed through the existing `edit_excel_workbook()` adapter on the existing provider-neutral `spreadsheet.edit` route, with explicit worksheet identity, 1-based bounded indexes, one line per operation and no `count` parameter. The previous attempt's hypothesis was disproved: an isolated real Excel 16.78 probe ran all twelve candidate forms and all twelve shifted content correctly and identically, so the `shift` parameter is unnecessary and the plain form is used. The real cause was adapter integration -- reopen validation asserted cell addresses that the structural change had moved, reporting a working shift as `SetCell string validation mismatch`; worksheets touched by a structural operation now report `displaced-by-structural-change` rather than asserting a stale address, and Excel's shift arithmetic is deliberately not reimplemented to predict new addresses. Validation reads the saved copy once at the end, so each structural operation now gets its own worksheet in the E2E and each read-back proves exactly one operation, with a fifth worksheet carrying the composite insert-then-delete sequence the previous attempt failed on. Real Excel 16.78 gate passes Phase C structural, Phase A, Phase B mutation, Phase B read, Spreadsheet Create, Spreadsheet Read, the full Rust suite, the frontend build and `git diff --check`. Source workbook preservation, save-copy, workbook/window restoration, Excel-process preservation and user-workbook preservation all hold. `verify/verify_p15_excel_phase_a.sh` now expects 11 tests from the `document::excel::tests::` filter instead of 8. Next Excel work is basic formatting, whose eight AppleScript primitives are already proven by the same probe run. Excel Common Capability, Office and P15 remain In Progress / In Progress / 5 of 11.
- 2026-09-02  Office takeover recorded: Word and PowerPoint Common Capability remain Complete; Excel Phase A, Phase B Mutation, and Phase B Multi-Sheet Read remain Complete. Excel Phase C is explicitly NOT LANDED after the temporary candidate passed Rust/API/unit validation but failed real Excel 16.78 column structural read-back; the worker restored stable baseline `633aeb42258a248ed80dd0fa760525a6207ed9d3`. Next owner starts with an isolated real-Excel whole-column-delete semantics probe, then completes Phase C without reopening Phase A/B. Remaining Office work includes Excel formatting, sort/filter, chart integration, formula-aware read and final workflow; iWork Common Capability review/completion; WPS deterministic-support classification; Google Workspace fallback completion; and final Provider Matrix acceptance. Office remains In Progress and P15 remains 5/11.
- 2026-09-02  Microsoft Excel Phase B completed: `spreadsheet.read` now returns worksheet list/count and supports backward-compatible first-sheet reads, explicit named-sheet reads, selected multi-sheet reads, and bounded all-sheet reads. Real Excel E2E passes list/count, named and multi-sheet content, missing-sheet fail-closed behavior, bounded output metadata, workbook/window restoration, Excel-process preservation, and user-workbook preservation. Read bounds are 16 worksheets, 200 rows x 64 columns per worksheet, 256 rendered characters per cell, and an approximately 60k protocol budget. Combined with the already accepted Phase B mutation subphase, Excel Phase B is complete. Next Excel work is Phase C row/column insertion and deletion. Excel Common Capability, Office, and P15 remain In Progress / In Progress / 5 of 11.
- 2026-09-01  Microsoft Excel Phase B mutation subphase completed: real Excel 16.78 E2E passes AddWorksheet through snapshot + unique set-difference identity discovery, RenameWorksheet, DeleteWorksheet, explicit named-sheet cell/formula edits, final-sheet delete fail-closed behavior, final worksheet-set reopen validation, save-copy, original preservation, fresh post-save ownership, workbook/window restoration, Excel-process preservation, and user-workbook preservation. No worksheet-index assumption, System Events, UI click, or global Excel setting is used. Full Phase B remains In Progress until worksheet list/count and bounded specific-sheet/multi-sheet read are accepted.

- 2026-09-01  Office acceptance correction: the previous closure validated capability categories but did not satisfy required provider coverage. Office is restored to In Progress and P15 to 5 of 11 until the common Document/Spreadsheet/Presentation layer, format-aware local/cloud resolver, Microsoft Word/Excel/PowerPoint adapters, executable Pages/Numbers/Keynote adapters, WPS automation investigation and deterministic coverage, Google Docs/Sheets/Slides fallback coverage, and realistic workflows meet the current completion rule. The 2026-08-31 implementation and E2E evidence remain valid historical evidence and are not deleted.
- 2026-08-31  P15 Office workflows completed. Document read/create/convert and Spreadsheet read/create remain PASS; Pages, Numbers, and Keynote 15.3.1 real E2E PASS. Provider-neutral Presentation now routes local `.key` read/create through deterministic Keynote automation with one-time create confirmation, mandatory read-back validation, no-overwrite, resolved file-alias ownership checks, and no application quit; an already-open document remains user-owned. Google Workspace remains the cloud provider with real Slides E2E PASS. PowerPoint and WPS were not expanded. P15 is 6 of 11 complete; next is Computer Control v1, then Vehicle Control v1.
- 2026-08-31  macOS Keychain prompt strategy corrected: the prior cache-only build failed real E2E with 4–5 prompts because startup eagerly touched several of six distinct Provider Keychain accounts under a rebuild-specific ad-hoc identity. Provider secrets are now lazy-loaded only for real operations, cached per account/process, and safely traceable behind `AI_OS_KEYCHAIN_TRACE=1`; normal startup/My AI/Connections/passive status use non-secret metadata and target zero reads. The unused local-signing experiment was removed. Stable Apple signing is a PRE-RELEASE REQUIREMENT, not a P15 dev prerequisite. Deterministic acceptance passed; one final real Mac E2E remains pending and no commit was made.
- 2026-08-31  macOS Keychain lazy credential loading real E2E PASS: newly rebuilt startup, My AI, and Connections produced zero prompts; first real use of one new Provider produced one prompt; repeated use and restart without rebuild produced zero prompts. The first Microsoft/Google metadata regression fix restored both to Connected but incorrectly read both OAuth secure items at startup, causing two Keychain prompts; that OAuth recovery attempt therefore failed real UX acceptance. The same run observed Taobao fall to Reconnect after its earlier restart E2E PASS. Startup OAuth reads were removed, persisted Connected metadata is retained until real invalidation evidence, and Connections now establishes the Browser recovery listener before its first snapshot and prevents stale per-provider snapshots from overwriting newer backend recovery. Deterministic Provider, Connections, Microsoft, Google, Browser, frontend, Keychain, and External Connector acceptance passed; final combined real E2E is pending. No commit was made.
- 2026-08-30  Privacy Policy published at `https://russellchen001.github.io/AI-OS/privacy/`; public contact is `aios.privacy@gmail.com`. The page is retained for future official OAuth integrations even though consumer eBay now uses Authenticated Browser.

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

### P15 Apple iWork connection-state fix — 2026-08-29

- Apple iWork connection state is no longer inferred from installation alone.
- Explicit Connect performs real read-only `osascript` Automation probes against installed Pages, Numbers, and Keynote applications.
- After successful verification, AI-OS stores only a non-sensitive local verification marker.
- Later Connections refresh/restart checks that marker and performs a fresh macOS Automation probe. Probe success restores `CONNECTED`; probe failure returns `AUTHORIZATION_REQUIRED`.
- Disconnect removes only the AI-OS verification marker and does not modify or bypass macOS TCC permissions.
- Technical decision: macOS remains the source of truth for live Automation authorization; AI-OS must not persist a blind `CONNECTED` state.

---

## Claude Code Handoff — P15 Connections / Authenticated Browser — 2026-08-29

> This section supersedes older P15 Connections / browser status notes where they conflict with the status below.

### Current branch / known baseline

- Active branch: `feature/p15-core-skills`.
- Google Workspace real E2E is completed and committed:
  - `8c2920a feat: complete google workspace real e2e`
- Google real E2E verified:
  - secure OAuth authorization
  - identity
  - Drive list
  - Docs create/readback/delete
  - Sheets create/write/readback/delete
  - Slides create/readback/delete
  - temporary resource cleanup
  - Evidence boundary
- Google OAuth desktop callback must keep the verified root loopback callback behavior. Do not revert it without another real OAuth E2E.
- Google OAuth client secret is stored only in macOS Keychain under the existing AI-OS OAuth client-secret path; never move the secret into source, frontend state, HANDOFF, Planner, Evidence, or Memory.

### Apple iWork — real bug fixed and real-UI validated

The previous iWork Connections bug was confirmed:

- installation detection always resolved installed Pages / Numbers / Keynote to `AUTHORIZATION_REQUIRED`;
- `connect_apple_iwork` could temporarily set the frontend to `CONNECTED`;
- the next Connections refresh recomputed state from installation only and lost the connected state.

The implemented fix uses:

- real read-only `osascript` Automation probes against installed Pages / Numbers / Keynote;
- a non-sensitive local AI-OS verified marker;
- refresh/restart re-probing of macOS Automation authorization;
- `CONNECTED` only when previous verification exists and the live probe succeeds;
- `AUTHORIZATION_REQUIRED` when the live probe fails;
- `APP_NOT_INSTALLED` when no iWork component is installed;
- Disconnect removes only the AI-OS verification marker and does not alter/bypass macOS TCC.

Automated validation completed:

- `connections::tests` — 7 passed, 0 failed.
- `verify/verify_p15_connections_onboarding.sh` — PASS after removing the obsolete hard-coded "5 passed" gate and checking required named behaviors instead.
- `npm run build` — PASS.
- `cargo check --manifest-path src-tauri/Cargo.toml` — PASS with existing warnings.
- `git diff --check` — PASS.

Real UI validation completed:

1. Apple iWork Connect -> `Connected`.
2. Rescan Apps -> remained `Connected`.
3. Quit AI-OS.
4. Restart AI-OS.
5. Apple iWork automatically remained `Connected`.

Before continuing, Claude Code must inspect `git status` and `git log`. If this iWork fix is still uncommitted, keep its scope to:

- `HANDOFF.md`
- `src-tauri/src/connections.rs`
- `verify/verify_p15_connections_onboarding.sh`

and commit it as:

`fix: persist and reverify iwork connection state`

Do not mix later authenticated-browser work into that commit.

### Authenticated Browser — confirmed root cause

Amazon / Taobao currently remain `WAITING_FOR_USER` even when the user is visibly logged in in their normal browser.

This is a confirmed architecture gap, not user error.

Current behavior:

- `begin_browser_login` creates/saves `BrowserLoginSession` metadata.
- frontend `ConnectionsCenter.tsx` calls `openUrl(capability.loginUrl)`, which opens the system/default browser.
- that browser session is not proven to be owned, inspected, or reusable by AI-OS.
- `verify_browser_login` currently intentionally calls `session.apply_verification(false)`.
- therefore browser-backed providers can never become `CONNECTED` through the current implementation.

Do NOT "fix" this by changing `apply_verification(false)` to `true`.

Opening a login page is never sufficient evidence of authentication.

### Existing MCP Browser is not a persistent authenticated-browser runtime

`src-tauri/src/mcp_runtime.rs` has been inspected completely (111 lines).

Its behavior is:

- each `tools/list` / `tools/call` creates a new stdio child process;
- `send_json_rpc` spawns the MCP server per call;
- stdin is written once and closed;
- there is no persistent browser process/context/session ownership in this layer.

`src-tauri/src/browser/registry.rs` currently exposes only `"mcp-browser"` and passes arbitrary `command` / `args` through to `call_mcp_tool`.

No repository configuration was found that binds `"mcp-browser"` to a concrete persistent Chrome/Chromium/DevTools browser context.

Technical decision:

- keep the generic MCP Runtime stateless;
- do not turn `mcp_runtime.rs` into the authenticated-session owner;
- ordinary `browser.search` / `browser.control` may continue to use the existing MCP abstraction;
- authenticated websites require a separate Persistent Authenticated Browser runtime.

### Persistent Authenticated Browser architecture decision

Required target architecture:

`Connections / Browser Skill`
→ `Persistent Authenticated Browser Runtime`
→ AI-OS-owned persistent browser profile/context
→ real provider-specific account-state verification
→ safe verification metadata
→ `CONNECTED`

The authenticated-browser runtime must:

- own its browser process/context/profile;
- persist the browser profile outside the repository under application data;
- never reuse the user's normal Chrome/Safari profile as if AI-OS owned it;
- never terminate unrelated user browser processes;
- preserve website login sessions inside the managed browser profile;
- expose only safe metadata outside the runtime:
  - opaque profile/session ref
  - provider ID
  - browser kind
  - running state
  - verified origin
  - non-secret account marker if required
  - timestamps
- never expose/store in frontend, Planner, Evidence, Memory, or HANDOFF:
  - passwords
  - raw cookies
  - session cookies
  - bearer tokens
  - authentication headers
- remain fail-closed:
  - browser opened != authenticated
  - browser running != authenticated
  - login page closed != authenticated
  - `CONNECTED` requires a real account verifier.

Preferred provider priority remains:

Official API/OAuth
→ Native Structured Interface
→ Authenticated Session
→ Deterministic Automation
→ Computer Use

Authenticated Browser is the Authenticated Session layer, not a replacement for official APIs.

### Amazon region handling

Do NOT hardcode Amazon Australia.

The current capability/login design that assumes one fixed Amazon country must be replaced by region-aware behavior.

Required behavior:

- user may use Amazon US, Australia, Japan, UK, Germany, or another supported regional site;
- authenticated browser should observe the real browser origin;
- after real account verification, persist a safe `verifiedOrigin`;
- later account-bound operations should reuse the actual verified regional origin;
- never infer that every AI-OS user belongs to `amazon.com.au`.

Example valid verified origins include:

- `https://www.amazon.com`
- `https://www.amazon.com.au`
- `https://www.amazon.co.jp`
- `https://www.amazon.co.uk`

The provider verifier must still validate that the observed origin genuinely belongs to the expected provider.

### Browser providers in scope

The Persistent Authenticated Browser foundation should initially support:

- Amazon
- Taobao
- JD
- Pinduoduo

Do not build four unrelated browser architectures.

Build one reusable authenticated-browser runtime with provider-specific verifier adapters.

### Existing reusable browser model

`src-tauri/src/browser/provider.rs` already contains concepts worth reusing:

- `BrowserSessionMetadata`
- `BrowserAuthenticationState`
- `BrowserAccountVerification`
- `BrowserSessionMetadata::verified(...)`
- `BrowserSessionMetadata::expire(...)`

`BrowserSessionMetadata::verified(...)` already models:

- non-empty account marker
- HTTPS verified origin
- opaque `AuthorizationRef`
- authenticated state
- verification timestamp

Prefer integrating/reusing this model instead of creating another parallel browser-authentication state machine unless there is a concrete architectural reason not to.

### Recommended implementation sequence

Do this serially.

#### BROWSER-A — persistent managed browser foundation

Implement:

- dedicated authenticated-browser runtime under `src-tauri/src/browser/`;
- deterministic supported Chromium-family browser discovery on macOS;
- dedicated AI-OS browser profile under Tauri app data;
- loopback-only browser control/debug channel;
- browser process lifecycle owned by AI-OS;
- readiness verification before reporting runtime ready;
- safe opaque session metadata;
- begin/open, inspect, and close operations;
- process ownership rules;
- restart/profile reuse behavior.

Do not mark any commerce account `CONNECTED` in BROWSER-A.

#### BROWSER-B — real account-state verification

Implement provider-specific verification adapters.

A verifier must produce:

- real provider match;
- real current/verified HTTPS origin;
- non-secret account marker;
- authenticated/not-authenticated result.

Then integrate it with Connections:

- `begin_browser_login` opens the AI-OS managed browser, not an unrelated default browser;
- polling/verification inspects the same owned authenticated session;
- successful verifier -> `CONNECTED`;
- failed/not-yet-authenticated verifier -> `WAITING_FOR_USER`;
- expired session -> `EXPIRED` / reconnect flow.

Do not infer authentication from URL alone.

#### BROWSER-C — real E2E

At minimum perform real E2E with:

- Amazon
- Taobao

Expected scenario:

1. Connections -> Connect.
2. AI-OS-managed browser opens.
3. User logs in.
4. AI-OS verifies actual account state.
5. Connections -> `Connected`.
6. AI-OS/browser/app restart.
7. managed profile is reused.
8. account state is reverified.
9. Connections restores `Connected` when the website session remains valid.

Then extend the same runtime to JD / Pinduoduo.

### Browser acceptance requirements

Behavior tests must cover at least:

- opaque browser profile/session reference contains no credentials;
- raw profile path is not exposed where an opaque ref is sufficient;
- browser profile is beneath AI-OS app data, not the user's normal browser profile;
- browser detection order is deterministic;
- opening/running a browser never equals authenticated/Connected;
- disconnect/close only targets AI-OS-owned processes;
- browser account metadata contains no password/cookie/token;
- provider/origin validation is fail-closed;
- Amazon is not hardcoded to `.com.au`;
- verified origin can represent different Amazon regional domains;
- restart/profile reuse does not blindly restore Connected without live account verification.

Do not use hard-coded expected total test counts in verifier scripts. Verify required named behaviors.

### Microsoft Graph external E2E blocker

The last full `./verify_all.sh` result before this handoff was:

- Core Suite: PASS 80/80.
- Google Workspace external real E2E: PASS.
- Microsoft Graph external E2E: blocked/failing because:
  `AI_OS_GRAPH_E2E_DRIVE_ID is required to select a non-destructive workbook range`.
- WPS: SKIP when broker unavailable.
- iWork: previous external automation coverage was limited, but the Connections/iWork issue above has since been real-UI validated.
- Presentation: still not complete.
- commerce external providers may SKIP where real credentials/session/provider access is unavailable.

Do not weaken the global gate merely to hide the Microsoft fixture requirement. Handle that separately after Connections/authenticated-browser work unless product priority changes.

### P15 status boundary

Do not inflate P15 completion.

Known P15 status before this handoff (historical; superseded by the 2026-08-31 Office completion record above):

- Real-World Task Closure architecture implemented.
- File management complete.
- Local Model complete.
- Google Workspace real E2E complete.
- Apple iWork Connections persistence bug fixed and real-UI validated.
- Persistent Authenticated Browser: architecture decided, implementation still pending unless the repository itself shows newer code.
- Amazon/Taobao/JD/Pinduoduo account verification: not complete.
- Microsoft Graph real external spreadsheet E2E still has the fixture blocker described above.
- Presentation remains In Progress.
- Overall P15 is not complete.

### Important note about the abandoned BROWSER-A1 draft

A BROWSER-A1 implementation was discussed immediately before this handoff, including a proposed `authenticated_runtime.rs`, but the user stopped that work and asked Claude Code to take over.

Do NOT assume that proposed code was executed or is present.

Claude Code must inspect the actual repository first:

- `git status --short`
- `git log --oneline -5`
- `src-tauri/src/browser/`
- `src-tauri/src/connections.rs`
- `src/components/ConnectionsCenter.tsx`

The repository is the source of truth for whether any BROWSER-A files exist.

### First action for Claude Code

1. Read this HANDOFF section.
2. Inspect git status and latest commits.
3. Ensure the validated iWork changes are committed separately.
4. Do not regress Google Workspace.
5. Implement BROWSER-A using the architecture above.
6. Run focused tests before proceeding to BROWSER-B.
7. Keep HANDOFF current after every accepted milestone.

---

## BROWSER-A — persistent managed browser foundation — 2026-08-29

**Status: implemented. Not yet validated on macOS by a real `cargo` run. BROWSER-B not started.**

### Repository state confirmed before this work

- Active branch `feature/p15-core-skills`.
- `0c3faab fix: persist and reverify iwork connection state` was already committed
  with the required scope (`HANDOFF.md`, `src-tauri/src/connections.rs`,
  `verify/verify_p15_connections_onboarding.sh`). No separate iWork commit was needed.
- The abandoned BROWSER-A1 draft was confirmed absent. `src-tauri/src/browser/`
  contained only `mod.rs`, `provider.rs`, `registry.rs`, `runtime.rs`.

### What was implemented

New file `src-tauri/src/browser/authenticated_runtime.rs`.

- deterministic macOS Chromium-family discovery order:
  Google Chrome → Chromium → Microsoft Edge → Brave Browser;
- dedicated AI-OS browser profile at
  `<app data>/authenticated-browser/profiles/<provider id>`;
- provider ids restricted to ASCII alphanumerics and `-`, so a provider id can
  never escape the managed profile root;
- `profile_is_ai_os_owned` — the user's own Chrome/Edge/Brave/Safari profile can
  never satisfy AI-OS profile ownership;
- loopback-only DevTools control channel: browser is launched with
  `--remote-debugging-port=0 --remote-debugging-address=127.0.0.1`, the real port
  is read back from the profile's `DevToolsActivePort`, and the endpoint is
  re-checked with a fail-closed loopback test before use;
- readiness is only reported after the loopback channel actually answers
  `/json/version` with `webSocketDebuggerUrl`; a spawned process alone is not ready;
- browser process lifecycle owned by AI-OS through a private registry keyed by
  provider id; termination can only ever select a process this runtime spawned
  and recorded, so unrelated user browser processes are unreachable;
- restart/profile reuse: the profile directory persists under application data,
  a live owned process is reused instead of relaunched, and a stale
  `DevToolsActivePort` is cleared before each launch;
- safe metadata only (`ManagedBrowserSession`): provider id, browser kind, opaque
  `browser-profile:<provider id>` session ref, running, ready, profile reused,
  `authenticated`, optional verified origin, optional account marker, timestamps.
  The raw profile path and the control-channel port never leave the runtime;
- region-aware fail-closed `origin_belongs_to_provider` covering Amazon
  (`.com`, `.com.au`, `.co.jp`, `.co.uk`, `.de`, `.fr`, `.it`, `.ca`, `.in`),
  Taobao, JD and Pinduoduo. Amazon is not pinned to `amazon.com.au`.
  This is the validator BROWSER-B verifiers will consume.

Operations exposed and registered in `src-tauri/src/lib.rs`:

- `open_authenticated_browser`
- `inspect_authenticated_browser`
- `close_authenticated_browser`

### Deliberate boundaries held

- `connections.rs` was not modified. `verify_browser_login` still calls
  `session.apply_verification(false)`; browser-backed providers stay
  `WAITING_FOR_USER`.
- `ConnectionsCenter.tsx` was not modified. `begin_browser_login` still opens the
  system browser. Rewiring it to the managed browser is BROWSER-B.
- `ManagedBrowserSession::authenticated` is hard-coded `false` in this layer and
  `permits_connected_state()` cannot return true without a verified origin and an
  account marker. BROWSER-A cannot mark any commerce account `CONNECTED`.
- The generic MCP Runtime (`mcp_runtime.rs`) and `browser/registry.rs` were left
  stateless and untouched.

### Verification

New verifier: `verify/verify_p15_browser_managed_runtime.sh`.

It checks required named behaviors rather than a hard-coded test count, and also
statically asserts that the runtime decides no connection state, that
`apply_verification(true)` does not appear in `connections.rs`, and that Amazon
is not a single-region constant.

Behavior tests in `browser::authenticated_runtime::tests`:

- `browser_discovery_order_is_deterministic`
- `managed_profile_lives_under_app_data_and_never_in_the_user_browser_profile`
- `session_reference_is_opaque_and_carries_no_credentials_or_raw_path`
- `running_or_reused_managed_browser_is_never_authenticated`
- `control_channel_is_loopback_only_and_readiness_requires_a_real_answer`
- `launch_arguments_pin_the_managed_profile_and_a_loopback_debug_channel`
- `only_ai_os_owned_processes_are_ever_selected_for_termination`
- `amazon_is_region_aware_and_origin_validation_is_fail_closed`

All 8 passed and the whole module compiled clean in an isolated harness that
reproduced the module with stubs for `AuthorizationRef` and the Tauri app handle.

**Still required on the macOS development machine before BROWSER-A is accepted:**

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml browser::authenticated_runtime::tests`
- `bash verify/verify_p15_browser_managed_runtime.sh`
- `bash verify/verify_p15_connections_onboarding.sh` (no regression)
- `npm run build`
- `./verify_all.sh` — note the core suite total rises by one script.

Real Chrome launch, readiness and profile reuse have not been exercised on the
real machine yet. Until that is done, treat BROWSER-A as implemented but unproven.

### Next step

BROWSER-B — provider-specific account-state verifiers, then integrate
`begin_browser_login` / `verify_browser_login` with this runtime. P15 remains
incomplete; nothing in this milestone changes the P15 status boundary above.

---

## BROWSER-B — real account-state verification — 2026-08-29

**Status: implemented. Not yet validated on macOS by a real `cargo` run, and no
real login has been performed. BROWSER-C not started.**

### What was implemented

Two new files plus Connections integration.

`src-tauri/src/browser/devtools.rs` — loopback DevTools transport.

- raw HTTP GET against the managed browser's own DevTools endpoint, tolerating
  Chromium holding the connection open after the response;
- `/json/list` target parsing and `/json/version` browser-channel parsing that
  do not depend on a particular transfer encoding;
- one DevTools protocol call over a loopback WebSocket (`tungstenite`), with a
  fail-closed `ws://<loopback>:<port>` guard, connect/read/write timeouts, a
  bounded read loop, and id matching so protocol events are never mistaken for
  the reply;
- protocol errors and page exceptions are errors, never results.

This module decides nothing about accounts or connection state.

`src-tauri/src/browser/account_verifier.rs` — provider verifier adapters.

A verification produces exactly what the handoff required:

- real provider match — the origin is read from **inside** the live page with
  `location.origin` and checked against `origin_belongs_to_provider`;
- real verified HTTPS origin — recorded as observed, so the user's actual
  regional Amazon site is what gets persisted;
- non-secret account marker;
- authenticated / not-authenticated / no-managed-session.

Provider adapters exist for Amazon, Taobao, JD and Pinduoduo. Each returns the
same `{origin, signal}` shape and takes its signal from a signed-in-only
surface (an account name **and** a sign-out affordance), never from the presence
of a login form and never from the URL.

Three states, not two: `NoManagedSession` means AI-OS owns no running managed
browser, which is not a verification failure.

### Account marker decision

The raw signed-in signal is a display name belonging to the user. It is reduced
to `account-<12 hex>` — the first 6 bytes of `SHA-256(provider_id + ":" +
normalized signal)` — before it can leave the runtime.

- proves an account is present;
- stable for the same account, so reconnects match;
- distinct per account and per provider;
- irreversible, so no name, email or address reaches Connections, Planner,
  Evidence, Memory or the frontend.

The verifier never reads `document.cookie`, `localStorage`, `sessionStorage` or
any authorization header. The new verifier script asserts this statically.

### Connections integration

`src-tauri/src/connections.rs`:

- `begin_browser_login` now calls `open_managed_browser` with the provider login
  URL, so the login happens in the browser AI-OS owns and can inspect. The
  system default browser is no longer used for browser-backed providers;
- `verify_browser_login` calls `verify_managed_account` and applies the real
  result. `apply_verification(bool)` is gone; `apply_account_verification`
  takes an `AccountVerification` and is the only path to `CONNECTED`;
- `BrowserLoginSession` gained `verified_origin` and `account_marker`, both
  cleared whenever verification fails;
- a previously verified account that stops verifying becomes `EXPIRED` and
  enters the reconnect flow; one that was never verified stays
  `WAITING_FOR_USER`;
- `list_connection_capabilities` now takes the app handle and restores
  browser-backed state through `restored_browser_state(previously_verified,
  managed_browser_running, live_verified)`. The live probe only runs when AI-OS
  already owns a running managed browser, so a refresh costs nothing otherwise;
- `disconnect_connection_provider` closes only the AI-OS-owned browser process
  and removes only the managed profile beneath application data.

`src/components/ConnectionsCenter.tsx`:

- no longer calls `openUrl(capability.loginUrl)`; the backend opens the managed
  browser;
- surfaces the verified origin on success and an explicit reconnect message on
  expiry.

### Restart / profile reuse semantics

Deliberate and fail-closed:

| previously verified | managed browser running | live verified | state |
| --- | --- | --- | --- |
| any | yes | yes | `CONNECTED` |
| yes | yes | no | `EXPIRED` |
| yes | no | — | `EXPIRED` |
| no | yes | no | `WAITING_FOR_USER` |
| no | no | — | `DISCONNECTED` |

After an AI-OS restart the managed browser is not running, so a previously
connected commerce account shows `EXPIRED` rather than a blindly restored
`CONNECTED`. Reconnect reopens the managed browser, the persisted profile still
holds the website session, and verification restores `CONNECTED` without the
user re-entering credentials. This satisfies the BROWSER-C expectation while
never restoring `CONNECTED` without live account verification.

### Verification

New verifier: `verify/verify_p15_browser_account_verification.sh`.
Named-behavior checks only, no hard-coded test counts. It also statically
asserts that Connections cannot reach `CONNECTED` without
`AccountVerification::Authenticated`, that the managed browser is what gets
opened, and that the verifier reads no cookie, storage or authorization header.

`verify/verify_p15_browser_managed_runtime.sh` was updated: its now-vacuous
`apply_verification(true)` grep was replaced with a check that a real verifier
result is the only path to `CONNECTED`.

Behavior tests added:

- `browser::devtools::tests` — 3 tests
- `browser::account_verifier::tests` — 6 tests
- `connections::tests` — 3 new tests
  (`browser_connected_requires_a_real_account_verification`,
  `restart_and_profile_reuse_never_blindly_restore_connected`,
  `verified_browser_session_carries_only_safe_evidence`)

Validation performed off the macOS machine:

- the whole `browser` module (BROWSER-A + BROWSER-B, 758 + 305 + 370 lines,
  including the Tauri command bodies) compiled clean in an isolated harness
  against the real `tungstenite 0.27` and `sha2 0.10`, with stubs only for
  `AuthorizationRef` and the Tauri app handle — **17 tests passed**;
- the new `connections.rs` logic was extracted and compiled the same way —
  **3 tests passed**;
- `npx tsc --noEmit` — **PASS** (TypeScript 5.8.3, no errors).

**Still required on the macOS development machine before BROWSER-B is accepted:**

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml browser:: connections::`
- `bash verify/verify_p15_browser_managed_runtime.sh`
- `bash verify/verify_p15_browser_account_verification.sh`
- `bash verify/verify_p15_connections_onboarding.sh`
- `npm run build`
- `./verify_all.sh` — the core suite total rises by two scripts relative to the
  80/80 baseline.

### Known limits to settle in BROWSER-C

- The provider probe selectors are DOM-shape dependent and will rot when a site
  changes its header. They fail closed (no signal -> not authenticated), so a
  stale selector shows `WAITING_FOR_USER`, never a false `CONNECTED`. Real E2E
  is what will confirm each selector.
- Verification polling runs every 2.5s from `ConnectionsCenter` while a session
  is `WAITING_FOR_USER`. Each poll is one loopback HTTP call plus one WebSocket
  evaluate with 3s timeouts. Measure this during BROWSER-C and back off if the
  UI feels heavy.
- `Target.createTarget` is used to open the login page when the managed browser
  is already running. Confirm against the installed Chrome build during E2E.
- The Amazon capability still starts at `https://www.amazon.com/ap/signin`. That
  is only a landing page, never the verified origin — the verified origin is
  whatever regional site the user actually signs in to. Consider letting the
  user pick their region at connect time.

### Next step

BROWSER-C — real E2E with Amazon and Taobao, then JD and Pinduoduo. P15 remains
incomplete; nothing in this milestone changes the P15 status boundary above.

---

## BROWSER-B Recovery Incident Handoff — 2026-08-30

### Status

BROWSER-B restart recovery is NOT complete and NOT accepted.

Do not mark Authenticated Browser restart recovery as complete.

Do not increase the P15 completion count because of this work.

The last real user-validated browser behavior before restart-recovery experiments was:

- Amazon Authenticated Browser manual Connect/Reconnect opens an AI-OS-managed Chrome session.
- Manual Amazon login can reach `Connected`.
- The original managed-browser DevTools readiness false-negative was traced to an incorrect HTTP Host header.
- `src-tauri/src/browser/devtools.rs` was corrected from `Host: 127.0.0.1` to `Host: 127.0.0.1:<dynamic DevTools port>`.
- That DevTools Host-header change fixed a real observed failure and should be preserved unless contrary evidence is found.

### Current unresolved problem

After Amazon has successfully reached `Connected`, restarting AI-OS does not reliably restore Amazon as `Connected`.

Multiple BROWSER-B restart-recovery experiments were attempted but were not accepted.

The latest observed recovery could stall at:

`[browser-recovery] provider=amazon-consumer stage=launch origin=https://www.amazon.com`

Earlier clean runs also reached the verifier and produced:

`[browser-verifier] provider=amazon-consumer provider_target=found origin=https://www.amazon.com`

`[browser-verifier] provider=amazon-consumer probe=complete origin_valid=true signal_present=false signed_in=false`

followed by repeated:

`[browser-recovery] provider=amazon-consumer attempt=N result=not_authenticated`

and eventually:

`[browser-recovery] provider=amazon-consumer completed connected=false`

These results must not be treated as proof that the Amazon login session itself is invalid because a separate browser lifecycle/profile ownership defect was also confirmed during debugging.

### Confirmed managed-browser lifecycle defect

A previous AI-OS-managed Chrome process remained alive after the previous AI-OS instance was gone.

A real observed Chrome process used the AI-OS managed Amazon profile:

`/Users/russellchen/Library/Application Support/com.russellchen.aios/authenticated-browser/profiles/amazon-consumer`

and arguments including:

`--remote-debugging-port=0`

`--remote-debugging-address=127.0.0.1`

`https://www.amazon.com/ap/signin`

The stale managed Chrome example had PID 73063.

The profile had active Chrome Singleton ownership state.

After manually terminating PID 73063 and waiting, this command returned no managed Amazon Chrome processes:

`ps aux | grep 'authenticated-browser/profiles/amazon-consumer' | grep -v grep`

The profile directory also no longer showed active `Singleton*` entries or `DevToolsActivePort`.

This confirms a real lifecycle problem:

AI-OS does not yet reliably guarantee that every browser process it owns is terminated when AI-OS exits.

This lifecycle defect must be resolved before restart recovery can be trusted.

### Required browser ownership safety boundary

Any lifecycle fix must preserve the existing ownership boundary.

AI-OS may terminate ONLY browser processes present in AI-OS's private owned-process registry.

AI-OS must NOT:

- scan for arbitrary user Chrome processes and kill them;
- terminate a process solely because its command line contains an AI-OS-looking profile path;
- touch the user's normal Chrome profile;
- adopt unknown browser processes into AI-OS ownership;
- expose cookies, tokens, credentials, raw profile paths, account identity, or DevTools control ports to frontend state.

DevTools must remain loopback-only.

### Confirmed DevTools readiness bug and valid fix

The original real behavior was:

AI-OS opened the managed Chrome.

Amazon login page opened correctly.

Around 25 seconds later AI-OS killed the Chrome because readiness incorrectly timed out.

Live diagnostics proved Chrome was actually serving DevTools successfully:

- `DevToolsActivePort` existed.
- `/json/version` returned HTTP 200 when queried correctly.
- `/json/list` returned HTTP 200.

Root cause:

The raw HTTP request in `src-tauri/src/browser/devtools.rs` used:

`Host: 127.0.0.1`

instead of:

`Host: 127.0.0.1:<dynamic port>`

The captured valid change was equivalent to:

```rust
let request =
    format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
```

After that correction, real manual Amazon login successfully reached `Connected`.

This change is considered a validated fix and should normally be retained.

### BROWSER-A baseline

Before restart-recovery experiments, focused BROWSER-A tests passed.

Known architecture:

- Managed browser discovery order:
  - Chrome
  - Chromium
  - Edge
  - Brave
- Managed profiles live under:
  - `<app data>/authenticated-browser/profiles/<provider id>`
- Provider IDs are validated.
- DevTools launch uses:
  - `--remote-debugging-port=0`
  - `--remote-debugging-address=127.0.0.1`
- Readiness reads `DevToolsActivePort`.
- Readiness verifies `/json/version`.
- AI-OS maintains a private owned-process registry.
- Persistent browser profiles are AI-OS-owned.
- Session metadata exposed outside the runtime does not include raw filesystem path or control port.
- Amazon origin validation is region-aware.
- Browser termination must remain restricted to AI-OS-owned processes.

Do not regress BROWSER-A while repairing BROWSER-B.

### Recovery v2 experiment

An experimental BROWSER-B recovery design was added.

The design attempted to keep `list_connection_capabilities()` fast and side-effect-free.

Recovery was moved into a background startup path.

The experimental design included:

saved browser metadata
→ persistent managed profile
→ headless managed Chrome
→ live provider verification
→ process-local recovery result
→ frontend recovery event

This implementation is NOT accepted.

### Experimental authenticated_runtime.rs changes

The captured experimental diff introduced launch modes equivalent to:

```rust
enum ManagedBrowserLaunchMode {
    Visible,
    Recovery,
}
```

Recovery mode added:

`--headless=new`

Manual Connect/Reconnect remained visible.

An experimental helper equivalent to this was added:

`open_managed_browser_for_recovery(...)`

A focused test verified that manual login did not use `--headless=new` while recovery did.

That unit test does NOT mean restart recovery passed real E2E.

### Real headless recovery observation

After clearing the stale managed Chrome process, a recovery run successfully started a real headless Chrome.

A real observed process contained:

`--user-data-dir=/Users/russellchen/Library/Application Support/com.russellchen.aios/authenticated-browser/profiles/amazon-consumer`

`--remote-debugging-port=0`

`--remote-debugging-address=127.0.0.1`

`--headless=new`

`https://www.amazon.com`

This proves the isolated headless launch shape can start.

It does NOT prove the restart-recovery implementation is correct.

### Amazon verifier observation

During at least one clean headless recovery run, the verifier found a real Amazon page target.

Observed:

`provider_target=found`

`origin=https://www.amazon.com`

`origin_valid=true`

but:

`signal_present=false`

`signed_in=false`

The original Amazon probe required both an account title and a sign-out control before returning an authenticated account signal.

The original selectors included:

`#nav-link-accountList-nav-line-1`

and sign-out evidence such as:

`#nav-item-signout`

or:

`a[href*="/gp/flex/sign-out"]`

The sign-out requirement may be brittle because Amazon sign-out UI may live in a menu/flyout that is not present in the initial DOM.

### Experimental Amazon verifier changes

During debugging, `src-tauri/src/browser/account_verifier.rs` was experimentally modified.

Experiments included:

- additional Amazon account-title selectors;
- removal of mandatory sign-out evidence;
- diagnostic booleans:
  - `accountTitlePresent`
  - `signInPresent`
  - `signOutPresent`
- `[browser-verifier]` diagnostic logging.

Focused verifier unit tests passed:

- every in-scope browser provider has a verifier;
- only real provider pages are probed;
- account marker remains irreversible/stable;
- Amazon verification remains region-aware;
- provider origin plus account signal are required;
- sign-in prompts do not authenticate.

Observed test result:

`6 passed; 0 failed`

These tests do NOT prove the experimental verifier is correct against the real Amazon headless DOM.

Treat these verifier changes as unaccepted experiments until the new agent inspects real behavior.

### Experimental connections.rs changes

The captured experimental diff added a process-local recovery cache equivalent to:

- `browser_recovery_cache()`
- `set_browser_recovery_result()`
- `browser_recovery_result()`
- `clear_browser_recovery_result()`

It also added a startup recovery routine equivalent to:

`recover_authenticated_browser_connections(app)`

The experimental routine:

1. loaded saved browser profiles;
2. skipped sessions that had never previously verified;
3. used saved `verified_origin`;
4. launched a recovery browser;
5. retried live verification up to 20 times;
6. stored a process-local Connected/Expired result;
7. emitted `browser-connection://recovered`;
8. closed the recovery browser after verification.

This implementation is NOT accepted.

### Experimental capability state

The experimental `list_connection_capabilities()` was changed so it did not itself launch Chrome or perform CDP operations.

Its browser state was derived from process-local recovery cache plus whether the saved session had previously verified.

Conceptually:

`Some(true) -> Connected`

`Some(false) -> Expired`

`no cache + previously verified -> Expired`

`otherwise -> Disconnected`

This was intended to keep capability listing side-effect-free.

The final authoritative connection-state architecture is still unresolved.

### Experimental frontend recovery event

`src/components/ConnectionsCenter.tsx` was experimentally changed to listen for:

`browser-connection://recovered`

and update the provider state.

A potential race was identified:

initial capability list returns Expired
→ background recovery emits Connected
→ an older/slower Connections refresh finishes later
→ stale Expired data can overwrite Connected

Do not finalize frontend recovery propagation until backend state authority and event ordering are clear.

### Experimental startup recovery

`src-tauri/src/lib.rs` was experimentally modified inside Tauri setup to spawn:

`connections::recover_authenticated_browser_connections(...)`

on a background thread.

This startup recovery thread is NOT accepted.

### Shutdown lifecycle repair attempts

After the stale managed Chrome/profile lock was identified, the next intended architecture was to add a lifecycle primitive equivalent to:

`close_all_managed_browsers()`

with this rule:

AI-OS normal shutdown
→ take only entries from AI-OS owned-process registry
→ terminate each owned browser child
→ wait/reap each child
→ release Chrome profile ownership

Attempts were then made to wire this into the Tauri application lifecycle.

Those attempts were NOT successfully validated.

Several generated patch scripts failed because their source anchors did not match the real `src-tauri/src/lib.rs`.

A later structural modification was also reported as still not working.

Therefore:

- do not assume shutdown cleanup is correctly implemented;
- do not assume `close_all_managed_browsers()` is correctly wired;
- inspect the actual working tree before changing anything;
- compile before making architectural assumptions.

### Last real lib.rs ending supplied by user

The real Tauri builder ending supplied during debugging was:

```rust
        memory::save_memory,
        memory::list_memory,
        memory::delete_memory,
    ])
    .run(tauri::generate_context!())
    .expect("error while running Tauri application");
}

pub mod task_engine;
```

Any shutdown lifecycle solution must be implemented against the repository's actual current Tauri version and actual source.

Do not guess the Tauri v2 RunEvent API.

### Last captured browser-related working tree

Before subsequent restore/shutdown attempts, the captured diff contained modifications in:

- `src-tauri/src/browser/account_verifier.rs`
- `src-tauri/src/browser/authenticated_runtime.rs`
- `src-tauri/src/browser/devtools.rs`
- `src-tauri/src/connections.rs`
- `src-tauri/src/lib.rs`
- `src/components/ConnectionsCenter.tsx`

Subsequent restore/patch attempts may have partially changed this state.

Therefore the CURRENT working tree must be treated as UNKNOWN until re-inspected.

### Required first action for Claude Code

Before modifying anything, run:

```bash
cd ~/AI-OS/dashboard
git status --short
git diff
cargo check --manifest-path src-tauri/Cargo.toml
```

Classify each Browser-related change as one of:

- validated fix;
- unaccepted experiment;
- partial shutdown attempt;
- unrelated existing work.

Do NOT blindly restore all modified files because the dynamic DevTools Host-header correction is validated and must not be lost.

### Required repair order

Claude Code must NOT continue the previous pattern of adding one diagnostic or one selector at a time.

Use this order.

#### 1. Inspect current repository state

Determine exactly what code is currently present.

Do not trust the previous experimental patches without inspection.

#### 2. Preserve the validated DevTools Host fix

Retain:

`Host: 127.0.0.1:<dynamic port>`

unless current source/testing proves a better equivalent implementation is already present.

#### 3. Re-establish stable BROWSER-A behavior

Do not regress:

- manual Amazon Connect;
- owned profile isolation;
- loopback DevTools;
- owned-process-only termination;
- safe metadata;
- region-aware Amazon origin checks.

#### 4. Solve managed-browser lifecycle first

Implement a deterministic browser shutdown primitive.

Required semantics for a function conceptually equivalent to `close_all_managed_browsers()`:

- operate only on the private AI-OS owned-process registry;
- safely remove/take owned process entries;
- terminate each owned browser child;
- wait/reap each child;
- never scan for arbitrary Chrome processes;
- never terminate unknown user browser processes.

Wire this into the real Tauri v2 application lifecycle after inspecting the repository's actual Tauri version/API.

Do NOT guess event variants.

#### 5. Real lifecycle acceptance before restart recovery

Required real E2E:

1. Start AI-OS.
2. Start an AI-OS-managed Amazon browser.
3. Confirm managed Amazon Chrome processes exist.
4. Quit AI-OS normally.
5. Wait briefly.
6. Confirm zero processes remain using `authenticated-browser/profiles/amazon-consumer`.
7. Confirm no active Chrome Singleton ownership remains.
8. Repeat at least three times.

Do not use terminal Ctrl+C as the primary normal-shutdown acceptance path.

A force-killed development shell is not equivalent to a normal Tauri application exit.

#### 6. Rebuild restart recovery only after lifecycle acceptance

Desired architecture:

saved previously verified metadata
+
AI-OS-owned persistent browser profile
→ temporary AI-OS-owned recovery browser
→ live provider verification
→ Connected or Expired
→ temporary recovery browser closes

Requirements:

- startup capability listing remains fast and side-effect-free;
- persisted metadata alone never produces Connected;
- profile existence alone never produces Connected;
- browser launch alone never produces Connected;
- live provider evidence is required;
- normal restart recovery must not open a visible browser window;
- explicit Connect/Reconnect remains visible;
- only AI-OS-owned processes may be terminated;
- recovery browser should close after verification unless execution explicitly needs an active session.

#### 7. Re-evaluate Amazon account evidence only after runtime is deterministic

Do not continue changing Amazon selectors while browser lifecycle/recovery launch remains unstable.

Final Amazon verifier must remain fail-closed.

Connected requires:

valid Amazon HTTPS origin
+
live signed-in-only account evidence

The following alone must never be considered authentication:

- profile exists;
- browser launched;
- Amazon URL opened;
- previous Connected metadata exists;
- cookie store exists.

Do not expose actual account display names, cookies, credentials, or tokens in logs/frontend state.

#### 8. Make backend connection state authoritative

Resolve any race between:

- startup capability list;
- background recovery;
- frontend refresh;
- recovery event.

Avoid this state regression:

Expired
→ recovery says Connected
→ old refresh completes
→ Expired again

Choose one authoritative backend state path and test its ordering.

### Required BROWSER-B acceptance criteria

BROWSER-B must not be marked complete until all of the following pass real E2E.

Manual Amazon Connect:

visible AI-OS managed browser
→ user logs in
→ Connected

Normal AI-OS shutdown:

all AI-OS-owned browser processes terminate
→ profile ownership released

Restart AI-OS:

no visible Amazon browser
→ temporary managed recovery browser
→ live Amazon verification
→ Connected when the real session remains authenticated

Logged-out/expired case:

live verification fails closed
→ Expired
→ never fabricates Connected

Disconnect:

AI-OS-owned browser closes
→ managed profile/connection state removed according to product semantics

User normal Chrome:

never killed
never modified
never adopted into AI-OS ownership

### Scope reminder

BROWSER-B remains incomplete.

Do not increase P15 completion status because of this Browser Recovery work.

Do not change unrelated P15 capability status while resolving this incident.

### Handoff instruction

Claude Code now owns diagnosis and repair of this BROWSER-B incident.

Priority order:

1. Inspect actual current diff.
2. Preserve validated DevTools Host fix.
3. Stabilize owned-browser shutdown lifecycle.
4. Prove lifecycle with real E2E.
5. Rebuild restart recovery cleanly.
6. Verify Amazon live account evidence.
7. Stabilize backend/frontend state propagation.
8. Run full real E2E.
9. Only then update BROWSER-B completion status.


---

## BROWSER-B Lifecycle Repair — 2026-08-30

**Status: lifecycle defect diagnosed and repaired in code. NOT accepted.**
**Restart recovery is still not implemented. BROWSER-B remains incomplete.**
**Do not increase the P15 completion count because of this work.**

### Working tree as actually found

The incident handoff said to treat the tree as unknown. It was inspected first.
It was much cleaner than feared — the unaccepted experiments had already been
rolled back. `HEAD` was `a1ea005`, branch `feature/p15-core-skills`, and only
four files were modified:

| File | Classification |
| --- | --- |
| `src-tauri/src/browser/devtools.rs` | **validated fix** — the dynamic `Host: 127.0.0.1:{port}` header. Preserved unchanged. |
| `src-tauri/src/browser/authenticated_runtime.rs` | partial shutdown attempt — `close_all_managed_browsers()` existed with correct registry-only ownership. |
| `src-tauri/src/lib.rs` | partial shutdown attempt — `build()` + `app.run()` wired to `RunEvent::Exit` only. |
| `HANDOFF.md` | the incident handoff itself. |

Not modified, i.e. the unaccepted experiments were **already gone**:
`account_verifier.rs`, `connections.rs`, `ConnectionsCenter.tsx`. There is no
recovery cache, no startup recovery thread, no `browser-connection://recovered`
event and no experimental Amazon selector change in the tree. Nothing had to be
reverted.

`src-tauri/target/debug/dashboard` was built at 06:17 from the current
`lib.rs` (06:17:27), so the shutdown wiring **did compile**. The failure was
behavioural, not a build error.

### Root cause

Four distinct defects, not one. The first two explain the orphaned Chrome; the
last two explain the `stage=launch` stall that was misread as an Amazon session
problem.

**1. Termination was `SIGKILL` only.** `close_managed_browser` and
`close_all_managed_browsers` went straight to `child.kill()`. A killed Chromium
never runs its shutdown path, so it never releases the profile's `SingletonLock`
/ `SingletonSocket` / `SingletonCookie`. This is exactly the "active Chrome
Singleton ownership state" observed on the profile.

**2. Only `RunEvent::Exit` was wired.** Confirmed against the real API for this
repository's Tauri (`tauri 2.11.5`, `tauri-runtime 2.11.3`) rather than guessed:
`RunEvent` is `#[non_exhaustive]`; `ExitRequested { code, api, .. }` is itself a
non-exhaustive struct variant; `Exit` is a unit variant;
`App::run<F: FnMut(&AppHandle<R>, RunEvent) + 'static>(self, callback: F)`.
`Exit` alone does not cover every shutdown path.

**3. The launch deleted its only evidence of an existing owner.**
`open_managed_browser` removed `DevToolsActivePort` *before* spawning. If a
previous managed Chrome still held the profile, that deleted the one file that
proved it.

**4. A handed-off launch was invisible.** With the profile still owned,
Chromium hands the URL to the surviving instance and the spawned process exits
within about a second. `wait_until_ready` never looked at the child, so it
polled for a port file that nothing would ever write, blocked the full 25 second
readiness timeout, and returned a generic "did not become ready". That is the
`stage=launch` stall. The Amazon session was never the problem here.

### Repair

`authenticated_runtime.rs`:

- `terminate_owned_process` — asks the browser to close over the loopback
  DevTools channel AI-OS already owns for that process (`Browser.close`), waits
  up to 5s for the owned child to exit, and only then falls back to
  `kill()` + `wait()`. Graceful exit is what releases the Singleton lock;
  the kill fallback keeps shutdown from hanging. Chromium often drops the socket
  before answering, so the reply is not treated as evidence — the process
  exiting is;
- `close_managed_browser` and `close_all_managed_browsers` both route through
  it, and both take entries out of the registry **before** releasing the lock
  and terminating, so a close never blocks other registry callers;
- `drain_owned_processes` empties the registry in one pass, making a second
  shutdown event a no-op instead of double work;
- `profile_is_owned_by_live_browser` — before launching, check whether the
  profile's DevTools port still answers. If it does, a browser owns the profile
  that is **not** in the owned registry. It is neither AI-OS's to terminate nor
  to adopt, so the launch is refused with a message naming the real condition
  instead of silently fighting it;
- the stale `DevToolsActivePort` is now removed only after that check proves
  nothing answers;
- `wait_until_ready` takes the owned child and stops the moment it exits, via a
  pure `readiness_progress(answered_port, owned_process_exited)` decision. A
  handed-off launch now fails in about a second with a message that says another
  browser instance is probably still using the profile;
- `profile_reused` no longer keys off `DevToolsActivePort` (which the launch
  path mutates); it keys off the profile's `Default` directory.

`lib.rs` — shutdown now runs on `RunEvent::ExitRequested` **and**
`RunEvent::Exit`, with the `_ => {}` arm the non-exhaustive enum requires.

### Ownership boundary held

Unchanged and now asserted by the verifier: only entries in the private
owned-process registry are ever terminated; no process scanning of any kind
(`pkill`/`killall`/`pgrep` are statically rejected); unknown browsers are never
adopted; the user's normal Chrome profile is never touched; DevTools stays
loopback-only; no credential, cookie, token, raw profile path or control port
leaves the runtime.

### Verification performed

- Whole `browser` module compiled clean in an isolated harness against the real
  `tungstenite 0.27` and `sha2 0.10` — **20 tests passed** (17 previous plus 3
  new).
- New behaviour tests: `shutdown_drains_the_owned_registry_and_repeats_safely`,
  `readiness_stops_as_soon_as_the_owned_browser_exits`,
  `a_live_profile_owner_is_recognised_from_the_published_port`.
- `verify/verify_p15_browser_managed_runtime.sh` gained lifecycle invariants:
  shutdown primitive exists; it is wired to both lifecycle events; the graceful
  close provably precedes the kill inside `terminate_owned_process`; no process
  scanning; the stale-owner guard exists. Its static half was run and passes.
- The Tauri lifecycle API was read from the docs for the exact pinned version,
  not guessed.

### Not done, deliberately

- **Restart recovery is still not implemented.** Per the incident handoff's own
  repair order, lifecycle acceptance comes first. Nothing was rebuilt on top of
  an unproven lifecycle.
- **The Amazon verifier was not touched.** Step 7 of the repair order says not
  to change selectors while the runtime is unstable. `account_verifier.rs` is
  byte-identical to `a1ea005`.
- **`SIGKILL` on AI-OS itself still orphans the browser.** Ctrl+C on a dev shell
  sends a signal no process can trap into an already-dead parent; a force-killed
  shell is not a normal exit. This is why the acceptance path below uses a real
  application quit.

### Required real E2E before this repair is accepted

Run on the Mac, from `~/AI-OS/dashboard`:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml browser:: connections::
bash verify/verify_p15_browser_managed_runtime.sh
bash verify/verify_p15_browser_account_verification.sh
bash verify/verify_p15_connections_onboarding.sh
npm run build
```

Then the lifecycle acceptance loop, three times, using a **normal application
quit** (Cmd+Q or closing the window), never Ctrl+C on the dev shell:

1. Start AI-OS.
2. Connect Amazon so a managed browser is running.
3. `ps aux | grep 'authenticated-browser/profiles/amazon-consumer' | grep -v grep`
   — expect the managed Chrome.
4. Quit AI-OS normally, wait a few seconds.
5. Same `ps` — expect **no** output.
6. `ls ~/Library/Application\ Support/com.russellchen.aios/authenticated-browser/profiles/amazon-consumer/ | grep -i singleton`
   — expect **no** active Singleton entries.
7. Start AI-OS and Connect Amazon again — it must open, not stall.

If step 5 or 6 still shows a survivor, capture the surviving PID's full command
line before killing it; that distinguishes "our child outlived us" from "a
helper process was re-parented", which need different fixes.

### Next step

Only after that loop passes three times: rebuild restart recovery per section
6 of the incident handoff (headless recovery browser, live verification,
`Connected`/`Expired`, one authoritative backend state path), then re-evaluate
the Amazon account evidence.

### Follow-up — Reconnect did nothing at all — 2026-08-30

Reported right after the lifecycle repair: clicking **Reconnect** produced no
browser window, **no message and no state change**.

That symptom ruled out the new stale-profile guard. A refused launch returns
`Err`, and `connect()` already catches it, sets `ERROR` and renders the text.
Silence meant `connect()` returned before it ever reached the backend.

**Frontend cause.** `connect()` opened with `if (running.current.has(providerId)) return;`
— a bare early return with no feedback. The provider stays in that set for the
whole duration of `begin_browser_login`, which can run for tens of seconds. So:
first click starts a slow launch; some other refresh repaints the card as
`EXPIRED`, which renders an enabled **Reconnect** button; every further click
hits the latch and returns silently. The button looks dead while a launch is in
fact still running.

**Backend cause, and the reason the window is so wide.** `open_managed_browser`
took the registry lock at the top and held it across profile checks, the spawn
and the entire readiness wait — up to ~25 seconds, and longer with the new
graceful-close and profile-owner probes. Every other registry caller queues
behind it: `control_port_for`, `managed_browser_is_running`,
`list_connection_capabilities`, `verify_browser_login`, and shutdown. One slow
launch froze the whole Connections panel, which is what stretched the latch
window from momentary to permanent-looking.

**Repair.**

- `open_managed_browser` now takes the registry lock only in two short critical
  sections — the "do we already own a live browser" check, and recording the new
  process — and holds nothing across the spawn and readiness wait;
- single-flight is preserved without the lock by `begin_launch`, a per-provider
  in-flight set with an RAII `LaunchGuard` that clears the marker however the
  launch ends. A second concurrent launch for one provider is refused with a
  message instead of spawning a browser that Chromium would immediately hand off
  to the first one;
- `connect()` now reports back on both early returns instead of returning
  silently, and `connectBrowser` sets a message *before* the slow invoke so the
  click has immediate feedback.

Verified: browser module compiles clean, **20 tests pass**; `npx tsc --noEmit`
passes. Verifier gained three invariants: the launch must not hold the registry
lock across readiness, concurrent launches must be guarded, and `connect()` must
always report back.

This does not change BROWSER-B status. Restart recovery is still not
implemented, and the lifecycle E2E in the section above is still the gate.

---

## BROWSER-B Restart Recovery — implemented 2026-08-30

Goal: after an AI-OS restart, Amazon stays **Connected** without the user doing
anything, and without a browser window appearing.

### Flow

```text
AI-OS starts
  -> Tauri setup seeds every previously verified provider as Pending
  -> background thread opens a HEADLESS managed browser on the persisted
     profile, at the origin the account was actually verified on
  -> live provider verification, retried up to 12 times over ~18s
  -> Connected or Expired written to the state authority
  -> recovery browser closed
  -> browser-connection://recovered emitted to the frontend
```

### One authoritative state path

`RecoveredBrowserState` (Pending / WaitingForUser / Connected / Expired) is the
single authority for browser-backed providers. Every path that learns something
writes it: `begin_browser_login`, `verify_browser_login`, recovery, disconnect.

`list_connection_capabilities` now only *reads* it. It launches no browser and
speaks no DevTools, so it is fast and side-effect free, and a slow refresh can
only read back the current answer. That removes the
`Expired -> Connected -> stale Expired` regression by construction: there is no
Expired until something decided one, and `Pending` surfaces as **Connecting**
rather than a wrong Expired that a later event would have to correct.

### Fail-closed guarantees kept

- a persisted profile alone never produces Connected;
- launching the recovery browser alone never produces Connected;
- only live provider evidence does;
- an account that never verified is not restorable at all;
- a lost website session becomes Expired, never a preserved Connected;
- recovery is headless; explicit Connect/Reconnect stays visible (asserted by
  `only_recovery_runs_headless_and_a_user_login_never_does`);
- the recovery browser is AI-OS-owned and closed after verification.

### Amazon evidence change

Requiring a sign-out control made a signed-in account read as signed out: that
control lives in a flyout that is not always in the initial DOM, which is what
produced `signal_present=false` during headless runs. The evidence is now the
account greeting **plus the absence of any sign-in affordance**
(`#nav-link-accountList[href*="/ap/signin"]`, `#nav-signin-tooltip`,
`form[action*="/ap/signin"]`, `#ap_email`, `#ap_password`). Still fail-closed:
no greeting, or any sign-in surface present, is never an authenticated account.

### Verification

Browser module **21 tests pass**; extracted connections logic **4 tests pass**;
`npx tsc --noEmit` passes. Not yet built or run on macOS.

---

## BROWSER-B Root Cause — the spawned process is not the browser — 2026-08-30

Diagnostics finally made this visible. Every launch, recovery and manual alike,
failed identically and in **408 ms**:

```text
08:26:05.486 [recovery] provider=amazon-consumer stage=launch has_verified_origin=true
08:26:05.894 [launch]   provider=amazon-consumer outcome=not_ready
                        detail=The managed browser exited before it was ready.
```

### The mistake in the ownership model

Chromium on macOS re-execs itself. The process AI-OS spawns exits within about
a second while the real browser carries on, detached. The runtime assumed
`std::process::Child` **was** the browser. It never was.

That single wrong assumption produced every symptom in this incident:

- `close_all_managed_browsers` killed an already-dead pid, so the real browser
  survived AI-OS — the orphaned Chrome and its held Singleton lock;
- `control_port_for` required `child.try_wait() == Ok(None)`, so it returned
  `None` for a perfectly healthy browser — account verification could never run,
  which is why Amazon never reached `Connected`;
- the readiness fast-fail added in the lifecycle repair turned the normal
  re-exec into an immediate hard failure, which is why launches that used to
  work stopped working and Reconnect showed `ERROR`.

### The fix: own the browser the browser identifies

- readiness no longer treats the spawned process exiting as failure. It keeps
  waiting on the DevTools endpoint, which is the authority on whether a browser
  came up;
- after readiness, AI-OS asks the browser for its own pid over the loopback
  channel (`SystemInfo.getProcessInfo`) and records it. The id comes from the
  browser running in AI-OS's own profile, over AI-OS's own control channel —
  nothing is scanned, name-matched or guessed;
- liveness everywhere (`control_port_for`, launch reuse, inspect) is now "the
  control channel answers", not "the child is alive";
- termination asks `Browser.close`, waits for the control channel to stop
  answering, and only then falls back to the two handles AI-OS legitimately
  holds: the spawned child and the browser-reported pid.

The ownership boundary is unchanged: no scanning, no adoption, no killing
anything AI-OS did not launch into its own profile.

Browser module: **21 tests pass**. Not yet run on macOS.

### Why no browser ever appeared, and why AI-OS froze — 2026-08-30

Second diagnostics pass. Two independent faults, both now fixed.

**1. A browser from a previous AI-OS run still owned the profile.**

```text
08:32:15.117 [recovery] stage=launch
08:32:15.322 [launch]   spawned_process_exited=true waiting_for_control_channel
08:33:04.842 [launch]   outcome=not_ready detail=exited without publishing a control channel
```

The spawned process died in ~200 ms and no DevTools endpoint ever appeared —
the signature of Chromium handing a launch off to whatever already owns the
profile. The earlier stale-owner guard could not see it, because that holder no
longer answers DevTools (its port file had been deleted by a previous launch),
so every launch, recovery and manual alike, failed the same way.

AI-OS now reclaims its own profile before launching: politely over DevTools
while the holder still answers, otherwise through the pid named by the
`SingletonLock` **inside AI-OS's own managed profile directory**. That pid is
verified to still be one of the supported browsers before any signal is sent, so
a recycled pid cannot be hit. Nothing is scanned, searched for or adopted — the
evidence comes from AI-OS's own directory, and only a browser AI-OS launched
into that profile can have written it.

**2. The 10–20 second freeze.**

`begin_browser_login`, `verify_browser_login` and `disconnect_connection_provider`
were synchronous Tauri commands, and Tauri runs those on the main thread. Each
waited up to 25 s on a real browser, freezing the window. They are now `async`
and do their work on `spawn_blocking`.

Also added: the chosen browser is recorded, and the managed browser's own stderr
is captured to `browser-launch-stderr.log`, so a browser that refuses to start
can say why in its own words instead of being guessed at.

Browser module: **22 tests pass**.

### Restart recovery confirmed working; manual login watched by the backend — 2026-08-30

Restart recovery is now real. Three consecutive AI-OS starts each restored the
account without a window appearing:

```text
08:39:02 [launch]   outcome=ready mode=Headless
08:39:08 [verifier] origin_valid=true signal_present=true authenticated=true
08:39:11 [recovery] completed=Connected      (repeated at 08:40 and 08:43)
```

The reclaim also fired and worked on the first of those runs:
`[reclaim] profile_held_by_previous_browser=true` → `outcome=released signal=-TERM`.

**Remaining fault: a completed manual login was never noticed.**

```text
08:45:00 [launch]   outcome=ready mode=Visible
08:45:09 [verifier] origin_valid=true signal_present=true authenticated=false
                    (no further verification, ever)
```

The one verification that ran happened before the user finished signing in — the
page still read "Hello, sign in", which the sign-in-prompt rule correctly
refused. Nothing checked again afterwards, because verification only happened
while the frontend's 2.5s poll was alive, and that poll is fragile: it is
rebuilt from `sessions` state on every change and its handler has no `catch`.

Fixed by moving it off the frontend entirely. `begin_browser_login` now starts a
backend watcher that verifies every 2s for up to three minutes, and on success
writes the state authority, saves the session and emits the same
`browser-connection://recovered` event that restart recovery uses. It gives up
early only after five consecutive "no managed browser" results, so a transient
probe failure cannot end the watch, and a browser the user closed does.

Also closed the last two unlogged early returns in `verify_managed_account`, so
every verification outcome is now on the record.

---

## BROWSER-B — accepted for Amazon — 2026-08-30

**Status: accepted by the user for Amazon. BROWSER-C is not done.**
**P15 completion status is unchanged by this work.**

### What real E2E actually proved

One continuous run, from the diagnostics record:

```text
08:52:12 [reclaim]  previous_browser_asked_to_close=true
08:52:19 [launch]   outcome=ready mode=Visible          visible login window
08:52:27 [login]    outcome=connected attempt=0         sign-in noticed by the backend
08:53:10 [close]    outcome=graceful                    normal quit closed the browser
08:53:55 [recovery] startup restorable=1
08:54:08 [verifier] origin_valid=true signal_present=true authenticated=true
08:54:11 [recovery] completed=Connected                 restored, no window
```

Accepted behaviours:

- manual Connect opens a visible AI-OS-managed browser and reaches `Connected`
  once the user actually signs in;
- Disconnect closes the browser, removes the managed profile and the stored
  website session, and the card returns to `DISCONNECTED`;
- Connect after a Disconnect opens a fresh visible login, as expected, and
  requires a new sign-in because the session really was removed;
- AI-OS restart restores `Connected` on its own, headless, with no window;
  observed on four separate starts (08:39, 08:40, 08:43, 08:54);
- normal quit closes the owned browser gracefully.

### Honest limits

- **Only Amazon has been proven.** The Taobao, JD and Pinduoduo verifiers exist,
  are unit-tested and are fail-closed, but none has been run against a real
  signed-in page. Their selectors should be treated as unverified.
- **A force-killed dev shell still orphans the browser.** Observed: the browser
  from the 08:44 session survived until it was reclaimed at 08:52. A SIGKILLed
  parent cannot run shutdown, so this is not fixable in-process. It is now
  *recovered from* rather than fatal: the next launch reclaims the profile.
- Restart recovery adds roughly 15 seconds of headless browser work at startup
  per previously connected provider. It is off the UI thread, but it is not
  free, and it scales with the number of connected browser providers.
- `browser-diagnostics.log` and `browser-launch-stderr.log` are gitignored and
  intentionally kept. They are what turned this from guesswork into diagnosis
  and are worth keeping until BROWSER-C is finished.

### Next

BROWSER-C: real E2E with Taobao, then JD and Pinduoduo, against the same
runtime. Do not assume their selectors work; check the `[verifier]` lines for
`signal_present` before changing anything.

## BROWSER-C — Taobao, JD, Pinduoduo — 2026-08-30

The runtime is already shared with Amazon and needs no further work: profile
reclaim, headless restart recovery, the backend login watcher, graceful
shutdown and the state authority are all provider-independent. The only
unproven part was the page evidence, and its selectors could not be authored
blind.

All four probes now share one shape and **report their own evidence**:

```json
{"origin": "...", "signal": "...|null",
 "evidence": {"key": "which candidate selector matched|null", "login": true|false}}
```

so a failed verification says *which* selector matched and whether a sign-in
affordance was on the page, instead of only that it failed. The matched text is
never recorded — `key` is a selector name, not a nickname.

Each provider tries several candidate selectors in order:

- **Taobao** — `.site-nav-login-info-nick`, `.site-nav-user .nick`,
  `#J_SiteNavLogin .site-nav-user`, `.nick-name`, `[class*="userNick"]`,
  `.site-nav-mytaobao .site-nav-menu-hd`
- **JD** — `#ttbar-login .nickname`, `.nickname`, `#ttbar-login a.link-nickname`,
  `[class*="nickname"]`, `.user-info .name`
- **Pinduoduo** — `.user-info .nickname`, `[class*="nickname"]`, `.user-name`,
  `[class*="userName"]`, `.personal-info .name`

One trap this avoids: on all three sites the **logout** link points at the login
host (`login.taobao.com/member/logout.htm`, `passport.jd.com/uc/login?...logout`).
Treating that as a sign-in affordance would make a signed-in account read as
signed out forever, so every sign-in selector excludes `[href*="logout"]`. A
test enforces it.

Host lists widened: Taobao now covers Tmall (shared account), JD covers
`home.jd.com` and `my.jd.com`, Pinduoduo covers the bare `yangkeduo.com` and
`mobile.pinduoduo.com`.

**Status: not accepted.** Nothing here has run against a real signed-in page.
Read the `[verifier]` lines after a real login: `matched=` names the selector
that worked, `login_present=` says whether a sign-in affordance was seen. If
`matched=none`, the candidate list is wrong for that site; if `login_present=true`
while signed in, the sign-in selector is catching something it should not.

### Why the three sat at Connecting, and why JD expired — 2026-08-30

All three signed in and reached `Connected`. On restart they stayed
`Connecting` for minutes and JD then went `Expired`. Two independent causes.

**1. Every verification cost about six seconds.**

`http_get` read until EOF, but Chromium keeps the DevTools HTTP connection open,
so each call paid a full 3 s read timeout — twice per verification, plus the
WebSocket round trip. With 12 recovery attempts per provider and providers
recovered **one after another**, four providers took six to seven minutes, and
everything after the first sat at `Connecting` the whole time.

Fixed both ways: `http_get` now stops as soon as the declared `Content-Length`
has arrived, and recovery runs one thread per provider. Each provider has its
own profile and its own browser, so there was never a reason to serialise them.

**2. Recovery was opening a page that cannot show an account.**

```text
[verifier] provider=jd-consumer origin_valid=true signal_present=false
           matched=none login_present=false authenticated=false   (x12)
```

Neither an account element nor a sign-in affordance was found — the page had
nothing on it. Recovery navigated to `session.verified_origin`, which for JD is
`passport.jd.com`: the host the login happened on, which shows no account state.
Amazon worked only because its verified origin *is* the storefront.

`account_home_url` now decides where recovery looks: Amazon keeps its verified
regional storefront, while Taobao, JD and Pinduoduo go to `www.taobao.com`,
`www.jd.com` and `mobile.yangkeduo.com`. A test asserts each destination still
passes that provider's own origin check.

Probe evidence also now reports `path`, `ready` and `links`, so a future miss
distinguishes a wrong selector from a blank, unloaded or challenge page. The
path is AI-OS's own navigation target, not browsing history.

### Three different faults, one per provider — 2026-08-30

Recovery is now fast and parallel: all four launch together and Amazon finished
in five seconds. Each remaining failure had its own cause, and the probe
evidence named all three.

**Taobao — the nickname was found and then vetoed.**

```text
matched=nick login_present=true path=/ ready=complete links=1717
```

`.site-nav-login-info-nick` matched, so the account *was* there. The sign-in
veto fired anyway: `a[href*="login.taobao.com"]` matches Taobao's own header
links on a signed-in page. The candidate `my-taobao`
(`.site-nav-mytaobao .site-nav-menu-hd`, "我的淘宝") was also wrong — it is
present whether or not anyone is signed in, so it was a false positive waiting
to happen and is gone. The veto is now only the standalone login page
(`#login-form`, `.login-blocks`) and a login URL specific enough that
`/member/logout.htm` cannot satisfy it.

**JD — no candidate matched a loaded page.**

```text
matched=none login_present=false path=/ ready=complete links=330
```

The page was fully loaded, so the selectors were simply wrong. JD now falls back
to the whole `#ttbar-login` bar: signed out it reads "你好，请登录" and the
sign-in-prompt rule refuses it; signed in it is the nickname. That is the same
shape that works for Amazon.

**Pinduoduo — an empty page.**

```text
links=0 ready=complete
```

Not a selector problem: nothing rendered at all. `--headless=new` advertises
`HeadlessChrome` in its user agent and the site served it nothing. Recovery now
launches with a desktop user agent built from the installed browser's own
version (`--version`), so it presents the same browser the user signed in as. It
claims nothing the user's own Chrome does not already claim, and only headless
recovery does it — a visible login keeps the real user agent, which a test
asserts.

The logout-link invariant is now stated as what it actually protects: a sign-in
selector must never match a login host generically, because on all three sites
the logout link points at the login host.

### The captures corrected two wrong assumptions — 2026-08-30

Taobao works. The header captures showed JD and Pinduoduo were failing for
reasons the selector evidence had pointed at wrongly.

**Pinduoduo was never an empty page.** `links=0` was read as "nothing
rendered". The capture shows a fully rendered mobile storefront, with the
user's own previous search still in the box — the session was alive the whole
time. Pinduoduo's mobile web uses `div` click handlers and has essentially no
`<a>` elements, so a link count of zero is normal there. The real problem is
that its mobile home has no account area at all.

**JD's storefront home does not render its account bar.** The capture is a
near-blank strip with only the logo: the top bar carrying "你好，请登录" or the
nickname never appeared, which is why every selector, including the whole
`#ttbar-login` fallback, found nothing on a "loaded" page.

Both were the same mistake — looking at a page that cannot answer the question.
Recovery now opens a page only a signed-in account can reach:
`https://home.jd.com/` and `https://mobile.yangkeduo.com/personal.html`. Being
there without having been bounced to a login page is itself live evidence, so
the probes treat a login path, a login host or a login form as the negative and
fall back to the page's own title as the positive. Signed out, both sites send
the browser to a login page, whose path and title the sign-in-prompt rule
already refuses.

Evidence now also carries the page title, which is what distinguishes an
account page from a login page.

---

## User-added authenticated browser sites — 2026-08-30

**Status: implemented, not yet run on macOS. The four built-ins are unchanged
and still work; this is additive.**

Users can now add any site themselves instead of choosing from a fixed four.
The whole managed-browser runtime is reused unchanged — profile ownership,
reclaim, headless restart recovery, the login watcher, graceful shutdown, the
state authority, diagnostics. Only the account evidence had to be solved
generically, because AI-OS cannot ship selectors for a site it has never seen.

### Two mechanisms, either of which suffices

**An account page.** If the user names a page only a signed-in account can
reach, staying on it is the evidence and being sent to a login page is the
refusal. This needs nothing about the site's markup and is tried first.

**Evidence learned at sign-in.** The managed browser samples a generic battery
of 14 structural selectors — logout affordances, account/nickname/avatar
markers, login forms, password fields, QR logins, captchas — before the user
signs in, and again when they press **I've signed in**. What appeared, and what
disappeared, is the discriminator, and it is stored.

Both are fail-closed. A site with neither an account page nor learned evidence
can be opened and signed in to, but can never report Connected.

### What is deliberately refused

- A key present both before and after sign-in is discarded: keeping it would
  make a signed-out page pass later.
- A login surface appearing (password field, login form, QR login, captcha) can
  never be learned as proof of an account, whatever the diff says.
- If nothing distinguishable appeared, no evidence is stored at all and the user
  is told to name an account page instead. Storing evidence that always passes
  would be worse than storing none.
- Host lists are exact, never a derived parent domain: getting eTLD+1 right
  needs a public suffix list, and guessing widens what counts as the site. Hosts
  grow only when a sign-in is actually verified somewhere on the same site.
- https only, everywhere, and any URL with credentials in it is rejected.
- Derived provider ids are prefixed `site-`, so a user site can never collide
  with or impersonate a built-in provider.

### Privacy

The generic probe reports **which selector keys matched** and nothing else — no
page text, no account name, no `textContent` at all. A user site is therefore
verified structurally, and AI-OS never learns whose account it is; its stored
marker records that a session on that site was verified, not who owns it. A
test and the verifier script both enforce that the probe reads no cookie,
storage or text.

### Surface

- `add_browser_site` / `remove_browser_site` / `list_browser_sites` /
  `confirm_browser_login`, stored in `browser-sites.json` under app data.
- User sites appear in the Connections list alongside the built-ins, with
  **I've signed in** and **Remove** buttons and an **Add a site** form.
- Removing a site removes its stored browser session too.

Browser module: **38 tests pass**, `npx tsc --noEmit` passes.

---

# ===== HANDOFF TO NEXT AGENT — P15 Authenticated Browser — 2026-08-30 =====

**Read this section first. It supersedes every earlier Browser section in this
file where they conflict.** Everything above it is the chronological record of
how these conclusions were reached; useful for *why*, not for *what is true now*.

## 1. Do this before anything else

The last commit has **never been compiled on macOS**. This session could not run
`cargo` (the machine reaching the repo has no Rust toolchain and cannot build a
macOS Tauri target). Everything was validated in an isolated harness against the
real `tungstenite`, `sha2` and `base64` crates, plus `npx tsc --noEmit` — but the
Tauri-coupled code in `connections.rs` and `lib.rs` has only been reviewed, not
built.

```bash
cd ~/AI-OS/dashboard
git status --short          # expect clean at 75e85a5
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml browser:: connections::
npm run build
bash verify/verify_p15_browser_managed_runtime.sh
bash verify/verify_p15_browser_account_verification.sh
bash verify/verify_p15_connections_onboarding.sh
```

If `cargo check` fails, it will be in `connections.rs` or `lib.rs`, most likely
around the four new commands (`add_browser_site`, `remove_browser_site`,
`confirm_browser_login`, `list_browser_sites`) or the `capabilities()` change.
The `browser/` module itself compiled clean with **38 tests passing**.

## 2. Status — accepted vs unproven

| Area | Status |
| --- | --- |
| Managed browser lifecycle (ownership, reclaim, shutdown) | **Accepted**, real E2E |
| Amazon: connect / disconnect / reconnect / restart recovery | **Accepted** by the user |
| Taobao, JD, Pinduoduo: connect and restart recovery | **Accepted** by the user |
| User-added arbitrary sites | **Implemented, never built or run** |
| Microsoft Graph external E2E | Still blocked on `AI_OS_GRAPH_E2E_DRIVE_ID` |

**P15 completion status is unchanged by all of this. Do not raise it.**

## 3. The one bug that caused almost everything

`std::process::Child` was treated as the browser. On macOS Chromium re-execs:
the spawned process exits in ~200 ms while the real browser continues detached.
That single wrong assumption produced the orphaned Chrome that held the profile
`Singleton` lock, the verification that could never run (`control_port_for`
demanded a live child), and a readiness fast-fail that broke every launch.

**The rule now: the DevTools control channel is the authority on whether a
browser exists, never the spawned child.** If you find yourself reaching for
`child.try_wait()` to answer "is the browser alive", it is the wrong question.

## 4. Invariants that must not be regressed

The verifier scripts assert most of these. Breaking one silently is how this
incident started.

- Only processes in the private owned-process registry are terminated. **No
  process scanning ever** — `pkill`, `killall`, `pgrep` are statically rejected.
  The one exception is the profile reclaim, which acts on the single pid named
  by the `SingletonLock` **inside AI-OS's own managed profile directory**, and
  only after confirming that pid is still a supported browser.
- A browser is asked to close (`Browser.close`) **before** it is killed. Only a
  graceful exit releases the profile Singleton lock; a test asserts the ordering.
- DevTools stays loopback-only, fail-closed.
- `CONNECTED` requires live account evidence. A persisted profile, an opened
  profile, a launched browser and previous Connected metadata are each, and
  together, insufficient.
- Restart recovery is headless; an explicit Connect/Reconnect is visible. A test
  asserts a user login never gets `--headless=new`.
- No credential, cookie, token, raw profile path or control port leaves the
  runtime. Verifier probes never read `document.cookie`, `localStorage`,
  `sessionStorage`; the generic site probe additionally never reads
  `textContent`.
- Amazon is region-aware. The verified origin is whatever regional site the user
  actually signed in on, never assumed.
- The launch never holds the registry lock across spawn/readiness, or one slow
  launch freezes all of Connections.
- `begin_browser_login`, `verify_browser_login`, `disconnect_connection_provider`
  and `confirm_browser_login` are `async` + `spawn_blocking`. As synchronous
  commands Tauri runs them on the main thread and the window freezes.

## 5. How the pieces fit

```text
Connections
  → begin_browser_login            visible managed browser at the login URL
  → backend login watcher          verifies every 2s for 3 minutes
  → RecoveredBrowserState          the single state authority
  → list_connection_capabilities   reads that authority, launches nothing

AI-OS start
  → refresh_browser_site_registry  user sites must be known first
  → begin_authenticated_browser_recovery
      seeds Pending synchronously  → shown as "Connecting", never a wrong Expired
      one thread per provider      → headless browser on the persisted profile
      live verification            → Connected or Expired, browser closed
      browser-connection://recovered
```

Files: `browser/authenticated_runtime.rs` (process/profile ownership),
`browser/devtools.rs` (loopback transport), `browser/account_verifier.rs`
(built-in provider evidence), `browser/site_registry.rs` (user-added sites),
`browser/diagnostics.rs` (the on-disk record), `connections.rs` (state authority,
recovery, commands).

## 6. Diagnostics — use these instead of guessing

This incident cost many rounds of blind selector guessing. It stopped the moment
the code started reporting what it saw. All three are gitignored.

- `browser-diagnostics.log` — every launch, close, reclaim, recovery and
  verification outcome. The `[verifier]` line carries `matched=` (which candidate
  selector hit), `login_present=`, `path=`, `ready=`, `links=`, `title=`.
- `browser-launch-stderr.log` — the browser's own stderr, for a browser that
  refuses to start.
- `browser-page-<provider>.png` — the page header a failed recovery was looking
  at, clipped to the top 220 px.

**The captures corrected two conclusions that the text evidence had pointed at
wrongly** (Pinduoduo's `links=0` was normal for a site with no `<a>` elements,
not an empty page; JD's storefront simply never renders its account bar). If a
provider fails, read the log and look at the capture before changing a selector.

## 7. Known limits, stated honestly

- **A force-killed AI-OS still orphans the browser.** Ctrl+C on a dev shell
  cannot run shutdown. It is *recovered from* — the next launch reclaims the
  profile — not prevented. Use a normal quit when testing shutdown.
- **Provider selectors are DOM-shape dependent and will rot.** They fail closed,
  so a stale selector shows `WAITING_FOR_USER` or `EXPIRED`, never a false
  `CONNECTED`.
- **Recovery costs ~15 s of headless browser work per connected provider** at
  startup. Parallel across providers, off the UI thread, but not free.
- **A user site verified structurally has no account identity.** AI-OS knows a
  session on that site is valid, not whose it is. That is deliberate.
- User sites are not in the Connect All ordering (`order` in
  `ConnectionsCenter.tsx`, `ConnectAllOnboarding::new()`); they are connected
  individually.
- Adding a *built-in* provider still touches ~8 places across three files. If
  more built-ins are planned, consolidate them into one table first — user sites
  already avoid this entirely.

## 8. Suggested next steps

1. Build and run the checks in §1. Fix whatever `cargo check` finds.
2. Exercise the user-added-site flow end to end on a real site. Prefer a site
   with a "my account" page for the first test: that path needs no learning.
3. If a site cannot be verified either way, the honest outcome is to tell the
   user to name an account page — not to loosen the fail-closed rules.
4. Then, separately, the Microsoft Graph fixture blocker.

# ===== HANDOFF UPDATE — P15 Authenticated Browser User-Added Site E2E — 2026-08-30 =====

This section supersedes the earlier user-added-site Browser status where it conflicts.

## Acceptance completed

Authenticated Browser deterministic acceptance passed on macOS:

- `cargo check --manifest-path src-tauri/Cargo.toml` — PASS
- Browser Rust tests — 41 passed, 0 failed
- Connections Rust tests — 12 passed, 0 failed
- Frontend production build — PASS
- `verify_p15_browser_managed_runtime.sh` — PASS
- `verify_p15_browser_account_verification.sh` — PASS
- `verify_p15_connections_onboarding.sh` — PASS
- `git diff --check` — PASS

Verifier maintenance performed during acceptance:

- updated the managed-runtime verifier from the obsolete spawned-process readiness test name to the current Chromium re-exec-aware readiness test;
- limited the account verifier forbidden-API static scan to production code so its own safety tests may name forbidden APIs without producing a false failure;
- replaced an invalid Cargo invocation containing two test filters with the valid `browser::` filter;
- replaced the obsolete restart/profile-reuse connection test requirement with the current restart-recovery and state-authority tests.

These changes update stale acceptance infrastructure to the current Browser architecture; they do not weaken Browser safety requirements.

## User-added arbitrary site real macOS E2E

Real E2E accepted using GitHub as a user-added site.

Configuration:

- sign-in page: `https://github.com/login`
- account page: `https://github.com/settings/profile`

Observed real flow:

1. User-added GitHub was created in Connections.
2. AI-OS launched its owned visible managed browser.
3. The user signed in on GitHub's real login page.
4. The user selected `I've signed in`.
5. AI-OS verified the authenticated account page and changed the connection to `CONNECTED`.
6. AI-OS was exited normally.
7. AI-OS was restarted.
8. GitHub automatically recovered as `CONNECTED` without requiring another manual sign-in.

Therefore user-added arbitrary authenticated sites are now accepted on macOS for the account-page verification path.

The fail-closed behavior was also observed in the same E2E work: a user-added site with neither a stored account page nor learned structural evidence did not become Connected.

GitHub's generic structural-learning-only path was not used as the acceptance criterion because an explicit account page provides stronger deterministic evidence and is already the preferred supported path when available.

## Google note

Google rejected login from the managed Chromium environment with its own "browser or app may not be secure" policy. This is an external provider/browser-policy restriction and is not treated as a failure of the generic user-added-site implementation.

## Current Browser status

Authenticated Browser macOS scope now has real E2E acceptance for:

- Amazon
- Taobao
- JD
- Pinduoduo
- user-added arbitrary sites through the account-page verification path

User-added sites remain excluded from Connect All.

The Browser safety invariants remain unchanged:

- no adoption or termination of arbitrary user browser processes;
- only AI-OS-owned managed profiles/processes are controlled;
- DevTools control channel is the browser-liveness authority;
- loopback-only DevTools access;
- Connected requires live authentication evidence;
- no cookie, storage, token, password, authorization header, account identity, raw profile path, or control port leaves the Browser runtime;
- verification remains fail-closed.

P15 completion count remains unchanged; Browser acceptance does not by itself increase the completed Core Skill count.

### GM-2A — Local ComfyUI Ready Detection — Completed

GM-2A establishes the real local ComfyUI discovery, lifecycle and Ready First
foundation for Generative Media.

Implemented and verified:

- Direct ComfyUI Local API probing through Rust HTTP, without shelling out to
  `curl` in the production adapter.
- Positive ComfyUI identification uses both `/system_stats` and `/object_info`;
  an arbitrary JSON HTTP service is not accepted as ComfyUI.
- Official Comfy Desktop installations are discovered on macOS from
  `~/Library/Application Support/Comfy Desktop/installations.json`.
- Local discovery keeps `standalone + installed` instances visible even when
  runtime files are damaged, so an installed-but-broken environment is not
  incorrectly classified as `not_installed`.
- Comfy Cloud registry entries are not treated as local ComfyUI engines.
- The macOS adapter resolves the real local Python runtime and `main.py` from
  the discovered Desktop installation.
- AI-OS can start the discovered ComfyUI backend directly without requiring the
  Comfy Desktop UI to be opened.
- AI-OS chooses a dynamic loopback port rather than assuming port 8188.
- Managed Comfy Desktop model-path configuration and shared input/output
  directories are preserved when starting the backend.
- Backend startup waits for the real Local API and validates it through the
  platform-neutral ComfyUI API probe.
- Backend lifecycle ownership is explicit: an AI-OS-started test backend is
  stopped and reaped after use.
- Local runtime facts feed `LocalMediaReadinessEvidence`.
- API reachability alone never means Ready.

Real-machine GM-2A acceptance on macOS verified:

- local instance discovery: passed
- runtime validation: passed
- direct backend startup: passed
- dynamic Local API endpoint: passed
- ComfyUI API health: passed
- backend shutdown: passed
- detected ComfyUI version: `0.34.5`
- final runtime state: `InstalledNotConfigured`
- `engine_installed = true`
- `engine_startable = true`
- `api_reachable = true`
- generation-profile requirements are intentionally still unverified, so
  `Ready` is false

Technical decisions established by GM-2A:

- The core local execution and health contract is the direct ComfyUI Local API.
- `comfy-cli` may be integrated later as an optional deterministic management
  interface, but it is not a hard dependency of local execution.
- `comfyui-mcp` may be used later as a secondary workflow-intelligence or
  authoring extension; it is not part of the core execution path.
- Platform-neutral ComfyUI API logic remains separate from platform-specific
  discovery, startup and filesystem handling.
- On macOS, official Comfy Desktop registry discovery is handled by the macOS
  Generative Media adapter rather than the provider-neutral core.
- Starting the Comfy Desktop application UI is not treated as proof that its
  local backend started.
- GM-2A does not install, configure or repair ComfyUI. Those remain later
  setup/repair responsibilities.
- `Ready` remains reserved for a generation profile whose workflow, required
  assets, custom nodes, integrity, smoke generation and output retrieval have
  all passed.

Final acceptance command:

`verify/verify_p15_gm_2a_ready_detection.sh`

Next Generative Media milestone:

**GM-2B — Workflow / Model / Node Readiness**

GM-2B must turn the currently healthy-but-not-configured local runtime into
profile-aware readiness by validating compatible workflows, required models,
VAE/text encoders/auxiliary assets, custom nodes and managed-asset integrity.

### Verification boundary — Core vs External E2E

Technical decision:

- Core verification must be deterministic and must not depend on mutable user-account state or interactive external application availability.
- Real Microsoft Office AppleEvent automation and real cloud-account tests belong to External E2E.
- External E2E environment prerequisites are not product regressions:
  - missing test fixtures -> `SKIP`
  - required user reauthorization -> `SKIP`
  - unavailable or temporarily unresponsive external application automation -> `SKIP`
- Once an External E2E environment is usable, genuine capability/assertion failures remain `FAIL`.
- `done.sh` therefore continues to gate on real product failures without allowing unrelated account expiry or GUI automation availability to invalidate an unrelated feature checkpoint.
- 2026-09-07 19:36  P15 GM-2A Local ComfyUI Ready Detection complete

### GM-2B — Workflow / Model / Node Readiness — Completed

GM-2B adds profile-aware readiness on top of the healthy local ComfyUI
runtime established by GM-2A.

First managed local profile:

`comfyui-checkpoint-t2i-v1`

Technical decisions:

- AI-OS owns the first API-format text-to-image workflow rather than requiring
  an ordinary user to create or save a ComfyUI workflow JSON manually.
- The first profile deliberately uses only ComfyUI core nodes:
  `CheckpointLoaderSimple`, `CLIPTextEncode`, `EmptyLatentImage`, `KSampler`,
  `VAEDecode`, and `SaveImage`.
- Therefore the first profile requires no custom nodes. Custom-node support
  remains part of the profile contract for future profiles rather than a
  prerequisite for the simplest local generation path.
- `/object_info` is the runtime authority for workflow node/input compatibility
  and checkpoint advertisement. Source-code presence alone is not readiness.
- A model becomes an AI-OS managed asset only through a profile manifest.
  The manifest pins profile ID/version, checkpoint path, byte size and SHA-256.
- Managed checkpoint paths are relative to the ComfyUI models root, must stay
  under `checkpoints/`, cannot traverse outside that root, and symlinks are
  refused for the managed profile.
- A checkpoint that exists on disk but is not advertised by the running ComfyUI
  instance is not considered usable.
- A checkpoint whose size or SHA-256 differs from the managed manifest fails
  integrity and cannot contribute to Ready.
- Arbitrary user-downloaded models are not silently promoted to trusted managed
  assets merely because ComfyUI can see them.
- Platform-neutral workflow/model/node/integrity logic lives in
  `generative_media/comfyui_profile.rs`.
- macOS-specific Comfy Desktop model-path discovery and backend lifecycle remain
  isolated in `generative_media/comfyui_macos.rs`.

Real-machine acceptance:

- ComfyUI installation: present
- backend start: passed
- Local API: passed
- first profile workflow compatibility: passed
- required core nodes: passed
- custom-node requirement: passed; this profile requires none
- current shared model inventory: no usable managed checkpoint
- managed profile manifest: not installed yet
- asset readiness: false
- integrity readiness: false
- smoke generation: intentionally not performed in GM-2B
- output retrieval: intentionally not performed in GM-2B
- final state remains `InstalledNotConfigured`; GM-2B does not fabricate Ready

Final acceptance:

`verify/verify_p15_gm_2b_profile_readiness.sh`

Roadmap adjustment from real-machine evidence:

The acceptance Mac currently has no generation model or managed profile asset.
A real GM-2C `/prompt` execution acceptance therefore cannot be completed
without first configuring a profile. Installation/configuration/repair belongs
to GM-3, not GM-2C.

The execution order is therefore:

`GM-2B -> GM-3 One-click Setup/Repair -> GM-2C Execution -> GM-2 Final Provider Integration`

This is an ownership-preserving reorder, not a scope expansion. GM-3 must make
the managed profile usable; GM-2C must then prove real prompt submission,
progress/history/cancellation and output retrieval against that profile.
- 2026-09-07 20:53  P15 GM-2B Workflow Model Node Readiness complete

### GM-3 — One-click Setup / Repair — Completed

GM-3 converts the first profile-aware ComfyUI configuration into an
AI-OS-managed, reproducible local generation setup.

Managed profile:

`comfyui-checkpoint-t2i-v1`

Bootstrap checkpoint:

`v1-5-pruned-emaonly-fp16.safetensors`

Technical decisions:

- The Stable Diffusion v1.5 FP16 checkpoint is a bootstrap compatibility model,
  not the long-term AI-OS quality recommendation.
- Setup / Repair is an explicit user-confirmed operation.
- The managed asset is pinned by relative path, exact byte size and SHA-256.
- Downloads use an AI-OS-owned `.part` file and support HTTP Range resume.
- A completed `.part` whose hash is wrong is treated as corrupted AI-OS staging
  data, removed and downloaded again; it cannot permanently poison retries.
- Network retries are bounded. The real large-file request window is also
  bounded and is not a background infinite retry loop.
- An exact pre-existing checkpoint may be adopted only after explicit Setup /
  Repair and complete size/hash verification.
- An unrelated unmanaged file with the bootstrap filename is never overwritten.
- A damaged checkpoint already owned by the AI-OS managed profile is backed up
  before replacement.
- Verified staging bytes are atomically renamed into the ComfyUI checkpoint
  directory.
- The private managed profile manifest is persisted atomically.
- A fresh ComfyUI backend must advertise the checkpoint before setup succeeds.
- Unrelated user checkpoints, LoRAs, workflows and custom nodes are untouched.
- Setup / Repair is idempotent; an already verified managed checkpoint is not
  downloaded again.

Readiness semantics:

- `smoke_generation_ok=false` or `output_retrieval_ok=false` does not mean
  Broken until that check has actually been attempted.
- The readiness report therefore separately records
  `smoke_generation_checked` and `output_retrieval_checked`.
- After GM-3, workflow/assets/nodes/integrity may all be true while execution
  validation is still pending; that state remains `InstalledNotConfigured`.
- GM-2C owns the transition produced by real execution/output evidence.

Real-machine GM-3 acceptance verifies:

- local ComfyUI runtime
- managed checkpoint installation/adoption/repair
- resumed-download contract
- pinned SHA-256 integrity
- fresh checkpoint advertisement
- workflow readiness = true
- asset readiness = true
- custom-node readiness = true
- integrity readiness = true
- smoke generation not yet checked
- output retrieval not yet checked
- stable state remains `InstalledNotConfigured`

Final acceptance:

`verify/verify_p15_gm_3_setup_repair.sh`

Next milestone:

**GM-2C — Execution**

GM-2C now owns the real:

`/prompt -> queue/progress -> history -> cancellation -> output retrieval`

Only real execution and retrieved output may satisfy the remaining Ready First
evidence. After GM-2C, GM-2 Final Provider Integration can register the local
ComfyUI provider only when the entire Ready contract passes.
- 2026-09-07 22:39  P15 GM-3 One-click Setup Repair complete

### GM-2C — Execution — Completed

GM-2C adds the first real local ComfyUI generation, cancellation and output
retrieval evidence on top of the managed profile configured by GM-3.

Real execution path:

`POST /prompt -> queue/running -> history -> SaveImage -> GET /view`

Real cancellation path:

`running prompt -> POST /interrupt -> leaves execution queue`

Technical decisions:

- Local execution continues to use the direct ComfyUI Local API.
- `comfy-cli` remains optional management infrastructure and is not required
  for generation.
- `comfyui-mcp` remains outside the core execution path.
- GM-2C uses bounded HTTP polling for v1 rather than making WebSocket a hard
  execution dependency.
- `/prompt` must return a real non-empty prompt ID.
- Execution history has a hard deadline and fails closed on malformed,
  failed, interrupted, incomplete or output-less results.
- Successful history is not sufficient by itself. AI-OS must locate a real
  image produced by `SaveImage`.
- Output retrieval is verified through ComfyUI `/view`; a filename in history
  is not treated as proof that usable output exists.
- `/view` parameters are encoded through the existing `url` crate. No
  additional reqwest query feature is required.
- Retrieved image bytes are bounded and must have a supported PNG, JPEG or
  WebP signature.
- Running-job cancellation is validated through `/interrupt`.
- Cancellation acceptance executes only against the isolated backend started
  and owned by AI-OS for that operation.
- Smoke generation uses the same GM-2B workflow contract and GM-3 managed
  checkpoint rather than a hidden test-only model.
- The smoke workload is intentionally small; it validates execution and output
  retrieval, not image quality.
- Successful execution evidence is persisted atomically in an AI-OS-owned
  profile execution-validation record tied to profile version, checkpoint
  SHA-256, execution-contract version and observed ComfyUI version.
- GM-2C proves complete Ready evidence for this execution report but does not
  yet register the production MediaProvider. Registry ownership remains
  GM-2 Final Provider Integration.

Real-machine acceptance:

- workflow ready = true
- required assets ready = true
- custom nodes ready = true
- integrity = true
- real `/prompt` submission = passed
- running queue observation = passed
- real `/interrupt` cancellation = passed
- real smoke generation = passed
- real history completion = passed
- SaveImage metadata = passed
- real `/view` retrieval = passed
- non-empty valid image bytes = passed
- smoke generation checked = true
- smoke generation ok = true
- output retrieval checked = true
- output retrieval ok = true
- complete GM-2C execution report = `Ready`

Final acceptance:

`verify/verify_p15_gm_2c_execution.sh`

Next milestone:

**GM-2 Final — Provider Integration**

GM-2 Final must consume durable GM-2C execution evidence only while it still
matches the managed profile, checkpoint, execution contract and relevant
ComfyUI runtime identity.

It must then register local ComfyUI only when the complete Ready First contract
passes and prove the production path:

`media.text-to-image -> Runtime -> MediaRouter -> MediaProvider -> ComfyUI`

The production result must return a normalized local image asset and preserve
Local First with no silent cloud fallback.

### GM-2C closeout — Core / External verification boundary corrected

The GM-2C product implementation and its real ComfyUI acceptance passed before
the repository-wide gate exposed an unrelated verification-infrastructure
regression.

Observed accepted GM-2C evidence:

- real `/prompt` submission: passed
- real running queue observation: passed
- real `/interrupt`: passed
- retrieved output bytes: 39637
- output MIME: `image/png`
- smoke generation: true
- output retrieval: true
- complete execution validation: `Ready`

The later `done.sh` failure was not a Generative Media failure. Pages and
PowerPoint returned AppleEvent timeout `-1712` while unrelated Office verifiers
were incorrectly still classified as Core.

Verification boundary correction:

- Core verification remains deterministic and does not drive mutable external
  Office/iWork GUI application state.
- Pages Core now contains only path/parser contracts.
- Numbers Core now contains only path/grid/parser contracts.
- PowerPoint Core now contains only bounded validation, routing and permission
  contracts.
- Office Conversion Core now contains only conversion semantics and routing
  contracts.
- Real Pages, Numbers, PowerPoint and cross-suite conversion tests have explicit
  `*_real_e2e.sh` wrappers and are classified by root `verify_all.sh` as
  External E2E.
- Existing iWork availability and Keynote Presentation real E2E probes now
  treat unavailable or temporarily unresponsive AppleEvent automation as
  External `SKIP`.
- External AppleEvent timeout `-1712` is environment unavailability and is
  reported as `SKIP`.
- Once the real application environment is usable, assertion/capability
  failures remain `FAIL`; the wrappers do not convert arbitrary failures into
  skips.
- This correction does not weaken GM-2C acceptance and does not modify the
  Generative Media execution implementation.

The standing rule remains:

`deterministic product contract -> Core`

`mutable real application/account environment -> External E2E`
- 2026-09-08 00:40  P15 GM-2C ComfyUI Execution complete

### GM-2 Final — Provider Integration — Completed

GM-2 is now complete as a production local Generative Media provider rather
than only a ComfyUI readiness/execution test stack.

Production path:

`media.text-to-image`
`-> Runtime canonical MediaRequest`
`-> production MediaProviderRegistry`
`-> MediaRouter`
`-> ComfyUiLocalProvider`
`-> managed local ComfyUI`
`-> /prompt -> history -> /view`
`-> AI-OS local asset storage`
`-> normalized MediaResult`

Ready First registration:

- the production registry does not register ComfyUI merely because ComfyUI is
  installed or its HTTP API answers;
- the managed profile must still satisfy workflow, asset, custom-node and
  integrity readiness;
- the GM-2C durable execution-validation record must still match:
  - profile ID
  - profile version
  - checkpoint SHA-256
  - execution contract version
  - current real ComfyUI runtime version;
- cancellation, smoke generation and output retrieval must all remain accepted
  execution evidence;
- stale checkpoint/runtime/profile evidence cannot register a Ready provider;
- execution repeats the Ready identity check against the newly started backend
  before generating the user's image.

Provider contract:

- provider ID: `comfyui`
- source: local
- supported production capability in this first profile:
  `media.text-to-image`
- provider authorization: none
- local readiness: `Ready`
- no cloud provider is silently selected if local routing cannot proceed;
- manual provider routing remains exact;
- execution/provider failure does not trigger an automatic provider switch.

Output contract:

- ComfyUI output bytes are retrieved through `/view`;
- the bytes must be a supported PNG/JPEG/WebP image;
- AI-OS persists the generated output atomically under its managed local media
  asset storage;
- shared `MediaOutput.handle` remains opaque:
  `asset://generative-media/...`;
- no absolute macOS filesystem path is required by the provider-neutral media
  contract;
- the returned `MediaResult` contains normalized provider identity, image MIME,
  opaque asset handle and non-secret execution metadata.

Runtime ownership remains unchanged:

`Planner -> Skill Resolver -> Shared Runtime Permission Admission -> Generative Media Executor -> MediaRouter -> MediaProvider`

Generative Media still does not execute inside OpenClaw Gateway.

Real-machine GM-2 Final acceptance proved:

- durable Ready evidence matched the current managed profile/runtime;
- production registry registered the real Ready local ComfyUI provider;
- Runtime normalized a real `media.text-to-image` request;
- MediaRouter selected the local provider;
- ComfyUI executed a real generation;
- real image bytes were retrieved;
- output bytes were persisted in AI-OS managed asset storage;
- normalized image MIME was returned;
- opaque asset handle was returned;
- the production result remained Local First and Ready.

Final acceptance:

`verify/verify_p15_gm_2_final_provider_integration.sh`

GM-2 overall is complete:

- GM-2A — Local ComfyUI Ready Detection
- GM-2B — Workflow / Model / Node Readiness
- GM-3 — One-click Setup / Repair
- GM-2C — Execution
- GM-2 Final — Provider Integration

Next Generative Media milestone:

**GM-4 — Cloud Providers**

GM-4 adds replaceable authenticated cloud media providers, explicit cloud
selection and paid-cloud budget/authorization semantics without weakening the
existing Local First rule or silently falling from a failed local route into
paid cloud execution.
- 2026-09-08 01:51  P15 GM-2 Final ComfyUI Provider Integration complete

---

## 2026-09-08 — Multi-Model / Multi-Agent / Multi-Skill Platform Decision

### Product identity

AI-OS is a **multi-model, multi-Agent, multi-Skill AI application platform**.

Models, Agents, and Skills are independent replaceable extension dimensions.

The current v1.0 implementation has one operational general execution Agent:
OpenClaw.

This is a current release-scope boundary, not a single-Agent architecture
decision.

The Runtime-to-Agent contract must remain general so future operational Agents
can be added without redesigning Task Engine, Planner, or Runtime.

### Reuse First / GitHub First

Before substantial new implementation:

1. inspect whether AI-OS already has the required capability;
2. search GitHub, open-source ecosystems, and mature products;
3. evaluate license, commercial compatibility, security, maintenance,
   maturity, platform support, architecture fit, installation/runtime burden,
   and ordinary-user UX;
4. prefer Adapter / Provider / Skill / MCP integration;
5. implement only the missing AI-OS-specific layer.

AI-OS should minimize custom implementation when mature compatible products
already solve the specialized problem.

AI-OS owns orchestration, Task / Plan lifecycle, Runtime policy, permissions,
confirmation, stable contracts, normalized results, and product experience.

External products remain replaceable specialized implementations behind those
boundaries.

### Shared infrastructure reuse

Skills and capability providers must reuse existing shared infrastructure when
the existing contract is sufficient.

This includes:

- Provider accounts;
- Provider Instances;
- Provider Registry;
- credentials;
- Keychain storage;
- Agent registry;
- model infrastructure;
- Skill infrastructure;
- Runtime execution infrastructure.

Do not create duplicate login, credential, Provider Instance, Agent registry,
model registry, or execution systems merely because another capability consumes
the same external service.

### P15 v1.0 boundary — exactly 10 Skills

The active P15 v1.0 capability boundary is now exactly:

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

### Vehicle Control — removed from v1.0

Vehicle Control is **not part of AI-OS v1.0**.

Vehicle Control is **not part of the active P15 boundary**.

AI-OS v1.0 will not implement or accept:

- Tesla Fleet API integration;
- Tesla App integration;
- vehicle state/control capabilities;
- vehicle navigation handoff;
- charging control;
- climate control;
- lock/unlock;
- any other Vehicle Control capability.

This decision **supersedes all earlier v1.0 / P15 Vehicle Control roadmap
decisions**.

Vehicle Control currently has no assigned implementation phase.

It may only return to the roadmap through a future explicit project-owner
decision.

### Generative Media — GM-4 account reuse

GM-4 reuses P13 / My AI Provider accounts and Provider Instances.

GM-4 must not create:

- a second xAI login;
- a second OpenAI login;
- duplicate OAuth;
- duplicate Device Code authentication;
- a Generative-Media-specific Provider Instance;
- duplicate Keychain credential storage.

`Connected` and `Media Executable` remain distinct states.

A My AI Provider can be connected while still lacking a legitimate executable
media entitlement.

### Generative Media — ComfyUI reuse

ComfyUI-Agent-Kit is accepted as the preferred reusable foundation for:

- ComfyUI hardware/runtime inspection where applicable;
- model/workflow knowledge;
- model acquisition;
- model-management assistance;
- ComfyUI Agent / MCP tooling.

ComfyUI-Mac-Silicon is accepted as the Apple Silicon knowledge source for:

- MPS;
- unified memory;
- Mac model compatibility;
- model recommendation;
- memory / precision / performance constraints.

AI-OS should not build a duplicate large ComfyUI model-advisor system unless
real product gaps remain after these reusable sources are evaluated.

GM-2 Ready First remains authoritative.

Model acquisition order:

```text
inspect actual machine/runtime
→ determine compatible model/version/quant
→ rank for user objective
→ confirm acquisition when required
→ download/install
→ GM-2 Ready First
→ real smoke/output validation
```

Do not download a large model first and only afterward discover that the target
machine cannot reasonably run it.

### Mano-P / Mano-CUA

Accepted as a v1.0 post-Generative-Media GUI / Computer Use intelligence
integration candidate.

Potential role:

- visual GUI understanding;
- GUI grounding;
- click/type/scroll/drag reasoning;
- multi-step visual interaction.

It remains behind existing AI-OS Agent / Skill / Runtime boundaries.

It does not replace Task Engine, Planner, Runtime, OpenClaw architecture, or
Computer Control.

Local mode is preferred.

Cloud screenshot/task transmission must not be a silent fallback.

### Local Model Optimization — llmfit primary

`AlexsJones/llmfit` is the preferred v1.0 reusable foundation for Local Model
optimization / recommendation in AI Center / My AI.

Primary reusable responsibilities:

- hardware profiling, including CPU, GPU, RAM and Apple unified memory;
- model-fit evaluation against the actual machine;
- model and quantization recommendation;
- memory, context-window and expected-speed estimation;
- benchmark evidence;
- installed-model discovery and model-acquisition assistance where appropriate.

Architecture boundary:

`llmfit` is an optimization / recommendation component, not the AI-OS model
execution system.

AI-OS continues to own:

- AI Center;
- Provider Registry;
- Local First routing;
- model/account identity;
- permissions and policy;
- model lifecycle orchestration;
- product UX.

oMLX and Ollama remain the current v1.0 Local Model execution Providers.

Magnitude is superseded as the planned v1.0 Local Model optimization
integration. It remains a future optional replaceable Local Inference Provider
only if a demonstrated product gap remains after oMLX / Ollama / llmfit, such
as materially better on-demand loading, inference throughput, or another
execution capability that AI-OS actually needs.

Do not integrate Magnitude merely to duplicate capabilities already supplied
by AI Center, oMLX, Ollama, or llmfit.

### Social / Community Intelligence

Social / Community Intelligence is explicitly deferred to v2.0.

MediaCrawler, Pachong, social-media-copilot, and related reviewed projects remain
future architecture / reuse references only.

Do not implement this capability during current P15 / v1.0 unless the project
owner explicitly changes the roadmap.

### Documentation authority

`AI_OS_MASTER_GUIDE.md` owns:

- product definition;
- architecture;
- development rules;
- P9-P17 roadmap.

`HANDOFF.md` remains the sole source of truth for:

- current repository state;
- completed work;
- current milestone;
- next work;
- blockers;
- accepted / rejected / deferred implementation decisions.

### Current Generative Media direction

Generative Media GM-0 through GM-6 and formal closure are complete. Do not
begin the next P15 Skill from this handoff.

Accepted post-Generative-Media reuse sequence:

1. Mano-P / Mano-CUA
2. llmfit Local Model Optimization / Recommendation integration
3. Cognitive Distillation completion
4. NAS Management foundation

Social / Community Intelligence remains v2.0.

Vehicle Control is not part of v1.0.

## 2026-09-09 — Local Model Optimization — llmfit Primary

Decision:

- `AlexsJones/llmfit` replaces Magnitude as the preferred v1.0 reusable
  foundation for Local Model hardware profiling, model-fit analysis,
  quantization recommendation, performance estimation and recommendation.
- llmfit remains behind AI-OS Local Model / AI Center boundaries and does not
  replace AI Center, Provider Registry, routing, permissions, oMLX or Ollama.
- oMLX and Ollama remain the current Local Model execution Providers.
- Magnitude is no longer a planned default v1.0 integration.
- Magnitude remains an optional future replaceable Local Inference Provider
  only if a concrete execution gap is demonstrated after llmfit + oMLX +
  Ollama.
- This decision supersedes the earlier post-Generative-Media Magnitude
  integration decision.
- Current Generative Media status is Complete. Next P15 Skill is Mano-P /
  Mano-CUA; it has not been started.

## 2026-09-12 — Generative Media Closure — Completed

Status: Complete.

- Formal closure verifier: `PASS=11 FAIL=0` across GM-0 through GM-6,
  Provider Registry, Ready/Setup/Repair, Local First/manual-selection
  invariants, Cloud account/budget boundaries, Prompt/Reference Intelligence,
  bounded Quality Loop, Runtime confirmation, and AR-1 Agent-owned execution.
- Real local evidence passed for ComfyUI text-to-image, VLM Setup/Repair,
  normal ReferenceSpec analysis, and one bounded correction followed by a real
  generation on the same Provider instance.
- Connected Cloud Provider metadata and deterministic execution boundaries
  passed. Real cloud image/video calls remain blocked by explicit account/spend
  confirmation and were not fabricated; no private reference was uploaded.
- Final Rust library suite, diff integrity, commit list, and clean worktree are
  recorded by the closing commit/report.

Current: Generative Media — Complete.

Next P15 Skill: Mano-P / Mano-CUA. Not started.

## 2026-09-12 — P15 GM-6 Bounded Quality Loop — Completed

Status: Completed.

- Provider-neutral `GenerationQualityAssessment` covers request satisfaction,
  reference adherence, composition, obvious failures/artifacts, video temporal
  coherence, retry usefulness, and confidence using deterministic output and
  Provider signals before any optional expensive evaluator.
- Normalized correction can clarify constraints, add negative constraints,
  adjust parameters/profile/reference weighting, or mark retry not useful.
  Execution occurs only after routing and never changes Provider or account.
- The hard product maximum is one automatic correction attempt. Cancellation,
  Runtime permission, Provider/account identity, policy/auth denial, and cloud
  cost authorization all fail closed. Cloud retry additionally requires an
  explicit bounded budget and a Provider cost estimator.
- Results record bounded attempt/assessment/correction metadata without raw
  prompts or media. Exhaustion returns a normalized terminal error; GM-6 does
  not implement Cognitive Distillation or long-term failure learning.
- Formal GM-6 acceptance: `PASS=15 FAIL=0`, including a real local bounded
  correction smoke: one deterministic transient first attempt followed by one
  real generation on the same Ready ComfyUI instance. The smoke-owned output
  was removed after validation. AR-1 Agent/Skill boundaries remained green.

Next: Mano-P / Mano-CUA. Do not start it as part of Generative Media closure.

## 2026-09-12 — P15 GM-5 Prompt / Reference Intelligence — Completed

Status: Completed.

- `CreativeIntent → PromptSpec → provider/model compiler` runs only after
  `MediaRouter` fixes the exact Provider and Provider Instance. Expert prompts
  remain verbatim unless structured constraints, references, or explicit
  enhancement require compilation; manual model/profile choices remain exact.
- Reference analysis is a replaceable adapter. The accepted local adapter pins
  `gokayfem/ComfyUI_VLM_nodes` revision
  `3f9612774e862f94d1dfbe3a1b36375a52870382`, uses `ModernVLM` for images and
  `VLMVideoTemporalReasoner` for video, and currently validates Qwen3-VL 2B.
- Normal execution is loopback-only, bounded, temporary, and never installs or
  downloads. Explicit confirmed Setup/Repair owns source/model installation,
  safe extraction, dependency shielding, managed backup, and Ready promotion.
- Ready requires compatible node schemas, a complete local model snapshot, a
  real 64x64 RGB workflow result, and successful normalization of that result
  into a traceable `ReferenceSpec`. The RGB fixture avoids a current ComfyUI
  PyAV incompatibility with 1x1 grayscale input; this is not a VLM node defect.
- Formal GM-5 acceptance: `PASS=20 FAIL=0`, including real Setup/Repair and
  normal Ready-adapter smoke. AR-1A remained `11/0`, AR-1B `19/0`, and the
  real OpenClaw round-trip passed after the configured local oMLX service was
  restored.
- `AlexsJones/llmfit` remains the accepted future Local Model hardware-fit
  foundation; GM-5 keeps only a replaceable Provider-scoped advisor boundary
  and does not invent a parallel suitability database.
- Known limitation: cloud generation/reference smoke remains explicit opt-in
  where account or paid-call confirmation is required; no private reference is
  uploaded and no paid confirmation is fabricated.

Next Generative Media milestone: GM-6 — Quality Loop.

## 2026-09-08 — P15 GM-4 Cloud Providers — Completed

Status: Completed.

Product/architecture decisions:

- Generative Media reuses the existing P13 / My AI Provider Instance and credential infrastructure.
- GM-4 does not create a second login, OAuth, Device Code, API-key storage, Keychain namespace, Provider Instance registry, or account UI.
- One connected Provider account remains reusable by Chat, AI Council, AI Arena, Generative Media, and future Skills.
- `Connected` and media execution entitlement remain distinct concepts.
- The current production machine has connected OAuth Provider Instances for:
  - `grok-default` (`providerId=grok`)
  - `openai-default` (`providerId=openai`)
- OAuth cloud media is treated as `subscription-entitlement`, not silently reclassified as metered API spend.
- API-key cloud media is fail-closed before credential access/network execution until AI-OS has an explicit media spend envelope.
- xAI is the recommended generic Cloud Media provider.
- OpenAI remains the secondary Cloud Media provider.
- Manual Provider selection remains exact.
- Provider execution failure, authorization rejection, entitlement rejection, budget rejection, or policy rejection does not cause silent Provider switching.
- Local First remains unchanged: failure/unreadiness of Local Media never silently creates paid cloud execution.

GM-4 production capabilities:

- xAI:
  - `media.text-to-image`
  - `media.image-edit`
  - `media.text-to-video`
  - `media.image-to-video`
- OpenAI:
  - `media.text-to-image`
  - `media.image-edit`

Production model defaults at implementation:

- xAI image: `grok-imagine-image-2.0`
- xAI video: `grok-imagine-video-1.5`
- OpenAI image: `gpt-image-2`

Execution path:

`MediaRequest`
→ `MediaRouter`
→ exact `MediaProvider`
→ existing P13 Provider Instance
→ lazy P13 credential resolution / OAuth refresh
→ official provider media endpoint
→ output bytes
→ AI-OS Generative Media asset storage
→ opaque `asset://generative-media/...`
→ normalized `MediaResult`

Security/budget invariants:

- Provider credentials remain outside `MediaProviderMetadata` and `MediaResult`.
- OAuth access tokens are loaded lazily only after routing and admission.
- API-key metered execution is rejected before backend entry, therefore before Keychain access and before network execution.
- Returned assets contain no provider secret material.
- Provider outputs are copied into AI-OS-managed asset storage rather than exposing temporary provider URLs.
- xAI video presigned download URLs are consumed internally and are not returned as the stable asset handle.

Reference boundary:

- GM-4 accepts AI-OS Generative Media asset handles and base64 image data URLs for cloud image-reference operations.
- Broader reference understanding, attachment interpretation, semantic prompt compilation, and reference intelligence remain GM-5 responsibilities.

Acceptance:

- deterministic Cloud Provider tests
- subscription OAuth execution path
- metered API fail-closed-before-backend path
- opaque AI-OS cloud asset normalization
- no secret material in result serialization
- unsupported capability rejected before provider execution
- xAI recommended / OpenAI secondary
- existing Local First router invariants preserved
- existing exact-provider/no-fallback executor invariant preserved
- connected Provider Instance configuration validated without reading Keychain secrets
- real cloud generation E2E is explicit opt-in only and may SKIP without failing deterministic GM-4 closure

Next Generative Media milestone:

- GM-5 — Prompt / Reference Intelligence
- then GM-6 — Quality Loop
- then Generative Media closure
- 2026-09-09 00:54  P15 GM-4 Cloud Providers complete; GM-2 production-registry test generalized for cloud providers

---

## Archived — 2026-09-09 GM-5 Core Intelligence Wiring Attempt

Historical record only. This uncommitted implementation was withdrawn on
2026-09-12 so GM-5 can restart from the clean GM-4 baseline.

The withdrawn experiment contained:

- `CreativeIntent -> PromptSpec -> provider/model-specific Prompt Compiler`
- PromptSpec structure is derived from the reusable media-prompt vocabulary
  direction of `f/prompts.chat`; AI-OS keeps the normalized Rust contract
  because Generative Media Runtime must not depend on a Node runtime
- deliberate expert prompts remain verbatim when no structured
  constraints/preferences/reference intelligence require recompilation
- Prompt compilation runs only after `MediaRouter` fixes the Provider identity
- automatic media model recommendation therefore cannot silently switch
  Provider or Provider Instance
- current executable model recommendations:
  - Ready ComfyUI text-to-image:
    managed SD1.5 FP16 profile
  - xAI image:
    `grok-imagine-image-2.0`
  - xAI video:
    `grok-imagine-video-1.5`
  - OpenAI image:
    `gpt-image-2`
- `MediaModelAdvisor` is a replaceable boundary for later llmfit and broader
  ComfyUI-Agent-Kit hardware/model/quant knowledge
- `ReferenceSpec` is now the provider-neutral image/video understanding shape
- `ReferenceAnalysisAdapterRegistry` is replaceable
- `gokayfem/ComfyUI_VLM_nodes` remains the preferred local execution foundation
  for reverse-prompt / image-video Reference Analysis
- image workflow contract uses `ModernVLM`
- video workflow contract uses `VLMVideoTemporalReasoner`
- default v1 local Reference VLM contract is `Qwen 3 VL 2B Instruct`
- image reference-conditioned generation canonicalizes to the already-existing
  `ImageEdit` / `ImageToVideo` Provider capability without changing Provider
  identity
- GM-5 metadata records recommendation/compiler information but never persists
  the user's generated prompt text

Reuse boundaries:

- `f/prompts.chat`:
  PromptSpec / media Prompt Builder vocabulary and semantics
- `gokayfem/ComfyUI_VLM_nodes`:
  local image/video Reference Analysis execution
- `SlavaSexton/ComfyUI-Agent-Kit`:
  model prompt-dialect, workflow and hardware/model knowledge source
- none of these projects replaces AI Center, MediaRouter, Provider Registry,
  P13 account infrastructure or Runtime ownership

Standing invariants:

- Local First is not Local-then-Cloud
- manual Provider selection remains exact
- model recommendation cannot silently switch Provider
- Provider execution/policy/auth/budget failure cannot trigger Provider switching
- GM-2 Ready First remains authoritative for local generation
- Reference Analysis must not silently upload local media to a cloud VLM
- Reference Analysis must not silently download a multi-GB VLM on first use
- P13 / My AI remains the credential and Provider account authority
- GM-6 self-correction/retry quality loops remain outside GM-5

This implementation is not part of the current tree. GM-5 is not started.

Next implementation work:

- connect the ComfyUI_VLM_nodes workflow contract to the existing managed
  ComfyUI endpoint / input / history execution path
- implement explicit Reference VLM setup/readiness so Qwen3-VL cannot download
  implicitly
- make `media.reference.image.analyze` and
  `media.reference.video.analyze` real production paths
- feed normalized ReferenceSpec values back into Prompt Compiler
- complete automatic local candidate/model/profile ranking
- run final GM-5 acceptance and full regression

## Archived — 2026-09-09 GM-5 Local Reference Analysis Attempt

Historical record only. The experimental local Reference Analysis path was
withdrawn before commit and is not part of the current implementation.

The withdrawn experiment included:

- existing GM-2C ComfyUI transport is reused for text-producing workflows
  through the same `/prompt` and `/history` lifecycle
- `ComfyUiVlmReferenceAnalysisAdapter` is a production adapter
- Reference Adapter identity includes exact local ComfyUI Provider Instance
- `media.reference.image.analyze` executes local ComfyUI_VLM_nodes `ModernVLM`
- `media.reference.video.analyze` executes local
  `VLMVideoTemporalReasoner`
- default Reference VLM is `Qwen/Qwen3-VL-2B-Instruct`
- explicit Reference Analysis never silently uploads a reference to Cloud
- manual Reference Analysis must match the selected ComfyUI adapter / instance
- AI-OS reference asset staging remains bounded and temporary
- local VLM execution requires the accepted ComfyUI_VLM_nodes revision,
  explicit setup evidence, local model cache and prior smoke validation
- normal generation uses normalized local `ReferenceSpec` when the local
  Reference Adapter is already Ready
- if local Reference Intelligence is not Ready, the original provider-native
  reference remains attached and generation does not silently switch Provider
- local generation accepts Reference Intelligence only from the same ComfyUI
  Provider Instance
- normalized ReferenceSpec values feed the GM-5 Prompt Compiler
- GM-5 result metadata records the number of normalized reference specs but
  does not persist the user's prompt text

Still required before GM-5 closure:

- explicit one-click Reference VLM setup / install / repair
- real Reference VLM smoke analysis before Ready promotion
- broader automatic local media model / profile candidate ranking
- final GM-5 full regression and closure

GM-6 quality/self-correction remains out of scope.

## Archived — 2026-09-10 GM-5 Reference VLM Setup Attempt

Historical record only. The uncommitted Reference VLM setup experiment was
withdrawn before acceptance and is not part of the current tree.

Architecture:

- Setup / Repair is a separate explicitly confirmed lifecycle from
  `media.reference.*` execution.
- The managed local node package is `gokayfem/ComfyUI_VLM_nodes`.
- AI-OS pins the accepted source revision
  `3f9612774e862f94d1dfbe3a1b36375a52870382`.
- Source acquisition uses AI-OS's existing Rust HTTP/archive dependencies rather
  than requiring system Git or developer tools.
- Archive extraction is bounded and rejects traversal, symlink and special-file
  entries.
- AI-OS never overwrites or silently adopts an unmanaged ComfyUI VLM node
  directory.
- A previously AI-OS-managed damaged node source is backed up before repair.
- `requirements.txt` is executed with the selected ComfyUI installation's own
  Python runtime.
- Setup rejects pinned requirements that attempt to install or replace torch,
  torchvision or torchaudio.
- Model acquisition remains owned by the reusable upstream `ModernVLM` node.
  AI-OS does not implement a second Hugging Face snapshot downloader.
- The official Qwen3-VL model download may occur only inside explicitly
  confirmed Setup / Repair when the real ModernVLM smoke workflow executes.
- The smoke workflow must return text that normalizes into AI-OS
  `ReferenceSpec`.
- Ready promotion occurs only after the expected local model cache exists and
  the real smoke has succeeded.
- The Ready marker schema is now v2 and is bound to the exact ComfyUI Provider
  Instance that passed the smoke.
- Any Setup / Repair attempt clears previous Ready evidence before mutation;
  failure therefore leaves Reference Analysis NotReady rather than preserving
  stale authorization.
- Normal `media.reference.*` execution remains unable to install nodes,
  install Python packages or download the multi-gigabyte VLM.

State at withdrawal:

- deterministic Setup / Repair implementation and tests were removed;
- the live Setup / Repair has not yet been executed in this milestone;
- no Qwen3-VL download is claimed yet;
- no ComfyUI Python dependency mutation is claimed yet;
- no acceptance evidence from this withdrawn attempt is current.

GM-6 remains out of scope.

---

## AR-1

AR-1 fixes the Kernel execution boundary only.
 — Agent Execution & Skill Invocation Boundary Realignment

Status: **Complete — 2026-09-12.**

### Architecture decision

AI-OS itself is not an Agent. AI-OS owns platform orchestration, governance,
Task Engine, Planner, Runtime, shared Memory, AI Center, permissions, policy,
and capability infrastructure.

User Do-task control plane:

`User -> Task Engine -> Planner -> Runtime -> AgentExecutionAdapter -> selected Agent`

v1 operational general execution Agent:

`OpenClaw`

The Runtime-to-Agent contract remains generic so future Agents can be added
without redesigning Task Engine, Planner, Runtime, or P15 Skills.

Capability plane during Agent execution:

`Agent -> Agent Skill Transport Adapter -> AI-OS Skill Invocation Gateway -> Runtime permission / confirmation / policy -> Skill backend -> Provider / MCP / native OS / external service`

Key boundaries:

- Planner may recommend or constrain Skills, but does not terminally execute a
  Skill backend for a user Do-task.
- Agent owns task execution and decides when an allowed Skill is needed.
- Runtime owns Agent lifecycle, scheduling, cancellation, recovery, permission,
  confirmation, policy, tracing and governance.
- Skill owns capability validation, backend invocation and normalized result.
- Provider owns specialized implementation.
- MCP is a Skill backend/provider transport, not an Agent or Task executor.
- MediaRouter is a Generative Media Skill backend router, not an Agent.
- Ollama and oMLX are model Providers, not Agents.
- Agent-authored Skill invocation requests cannot self-declare user
  confirmation or permission approval. Trusted confirmation state comes from
  Runtime-issued execution context.
- Setup/install/repair, readiness/health discovery, passive status, deterministic
  acceptance fixtures, and AI Center Ask/model invocation may use non-Agent
  paths when they are not executing a user Do-task.

### AR-1A — Architecture Contracts

Status: **Accepted — 2026-09-12.**

Implemented contract layer:

- generic `AgentId`;
- generic `AgentExecutionRequest`, result, progress and error contracts;
- generic `AgentExecutionAdapter`;
- Runtime-issued `SkillInvocationContext`;
- Agent-authored `SkillInvocationRequest`;
- generic `SkillBackend`;
- generic `SkillInvocationGateway`;
- tests proving selected Agent identity is retained;
- tests proving allowed capabilities are constraints rather than task
  executors;
- tests proving an Agent Skill request cannot self-authorize confirmation;
- tests proving permission denial occurs before Skill backend execution.

Acceptance evidence: `verify/verify_ar1a_agent_skill_contract.sh` returned
`PASS=11 FAIL=0`.

Do not implement AR-1 by merely renaming all Skill executor kinds to
`openclaw`.

---

<!-- AI_OS_CANONICAL_PRODUCT_DEFINITION_START -->

## Canonical AI-OS Product Definition

Status: canonical product and architecture constraint.

This definition supersedes older wording that frames AI-OS as an Agent, an
LLM, a Skill, or a domain-specific executor.

### Product identity

AI-OS is an AI-native operating system.

AI-OS itself is NOT:

- an Agent;
- an LLM;
- a Skill;
- a domain capability implementation.

AI-OS exists to connect, orchestrate and govern:

- multiple Agents;
- multiple LLMs;
- multiple Skills;
- shared Memory;
- Providers;
- permissions and policy;
- task and execution lifecycle.

Canonical relationship:

`User -> AI-OS -> Agent(s) + LLM(s) + Skill(s) -> Outcome`

AI-OS should contain the minimum amount of first-party code required to make
that relationship reliable.

### Minimal Kernel principle

AI-OS Core stays small.

The Kernel owns:

- Task Engine;
- Planner;
- Runtime;
- Agent Registry and selection;
- LLM / Provider Registry and routing;
- Skill Registry and exposure;
- shared Memory coordination;
- permissions, confirmation and policy;
- execution lifecycle;
- cancellation and recovery;
- tracing and observability;
- compatibility and capability negotiation.

The Kernel SHOULD NOT reimplement a capability when an acceptable mature
Skill, MCP server, library, CLI, API, Agent tool, Provider, native adapter, or
open-source project can be safely reused.

Canonical implementation rule:

`reuse -> adapt -> wrap -> delegate -> build only when necessary`

### Agent-owned execution

For user Do-tasks, AI-OS manages execution but does not become the Agent.

Control plane:

`User -> Task Engine -> Planner -> Runtime -> selected Agent`

Capability plane during Agent execution:

`Agent -> Agent Skill Transport Adapter -> Skill Invocation Gateway -> Skill backend -> Provider / MCP / native tool / external service`

Responsibilities:

AI-OS owns:

- orchestration;
- governance;
- permissions;
- routing;
- Memory;
- lifecycle;
- compatibility;
- tracing.

Agent owns:

- task execution;
- action sequencing;
- deciding which available Skills to invoke;
- interpreting intermediate results.

LLM owns:

- reasoning / generation capability used by an Agent or other AI-OS subsystem.

Skill owns:

- a reusable capability contract;
- validation of capability input;
- normalized capability output.

Provider / backend owns:

- concrete implementation.

### Multi-Agent compatibility principle

Agent compatibility is capability-based and determined by capability discovery and contract negotiation, never by an exact Agent version.


AI-OS MUST bind to capability contracts, not exact Agent versions.

OpenClaw 2026.8.2 is only a current development and acceptance environment.
It is not an AI-OS product dependency.

Agent version strings are metadata for diagnostics and compatibility rules.

Execution support is determined through capability discovery and contract
negotiation.

Relevant Agent capabilities may include:

- task execution;
- sessions;
- structured tool invocation;
- Skill invocation;
- MCP transport;
- native tool or plugin transport;
- progress events;
- cancellation;
- durable sessions;
- Agent-scoped Skill exposure.

Rules:

- unknown newer Agent versions are allowed when required capability probes pass;
- known versions do not automatically enable capabilities they do not expose;
- version alone MUST NOT enable or disable a Skill;
- known version defects may trigger targeted compatibility rules;
- Agent-specific protocol changes stay inside Agent / Transport Adapters.

### Skill architecture

Skills are AI-OS capabilities and are not permanently owned by one Agent.

Canonical direction:

`External capability -> AI-OS Skill Adapter -> AI-OS Skill Contract -> compatible Agent exposure`

A Skill should be adapted once and then exposed to compatible Agents through
transport adapters.

MCP is one transport option.

MCP is NOT the Skill Invocation Gateway.

Future transports may include:

- native Agent tools;
- plugins;
- local RPC;
- other stable Agent protocols.

Task Engine, Planner and Skill contracts must not need rewriting when the
transport changes.

### Autonomous capability expansion

A future core capability of AI-OS is autonomous Skill acquisition.

When the user needs a capability AI-OS does not currently possess:

`Need -> Discover -> Evaluate -> Adapt -> Sandbox/Test -> Register -> Expose -> Use`

Candidate capability sources may include:

- GitHub;
- MCP ecosystems;
- Agent Skills;
- libraries;
- CLIs;
- APIs;
- mature open-source projects.

AI-OS should eventually be able to:

- discover candidate implementations;
- inspect license;
- inspect maintenance state;
- inspect security;
- inspect compatibility;
- choose an acceptable candidate;
- create the minimum required adapter;
- validate the capability;
- expose it to compatible Agents;
- retain it for future reuse.

This is capability self-expansion.

It does NOT authorize unrestricted self-modification of:

- Runtime Kernel;
- Task Engine;
- permission model;
- security policy;
- governance boundaries.

### Strategic Intelligence Layer

Multi-Agent, multi-LLM and multi-Skill orchestration is foundational
infrastructure.

The strategic AI-OS Intelligence Layer is:

1. Cognitive Distillation;
2. AI Council;
3. AI Arena.

#### P15 Cognitive Distillation

P15 builds the foundation for structured cognitive distillation.

Its purpose is not only summarization.

It should produce reusable cognitive data representing a person, expert, Agent,
decision style or behavior pattern.

Possible structured dimensions include:

- beliefs;
- priorities;
- values;
- decision heuristics;
- risk preference;
- recurring strategies;
- historical decisions;
- behavioral patterns;
- evidence and provenance;
- contradictory evidence;
- confidence.

The output must be consumable by P16 AI Council.

#### P16 AI Council

P16 owns AI Council runtime and Council operating modes.

The two previously selected GitHub projects are studied for their AI Council /
multi-Agent operating patterns and may be directly integrated when appropriate
instead of reimplemented.

Current external integration candidates:

- Paperclip;
- Agency Agents.

They are NOT Cognitive Distillation systems.

AI Council must be able to consume P15 distilled cognitive data.

A primary AI Council capability is persona / expert simulation:

`Distilled cognitive profile + new situation -> Council simulation`

Council can simulate questions such as:

- how would this person likely understand the situation;
- what would they focus on;
- how might they reason;
- how might they decide;
- how might they handle the situation;
- what action might they take.

In AI-OS product terminology, "prediction" in this context primarily means:

using a distilled cognitive profile to simulate and anticipate how the modeled
person may think, decide or act when facing a new situation.

This is NOT defined as a separate generic probability forecasting engine.

Simulation output is modeled inference and must not be presented as certainty
about a person's actual private thoughts.

#### AI Arena

AI Arena evaluates and compares:

- Agents;
- LLMs;
- Council members;
- Council compositions;
- strategies;
- prompts;
- simulation outputs.

Over time Arena evidence can improve:

- Agent routing;
- LLM routing;
- Council composition;
- distilled profiles;
- Skill selection;
- simulation quality.

Canonical intelligence loop:

`Distill -> Simulate -> Compare -> Validate -> Refine`

### External project integration principle

AI-OS may directly integrate mature external projects when doing so reduces
code, risk and duplicated engineering.

External projects remain replaceable components behind adapters.

They do NOT become AI-OS itself.

AI-OS retains ownership of:

- system contracts;
- governance;
- Task lifecycle;
- permissions;
- Memory;
- Agent Registry;
- LLM / Provider Registry;
- Skill Registry;
- interoperability.

### AR-1 boundary

AR-1 repairs the Kernel execution boundary only.

AR-1 DOES NOT implement:

- AI Council;
- Paperclip integration;
- Agency Agents integration;
- autonomous Skill discovery;
- autonomous adapter generation;
- Cognitive Distillation behavior;
- AI Arena;
- new business-specific Skills.

AR-1B target:

`User -> Task/Planner/Runtime -> selected Agent -> Skill Invocation Gateway -> existing Skill backend`

AR-1B also establishes capability-negotiated Agent compatibility.

No exact OpenClaw or future Agent version may become a product-level execution
dependency.

<!-- AI_OS_CANONICAL_PRODUCT_DEFINITION_END -->

<!-- AR1B_IMPLEMENTATION_STATUS_START -->

## AR-1B — Agent-Owned Execution Realignment

Status: **Accepted — 2026-09-12.**

Acceptance evidence:

- AR-1B verifier: `PASS=19 FAIL=0`
- targeted Agent execution Rust tests passed
- targeted Plan Runtime bridge Rust tests passed
- targeted Task execution Rust tests passed
- targeted rustfmt check passed
- `git diff --check` passed
- the AR-1C real OpenClaw execution fixture passed against the active gateway

AR-1B establishes the minimal control-plane correction:

`Task -> Plan(selected Agent) -> Runtime -> AgentExecutionAdapter`

Key constraints:

- selected Agent identity is Plan execution metadata, not Skill input;
- Planner does not terminally execute Browser, Local Model, Media or other Skill backends;
- Plan Runtime no longer dispatches user Do-tasks by `skill.executor.kind`;
- registered Skill capabilities become Agent exposure constraints;
- `agent.execute` is a control-plane PlanStep for Do-tasks that do not name a specific Skill;
- Agent compatibility is based on capability probing and contract negotiation;
- exact Agent versions are diagnostic metadata only;
- OpenClaw remains the v1 Agent Adapter but is not a version-locked product dependency;
- Skill transport capability is modeled separately from the SkillInvocationGateway;
- AR-1C supplies the separate Agent Skill transport and real invocation loop.

AR-1C completes:

`Agent -> Agent Skill Transport Adapter -> SkillInvocationGateway -> Skill backend`

and proves the path with a real OpenClaw Agent invocation.

The existing Skill manifest `executor` field may remain temporarily for frontend
serialization/backward compatibility, but Plan Runtime must not use it to route
user Do-tasks.

<!-- AR1B_IMPLEMENTATION_STATUS_END -->

## AR-1C — Agent Skill Transport and Gateway Loop

Status: **Accepted — 2026-09-12.**

Acceptance evidence:

- `verify/verify_ar1c_agent_skill_transport.sh`: `PASS=15 FAIL=0`
- real OpenClaw E2E: `1 passed; 0 failed`
- safe fixture capability: `filesystem.scan`
- real path: selected OpenClaw Agent -> `AgentSkillTransportAdapter` ->
  `SkillInvocationGateway` -> Runtime permission/confirmation -> existing
  filesystem backend -> normalized Skill result -> Agent completion
- the Agent request contains no confirmation, permission, trusted automation,
  backend, provider or elevated-authority field
- permission denial and non-exposed capability denial occur before backend
  invocation
- capability negotiation, not an exact OpenClaw version, controls transport
  admission

## Current milestone

- Kernel correction: AR-1 Complete
  - AR-1A Accepted
  - AR-1B Accepted (`PASS=19 FAIL=0`)
  - AR-1C Accepted (`PASS=15 FAIL=0`, real OpenClaw E2E PASS)
- Generative Media: GM-4 Complete
- GM-5: In Progress — provider-scoped Prompt Intelligence implemented; Reference Intelligence pending
- GM-6: Not started

Next:

- GM-5 — Prompt / Reference Intelligence

GM-5 technical decision: MediaRouter fixes Provider identity first; the
Provider-scoped Prompt Compiler and Model/Profile Advisor may refine only the
request beneath that exact Provider/instance and preserve explicit user model,
profile and verbatim-prompt choices. Prompt metadata persists a digest, never
the generated or original prompt text.

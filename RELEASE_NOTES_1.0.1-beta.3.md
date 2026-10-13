# AI-OS 1.0.1-beta.3 Release Notes

Date: 2026-10-11

## What changed

- Chat requests now recover a connected oMLX runtime at the actual Provider
  execution boundary, including relaunch and idle-stop cases.
- Streaming UI updates are batched and historical Markdown rendering is
  memoized for smoother long conversations.
- Chat cancellation no longer leaves submission locked. Message Copy, Edit,
  Share and Regenerate actions are available with corrected layout and spacing.
- oMLX is now the only supported local model Provider. Retired local-provider
  registration, routing, UI, runtime adapters and protocols were removed.
- The application icon now uses the accepted dark Liquid Glass treatment while
  preserving the AI-OS cyan/amber interlocking-ring identity.

## Release evidence

- Version consistency: PASS (`1.0.1-beta.3`).
- Beta.3 chat regression, oMLX-only regression and icon validation: PASS.
- Full Rust tests, `cargo check`, frontend production build and source integrity: PASS.
- Release `.app`, DMG signature, read-only mount and package-data hygiene: PASS.
- DMG SHA-256:
  `02c38948f3a76609938db22a9ce85436805e11f13f6c4a18853663f29374c968`.

## Distribution boundary

The application and DMG are ad-hoc signed local release candidates. This Mac
has no Developer ID Application identity and no Apple notarization credentials,
so notarization was skipped. No tag, push, upload or external release has been
performed.

The repository-wide gate still contains pre-existing formatting and
environment-dependent failures outside the Beta.3 scope. Real Tauri acceptance
for overnight oMLX recovery and the complete chat-action workflow remains a
manual acceptance item.

## Next milestone

Beta.4 is Adaptive Context & Knowledge Engine: model-aware context budgeting,
long-conversation compaction, durable memory, file retrieval and project
knowledge management. It is not part of Beta.3.

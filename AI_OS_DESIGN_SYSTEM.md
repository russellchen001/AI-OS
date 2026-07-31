# AI-OS Design System

**Version:** 1.0
**Status:** Implementable target specification
**Date:** 2026-08-01

## 1. Authority and Status

This document owns visual language and component rules. The Master Guide prevails on product, architecture, roadmap, or scope conflict. The UI Spec owns experience behavior; the Figma Blueprint owns design-file delivery. Tokens below are target contracts and are not claims of complete implementation.

## 2. Brand Principles

AI-OS is bright, calm, modern, precise, local-first, and quietly capable. The default light experience is white-first, using white as the dominant workspace color and warm-neutral whites only to separate navigation, controls, and nested surfaces. Use clear hierarchy, generous whitespace, durable state cues, and limited accent color. Avoid grey-heavy dashboard styling, decorative gradients, emoji as structural icons, arbitrary color, and excessive motion.

## 3. Theme and Semantic Tokens

Implement tokens as CSS variables and matching Figma variables. Components consume semantic tokens, never raw palette values.

| Token | Light | Dark | Use |
|---|---:|---:|---|
| `color.bg.canvas` | `#FFFFFF` | `#101311` | dominant app/workspace background |
| `color.bg.sidebar` | `#F7F8F7` | `#171B19` | subtly separated navigation |
| `color.bg.surface` | `#FFFFFF` | `#1C211E` | cards/panels |
| `color.bg.subtle` | `#F3F5F4` | `#252B27` | secondary controls and nested surfaces |
| `color.text.primary` | `#202523` | `#F1F4F2` | primary text |
| `color.text.secondary` | `#555D59` | `#B7C0BB` | supporting text |
| `color.text.muted` | `#737C77` | `#8F9A94` | metadata |
| `color.border.default` | `#DDE2DF` | `#343C37` | boundaries |
| `color.accent.primary` | `#176B52` | `#62B596` | primary action/focus |
| `color.status.success` | `#187A55` | `#62C89D` | success |
| `color.status.warning` | `#9A6500` | `#E4B65A` | warning/approval |
| `color.status.danger` | `#B43D45` | `#F07B82` | destructive/error |
| `color.status.info` | `#356EAA` | `#79ACE0` | informational |

Use paired foreground/background status tokens and verify WCAG AA contrast. Never encode state by color alone. High-contrast mode may override values without component changes.

White is the primary light-theme field, not an accent. Do not fill every card with grey to manufacture hierarchy. Prefer spacing, borders, typography, and restrained elevation; use `bg.sidebar` and `bg.subtle` only where separation is necessary. Preserve the dark theme as a fully supported user choice rather than an automatic inversion.

## 4. Typography

System sans: `Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`. Editorial display: `"Iowan Old Style", "Palatino Linotype", Georgia, serif`, limited to brand and major workspace titles. Mono: `"SFMono-Regular", Consolas, monospace`.

| Style | Size/line | Weight |
|---|---|---|
| Display | 40/48 | 500 |
| H1 | 32/40 | 600 |
| H2 | 24/32 | 600 |
| H3 | 18/26 | 650 |
| Body | 15/23 | 400 |
| Body small | 13/20 | 400 |
| Label | 12/16 | 650 |
| Caption | 11/16 | 500 |
| Code | 13/20 | 400 |

Minimum body text is 13px; do not use letter spacing to compensate for unreadably small text.

## 5. Spacing, Size, Radius, Border, Shadow

Spacing scale: `0, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64`. Standard control heights: 32 compact, 40 default, 48 touch/emphasis. Touch target minimum: 44×44. Content widths: readable text 720px; standard page 1080px; wide workspace fluid.

Radius: 6 small, 10 control, 14 card, 18 panel, full pill. Borders: 1px default, 2px focus/high emphasis. Shadows: none, low (`0 1px 2px / 6%`), medium (`0 8px 24px / 10%`), overlay (`0 18px 48px / 18%`). Dark theme reduces shadow and strengthens borders.

## 6. Icons and Layout Grid

Use one outlined icon family with 1.75–2px strokes; sizes 16, 20, 24. Icons require accessible labels when meaning is not accompanied by text. Do not mix emoji, glyphs, and outlined icons for navigation.

Desktop grid: 12 columns, 24px gutters, 32px margins. Tablet: 8 columns, 20px gutters, 24px margins. Mobile: 4 columns, 16px gutters/margins. Workspace panes use resizable constraints: sidebar 72/238px; context panel 320–400px; main pane minimum 480px on desktop.

## 7. Motion

Durations: instant 0–80ms, quick 120ms, standard 180ms, deliberate 240ms, overlay 300ms. Easing: enter `cubic-bezier(.2,.8,.2,1)`; exit `cubic-bezier(.4,0,1,1)`; movement `cubic-bezier(.4,0,.2,1)`. Animate opacity and transform when possible. No decorative looping motion. Reduced-motion removes travel and shortens transitions to ≤80ms.

## 8. Interaction States

All interactive components define default, hover, focus-visible, pressed, selected, disabled, loading, and error where applicable. Focus uses a 2px accent ring plus offset and is never removed. Disabled controls remain legible and explain why when consequential. Loading preserves labels/width, prevents duplicate activation, and announces completion or failure appropriately.

## 9. Core Components

- **Button:** primary, secondary, quiet, destructive, icon; small/default/large; loading retains label.
- **Input/Textarea/Search/Select:** persistent label, optional help, error association, character/limit state; placeholder is not a label.
- **Card:** flat/default/interactive/selected; selection is not represented only by elevation.
- **Sidebar:** brand, primary navigation, history, utilities; collapsible without losing accessible names.
- **Panel/Drawer:** header, content, optional sticky actions; traps focus only when modal.
- **Dialog:** explicit title, consequence, safe default, Escape behavior, focus return.
- **Toast:** supplemental only; never the sole record of approval, error, or completed task.
- **Tabs:** keyboard roving focus; use only for peer views, not sequential steps.
- **Badge/Status:** semantic icon + text; compact but never ambiguous.
- **Table/List:** responsive row alternatives, sortable headers, empty/loading/error states.
- **Skeleton/Progress:** match content geometry; use determinate progress only when truthful.

## 10. Domain Components

- **ConversationRow:** title, timestamp, task marker, menu; rename/archive/delete variants.
- **MessageBlock:** user/AI/system/tool summary, source/provenance, streaming/error/retry variants.
- **TaskCard/TaskPanel:** objective, lifecycle, steps, execution, approval, recovery, result.
- **ApprovalCard:** requested effect, target, permission, risk, reversibility, approve/deny.
- **ProviderCard:** provider identity, connection/configuration/health, models, usage, actions.
- **AgentCard:** location, capabilities, installation/configuration/health, permissions, operations.
- **CouncilTeamProposal:** Chief of Staff rationale, expert roles, editable assembly, consent state.
- **CouncilDiscussion/Report:** speaker role, claims/evidence, dissent, consensus, assumptions, confidence.
- **ArenaModeCard:** mode identity, purpose, participant/rule requirements.
- **ArenaParticipant/Roster:** variable participant count, model, role, independent or team grouping, status, turn, score, add/remove controls, formation presets, and hidden-information treatment. Layout must not encode a fixed 1v1 assumption.
- **ArenaStage/Timeline:** round, event, live state, audience controls, moderation, replay.
- **ArtifactCard/Preview:** type, thumbnail, title, provenance, related task, storage/export actions.

Domain components display backend truth and do not perform provider, Runtime, or permission logic.

## 11. Responsive Rules

At ≥1200px, use persistent sidebar and optional contextual panel. At 768–1199px, collapse navigation and present secondary panels as overlays. Below 768px, use a single-pane hierarchy with sheets for history/setup. Tables become labeled cards; Council/Arena multi-column views become a timeline or participant carousel without hiding turn/state controls.

## 12. Accessibility and Content

Target WCAG 2.2 AA. Support keyboard-only use, 200% zoom, text resizing, screen readers, high contrast, reduced motion, and non-color cues. Use semantic HTML before ARIA. Plain-language status content follows: what happened → impact → safe next step. Never expose secrets, raw internal errors, or fabricated certainty.

## 13. Implementation Governance

New UI must use tokens and existing components before adding variants. A new token requires semantic reuse across at least two contexts or a documented domain need. Visual QA covers both themes, all breakpoints, keyboard focus, contrast, loading/error/disabled states, and screenshot regression where available.

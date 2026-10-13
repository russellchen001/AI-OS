# AI-OS 1.0.1-beta.4.1

Beta 4.1 completes the local Auto model-selection correction, removes the
AI-OS-owned Benchmark implementation and grounds unfamiliar public product
facts with existing OpenClaw search evidence.

## Included

- Auto keeps llmfit hardware-fit, memory, speed and score evidence when an
  installed 4bit model differs from llmfit's preferred future quantization.
- On the verified MacBook Air M4 16 GB model set, Auto selects
  `Qwen2.5-7B-Instruct-4bit` because it is the installed model with valid
  llmfit fit evidence; the model name is not hardcoded into routing logic.
- Manual selection remains single-candidate and never falls through to Auto.
- A cold-start Auto request now waits briefly for llmfit evidence instead of
  using alphabetical local-model order while the assessment cache is empty.
  The wait is bounded; unevaluated local models cannot win a timeout by name.
- Safe routing diagnostics record assessment timing, selected model, attempts
  and fallback state without storing conversation text.
- The self-built Benchmark module, cache and Benchmark-only Provider request
  path are removed. Normal oMLX chat streaming and TTFT logging remain.
- The completed Project Bar is unchanged.
- Public product questions containing a specific model identifier use a narrow
  local gate and the existing Runtime/OpenClaw `browser.search` path. Only the
  compact public identifier is searched; ordinary chat remains Local First.
- A product model mentioned as background (for example `我在研究 NAS DS925+`)
  stays a local conversation. Only a factual question about it or an explicit
  search request is verified online, and the local model is told not to invent
  a vendor or specifications.
- Search evidence excludes DuckDuckGo ads, ad/click-tracking redirects,
  tracking parameters, oversized URLs, results unrelated to the asked model, and
  look-alike domains. Pages from the site that answers most consistently rank first.
- Search results are restated as a short Simplified-Chinese answer, using the
  retrieved snippets only, followed by clean source links.
- A product question is answered by the selected model first, and the reply shows
  which model answered. When the model is not certain, AI-OS searches and then
  answers from the evidence. An explicit "搜索 / 查一下" request searches directly.
- Local First: AI-OS-internal prompts (search grounding, product answers, intent
  classification) no longer move Auto turns to a cloud model; Auto judges a
  request's complexity by the user's own words.

## Local release boundary

- Built artifact: `src-tauri/target/release/bundle/dmg/AI OS_1.0.1-beta.4.1_aarch64.dmg`
- Real installed-app acceptance passed for the first and later Auto requests:
  both selected `Qwen2.5-7B-Instruct-4bit` in one attempt with no fallback.
- Real installed-app fact acceptance passed twice for DS925+: the first request
  exposed Synology's official product page, and the later request returned the
  official Synology page as result 1. A separate greeting remained local and
  was answered by oMLX `Qwen2.5-7B-Instruct-4bit`.
- DMG SHA-256:
  `22d12f21011f42cef37a9f5ae43d45daf8c89f960bcd5fc61d9148f3fb78d11d`
  (final dependency-security archive. It contains the owner-accepted implementation and supersedes the accepted `c1801397…` package without application-source changes.)
- The app and DMG pass strict deep-signature and package-data hygiene checks.
- Compatible lockfile updates remove all known Rust vulnerabilities and reduce
  npm findings to five low-severity KaTeX/math-chain items. The remaining npm
  fix is breaking and is deferred.
- The build uses an ad-hoc signature. It is not Developer ID signed, notarized,
  uploaded or published. The local archive tag is
  `ai-os-v1.0.1-beta.4.1`.

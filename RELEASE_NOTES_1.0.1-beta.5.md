# AI-OS 1.0.1-beta.5

Beta 5 adds two capabilities: cloud-drive share downloads and local
Computer Use (AI-OS operating a Mac app on request, on-device).

## Cloud-drive share downloads

Beta 5 adds built-in cloud-drive share downloads: send a share link (with its
extraction code) in chat and AI-OS downloads every file to
`~/Downloads/AI-OS/<share>`, verifies size and SHA-256 (plus the provider's
MD5 where listed), and reports the saved paths. Nothing is installed by hand.

### Providers

| Provider | Route | Sign-in |
|---|---|---|
| 百度网盘 | Baidu's official `bdpan` CLI (offered for install and authorization once); web fallback for small files | once |
| 夸克网盘 / UC 网盘 | save to a temporary drive folder, download, delete the folder | once, in AI-OS |
| 阿里云盘 | download URL straight from the share | once, in AI-OS |
| 123 云盘 | guest download; sign-in only if 123 refuses a guest | usually none |
| 天翼云盘 / 移动云盘 | download URL straight from the share | once, in AI-OS |
| 迅雷云盘 | guest links for the types Xunlei allows; other files are saved with the page's own 转存 button and handed to the Thunder app (one 取回 click) | once, in AI-OS |

光鸭云盘 links are recognised but not yet downloadable.

The chat shows live progress while a share downloads (stage, file i/N, bytes of the total).

### Verified in the installed app

- 百度网盘 (bdpan, 204 MB), 123 云盘 (12 files, 159 MB, guest), 迅雷云盘 (217 MB via Thunder hand-off), 夸克网盘 (2 files, 113 MB, save → download → temporary folder removed).

### Known limits

- Not tested end to end (no accounts): 阿里云盘, UC 网盘, 天翼云盘, 移动云盘 — their anonymous share reading is verified against live public shares.
- Free accounts are throttled by the providers (Quark ≈75 KB/s; Xunlei guest links are slow).
- Xunlei: the hand-off overwrites the clipboard briefly (text is restored), Thunder must be signed in to the same account, and the saved copy stays in the user's Xunlei drive.
- Sign-in sessions are kept in per-drive AI-OS WebView stores; ad-hoc signing only.

## Computer Use (local)

Ask in chat for an action in one named Mac app (for example "只操作当前打开的“计算器”应用。只按一次数字 8") and AI-OS does it: Peekaboo observes the app and performs the click or typing, AI-OS plans each step with a local tool-calling model on oMLX, and checks the result against the app's accessibility state.

- Local and free: the model runs on the Mac; no screenshots leave it.
- Bounded: only the named app, at most 3 steps and 120 seconds per task; credential-like tasks and password fields are refused; no shell; keys limited to return, tab, escape, delete and arrows.
- Stop works while it is running ("Computer Use 已取消，未继续操作"); a hung action is ended at the task deadline.

### Requirements

- Peekaboo.app (OpenClaw, MIT) installed as the bridge host, with 录屏与系统录音 and 设备控制和数据访问 (Accessibility) granted to it, plus the `peekaboo` CLI from `openclaw/tap`.
- A connected oMLX local model that supports tool calls (chosen automatically by capability).

### Verified

- Provider timeout and cancellation during an executing action (3/3 each); TextEdit single-step 3/3 and two-step 3/3; Calculator single click 3/3; isolated installed-app acceptance (3/3 completed, stop → cancelled with no change, app usable afterwards).

### Known limits

- An action already handed to Peekaboo.app (one click or keystroke) cannot be recalled by stop or timeout; no further action starts.
- Local-model judgement is the weakest link; each new app should get its own repeatable live test.
- AI-OS does not yet guide other users through installing Peekaboo.app and granting its permissions.

# AI-OS 1.0.1-beta.5

Beta 5 adds built-in cloud-drive share downloads: send a share link (with its
extraction code) in chat and AI-OS downloads every file to
`~/Downloads/AI-OS/<share>`, verifies size and SHA-256 (plus the provider's
MD5 where listed), and reports the saved paths. Nothing is installed by hand.

## Providers

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

## Verified in the installed app

- 百度网盘 (bdpan, 204 MB), 123 云盘 (12 files, 159 MB, guest), 迅雷云盘 (217 MB via Thunder hand-off), 夸克网盘 (2 files, 113 MB, save → download → temporary folder removed).

## Known limits

- Not tested end to end (no accounts): 阿里云盘, UC 网盘, 天翼云盘, 移动云盘 — their anonymous share reading is verified against live public shares.
- Free accounts are throttled by the providers (Quark ≈75 KB/s; Xunlei guest links are slow).
- Xunlei: the hand-off overwrites the clipboard briefly (text is restored), Thunder must be signed in to the same account, and the saved copy stays in the user's Xunlei drive.
- Sign-in sessions are kept in per-drive AI-OS WebView stores; ad-hoc signing only.

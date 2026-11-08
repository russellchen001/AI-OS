# AI-OS Beta 7.1 — 已发布 GitHub 预发布版（2026-11-09）

## 当前状态
- **Beta 7.1 已发布（2026-11-09，用户批准）**：版本 1.0.1-beta.7.1，构建提交 236d279；Tag `ai-os-v1.0.1-beta.7.1`；Release「AI-OS 1.0.1 Beta 7.1」（prerelease）https://github.com/russellchen001/AI-OS/releases/tag/ai-os-v1.0.1-beta.7.1 ；DMG `~/AI-OS/releases/AI-OS_1.0.1-beta.7.1_aarch64.dmg` SHA256 `a750622a3f8e50cd00b706ff8415589bff612e8696c835f4bca20a1a14605473`（唯一保留 DMG，Beta 7 DMG 已移入废纸篓，GitHub 上仍有）。安全 checks 29/29（Rust 1143/0/90），artifacts 2/2；用户确认 Claude 模型选择实测正常。
- 唯一当前状态来源为本文件；设计以 AI_OS_MASTER_GUIDE.md 为参考。
- 分支 beta7，发布基线 ccc3e5d44a26c6ae2b6ed0dff779238a5053dae2；版本 1.0.1-beta.7。
- 用户已明确确认最终人工验收完成。B7-01 至 B7-08 功能开发完成并冻结；本轮不新增功能。
- **已发布（2026-11-09，用户批准推送/Tag/Release）**：分支 beta7 已推送；Tag `ai-os-v1.0.1-beta.7`；GitHub Release「AI-OS 1.0.1 Beta 7」（prerelease）https://github.com/russellchen001/AI-OS/releases/tag/ai-os-v1.0.1-beta.7 。构建提交 `76e6b19`（干净工作树）；DMG `~/AI-OS/releases/AI-OS_1.0.1-beta.7_aarch64.dmg` SHA256 `e01786ccb23124485e12ca61f0a155729e04c34cb77ba1e2307423a1bb95d0f0`，主程序 dashboard `01b30b28…e9db`。再次 Push/Tag/Release 仍需用户明确批准。
- B7-01 底栏修复已提交（76e6b19），四种布局 + 真实完成点击回执在提交前重跑 PASS；原失败机器仍未用最终包确认。最终安全 checks 29/29 PASS（Rust 1141/0/90 ignored，其余组 4/0、12/0、2/0）；artifacts 2/2 PASS。
- 公开发布 BLOCKED：无 Developer ID 身份、Apple 公证及 Gatekeeper 通过证据；不能把临时签名当正式签名。
- 严格全仓格式检查仍 FAIL；剩余 53 个历史文件已单列，不作无关批量格式化。

## Grok 账号登录回归诊断（2026-11-08，Claude）
- 结论：Beta 7 **没有删除** Grok 账号登录。Beta 6 发布版 ccc3e5d 已含 bbfb9f7（移除冒充 Grok CLI 的 grok-oauth，违反 xAI AUP，不恢复）；Beta 6/7 的账号路径都是官方 Grok CLI（`grok login`，本机 1.0.70 仍支持浏览器/设备码登录）。4bf13ea 只把 Grok 接入通用 CLI 进程层，未改认证方式。
- 实测已安装 Beta 7：xAI 卡片 Connected，「Manage connection」+「Use API key」；Sign in again 打开的对话框同时有 Account sign-in 与 API key；Credential=Account sign-in、Managed by the official CLI；Verify Grok CLI 通过；一次真实请求「grok-4.7 (Grok CLI): real generation verified」；完全退出重启后 xAI/OpenAI/Anthropic/Google 仍 Connected。未连接时卡片按钮为「Sign in with Grok」+「Use API key」（源码与后端测试确认）。
- 确认的回归（已修，e245130）：Beta 7 通用 `connectCliProvider` 把 Beta 6 未登录提示「Run `grok login` in Terminal…」换成 Claude 专用文案「Retry sign-in, open the official setup help…」，而 xAI 对话框没有这些按钮，用户只看得到 API Key 出路。现无应用内登录的 CLI（Grok、Gemini）恢复显示自身状态消息（含 `grok login`）并提示可改用 API key；Claude Code 文案不变。
- 验收：verify/verify_beta7_grok_account.sh PASS（隔离平台：未登录提示含 grok login、无假连接、未安装须审批、已登录直接连接；Rust grok 7/0/1 ignored）；回归 beta7_claude_setup、cli_provider_manager、beta6_provider_stability、p15_gm_4_cloud_providers、beta7_credential_vault、beta7_unified_credentials、beta7_keychain_prompt_regression 全 PASS（按原解释器）；Rust openai 11/0；npm run build、git diff --check PASS。未改 Rust、凭据、Keychain。
- 小问题已修（e20a457）：Manage 对话框验证/刷新时 Disconnect 按钮不再误显示 "Disconnecting…"（独立 isDisconnecting，只在真正断开时显示）。
- 型号标签去写死（3f2a510、46248f8，用户要求）：My AI 云端卡片不再带写死型号（原 OpenAI 显示 GPT-5.6，而账号实际验证默认为 GPT-6.1-Sol）；已连接卡片显示该连接 generation-verified 的模型（默认在前，最多 3 个），未连接不显示型号。前端备用目录只给无列表 API 的 Doubao、compatible 保留占位（须真实请求验证）；OpenAI/Anthropic/Google/xAI/DeepSeek/OpenRouter/Kimi/Meta 不再回退到臆造型号。src/config/modelPricing.ts 是计价表，按型号 ID 估算成本，未改。verify/verify_beta7_provider_card_models.sh PASS；grok_account、claude_setup、provider_stability、information_architecture、unified_credentials、cli_provider_manager、p15_gm_4 PASS；npm run build、git diff --check PASS。
- RC2–RC4 均已被正式包取代并移入废纸篓。未完成（人工、非阻塞）：在未登录 Grok CLI 的机器上看 `grok login` 提示。用户 2026-11-09 确认 RC4 卡片型号与断开按钮正常。

## Beta 7.1 内容
- 1f5d950：Claude Code 账号登录可在 AI-OS 内选模型（账号默认 + 本机 Claude Code `--help` 列出的 `--model` 别名，当前 fable/opus/sonnet），每个须真实 Verify 才进聊天；拒绝选项样式的模型 ID。verify/verify_beta7_claude_models.sh。
- 236d279：版本号升至 1.0.1-beta.7.1（package、lock、Cargo、tauri.conf、constants、verify_beta_version、RC 构建路径）。

## 已完成
| ID | 功能 | 状态 |
|---|---|---|
| B7-01 | 首次设置三步、连接后返回 Workspace、显式完成及重启持久化 | 旧包回归 FAIL；修复版浏览器、行为、原生实机及构建 PASS，原失败机器新包确认待验 |
| B7-02 | 依赖检测、安装同意、校验及失败恢复 | PASS，用户确认新用户 Claude 安装/登录成功及最终验收 |
| B7-03 | 首次可用 AI 聊天与连接引导 | PASS，用户最终验收；Auto/手动本地模型直接回答 |
| B7-04 | 权限状态与系统设置引导 | PASS，不代用户授权、不绕过 TCC |
| B7-05 | 可选诊断、故障隔离、支持的修复和复检 | PASS，用户最终验收及故障行为回归 |
| B7-06 | 全新 macOS 用户真实安装及首次流程 | PASS，依据用户 2026-11-08 最终人工验收确认 |
| B7-07 | 顶部/底部跳转、输入框及滚动行为 | PASS，用户最终验收及既有实机证据 |
| B7-08 | 统一保险库、安全接入、非破坏迁移及密码提示控制 | PASS，原失败用户首次 1 次，始终允许后同 App 完全退出重启 0 次 |
- 普通咨询误入 OpenClaw、手动 oMLX 选择与答案可靠性 P0 已完成验收，不重新开发。
- Setup 第三步标题“Step 3 of 3 · 完成设置”，按钮“完成设置，开始聊天”，完成后明确反馈。
- 原用户 Setup 值 complete 与其回忆点击 Start chatting 一致；不能据该值推断提前完成，不重置向导。
- 人工确认是用户证据，不伪称自动化执行本人授权或全新系统用户操作。

## 技术决策
- Setup 第三步主要完成按钮置于不可收缩的独立底栏，只有权限/诊断内容滚动；不通过自动完成、缩小整页文字或修改账号状态规避可达性问题。
- 保留已验收功能；只允许阻断修复、格式和验收/发布准备维护。新功能另立版本。
- 复用 Beta 6 Provider/安装/Agent 基础架构；不重构 Council、Arena 或 Runtime。
- 统一保险库使用 com.ai-os.unified-vault.v1；共享成功快照、版本失效与进程锁保护；拒绝/失败不缓存为成功，日志无秘密。
- 旧凭据首次使用只复制，不自动删除；只有用户显式删除连接才执行对应删除。当前不改凭据策略。
- 咨询直接由选定模型回答，真正执行请求走授权路径；手动本地选择不静默切到云端。
- 高后果建议按能力与可靠来源核验；证据不足停止具体购买/安装指令；记忆不是事实证明。低风险保持快速路径。
- Setup 页面导航与完成分离；只有第三步显式完成才持久化 complete；可选 Agent 诊断不阻塞连接成功状态。
- Claude 安装/登录须本人批准，官方原生包做校验；不代管 CLI 登录令牌；不静默安装依赖。
- RC 格式门槛比较固定 Beta 6 基线的修正内容/数量，提交后仍有效；新文件必须完全格式正确。
- 本轮 7 个格式编辑文件前后的 rustfmt 标准化内容一致，不改变业务逻辑。
- 安全验收使用明确白名单、遵守原脚本解释器，不自动枚举不受控脚本；默认 verify_all 不等于安全模式。
- 用户要求保留历史格式问题，因此本地提交以无新增格式债务门槛验收；全仓 cargo fmt 不伪报 PASS。
- ad-hoc 校验只证明测试包封装；正式发布必须独立完成 Developer ID、Hardened Runtime、时间戳、公证及 Gatekeeper。
- RC 清理只处理可重建产物、旧测试镜像、明确的 E2E 临时目录和完全空目录；源码、锁文件、验收、构建资源及未提交原始快照继续保留。

## 本轮验收
- 新布局回归：使用真实 React 组件和 App.css、隔离平台接口，在 600×450、900×500、1100×720 及 150% 文字/长错误场景测量按钮矩形、实际样式和 elementFromPoint。旧实现前三种均位于可见区域外；修复后四种均可达，最小场景实际点击关闭并保存 complete。记录与当前源码哈希绑定，verify/verify_beta7_setup_layout.sh 可校验浏览器回执。
- 本轮 Setup 完整行为/前端构建及 Auto/手动本地路由回归 PASS；Setup hook、保险库、Provider 源码和 /Applications 主程序哈希与本轮前一致。未改真实凭据或重置进度。
- 新原生 App 初始即显示底部整行完成按钮；实际点击后关闭向导、返回 Workspace 并显示“三步设置已完成”。进程 85342 的可执行路径已核对为本轮 target/release/bundle/macos/AI OS.app。不是旧 App 或仅隔离 fixture 的结果。
- 本轮 App/helper/DMG 重建、资源签名、包内容洁净度及交付镜像完整性 PASS；仅为临时签名测试包。本轮未重跑后端全量单元测试，Rust 源码未改，完成原生 release 编译；下方旧全量结果为既有证据。
- Rust 全量已通过：主组 1141 passed / 0 failed / 90 ignored；其他非空组 4/0、12/0、2/0。ignored 不计 PASS。
- Beta 7 格式范围检查 PASS（14 个 Rust 文件）；全仓历史 53 文件仍 FAIL，见 docs/BETA7_RC_FORMAT_DEBT.md。
- 安全白名单 checks：29/29 PASS（完整 Rust、cargo check、前端、格式范围、差异及 24 个专项/兼容脚本）；首次调用旧脚本解释器有误，已修为遵守 zsh 后整套重跑通过。
- artifacts：2/2 PASS，App/helper/DMG 重建、App/镜像内资源签名、包内个人数据检查及镜像完整性通过；均为临时签名测试包，不是正式签名通过。
- 发布安全实测：0 valid identities，App flags=adhoc/无 TeamIdentifier；stapler 退出 65（无票据），Gatekeeper 退出 3（rejected），保持 BLOCKED。
- 安全入口：bash verify_all.sh --beta7-rc-safe checks；产物阶段参数 artifacts。
- 安全 checks 包含完整 Rust、cargo check、前端、差异检查及 24 个专项/兼容脚本。
- 安全 artifacts 只在 beta7 工作树构建 App/helper/DMG，不启动或安装应用。
- 历史默认 verify_all 全套入口仍 BLOCKED：混合旧安装环境、真实启动及外部账号检查；不宣称已全跑。
- 旧安装恢复脚本经查只读，但断言 Beta 5 且需要旧参数，不适用于当前 Beta 7；此前审批拒绝不代表已证明它会覆盖应用。
- 真实 App 启动诊断、外部 live E2E、新正式签名首次授权及 ignored 用例不由本轮隔离回归替代。
- 详细安全范围、BLOCKED 项及正式发布条件：docs/BETA7_RC_RELEASE_READINESS.md。

## Git 与保护边界
- 本轮已审查现有差异，均属于 Beta 7；备份、日志、构建产物不提交，依赖锁文件正常提交。
- 本地功能提交：2d45072ebade928e90dc86762c49514fc6b6e2cb（55 文件）；RC 工具与交接归档提交：39d545a4c264cc17356ae86e69736faf3c3149da（10 文件）；产物验收文档为后续独立提交，见 git log。2026-11-09 起后续提交见 git log；Beta 7 已按用户批准推送、打 Tag、发布 GitHub prerelease；未执行 done.sh。
- /Applications/AI OS.app 在本轮开始前已为 Beta 7，主程序 SHA256 36507ab2dbb3a0703980dc8610ae824b204dcc6f03e8ff75e07866d7da448a03；不覆盖、不恢复安装目录。
- Beta 6 工作树 HEAD a4d8a14e171f8e9e71907fdeac632f6defd88576 未改；已有备份 HANDOFF.md.bak-20261030-223455 保留。
- 不删除、重置或无授权改写现有 Keychain/Provider 凭据；原生集成测试只用随机测试项。

## 当前候选
- 当前发布包（唯一保留 DMG）：`~/AI-OS/releases/AI-OS_1.0.1-beta.7.1_aarch64.dmg`，SHA256 `a750622a3f8e50cd00b706ff8415589bff612e8696c835f4bca20a1a14605473`，构建提交 236d279，ad-hoc、未公证，codesign --verify --deep --strict 与 hdiutil verify PASS。Beta 7（e01786cc…，76e6b19）、旧 RC1 路径及以下清理节中的"当前唯一朋友测试包"均为历史，已被取代。
- 新签名未执行本人首次授权，不将人工 PASS 扩大为跨重建零弹窗承诺；正式签名后的升级和授权仍需验证。

## RC 交友测试前清理（2026-11-08）
- 活跃 Beta 7 工作树由约 15GB 降至约 322MB；Git 仍为 beta7 / 560756a，清理后无源码差异。
- 经用户明确确认，已永久删除约 14.5GiB 可重建缓存、前端构建输出、旧诊断日志、5 个旧 Beta 7 DMG、25 个文档转换/E2E 临时目录，以及 64 个空日期目录和 24 个空任务目录；不可恢复。
- 实测磁盘可用空间由约 41.2GiB 增至约 55.6GiB，实际释放约 14.4GiB；本次创建的两个隔离目录均已删除。
- 保留全部 Beta 7 源码、Git/worktree 元数据、配置与锁文件、AGENTS/HANDOFF/主指南、verify、构建资源、node_modules、当前唯一交友测试 DMG、迁移后的 94 份 RC1 证据及未提交手工备份。
- 当前唯一朋友测试包：AI_OS_1.0.1-beta.7_RC1_SETUP_FOOTER_FIX_UNSIGNED_NOT_NOTARIZED_aarch64.dmg；SHA256 a0e1bd1c4cd2c2d6104140d43757ce62d0dc45fa00f98d548fd93100c3a7e2c0；固定底栏修复、镜像完整性及原生 App 点击验收 PASS。
- 清理后 cargo metadata、前端构建、关键本地路由测试及朋友测试 DMG 校验 PASS；复查生成的 dist 已重新隔离。
- 该包仍为 ad-hoc、未 Developer ID 签名、未公证，Gatekeeper 可能拒绝；不建议关闭或绕过系统安全保护。
- 用户随后明确决定只保留 2026-11-08 的当前 RC1；已在迁移 RC1 证据后永久删除 ~/Documents/Codex 下其余 14 个日期目录（约 2.3GB），不可恢复。Beta 5/6 代码工作树、/Applications/AI OS.app、用户数据、Keychain 和 NAS 未改；不推送、不打 tag、不正式发布。

## 正式发布条件与本人下一步
1. 本人确认有效 Apple Developer Program 会员，并由 Account Holder 办理 Developer ID Application 证书及对应私钥在构建 Mac 可用。
2. 本人在本机配置公证授权（Team ID + app-specific password，或适用 API key）；不要把密码/私钥/Token 发到聊天或写源码。
3. 当前 0 valid identities；notarytool/stapler 已安装。当前构建固定 ad-hoc，需在正式签名阶段切换身份、时间戳、Hardened Runtime/entitlements，并签主程序及 helper。
4. 正式签名后 Apple notarization Accepted、staple 和 Gatekeeper 通过，再验升级保留数据与 Keychain 行为；重新封装并记录最终分发哈希。
5. 历史全仓格式与未执行的默认总验收门槛保持单独记录，不能宣称无条件全绿；正式发布另需本人授权。
- Beta 7 已作为 GitHub 预发布版发布（与 Beta 6 相同的 ad-hoc 条件）；面向陌生用户的公开正式版仍需上述签名与公证，不绕过 Gatekeeper。

## 历史与已否决
- 全部原 HANDOFF（含 Beta 6、此前 FAIL/待验和技术记录）已原样归档到 docs/archive/HANDOFF_HISTORY.md；不作为第二份当前状态。
- 不通过隐藏向导、强制完成、硬编码答案、统一免责声明、重置凭据或放宽 macOS 安全机制来过验收。

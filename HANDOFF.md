# AI-OS Beta 7 — RC1 发布准备（2026-11-08）

## 当前状态
- 唯一当前状态来源为本文件；设计以 AI_OS_MASTER_GUIDE.md 为参考。
- 分支 beta7，发布基线 ccc3e5d44a26c6ae2b6ed0dff779238a5053dae2；版本 1.0.1-beta.7。
- 用户已明确确认最终人工验收完成。B7-01 至 B7-08 功能开发完成并冻结；本轮不新增功能。
- 当前收尾完成：Beta 7 范围格式 PASS，安全 checks 29/29、artifacts 2/2 PASS；本地功能提交 2d45072、RC 工具/文档提交 39d545a，新候选从干净的 39d545a 固定构建。后续仅补交验收产物记录，功能继续冻结。
- 公开发布 BLOCKED：无 Developer ID 身份、Apple 公证及 Gatekeeper 通过证据；不能把临时签名当正式签名。
- 严格全仓格式检查仍 FAIL；剩余 53 个历史文件已单列，不作无关批量格式化。

## 已完成
| ID | 功能 | 状态 |
|---|---|---|
| B7-01 | 首次设置三步、连接后返回 Workspace、显式完成及重启持久化 | PASS，用户最终验收及行为回归 |
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

## 本轮验收
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
- 本地功能提交：2d45072ebade928e90dc86762c49514fc6b6e2cb（55 文件）；RC 工具与交接归档提交：39d545a4c264cc17356ae86e69736faf3c3149da（10 文件）；产物验收文档为后续独立提交，见 git log。没有推送、创建/移动标签或正式发布，未执行 done.sh。
- /Applications/AI OS.app 在本轮开始前已为 Beta 7，主程序 SHA256 36507ab2dbb3a0703980dc8610ae824b204dcc6f03e8ff75e07866d7da448a03；不覆盖、不恢复安装目录。
- Beta 6 工作树 HEAD a4d8a14e171f8e9e71907fdeac632f6defd88576 未改；已有备份 HANDOFF.md.bak-20261030-223455 保留。
- 不删除、重置或无授权改写现有 Keychain/Provider 凭据；原生集成测试只用随机测试项。

## 当前候选
- 本轮镜像：outputs/AI_OS_1.0.1-beta.7_RC1-local_aarch64.dmg；SHA256 7e8f80b6df0733fbf19dbddb40c1ed45ec32c13b257142dc13807386d4c89a11；旧交付镜像保留。
- 构建源：39d545a4c264cc17356ae86e69736faf3c3149da，构建开始时工作区干净；之后只改验收文档。
- App：src-tauri/target/release/bundle/macos/AI OS.app；主程序 SHA256 1c57641c5b4c9645df2147e40a377a27aaa13d21aaa4be512b4dd59cade34aa2；CDHash 1737d77a2206fcc8a8143352d0ece15602efa412。
- 新签名未执行本人首次授权，不将原包人工 PASS 扩大为跨重建零弹窗承诺；正式签名后的升级和授权仍需验证。
- /Users/Shared/AI-OS-Beta7-Test.app 保持原已验测试版，不自动替换。
- 交付根目录：/Users/russellchen/Documents/Codex/2026-11-04/referenced-chatgpt-conversation-this-is-an/outputs。
- 最终报告：交付目录 Beta7_RC1_RELEASE_PREP_2026-11-08.md；日志与逐项结果在 beta7-rc-prep-20261108/。

## 正式发布条件与本人下一步
1. 本人确认有效 Apple Developer Program 会员，并由 Account Holder 办理 Developer ID Application 证书及对应私钥在构建 Mac 可用。
2. 本人在本机配置公证授权（Team ID + app-specific password，或适用 API key）；不要把密码/私钥/Token 发到聊天或写源码。
3. 当前 0 valid identities；notarytool/stapler 已安装。当前构建固定 ad-hoc，需在正式签名阶段切换身份、时间戳、Hardened Runtime/entitlements，并签主程序及 helper。
4. 正式签名后 Apple notarization Accepted、staple 和 Gatekeeper 通过，再验升级保留数据与 Keychain 行为；重新封装并记录最终分发哈希。
5. 历史全仓格式与未执行的默认总验收门槛保持单独记录，不能宣称无条件全绿；正式发布另需本人授权。
- 可以进入发布准备；尚不能公开发布，不绕过 Gatekeeper。

## 历史与已否决
- 全部原 HANDOFF（含 Beta 6、此前 FAIL/待验和技术记录）已原样归档到 docs/archive/HANDOFF_HISTORY.md；不作为第二份当前状态。
- 不通过隐藏向导、强制完成、硬编码答案、统一免责声明、重置凭据或放宽 macOS 安全机制来过验收。

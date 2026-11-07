# Beta 7 RC1 — 发布准备与验收边界

日期：2026-11-08。版本：1.0.1-beta.7。当前状态仅以根目录 HANDOFF.md 为准。

## 功能冻结与源码

- B7-01 至 B7-08 已由用户确认最终人工验收完成。普通聊天、Auto/手动路由及答案可靠性继续冻结。
- RC 收尾只修 Beta 7 格式、新增验收工具及整理文档/Git，不新增产品功能。
- 基线 ccc3e5d44a26c6ae2b6ed0dff779238a5053dae2；Beta 6 工作树和系统安装目录不修改。
- 备份 `*.bak-*`、日志、依赖目录及构建产物不提交；依赖锁文件属于可重现源码，保留提交。

## 格式门槛

- `bash verify/verify_beta7_format_scope.sh`：比较 Beta 7 Rust 文件与固定发布基线的 rustfmt 修正内容/数量。新文件必须完全符合 rustfmt；已修改文件不能引入新的格式修正。
- 7 个格式编辑文件在编辑前后的 rustfmt 标准化内容完全一致。
- 整体 `cargo fmt --check` 仍 FAIL，剩余 53 个文件的历史差异见 [格式债务清单](BETA7_RC_FORMAT_DEBT.md)。此结果不被伪装为 PASS，也未大规模格式化。
- 用户本轮明确要求修复 Beta 7 范围、保留历史问题，因此本地提交使用“无新增格式债务”门槛；不推定历史质量债务已经消失。

## 安全验收入口

本轮 checks 最终结果：29/29 PASS（5 个基础门槛及 24 个专项脚本）。完整 Rust 主组 1141 passed / 0 failed / 90 ignored，其他非空组 4/0、12/0、2/0。前端及 cargo check PASS。初次入口将两份 zsh 脚本误按 bash 调用，修正解释器后整套重新通过；不是删除失败项目过关。artifacts 最终 2/2 PASS，完成 App/helper/DMG 构建、资源签名/包内容与镜像完整性验证。

本地功能代码提交：2d45072ebade928e90dc86762c49514fc6b6e2cb。RC 验收入口和文档独立提交；没有把备份、日志、dist/target 或 node_modules 纳入提交。

固定构建源提交：39d545a4c264cc17356ae86e69736faf3c3149da（构建开始时工作区干净）。后续只补交产物及验收文档。交付文件 AI_OS_1.0.1-beta.7_RC1-local_aarch64.dmg，SHA-256 7e8f80b6df0733fbf19dbddb40c1ed45ec32c13b257142dc13807386d4c89a11。主程序 SHA-256 1c57641c5b4c9645df2147e40a377a27aaa13d21aaa4be512b4dd59cade34aa2。

本轮最终候选仍为 ad-hoc：CDHash 1737d77a2206fcc8a8143352d0ece15602efa412，stapler validate 退出 65（无票据），Gatekeeper 退出 3（rejected）。这些是实际发布限制，不是 PASS；没有执行新签名首次授权、没有替换系统或原测试 App。

```sh
bash verify_all.sh --beta7-rc-safe checks
bash verify_all.sh --beta7-rc-safe artifacts
```

`checks` 运行完整 Rust、cargo check、前端构建、格式范围检查、差异检查及 24 个明确列出的专项/兼容脚本。日志通过 AIOS_RC_LOG_DIR 指定；默认保存在独立临时目录。`artifacts` 仅构建工作树 App/内置 helper/DMG 并验证，不安装或启动应用。

本轮不运行原始不带参数的全量入口，且安全入口不会自动搜集新脚本。完整 Rust 中 ignored 用例不计 PASS；随机测试 Keychain 项和生产保险库严格分开，不删除或重置用户既有凭据。

### 为什么原总入口涉及安装目录

`verify_all.sh` 原默认模式枚举所有 verify_*.sh，并串行运行部分外部 E2E，混合了不同时期的环境假设。

- `verify_beta6_install_restore.sh`：读取 /Applications/AI OS.app 并比对恢复的 Beta 5、备份及旧 DMG。实查为只读，没有复制/安装操作；但当前安装已是 Beta 7，所需旧恢复参数也不适用。此前自动审批因总入口范围不明拒绝，不代表已经证明此脚本会覆盖应用。
- `verify_beta7_first_launch_diagnostic.sh`：会真实启动 /Users/Shared/AI-OS-Beta7-Test.app，并查询系统日志，不是纯隔离测试。
- 历史 external/live 验收：可能要求真实账号、授权、外部服务或其他安装环境。不能把本轮源码回归当作这些 E2E 已执行。

### BLOCKED / 未执行项

| 项目 | 本轮状态与原因 |
|---|---|
| 原默认 verify_all 全套入口 | BLOCKED；混合安装/启动/外部环境门槛，本轮使用安全白名单，不声明其通过 |
| 旧 Beta 5 恢复比对 | 不适用当前 Beta 7；未提供旧安装恢复参数，不去改安装制造测试条件 |
| 真实 App 首次启动诊断脚本 | BLOCKED；会启动既有资料的测试 App，未在本轮重复账号/系统授权验收 |
| 90 项 ignored Rust 用例 | 未执行，不计 PASS；环境/账号用例不因默认 cargo test 通过自动解禁 |
| 外部账号/服务 live E2E | BLOCKED；本轮没有代用户授权、下单、发消息或修改外部账号 |
| 新签名首次授权/升级验收 | BLOCKED；正式 Developer ID 尚缺，当前不承诺跨重建零弹窗 |

## 正式签名与公证

本机只读检查：0 valid code-signing identities。notarytool、stapler 已安装。当前 tauri.conf.json 的 signingIdentity 为 `-`，构建脚本也固定临时签名；当前 App flags=adhoc，无正式 TeamIdentifier。资源签名校验通过仅证明测试包封装有效。

正式发布前必须：

1. 本人确认 Apple Developer Program 会员有效；若未加入则先办理，账号及协议由本人处理。
2. 由 Account Holder 创建 **Developer ID Application** 证书，并在构建 Mac 上具备对应私钥。若证书在另一台 Mac 创建，需要由本人安全导入证书和私钥，不只复制 .cer；不把私钥或密码贴到聊天。DMG 内分发 .app 不需要为此额外申请 Installer 证书；以后使用 .pkg 才另行评估。
3. 本人在本机配置公证授权：Apple 账号、Team ID 和 app-specific password 的本地凭据配置，或适用的 App Store Connect API key。不得把这些秘密写入源码或验收日志。当前会员资格与账号授权未代为验证。
4. 在授权正式签名阶段切换当前强制 ad-hoc 的构建参数；对内置 helper 和 App 依次使用正式身份、安全时间戳及必要 Hardened Runtime/entitlements，再验证签名链。不能仅拿到证书就继续使用当前测试构建脚本冒充正式包。
5. 提交 Apple notarization，等待 Accepted，staple 并校验票据，Gatekeeper 必须通过；正式签名后复验升级保留数据与 Keychain 行为，最后重新封装和记录分发哈希。

本人完成会员、证书/私钥和公证授权准备后，技术签名与验收可以继续；本轮不申请证书、不导入私钥、不修改信任、不绕过 Gatekeeper、不上传公证、不推送/打标签/发布。

Apple 官方依据：

- [Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)：Application/Installer 用途及创建角色。
- [Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)：Developer ID、公证与 Hardened Runtime。
- [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)：notarytool 工作流及授权方式。

## 发布结论

功能开发完成、功能冻结；Beta 7 范围代码格式及安全回归通过后可形成已提交的本地功能 RC。全仓历史格式、默认全套入口、正式签名/公证/Gatekeeper 仍不宣称通过。可以继续发布准备，不能正式公开发布。

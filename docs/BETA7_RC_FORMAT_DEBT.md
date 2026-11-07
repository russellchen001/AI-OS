# Beta 7 RC — historical Rust format debt

Baseline: `ccc3e5d44a26c6ae2b6ed0dff779238a5053dae2` (released Beta 6).

The whole-repository `cargo fmt --check` remains FAIL. This is not a waived or passing check. Beta 7 introduced corrections were repaired; `verify/verify_beta7_format_scope.sh` compares correction content and counts against this fixed baseline, even after Beta 7 is committed.

The seven files edited in RC preparation produce identical rustfmt-normalized source before and after the edit; no business logic was changed. Existing unrelated formatting was intentionally preserved.

| Remaining file | Evidence |
|---|---|
| `src-tauri/src/bin/ai-os-cloud-drive-mcp.rs` | Byte-identical to baseline |
| `src-tauri/src/browser/account_verifier.rs` | Byte-identical to baseline |
| `src-tauri/src/browser/authenticated_runtime.rs` | Byte-identical to baseline |
| `src-tauri/src/browser/devtools.rs` | Byte-identical to baseline |
| `src-tauri/src/browser/diagnostics.rs` | Byte-identical to baseline |
| `src-tauri/src/browser/mod.rs` | Byte-identical to baseline |
| `src-tauri/src/browser/site_registry.rs` | Byte-identical to baseline |
| `src-tauri/src/cli_process.rs` | Byte-identical to baseline |
| `src-tauri/src/cli_provider_manager.rs` | Modified file; remaining corrections match baseline via scope verifier |
| `src-tauri/src/cloud_drive.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_123.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_139.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_189.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_aliyun.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_baidu.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_core.rs` | Byte-identical to baseline |
| `src-tauri/src/cloud_drive_quark.rs` | Byte-identical to baseline |
| `src-tauri/src/computer_use/peekaboo.rs` | Byte-identical to baseline |
| `src-tauri/src/connections.rs` | Byte-identical to baseline |
| `src-tauri/src/conversations.rs` | Byte-identical to baseline |
| `src-tauri/src/document.rs` | Byte-identical to baseline |
| `src-tauri/src/document/excel.rs` | Byte-identical to baseline |
| `src-tauri/src/document/iwork_convert.rs` | Byte-identical to baseline |
| `src-tauri/src/document/numbers.rs` | Byte-identical to baseline |
| `src-tauri/src/document/pages.rs` | Byte-identical to baseline |
| `src-tauri/src/document/pdf.rs` | Byte-identical to baseline |
| `src-tauri/src/document/pdf_layout.rs` | Byte-identical to baseline |
| `src-tauri/src/document/resolver.rs` | Byte-identical to baseline |
| `src-tauri/src/document/structured.rs` | Byte-identical to baseline |
| `src-tauri/src/document/word.rs` | Byte-identical to baseline |
| `src-tauri/src/gemini_cli.rs` | Byte-identical to baseline |
| `src-tauri/src/google_workspace.rs` | Byte-identical to baseline |
| `src-tauri/src/grok_cli.rs` | Byte-identical to baseline |
| `src-tauri/src/keychain_trace.rs` | Byte-identical to baseline |
| `src-tauri/src/lib.rs` | Modified file; remaining corrections match baseline via scope verifier |
| `src-tauri/src/logs.rs` | Byte-identical to baseline |
| `src-tauri/src/models.rs` | Byte-identical to baseline |
| `src-tauri/src/openclaw.rs` | Byte-identical to baseline |
| `src-tauri/src/remote_linco.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/adapters.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/agent_skill_transport.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/lifecycle.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/openclaw_gateway_adapter.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/operations.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/registry.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/skills/contracts.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/skills/registry.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/speech.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/trusted_automation.rs` | Byte-identical to baseline |
| `src-tauri/src/runtime/web_search.rs` | Modified file; remaining corrections match baseline via scope verifier |
| `src-tauri/src/system/apps.rs` | Byte-identical to baseline |
| `src-tauri/src/system/clipboard.rs` | Byte-identical to baseline |
| `src-tauri/src/system/inspect.rs` | Byte-identical to baseline |

These 53 files are explicitly excluded from this formatting-only scope. Repairing them is a separate maintenance task, not a silent all-green RC claim.

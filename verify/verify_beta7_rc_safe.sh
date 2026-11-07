#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-checks}"
case "$mode" in checks|artifacts) ;; *) echo 'FAIL: beta7_rc_safe — use checks or artifacts'; exit 1 ;; esac
log_dir="${AIOS_RC_LOG_DIR:-$(mktemp -d /private/tmp/aios-beta7-rc.XXXXXX)}"
mkdir -p "$log_dir"
failed=0
run() {
  local name="$1"
  shift
  if "$@" > "$log_dir/$name.log" 2>&1; then
    printf '✅ %s\n' "$name"
    printf 'PASS\t%s\n' "$name" >> "$log_dir/results-$mode.tsv"
  else
    local code=$?
    printf '❌ %s (exit %s)\n' "$name" "$code"
    tail -12 "$log_dir/$name.log"
    printf 'FAIL\t%s\t%s\n' "$name" "$code" >> "$log_dir/results-$mode.tsv"
    failed=1
  fi
}
: > "$log_dir/results-$mode.tsv"
if [ "$mode" = artifacts ]; then
  # Builds/seals only the worktree bundle. No installation or app launch.
  run app-dmg-build bash scripts/build_beta_macos.sh
  run dmg-integrity hdiutil verify 'src-tauri/target/release/bundle/dmg/AI OS_1.0.1-beta.7_aarch64.dmg'
else
  run format-scope bash verify/verify_beta7_format_scope.sh
  run rust-full cargo test --manifest-path src-tauri/Cargo.toml
  run rust-check cargo check --manifest-path src-tauri/Cargo.toml
  run frontend-build npm run build
  run whitespace git diff --check
  # Explicitly audited allowlist: fixtures, pure checks, and random native
  # test items. Never auto-expand to every script in verify/.
  for script in \
    beta7_first_launch beta7_claude_setup beta7_chat_local_routing \
    beta7_answer_reliability beta7_macos_permissions beta7_chat_jump \
    beta7_credential_vault beta7_vault_failure_preflight \
    beta7_vault_failure_preservation beta7_vault_migration \
    beta7_vault_process_lock beta7_vault_lock_contention \
    beta7_unified_credentials beta7_keychain_prompt_regression \
    beta7_openclaw_startup beta_clean_profile beta_version \
    beta6_provider_stability beta6_render_security \
    beta6_information_architecture beta6_artifacts_archive \
    beta4_context_budget beta4_knowledge_engine beta4_new_chat_control; do
    case "$script" in
      beta4_context_budget|beta4_knowledge_engine)
        run "$script" /bin/zsh "verify/verify_$script.sh" ;;
      *) run "$script" bash "verify/verify_$script.sh" ;;
    esac
  done
fi
echo 'BLOCKED: broader live/account/installation probes are excluded; see docs/BETA7_RC_RELEASE_READINESS.md.'
echo "Logs: $log_dir"
if [ "$failed" -ne 0 ]; then echo "FAIL: beta7_rc_safe — $mode"; exit 1; fi
echo "PASS: beta7_rc_safe — $mode allowlist only; not a public-release approval"

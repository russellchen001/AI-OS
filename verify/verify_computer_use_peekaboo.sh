#!/bin/zsh
set -u

repo="${0:A:h:h}"
fixture="/private/tmp/aios-computer-use-fixture.txt"
logs="$(mktemp -d /private/tmp/aios-computer-use-verify.XXXXXX)"
trap 'rm -rf "$logs"' EXIT
cd "$repo/src-tauri" || exit 1

if cargo test --lib computer_use:: -- --test-threads=1 > "$logs/unit.log" 2>&1; then
  print '✅ Computer Use contract and guard tests'
else
  print '❌ Computer Use contract and guard tests'
  tail -12 "$logs/unit.log"
  print 'FAIL: Computer Use Peekaboo — unit tests'
  exit 1
fi

if node --experimental-strip-types "$repo/verify/p15_air_1c_hybrid_intent.behavior.mjs" > "$logs/intent.log" 2>&1 &&
   node --experimental-strip-types "$repo/verify/computer_use_output.behavior.mjs" > "$logs/output.log" 2>&1 &&
   cargo test --lib computer_use_input_is_grounded_to_goal_and_bounded -- --test-threads=1 > "$logs/contract.log" 2>&1 &&
   cargo test --lib mp0_selected_agent_crosses_transport_gateway_backend_registry_and_provider -- --test-threads=1 > "$logs/router.log" 2>&1; then
  print '✅ GUI intent, result presentation, bounded contract, and Agent selection route'
else
  print '❌ GUI intent, result presentation, input contract, or Agent selection route'
  tail -12 "$logs/intent.log" "$logs/output.log" "$logs/contract.log" "$logs/router.log" 2>/dev/null
  print 'FAIL: Computer Use Peekaboo — normal task route'
  exit 1
fi

if cargo test --lib live_peekaboo_probe -- --ignored > "$logs/probe.log" 2>&1; then
  print '✅ Signed Peekaboo bridge, permissions, and local tool-calling model'
else
  print '❌ Signed Peekaboo bridge, permissions, or local tool-calling model'
  tail -12 "$logs/probe.log"
  print 'FAIL: Computer Use Peekaboo — live probe'
  exit 1
fi

if [[ ! -f "$fixture" ]]; then
  print 'AI-OS Computer Use local fixture.' > "$fixture"
fi
open -a TextEdit "$fixture"
for run in 1 2 3; do
  if cargo test --lib live_peekaboo_textedit_fixture -- --ignored > "$logs/live.log" 2>&1; then
    print "✅ TextEdit single-step input and fresh AX readback ($run/3)"
  else
    print "❌ TextEdit single-step input or AX readback ($run/3)"
    tail -12 "$logs/live.log"
    print 'FAIL: Computer Use Peekaboo — live TextEdit fixture'
    exit 1
  fi

  if cargo test --lib live_peekaboo_two_step_textedit_fixture -- --ignored > "$logs/two-step.log" 2>&1; then
    print "✅ Two model-planned actions and final AX readback ($run/3)"
  else
    print "❌ Two model-planned actions or final AX readback ($run/3)"
    tail -12 "$logs/two-step.log"
    print 'FAIL: Computer Use Peekaboo — two-step fixture'
    exit 1
  fi
done

if cargo test --lib live_production_skill_invocation_reaches_peekaboo -- --ignored > "$logs/runtime.log" 2>&1; then
  print '✅ Production Computer Use Skill routes to Peekaboo'
else
  print '❌ Production Computer Use Skill did not complete'
  tail -12 "$logs/runtime.log"
  print 'FAIL: Computer Use Peekaboo — Runtime route'
  exit 1
fi

for run in 1 2 3; do
  if cargo test --lib live_peekaboo_calculator_single_click_reports_completion -- --ignored > "$logs/calculator.log" 2>&1; then
    print "✅ Calculator: one click on 8, display 8, reported completed ($run/3)"
  else
    print "❌ Calculator single click or completion report ($run/3)"
    tail -12 "$logs/calculator.log"
    print 'FAIL: Computer Use Peekaboo — Calculator'
    exit 1
  fi
done

if cargo test --lib live_peekaboo_action_timeout_kills_stalled_child -- --ignored > "$logs/timeout.log" 2>&1; then
  print '✅ Provider timeout ends a hung GUI action child; Calculator unchanged'
else
  print '❌ Provider timeout did not end a hung GUI action child cleanly'
  tail -12 "$logs/timeout.log"
  print 'FAIL: Computer Use Peekaboo — action timeout'
  exit 1
fi

if cargo test --lib live_peekaboo_cancellation_during_active_action -- --ignored > "$logs/cancel.log" 2>&1; then
  print '✅ Cancellation during an executing GUI action ends it; Calculator unchanged'
else
  print '❌ Cancellation during an executing GUI action failed'
  tail -12 "$logs/cancel.log"
  print 'FAIL: Computer Use Peekaboo — active cancellation'
  exit 1
fi

print 'PASS: Computer Use Peekaboo'
exit 0

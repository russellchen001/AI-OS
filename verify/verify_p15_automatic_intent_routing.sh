#!/bin/zsh

cd "$(dirname "$0")/.." || exit 1

FAIL=0

echo "=== P15 AUTOMATIC INTENT ROUTING VERIFY ==="

echo
echo "--- router service ---"

if rg -q 'export async function classifyAutomaticIntent' \
  src/services/intentRouting.ts; then
  echo "PASS: automatic intent classifier exists"
else
  echo "FAIL: automatic intent classifier missing"
  FAIL=1
fi

if rg -q 'taskType: "ASK"|"DO"' \
  src/services/intentRouting.ts; then
  :
fi

echo
echo "--- router does not select Skill capabilities ---"

if rg -n \
  'filesystem\.|download\.start|models\.(list|show|pull|delete)|nas\.|email\.|calendar\.|media\.' \
  src/services/intentRouting.ts
then
  echo "FAIL: intent router contains concrete Skill routing"
  FAIL=1
else
  echo "PASS: intent router classifies ASK/DO only"
fi

echo
echo "--- ChatPage uses automatic route ---"

if rg -q \
  'classifyAutomaticIntent\(content, selectedModel\)' \
  src/pages/ChatPage.tsx
then
  echo "PASS: ChatPage invokes automatic intent routing"
else
  echo "FAIL: ChatPage does not invoke automatic intent routing"
  FAIL=1
fi

if rg -q \
  'submitChatTask\(content, routedTaskType\)' \
  src/pages/ChatPage.tsx
then
  echo "PASS: Task Engine receives routed ASK/DO type"
else
  echo "FAIL: routed task type does not reach Task Engine"
  FAIL=1
fi

echo
echo "--- generic DO execution ---"

if rg -U -q \
  'executeChatWorkTask\(\s*task\.taskId,\s*"openclaw",\s*\)' \
  src/pages/ChatPage.tsx
then
  echo "PASS: DO uses generic Agent execution without composer-selected capability"
else
  echo "FAIL: DO execution is not generic"
  FAIL=1
fi

echo
echo "--- exact approval binding guard ---"

if python3 <<'PY'
from pathlib import Path

source = Path("src/pages/ChatPage.tsx").read_text()
needle = "executeChatWorkTask("
offset = 0
forbidden = []

while (start := source.find(needle, offset)) != -1:
    depth = 0
    end = start
    for index in range(start + len(needle) - 1, len(source)):
        char = source[index]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                end = index + 1
                break
    call = source[start:end]
    if "userConfirmed: true" in call and not (
        "capability: approval.capability" in call
        and "input: approval.input" in call
    ):
        forbidden.append(call[:240])
    offset = max(end, start + len(needle))

if forbidden:
    raise SystemExit("blanket generic userConfirmed approval found")
PY
then
  echo "PASS: user confirmation is bound to exact capability and input"
else
  echo "FAIL: blanket generic userConfirmed approval found"
  FAIL=1
fi

echo
echo "--- visible composer shortcut guard ---"

COMPOSER_VISIBLE_TMP="/tmp/ai-os-air-composer-visible.txt"

python3 <<'PY' > "$COMPOSER_VISIBLE_TMP"
from pathlib import Path

text = Path("src/pages/ChatPage.tsx").read_text()
start = text.index('<div className="composer-dock">')
end = text.index('</form>', start)
print(text[start:end])
PY

for label in \
  'Task mode:' \
  '>Chat<' \
  '>Work<' \
  'Scan folder' \
  'Read file' \
  'Write file' \
  'Move file' \
  '>Download<' \
  'List models' \
  'Inspect model' \
  'Download model' \
  'Delete model' \
  'Run this task with'
do
  if rg -F -q "$label" "$COMPOSER_VISIBLE_TMP"; then
    echo "FAIL: visible composer still contains legacy control: $label"
    FAIL=1
  fi
done

if [ "$FAIL" -eq 0 ]; then
  echo "PASS: legacy capability/mode controls are absent from visible composer"
fi

echo
echo "--- required composer controls ---"

for label in \
  'aria-label="Attach files"' \
  'selectedModel?.label ?? "Auto"' \
  'aria-label="Send"'
do
  if rg -F -q "$label" "$COMPOSER_VISIBLE_TMP"; then
    echo "PASS: composer retains $label"
  else
    echo "FAIL: composer missing $label"
    FAIL=1
  fi
done

echo
echo "--- safe fallback ---"

if rg -q \
  'taskType: "ASK"' \
  src/services/intentRouting.ts &&
   rg -q \
  'source: "safe-fallback"' \
  src/services/intentRouting.ts
then
  echo "PASS: classifier failure fails closed to ASK"
else
  echo "FAIL: safe ASK fallback missing"
  FAIL=1
fi

echo
echo "--- frontend build ---"

npm run build
BUILD_RC=$?

if [ "$BUILD_RC" -eq 0 ]; then
  echo "PASS: frontend build"
else
  echo "FAIL: frontend build"
  FAIL=1
fi

echo
echo "--- diff check ---"

git diff --check
DIFF_RC=$?

if [ "$DIFF_RC" -eq 0 ]; then
  echo "PASS: git diff --check"
else
  echo "FAIL: git diff --check"
  FAIL=1
fi

echo
echo "=== RESULT ==="

if [ "$FAIL" -eq 0 ]; then
  echo "P15_AUTOMATIC_INTENT_ROUTING=PASS"
else
  echo "P15_AUTOMATIC_INTENT_ROUTING=FAIL"
fi

exit "$FAIL"

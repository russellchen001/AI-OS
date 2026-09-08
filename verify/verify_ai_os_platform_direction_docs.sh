#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

fail() {
  echo "FAIL AI-OS Platform Direction Docs: $1"
  exit 1
}

[ -f AI_OS_MASTER_GUIDE.md ] ||
  fail "AI_OS_MASTER_GUIDE.md missing"

[ -f HANDOFF.md ] ||
  fail "HANDOFF.md missing"

grep -Fq \
  'AI-OS is a **multi-model, multi-Agent, multi-Skill AI application platform**.' \
  AI_OS_MASTER_GUIDE.md ||
  fail "platform identity missing"

grep -Fq \
  '## Reuse First / GitHub First' \
  AI_OS_MASTER_GUIDE.md ||
  fail "Reuse First / GitHub First missing"

grep -Fq \
  'AI-OS is architecturally a multi-Agent platform.' \
  AI_OS_MASTER_GUIDE.md ||
  fail "multi-Agent architecture boundary missing"

grep -Fq \
  'ComfyUI-Agent-Kit' \
  AI_OS_MASTER_GUIDE.md ||
  fail "ComfyUI-Agent-Kit missing"

grep -Fq \
  'ComfyUI-Mac-Silicon' \
  AI_OS_MASTER_GUIDE.md ||
  fail "ComfyUI-Mac-Silicon missing"

grep -Fq \
  'GM-4 must reuse Provider accounts and Provider Instances already connected' \
  AI_OS_MASTER_GUIDE.md ||
  fail "GM-4 shared Provider account rule missing"

grep -Fq \
  '#### Mano-P / Mano-CUA' \
  AI_OS_MASTER_GUIDE.md ||
  fail "Mano-P / Mano-CUA missing"

grep -Fq \
  '#### Magnitude' \
  AI_OS_MASTER_GUIDE.md ||
  fail "Magnitude missing"

grep -Fq \
  '## v2.0 Deferred — Social / Community Intelligence' \
  AI_OS_MASTER_GUIDE.md ||
  fail "Social / Community Intelligence v2 deferral missing"

python3 <<'PY'
from pathlib import Path

text = Path("AI_OS_MASTER_GUIDE.md").read_text()

start = text.find("## P15 — Core Skills")
end = text.find("## P16 — Strategic Intelligence and AI Council")

if start == -1 or end == -1 or start >= end:
    raise SystemExit(
        "FAIL verifier: P15/P16 roadmap boundaries missing"
    )

p15 = text[start:end]

expected = [
    "1. Email and Calendar",
    "2. Browser and Search",
    "3. File Management",
    "4. Downloads",
    "5. NAS Management",
    "6. Document / Spreadsheet / Presentation Workflows",
    "7. Local Model Management",
    "8. Computer Control",
    "9. Local Generative Media",
    "10. Cognitive Distillation Foundation",
]

for item in expected:
    if item not in p15:
        raise SystemExit(
            f"FAIL verifier: missing P15 Skill: {item}"
        )

if "11." in p15:
    raise SystemExit(
        "FAIL verifier: P15 contains more than 10 numbered Skills"
    )

if "Vehicle Control" in p15:
    raise SystemExit(
        "FAIL verifier: Vehicle Control still exists inside active P15 roadmap"
    )

print("✓ P15 contains exactly the accepted 10 Skills")
print("✓ Vehicle Control absent from active P15 roadmap")
PY

grep -Fq \
  'Vehicle Control is **not part of AI-OS v1.0**.' \
  HANDOFF.md ||
  fail "Vehicle Control v1 removal decision missing"

grep -Fq \
  'supersedes all earlier v1.0 / P15 Vehicle Control roadmap' \
  HANDOFF.md ||
  fail "Vehicle Control supersession rule missing"

grep -Fq \
  'GM-4 reuses P13 / My AI Provider accounts and Provider Instances.' \
  HANDOFF.md ||
  fail "HANDOFF GM-4 account reuse missing"

grep -Fq \
  'ComfyUI-Agent-Kit is accepted as the preferred reusable foundation' \
  HANDOFF.md ||
  fail "HANDOFF ComfyUI-Agent-Kit decision missing"

grep -Fq \
  'Accepted as a v1.0 post-Generative-Media GUI / Computer Use intelligence' \
  HANDOFF.md ||
  fail "HANDOFF Mano-P decision missing"

grep -Fq \
  'Accepted as a v1.0 post-Generative-Media Local Model optimization / inference' \
  HANDOFF.md ||
  fail "HANDOFF Magnitude decision missing"

grep -Fq \
  'Social / Community Intelligence is explicitly deferred to v2.0.' \
  HANDOFF.md ||
  fail "HANDOFF Social Intelligence deferral missing"

if [ -f ROADMAP.md ]; then
  fail "ROADMAP.md must not exist; roadmap belongs in AI_OS_MASTER_GUIDE.md"
fi

git diff --check ||
  fail "git diff --check"

echo "✓ Multi-Model platform"
echo "✓ Multi-Agent platform"
echo "✓ Multi-Skill platform"
echo "✓ Reuse First / GitHub First"
echo "✓ shared Provider/account infrastructure"
echo "✓ P15 exactly 10 Skills"
echo "✓ Vehicle Control removed from v1.0"
echo "✓ ComfyUI-Agent-Kit"
echo "✓ ComfyUI-Mac-Silicon"
echo "✓ GM-4 reuses P13 / My AI"
echo "✓ Mano-P / Mano-CUA"
echo "✓ Magnitude"
echo "✓ Social / Community Intelligence → v2.0"
echo "✓ Roadmap remains inside AI_OS_MASTER_GUIDE.md"
echo "✓ HANDOFF remains current-state authority"
echo "✓ git diff --check"

echo "PASS AI-OS Platform Direction Docs"

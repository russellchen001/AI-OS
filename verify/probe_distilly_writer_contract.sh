#!/bin/bash
# Probe: prove the exact Distilly writer CLI contract AI-OS depends on, without
# OpenClaw, without an Agent turn and without touching the user's profile store.
#
# It answers the questions the CD-1B-1 creator prompt bets on:
#   1. does tools/skill_writer.py exist at the deterministic path
#   2. do --work / --persona take FILE PATHS (not literal text)
#   3. does --no-install-claude-skill really skip every host install
#   4. does create produce the five required artifacts, non-empty
#   5. does the supplied content survive verbatim into work.md / persona.md
#   6. does anything get written outside --base-dir
set -u

WRITER="${AI_OS_DISTILLY_WRITER:-$HOME/.openclaw/workspace-ai-os-files/skills/distilly/tools/skill_writer.py}"
SLUG="probe-distilly-contract"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/profiles"

fail() { echo "FAIL $*"; exit 1; }

[ -f "$WRITER" ] || fail "writer not found at $WRITER"
python3 "$WRITER" --help >/dev/null 2>&1 || fail "writer --help did not exit 0"

python3 "$WRITER" --help 2>&1 | grep -q -- '--work WORK' || fail "--work is not an accepted option"
python3 "$WRITER" --help 2>&1 | grep -qi 'Path to the work.md content file' \
  || fail "--work is not documented as a PATH; the creator prompt passes a path"
python3 "$WRITER" --help 2>&1 | grep -qi 'Path to the persona.md content file' \
  || fail "--persona is not documented as a PATH; the creator prompt passes a path"

WORK_MARKER="probe-work-marker-$$"
PERSONA_MARKER="probe-persona-marker-$$"
printf '# Work\n\n- %s\n' "$WORK_MARKER" > "$WORK/work-input.md"
printf '# Persona\n\n- %s\n' "$PERSONA_MARKER" > "$WORK/persona-input.md"

BEFORE="$(mktemp)"
find "$(dirname "$(dirname "$WRITER")")" -type f ! -name '*.pyc' -newermt '-1 second' > "$BEFORE" 2>/dev/null

OUT="$(python3 "$WRITER" \
  --action create \
  --character colleague \
  --slug "$SLUG" \
  --name Probe \
  --work "$WORK/work-input.md" \
  --persona "$WORK/persona-input.md" \
  --base-dir "$WORK/profiles" \
  --no-install-claude-skill 2>&1)"
RC=$?
[ "$RC" -eq 0 ] || { echo "$OUT"; fail "create exited $RC"; }

echo "$OUT" | grep -qi 'Host installs: skipped' \
  || fail "create did not report host installs skipped; it may have installed a Skill"

for NAME in SKILL.md manifest.json meta.json persona.md work.md; do
  FILE="$WORK/profiles/$SLUG/$NAME"
  [ -s "$FILE" ] || fail "required artifact missing or empty: $NAME"
done

grep -q "$WORK_MARKER" "$WORK/profiles/$SLUG/work.md" \
  || fail "work.md does not contain the supplied work content"
grep -q "$PERSONA_MARKER" "$WORK/profiles/$SLUG/persona.md" \
  || fail "persona.md does not contain the supplied persona content"

grep -rq "$WORK/work-input.md" "$WORK/profiles/$SLUG/work.md" 2>/dev/null \
  && fail "work.md contains the input PATH instead of the input CONTENT"

STRAY="$(find "$(dirname "$(dirname "$WRITER")")" -type f ! -path '*__pycache__*' -newer "$BEFORE" 2>/dev/null)"
[ -z "$STRAY" ] || fail "writer wrote outside --base-dir: $STRAY"

echo "PASS Distilly writer CLI contract"
echo "  writer          $WRITER"
echo "  --work/--persona take file paths, content copied verbatim"
echo "  host installs   skipped"
echo "  artifacts       SKILL.md manifest.json meta.json persona.md work.md (all non-empty)"
echo "  known benign    python bytecode caches under distilly/tools/__pycache__"

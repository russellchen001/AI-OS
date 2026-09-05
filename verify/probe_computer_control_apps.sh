#!/bin/bash
# Addressing applications without touching the GUI -- second pass.
#
# The first pass answered two questions and got two wrong, both because of
# mistakes in the probe rather than in the machine, and both on the questions
# that matter most. They are re-asked here properly.
#
# Settled already, and not repeated:
#   - installed applications carry a stable bundle identifier, readable with
#     `mdfind -attr kMDItemCFBundleIdentifier` (492 of 499 on this machine) and
#     agreeing with the bundle's own Info.plist
#   - the running/quit round trip is observable: 0 -> 1 -> 0
#   - `quit` REPORTS SUCCESS for an application that is not running, so the
#     return value proves nothing and the adapter must observe state instead
#
# Re-asked, because the first pass measured its own bugs:
#   1. what does `open -b` EXIT with for an identifier that does not exist --
#      the first pass read the exit status of `sed` at the end of a pipeline
#   2. what does System Events return for the running applications -- the first
#      pass used a C-style ternary, which is not AppleScript, and got a syntax
#      error that said nothing about permission
#
# And one that was never asked: does `open -b` for a REAL application report
# anything useful when the application is already running.
set -u

say() { printf '\n== %s\n' "$1"; }
ok()  { printf '   %s\n' "$1"; }

say "1. open -b with an identifier nothing is installed under"

# No pipeline. The exit status has to be this command's own.
output="$(/usr/bin/open -b com.ai-os.definitely.not.installed 2>&1)"
status=$?
ok "exit status: $status"
ok "said: ${output:-（nothing）}"

if [ "$status" -eq 0 ]; then
  ok "-> the exit status CANNOT be trusted; the adapter must check the identifier"
  ok "   exists before launching, and confirm the application afterwards"
else
  ok "-> the exit status is usable, and the message is on stderr"
fi

say "2. open -b for an application that is already running"

/usr/bin/open -b com.apple.calculator
sleep 2
output="$(/usr/bin/open -b com.apple.calculator 2>&1)"
status=$?
ok "second launch exit status: $status"
ok "said: ${output:-（nothing）}"
ok "-> a launch says nothing about whether it STARTED anything, so 'was already"
ok "   running' has to come from observing before and after"

say "3. System Events: the running applications, by bundle identifier"

/usr/bin/osascript <<'OSA' 2>&1 | sed 's/^/   /'
try
  tell application "System Events"
    set found to bundle identifier of every application process whose background only is false
  end tell
  set AppleScript's text item delimiters to ", "
  set shown to {}
  repeat with index from 1 to (count of found)
    if index > 3 then exit repeat
    set end of shown to item index of found
  end repeat
  return (count of found) & " running, first few: " & (shown as text)
on error message number code
  return "refused: " & message & " (" & code & ")"
end try
OSA

say "4. and the same list without System Events, in case Accessibility is absent"

# System Events needs Accessibility, which this machine has and another machine
# will not. If there is a path that does not, the adapter can degrade instead of
# refusing. `lsappinfo` was tried and returned localized display names; this
# asks whether it will give bundle identifiers when asked properly.
if command -v lsappinfo >/dev/null 2>&1; then
  ok "lsappinfo asns and their bundle ids:"
  lsappinfo list 2>/dev/null | grep -oE 'ASN:0x[0-9a-fx-]+:' | head -4 | while read -r asn; do
    id="$(lsappinfo info -only bundleid "${asn%:}" 2>/dev/null | sed 's/.*= *//' | tr -d '"')"
    printf '     %s -> %s\n' "${asn%:}" "${id:-（none）}"
  done
else
  ok "lsappinfo not present"
fi

say "5. tidy up"
/usr/bin/osascript -e 'tell application id "com.apple.calculator" to quit' >/dev/null 2>&1
sleep 1
ok "calculator running now: $(/usr/bin/osascript -e 'tell application "System Events" to return (count of (every application process whose bundle identifier is "com.apple.calculator"))' 2>/dev/null)"

say "done"

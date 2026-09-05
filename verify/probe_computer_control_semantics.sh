#!/bin/bash
# What can Computer Control actually do on this machine, deterministically?
#
# Computer Control is defined by its boundary: deterministic, system-level,
# directly callable. Not a second Agent, no planning, no driving the GUI --
# looking at the screen and clicking belongs to Computer Use.
#
# Every capability in the v1 list has to be reachable WITHOUT crossing that
# line, and several of them may not be. This asks the machine rather than
# assuming, before a single adapter is written, and it reports what each answer
# COST: a permission prompt, an approval, or nothing.
#
# It changes nothing. The volume is written back to the value it already had,
# which proves the write path without moving anything the person set. The only
# application it launches is Calculator, and it quits it again.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

say() { printf '\n== %s\n' "$1"; }
ok()  { printf '   %s\n' "$1"; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------- 1. sysinfo
say "1. sysinfo: is this system supported, and what does it report"

# Built in a temp crate so nothing lands in the repository. The question is not
# whether the crate compiles -- it is whether it reports REAL numbers here, and
# whether it admits when it cannot. On an unsupported system sysinfo returns
# empty values instead of failing, which is the exact silent lie the adapter
# will have to refuse.
CRATE="$WORK/sysprobe"
if command -v cargo >/dev/null 2>&1; then
  cargo new -q "$CRATE" 2>/dev/null
  ( cd "$CRATE" && cargo add sysinfo -q >/dev/null 2>&1 )
  cat > "$CRATE/src/main.rs" <<'RUST'
use sysinfo::{Disks, Networks, System};

fn main() {
    println!("IS_SUPPORTED_SYSTEM = {}", sysinfo::IS_SUPPORTED_SYSTEM);

    let mut system = System::new_all();
    // Usage is a difference between two samples, so one refresh is not enough.
    std::thread::sleep(std::time::Duration::from_millis(300));
    system.refresh_all();

    println!("cpus = {}", system.cpus().len());
    if let Some(cpu) = system.cpus().first() {
        println!("cpu brand = {:?}", cpu.brand().trim());
        println!("cpu usage sample = {:.1}%", cpu.cpu_usage());
    }
    println!("global cpu usage = {:.1}%", system.global_cpu_usage());

    println!("memory total = {} MiB", system.total_memory() / 1024 / 1024);
    println!("memory used  = {} MiB", system.used_memory() / 1024 / 1024);
    println!("memory avail = {} MiB", system.available_memory() / 1024 / 1024);
    println!("swap total   = {} MiB", system.total_swap() / 1024 / 1024);

    let disks = Disks::new_with_refreshed_list();
    println!("disks = {}", disks.len());
    for disk in disks.iter().take(4) {
        println!(
            "   {:?} at {:?}  {} GiB total, {} GiB free, fs {:?}, removable {}",
            disk.name(),
            disk.mount_point(),
            disk.total_space() / 1024 / 1024 / 1024,
            disk.available_space() / 1024 / 1024 / 1024,
            disk.file_system(),
            disk.is_removable()
        );
    }

    let networks = Networks::new_with_refreshed_list();
    println!("interfaces = {}", networks.len());
    for (name, data) in networks.iter().take(5) {
        println!(
            "   {name}  received {} MiB total, transmitted {} MiB total",
            data.total_received() / 1024 / 1024,
            data.total_transmitted() / 1024 / 1024
        );
    }

    println!("processes = {}", system.processes().len());
    let mut sample: Vec<_> = system.processes().values().collect();
    sample.sort_by_key(|process| std::cmp::Reverse(process.memory()));
    for process in sample.into_iter().take(3) {
        println!(
            "   pid {} {:?} {} MiB",
            process.pid(),
            process.name(),
            process.memory() / 1024 / 1024
        );
    }
}
RUST
  ( cd "$CRATE" && cargo run -q 2>&1 | sed 's/^/   /' ) || ok "sysinfo probe failed to build or run"
else
  ok "cargo not on PATH -- cannot probe sysinfo"
fi

# ----------------------------------------------------------- 2. applications
say "2. applications: list, launch and quit WITHOUT touching the GUI"

ok "installed, by Spotlight index (fast, no permission):"
mdfind "kMDItemContentType == 'com.apple.application-bundle'" 2>/dev/null \
  | head -5 | sed 's/^/     /'
ok "   total found: $(mdfind "kMDItemContentType == 'com.apple.application-bundle'" 2>/dev/null | wc -l | tr -d ' ')"

ok "running, by lsappinfo (undocumented but present):"
if command -v lsappinfo >/dev/null 2>&1; then
  lsappinfo list 2>/dev/null | grep -E '^\s*[0-9]+\) ' | head -5 | sed 's/^/     /'
else
  ok "     lsappinfo not present"
fi

ok "running, via System Events (may ask for Automation permission):"
/usr/bin/osascript <<'OSA' 2>&1 | sed 's/^/     /'
try
  tell application "System Events"
    set ids to bundle identifier of every application process whose background only is false
  end tell
  return "ok: " & (count of ids) & " foreground apps"
on error message number code
  return "refused: " & message & " (" & code & ")"
end try
OSA

ok "launch and quit Calculator by bundle id:"
/usr/bin/open -b com.apple.calculator 2>&1 | sed 's/^/     /'
sleep 2
/usr/bin/osascript <<'OSA' 2>&1 | sed 's/^/     /'
try
  tell application id "com.apple.calculator" to quit
  return "quit accepted"
on error message number code
  return "quit refused: " & message & " (" & code & ")"
end try
OSA

# -------------------------------------------------------------- 3. clipboard
say "3. clipboard: what is on it, and of what types"

ok "types currently on the clipboard (NOT its contents):"
/usr/bin/osascript -e 'clipboard info' 2>&1 | sed 's/^/     /'
ok "plain-text length only, so nothing secret is printed:"
ok "     $(/usr/bin/pbpaste 2>/dev/null | wc -c | tr -d ' ') bytes"

# ------------------------------------------------------------------ 4. audio
say "4. audio: read the volume, and write back the SAME value"

/usr/bin/osascript <<'OSA' 2>&1 | sed 's/^/   /'
try
  set settings to (get volume settings)
  set current to output volume of settings
  set muted to output muted of settings
  -- Written back unchanged: proves the write path without moving anything.
  set volume output volume current
  return "output volume " & current & ", muted " & muted & ", write accepted"
on error message number code
  return "refused: " & message & " (" & code & ")"
end try
OSA

ok "output devices:"
/usr/sbin/system_profiler SPAudioDataType 2>/dev/null \
  | grep -E "^ {8}[A-Za-z].*:$" | head -6 | sed 's/^/  /'

# ------------------------------------------------------------------ 5. power
say "5. power: battery, source, and preventing sleep"

/usr/bin/pmset -g batt 2>&1 | sed 's/^/   /'
ok "sleep prevention tool: $(command -v caffeinate >/dev/null 2>&1 && echo 'caffeinate present' || echo 'caffeinate MISSING')"
ok "currently asserting sleep prevention:"
/usr/bin/pmset -g assertions 2>/dev/null | grep -E "PreventUserIdleSystemSleep|PreventUserIdleDisplaySleep" | head -2 | sed 's/^/     /'

# ---------------------------------------------------------- 6. notifications
say "6. notifications: can one be posted, and who does the system think sent it"

/usr/bin/osascript -e 'display notification "AI-OS Computer Control probe" with title "AI-OS"' 2>&1 | sed 's/^/   /'
ok "(if a banner appeared, note WHICH app name it showed -- that is the identity"
ok " a notification from this path will carry, and it is probably not AI-OS)"

# ------------------------------------------------------------ 7. permissions
say "7. permissions: which TCC states can be READ, not granted"

ok "Accessibility (System Events UI access):"
/usr/bin/osascript <<'OSA' 2>&1 | sed 's/^/     /'
try
  tell application "System Events" to return "enabled: " & (UI elements enabled)
on error message number code
  return "refused: " & message & " (" & code & ")"
end try
OSA

ok "Screen recording, microphone and camera, via the ObjC bridge:"
/usr/bin/osascript -l JavaScript <<'OSA' 2>&1 | sed 's/^/     /'
ObjC.import('AVFoundation');
function run() {
  const out = [];
  try {
    ObjC.import('CoreGraphics');
    out.push('screen preflight: ' + $.CGPreflightScreenCaptureAccess());
  } catch (error) {
    out.push('screen preflight unavailable: ' + error.message);
  }
  for (const [label, media] of [['microphone', 'soun'], ['camera', 'vide']]) {
    try {
      const status = $.AVCaptureDevice.authorizationStatusForMediaType(media);
      out.push(label + ' authorization status: ' + status +
               ' (0 notDetermined, 1 restricted, 2 denied, 3 authorized)');
    } catch (error) {
      out.push(label + ' unavailable: ' + error.message);
    }
  }
  return out.join('\n');
}
OSA

ok "Full Disk Access, inferred by reading a protected path:"
if /bin/ls "$HOME/Library/Safari" >/dev/null 2>&1; then
  ok "     readable -- Full Disk Access appears granted to the shell running this"
else
  ok "     not readable -- Full Disk Access appears NOT granted to this shell"
fi

ok "can the right settings pane be opened deterministically (not opened now):"
ok "     x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"

say "done -- nothing was changed; Calculator was launched and quit"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SessionLog.cpp app/framework/SessionLog.hpp app/framework/HangWatchdog.cpp
# check-all sources: app/framework/HangWatchdog.hpp app/mcp/LogRing.cpp app/report/ReportCenter.cpp app/main.cpp
# check-all sources: app/framework/TestDriver.cpp app/MainWindowDebugLog.cpp
# check-all sources: tools/ui-scripts/session-log.mitcad-ui tools/ui-scripts/session-log-ended.mitcad-ui
# check-all sources: tools/ui-scripts/session-log-shutdown.mitcad-ui app/framework/ShutdownSteps.cpp
# check-all sources: app/framework/ShutdownSteps.hpp app/WindowManager.cpp
# The session log, the hang marker and the notice of an unexpected end
# (mitcad#1896) through the real UI, the shared input scripts
# tools/ui-scripts/session-log.mitcad-ui and session-log-ended.mitcad-ui
# (also run by the Windows and macOS wrappers):
#   1. The first run (MITCAD_HANG_WATCHDOG_MS=1000): its session log holds
#      the start-up, a few commands' lines and, after a test hang of 3 s,
#      "UI not responding for 1 s" and "UI responding again after 3.x s";
#      it is killed, so its file has no clean end and its lock stays.
#   2. The second run: the start finds the first run ended unexpectedly, the
#      notice's Open Log Folder opens the folder; its clean end writes the
#      file's last line and removes its lock. Its teardown's steps
#      (mitcad#2275) are in the file, the watchdog off, no step's warning.
#   3. The third run (session-log-shutdown.mitcad-ui, the watchdog at
#      1000 ms, the shutdown step "display mesh writes" held for 3 s by
#      MITCAD_TEST_SHUTDOWN_DELAY): the step's line and the watchdog's
#      warning naming it are in the file, and the run still ends cleanly.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-session-log-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-session-log.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
logs=$work/logs
export MITCAD_DEBUG_LOG_DIR=$logs MITCAD_TEST_LOG_URLS=1 MITCAD_TEST_STEP_MS=100
unset MITCAD_SESSION_LOG MITCAD_DEBUG_LOG

# run_script script description: MitCad with the script until it ends.
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1
  ui_start_app --ui=qt --no-mcp
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$2: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$2: the driver failed"; exit 1; }
  UI_RUNNER=""
  grep -q 'TestDriver: done\|TestDriver: kill\|TestDriver: quit (' "$UI_LOG" || { cat "$UI_LOG" >&2; ui_fail "$2: not to its end"; exit 1; }
  echo "ok   $2"
}

# file_has "regex" description file
file_has() {
  grep -qE -- "$1" "$3" || { ui_fail "$2: no line matching '$1' in $3"; exit 1; }
  echo "ok   $2"
}

ui_start_display

echo "--- The first run: its log, a few commands, a hang of 3 s, killed"
export MITCAD_HANG_WATCHDOG_MS=1000
unset MITCAD_UNEXPECTED_END_NOTICE
run_script session-log.mitcad-ui "the first run"
first=$(ls "$logs"/session-*.log 2> /dev/null | head -1)
[ -n "$first" ] || ui_fail "no session log in $logs"
[ "$(ls "$logs"/session-*.log | wc -l)" = 1 ] || ui_fail "more than one session log: $(ls "$logs")"
[ "$(head -1 "$first")" = "MitCad session log" ] || ui_fail "the file does not begin with its header"
echo "ok   one session log, with its header: $(basename "$first")"
file_has 'info +Open Log Folder: ' "Open Log Folder's line" "$first"
file_has 'Create Sketch: select a plane' "Create Sketch's line" "$first"
file_has 'warning +\[hang watchdog\] UI not responding for [0-9]+ s \(last action: test step freeze-ui 3000 ms' \
  "the hang's line, from the watchdog's thread" "$first"
file_has 'info +UI responding again after [0-9]+\.[0-9] s$' "how long it lasted" "$first"
grep -q '^--- The session ended' "$first" && ui_fail "a killed session has a clean end"
lock=${first%.log}.lock
[ -f "$lock" ] || ui_fail "the killed session's lock is gone: $lock"
echo "ok   no clean end; its lock left"

echo "--- The second run: the notice of the unexpected end, a clean end"
unset MITCAD_HANG_WATCHDOG_MS
export MITCAD_UNEXPECTED_END_NOTICE=1
run_script session-log-ended.mitcad-ui "the second run"
grep -qF "Previous session ended unexpectedly: $(basename "${first%.log}")" "$UI_LOG" ||
  ui_fail "the first run is not named as ended unexpectedly"
echo "ok   the first run named as ended unexpectedly"
second=$(ls "$logs"/session-*.log | grep -vF "$first" | head -1)
[ -n "$second" ] || ui_fail "no second session log"
tail -1 "$second" | grep -q '^--- The session ended: ' || ui_fail "the second run's last line: $(tail -1 "$second")"
echo "ok   the second run's clean end: $(tail -1 "$second")"
ls "$logs"/session-*.lock > /dev/null 2>&1 && ui_fail "locks left: $(ls "$logs"/session-*.lock)"
echo "ok   no lock left"
file_has 'info +Shutdown begins: ' "the teardown's first line" "$second"
file_has 'info +Shutdown: the event loop ended$' "the end of the event loop" "$second"
file_has 'info +Shutdown: display mesh writes done \([0-9]+ ms\)$' "the display mesh writes' step" "$second"
file_has 'info +Shutdown: window manager: window-[0-9]+ destroyed done \([0-9]+ ms\)$' "a window's destruction" \
  "$second"
file_has 'info +Shutdown: document session: the job cancelled, the model worker stops$' \
  "the document session's step" "$second"
file_has 'info +Shutdown: the session log ends$' "the last step before the lock goes" "$second"
grep -q 'Shutdown step not finished' "$second" && ui_fail "a step's warning with the watchdog off"
echo "ok   no step's warning with the watchdog off"

echo "--- The third run: a shutdown step held for 3 s, the watchdog's warning naming it"
export MITCAD_HANG_WATCHDOG_MS=1000 'MITCAD_TEST_SHUTDOWN_DELAY=3000:display mesh writes'
unset MITCAD_UNEXPECTED_END_NOTICE
run_script session-log-shutdown.mitcad-ui "the third run"
unset MITCAD_HANG_WATCHDOG_MS MITCAD_TEST_SHUTDOWN_DELAY
third=$(ls "$logs"/session-*.log | grep -vF -e "$first" -e "$second" | head -1)
[ -n "$third" ] || ui_fail "no third session log"
file_has 'info +Shutdown: display mesh writes$' "the held step's line" "$third"
file_has 'info +Shutdown: test delay of 3000 ms in display mesh writes$' "the test's delay" "$third"
file_has 'warning +\[hang watchdog\] Shutdown step not finished after [0-9]+\.[0-9] s: display mesh writes$' \
  "the watchdog's warning naming the step" "$third"
file_has 'info +Shutdown: display mesh writes done \([0-9]{4,} ms\)$' "the step's end, after the delay" "$third"
file_has 'info +Shutdown step finished after [0-9]+\.[0-9] s: display mesh writes$' "the watchdog saw it finish" \
  "$third"
tail -1 "$third" | grep -q '^--- The session ended: ' || ui_fail "the third run's last line: $(tail -1 "$third")"
echo "ok   the third run's clean end: $(tail -1 "$third")"
ls "$logs"/session-*.lock > /dev/null 2>&1 && ui_fail "locks left: $(ls "$logs"/session-*.lock)"
echo "ok   no lock left"

ui_finish 'UI session log, hang marker, unexpected end and shutdown steps'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SessionLog.cpp app/framework/HangWatchdog.cpp app/report/ReportCenter.cpp
# check-all sources: tools/ui-scripts/session-log.mitcad-ui tools/ui-scripts/session-log-ended.mitcad-ui
# The session log, the hang marker and the notice of an unexpected end
# (mitcad#1896) on macOS, in the Qt frontend, the same scripts as the Linux
# and Windows tests': tools/ui-scripts/session-log.mitcad-ui (a few
# commands, a test hang of 3 s with MITCAD_HANG_WATCHDOG_MS=1000, killed)
# and session-log-ended.mitcad-ui (the notice, Open Log Folder, a clean
# end), then the files of the test's own log folder.
# Usage: tools/ui-macos-session-log-test.sh <app> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-session-log}"
UI_TIMEOUT=180
logs=$UI_OUT/session-logs
rm -rf "$logs"
export MITCAD_DEBUG_LOG_DIR=$logs MITCAD_TEST_LOG_URLS=1 MITCAD_TEST_STEP_MS=100
unset MITCAD_SESSION_LOG MITCAD_DEBUG_LOG MITCAD_UNEXPECTED_END_NOTICE

echo "--- The first run: its log, a few commands, a hang of 3 s, killed"
MITCAD_HANG_WATCHDOG_MS=1000 MITCAD_TEST_INPUT=$here/ui-scripts/session-log.mitcad-ui \
  ui_run_app session-log --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log 'TestDriver: kill' 'the first run to its end'
ui_expect_no_log 'TestDriver: FAIL' 'the first run: no failed driver assertion'
first=$(ls "$logs"/session-*.log 2> /dev/null | head -1)
if [ -z "$first" ] || [ "$(ls "$logs"/session-*.log | wc -l | tr -d ' ')" != 1 ]; then
  ui_fail "not one session log in $logs"
else
  [ "$(head -1 "$first")" = "MitCad session log" ] && ui_pass "one session log, with its header" ||
    ui_fail "the file does not begin with its header"
  grep -qE 'warning +\[hang watchdog\] UI not responding for [0-9]+ s \(last action: test step freeze-ui 3000 ms' \
    "$first" && ui_pass "the hang's line, from the watchdog's thread" || ui_fail "no hang line in $first"
  grep -qE 'info +UI responding again after [0-9]+\.[0-9] s$' "$first" && ui_pass "how long it lasted" ||
    ui_fail "no 'responding again' line in $first"
  grep -q '^--- The session ended' "$first" && ui_fail "a killed session has a clean end"
  [ -f "${first%.log}.lock" ] && ui_pass "no clean end; its lock left" || ui_fail "the killed session's lock is gone"
fi

echo "--- The second run: the notice of the unexpected end, a clean end"
MITCAD_UNEXPECTED_END_NOTICE=1 MITCAD_TEST_INPUT=$here/ui-scripts/session-log-ended.mitcad-ui \
  ui_run_app session-log-ended --ui=qt --no-native-dialogs --no-recovery --no-mcp
# The shared second script ends with quit, which logs its completed step count.
ui_expect_log '^TestDriver: quit [(]14 of 14 steps[)]$' 'the second run through all steps and its final quit'
ui_expect_no_log 'TestDriver: FAIL' 'the second run: no failed driver assertion'
if [ -n "$first" ]; then
  ui_expect_log "^Previous session ended unexpectedly: $(basename "${first%.log}")\$" \
    'the first run named as ended unexpectedly'
  second=$(ls "$logs"/session-*.log | grep -vF "$first" | head -1)
  if [ -n "$second" ] && tail -1 "$second" | grep -q '^--- The session ended: '; then
    ui_pass "the second run's clean end"
  else
    ui_fail "the second run's file has no clean end"
  fi
fi
ls "$logs"/session-*.lock > /dev/null 2>&1 && ui_fail "locks left: $(ls "$logs"/session-*.lock)"
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt session log, hang marker and unexpected end'

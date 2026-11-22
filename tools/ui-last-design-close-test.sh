#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/WindowManager.cpp app/WindowManager.hpp app/DocumentFrame.cpp app/DocumentFrame.hpp
# check-all sources: app/MainWindowStart.cpp app/files/MainWindowFiles.cpp app/framework/AppSettings.cpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriverDocuments.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/last-design-close.mitcad-ui tools/ui-scripts/last-design-close-restart.mitcad-ui
# check-all sources: tools/ui-scripts/last-design-close-tab.mitcad-ui tools/ui-scripts/panel-message-top.json tools/cli
# Closing the last design (mitcad#2142) through the real UI, the shared
# input scripts tools/ui-scripts/last-design-close*.mitcad-ui (also run by
# tools/ui-windows-last-design-close-test.ps1 and, all but the tab's ×,
# tools/ui-macos-last-design-close-test.sh) on a cube that mitcad-cli makes:
#   1. last-design-close (Floating): with Preferences › General › Quit
#      MitCad when the last design is closed off (the default), File › Close
#      of the only design shows the Start page with no design open and the
#      process runs on; Open and New Design start from there; the
#      preference is turned on in Preferences.
#   2. last-design-close-restart (Floating): after a restart the preference
#      is still on, and File › Close of the last design quits: the run ends
#      with "Document windows closed: empty process registry" and the
#      session log's end line, before the driver's end.
#   3. last-design-close-tab, in the Docked and the Floating layout: the
#      only design's tab × shows the Start page with the preference off, and
#      quits with it on.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-last-design-close-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-last-design-close.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
"$cli" run "$here/ui-scripts/panel-message-top.json" --save "$work/part.mitcad" || exit 1
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
unset MITCAD_SESSION_LOG MITCAD_DEBUG_LOG
ui_start_display

# run_script <script> <chrome> <ends>: one of the shared scripts, the app
# started on the design in that layout, with the settings the runs share
# and a session log folder of its own. <ends> "done": the driver runs to
# its end; "quit": MitCad quits before it (the last design closed).
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  export MITCAD_DEBUG_LOG_DIR=$work/logs-$1-$2
  ui_start_app --ui=qt --no-mcp --chrome="$2" --open "$work/part.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the driver timed out ($1, $2)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "MitCad failed ($1, $2)"; exit 1; }
  UI_RUNNER=""
  grep -q '^TestDriver: FAIL' "$UI_LOG" && { cat "$UI_LOG" >&2; ui_fail "a failed driver step ($1, $2)"; exit 1; }
  if [ "$3" = done ]; then
    ui_expect_log 'TestDriver: done' "the shared steps ran to their end ($1, $2)"
    ui_expect_log 'Last design closed: Start page' "the last design closed to the Start page ($1, $2)"
    grep -q '^Document windows closed: empty process registry$' "$UI_LOG" &&
      ui_fail "MitCad quit after the last design closed ($1, $2)"
    echo "ok   MitCad ran on with no design open ($1, $2)"
  else
    grep -q 'TestDriver: done' "$UI_LOG" && ui_fail "the driver reached its end: MitCad did not quit ($1, $2)"
    ui_expect_log 'Last design closed: quitting' "the last design closed with the preference on ($1, $2)"
    ui_expect_log 'Document windows closed: empty process registry' "MitCad quit ($1, $2)"
    session=$(ls "$MITCAD_DEBUG_LOG_DIR"/session-*.log 2> /dev/null | head -1)
    [ -n "$session" ] || ui_fail "no session log in $MITCAD_DEBUG_LOG_DIR ($1, $2)"
    [ -n "$session" ] && { tail -1 "$session" | grep -q '^--- The session ended: ' ||
      ui_fail "the session log's last line: $(tail -1 "$session") ($1, $2)"; }
    echo "ok   the session ended cleanly ($1, $2)"
  fi
}

run_script last-design-close floating done
ui_expect_log 'Preferences: quit when the last design is closed on' 'the preference turned on in Preferences'
run_script last-design-close-restart floating quit
run_script last-design-close-tab docked quit
ui_expect_log 'Document tab closed: part' "the tab's × closed the design (Docked)"
run_script last-design-close-tab floating quit
ui_expect_log 'Document tab closed: part' "the tab's × closed the design (Floating)"
ui_finish 'UI closing the last design'

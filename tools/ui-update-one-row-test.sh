#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/update app/MainWindow.cpp app/WindowManager.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/update-one-row.mitcad-ui
# check-all sources: tools/ui-scripts/panel-message-top.json tools/cli
# One update for the whole application (mitcad#1890) through the real UI,
# the shared input script tools/ui-scripts/update-one-row.mitcad-ui (also
# run by tools/ui-windows-update-one-row-test.ps1 and
# tools/ui-macos-update-one-row-test.sh) with the update tests' fake offer
# (the driver's step update-fake-offer: no server, no request), on two
# cubes that mitcad-cli makes: Install accepted in the first design's window, a
# second design opened shows the download in its row at once, the first
# design closed leaves the row in the second, where the download goes on
# and reaches "downloaded"; then Exit during a download asks, and Keep
# Downloading keeps MitCad open until it is downloaded. (The update was
# the window's: closing the design that accepted it closed its row and
# dropped the download.)
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-update-one-row-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-update-one-row.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
for name in first second; do
  "$cli" run "$here/ui-scripts/panel-message-top.json" --save "$work/$name.mitcad" || exit 1
done
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/update-one-row.mitcad-ui
# MITCAD_NO_UPDATE_CHECK stays set (ui-test-lib.sh): no automatic check and
# no request; the fake offer answers Check for Updates all the same.
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/first.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the update driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "the update driver failed"; exit 1; }
ui_expect_log 'TestDriver: done' "the shared steps ran to their end"
ui_expect_log "Update row: in 2 window(s)" "the second design's window shows the update's row"
# After the first window closed: the download went on in the second.
after=$(sed -n '/^Update row: in 2 window(s)$/,$p' "$UI_LOG" | sed -n '/^Update row: in 1 window(s)$/,$p')
grep -qE '^Update download: [0-9]+%$' <<< "$after" ||
  ui_fail "no progress of the download after the first window closed"
grep -qF 'Update notice (message): MitCad 999.0.0 is downloaded.' <<< "$after" ||
  ui_fail "the second window's row did not reach downloaded"
echo "ok   the download went on in the second window and reached downloaded"
grep -qE '^Update: cancelled' "$UI_LOG" && ui_fail "the download was cancelled"
ui_expect_log "Update question: MitCad 999.0.0 is still downloading" "Exit during the download asks"
ui_expect_log "Update: the download goes on, MitCad stays open" "Keep Downloading keeps MitCad open"
ui_finish "UI one update for the application"

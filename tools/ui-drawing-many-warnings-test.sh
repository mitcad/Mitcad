#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/CommandDockWidth.cpp app/framework/HangWatchdog.cpp tools/cli
# check-all sources: app/framework/TestDriver.cpp app/browser/BrowserController.cpp
# check-all sources: tools/ui-scripts/drawing-many-warnings.mitcad-ui tools/ui-scripts/drawing-many-warnings.json
# A base view with many warnings (mitcad#2476, mitcad#2507; GitHub
# mitcad/Mitcad#694: a view of a large design kept 141 bodies that were
# gone, one warning each, and the window stopped answering once the view
# was selected) through the real UI, Docked, the shared input script
# tools/ui-scripts/drawing-many-warnings.mitcad-ui (also run by
# tools/ui-windows-drawing-many-warnings-test.ps1 and
# tools/ui-macos-drawing-many-warnings-test.sh) on the design that
# mitcad-cli makes (tools/ui-scripts/drawing-many-warnings.json: Plate, a
# 40 x 30 x 5 body, placed 60 times with a drawing whose base view at 1:20
# draws them all, 49 of the placements then deleted):
#   - the view selected: its panel says the warning once, with how many
#     times it came; its fields are in their scroll area and the drawing's
#     area keeps its size (the window its own); its Shows list, folded
#     under its heading (mitcad#2498), opened;
#   - Close Drawing: the 3D view again; the drawing opened again keeps its
#     view at 1:20 with its warning, and its Shows list open;
#   - no line of the hang watchdog (on at 15 s) all along.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-many-warnings-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-many-warnings.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-many-warnings.mitcad-ui
# The hang watchdog on, with room for the slow steps of a software
# renderer: the window that stopped answering did so for minutes.
export MITCAD_HANG_WATCHDOG_MS=15000
"$cli" run "$here/ui-scripts/drawing-many-warnings.json" --save "$work/plates.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli"; }
echo "ok   a design of 60 plates, 49 deleted, with a drawing made"
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=docked --no-native-dialogs --open "$work/plates.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'many warnings driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
if grep -F '[hang watchdog]' "$UI_LOG"; then
  ui_fail "the window stopped answering"
fi
echo "ok   no line of the hang watchdog"
ui_expect_log 'Drawing view 1 panel warnings (49 of them): an occurrence is gone (49 times)' \
  'the warning said once, with how many times it came'
ui_expect_log 'Drawing view 1 Shows opened' 'its Shows list, folded under its heading, opened'
ui_expect_log 'Drawing closed' 'Close Drawing showed the 3D view again'
ui_expect_log 'TestDriver: done' 'the drawing opened again, its view selected, its Shows list open, the window answering'
ui_finish "UI drawing view with many warnings test"

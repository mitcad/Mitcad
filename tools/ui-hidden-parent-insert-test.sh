#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: app/MainWindowVisibility.cpp app/commands/PrimitiveCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/hidden_parents.rs core/model-api/src/hidden_parents.rs tools/cli
# check-all sources: tools/ui-scripts/hidden-parent-insert.mitcad-ui tools/ui-scripts/hidden-parent-insert.json
# check-all sources: tools/ui-scripts/hidden-parent-insert-part.json
# Inserting into a component under a hidden parent (mitcad#1995, GitHub
# mitcad/Mitcad#430) through the real UI, the shared input script
# tools/ui-scripts/hidden-parent-insert.mitcad-ui (also run by
# tools/ui-windows-hidden-parent-insert-test.ps1 and
# tools/ui-macos-hidden-parent-insert-test.sh). mitcad-cli writes the design
# to insert (a box) and the design the app opens: Fairlead:1 with a body
# and Post:1, empty, in it. Fairlead:1 hidden, the rows in it have the
# dimmed closed eye; with Post:1 active, an insert and New Component show
# Fairlead:1 in their undo step and say so in the status bar; a click on
# Post:1's dimmed eye shows Fairlead:1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hidden-parent-insert-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-hidden-parent-insert.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
"$cli" run "$here/ui-scripts/hidden-parent-insert-part.json" --save "$work/part.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design to insert"; exit 1; }
"$cli" run "$here/ui-scripts/hidden-parent-insert.json" --save "$work/fairlead.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design"; exit 1; }
# The script with the inserted design's path.
sed "s|@PART@|$work/part.mitcad|" "$here/ui-scripts/hidden-parent-insert.mitcad-ui" > "$work/hidden-parent-insert.mitcad-ui"
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$work/hidden-parent-insert.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/fairlead.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "hidden parent insert driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Browser eye Root/Fairlead:1/Post:1: hidden with Fairlead:1' "Post:1's eye is hidden with Fairlead:1"
ui_expect_log 'Hidden parents shown: Fairlead:1 (by insert_component)' 'the insert shows Fairlead:1'
ui_expect_log 'Fairlead:1 was hidden; shown so the new component can be seen.' 'the status bar says so'
ui_expect_log 'Hidden parents shown: Fairlead:1 (by create_component)' 'New Component shows Fairlead:1'
ui_expect_log 'Visibility Root/Fairlead:1/Post:1: shown with its parents (O1)' "Post:1's dimmed eye shows Fairlead:1"
ui_expect_log 'TestDriver: done' 'inserting under a hidden parent'
ui_finish 'UI inserting under a hidden parent'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/SelectionLook.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/selection-look.mitcad-ui tools/ui-scripts/selection-look.json
# The selection's look on macOS (mitcad#1803, mitcad#2063): the shared
# input script of tools/ui-selection-look-test.sh at the screen's own
# device pixel ratio, through the real UI. In the view's core profile
# OCCT drew every line 1 device pixel wide whatever its width; the
# script's view-line-width steps read the view's pixels back: the
# selected top edge at least 3 logical pixels wide at Normal and wider at
# Thick, the sketch's plain line at least 2 logical pixels (less the
# driver's half pixel). The view's frame after each measurement is kept
# in <output>/files/line-<step>.png (the pictures for the ticket).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo 'usage: ui-macos-selection-look-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/selection-look.json" --save "$MITCAD_TEST_WORK/selection-look.mitcad" || exit 1
# The shared script, the view's frame saved after each measured line. The
# color dialog's HTML field is focused by its name: macOS has no Alt
# mnemonics, so the label's Alt+H reaches no field there.
script=$UI_OUT/selection-look.mitcad-ui
awk '/^key alt\+h$/ { print "focus-widget qt_colorname_lineedit"; next }
  { print }
  /^view-line-width / { printf "capture-view-image '\''${MITCAD_TEST_WORK}/line-%s.png'\''\n", $2 }' \
  "$here/ui-scripts/selection-look.mitcad-ui" > "$script" || exit 1
# Preferences is the Settings window here (no OK): Cmd+W closes it, as in
# ui-macos-preferences-search-test.sh. --no-native-dialogs: the Qt color
# dialog with its HTML field, which the script types into (the system's
# color panel has none).
export MITCAD_TEST_PREFERENCES_DONE=ctrl+w
MITCAD_TEST_INPUT=$script MITCAD_TEST_STEP_MS=100 \
  ui_run_app selection-look --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/selection-look.mitcad"
ui_expect_log '^View selection look: color #1f7aff, line width normal; ' 'Normal and the selection blue by default'
ui_expect_log '^Preferences: selection color #d02090, line width thick' 'Selected line width saved'
for step in normal thick sketch; do
  [ -s "$MITCAD_TEST_WORK/line-$step.png" ] || ui_fail "no picture of the view at the $step line"
done
# "TestDriver: view line width <px> px at <x>,<y> in <#rrggbb>, ratio <r>...":
# the edge at Normal, at Thick, the sketch's line.
widths=$(sed -n 's/^TestDriver: view line width \([0-9.]*\) px .*/\1/p' "$UI_LOG_FILE" | tr '\n' ' ')
echo "line widths in device pixels (Normal edge, Thick edge, sketch line): $widths"
[ "$(wc -w <<< "$widths")" -eq 3 ] || ui_fail "three measured lines wanted, got: $widths"
ui_expect_log '^TestDriver: done' 'the edge and the sketch line drawn wide in the view'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: selection look and line widths on macOS'

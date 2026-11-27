#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeDialogs.mm app/platform/NativeDialogs.hpp
# check-all sources: app/browser/ConfigurationsDialog.cpp app/browser/ConfigurationsData.cpp app/browser/AppearanceDialog.cpp
# check-all sources: app/framework/TestDriver.cpp app/platform/macos/NativeTestInput.mm
# check-all sources: tools/ui-scripts/native-macos-rows-appearances.mitcad-ui tools/ui-scripts/native-rows-appearances.json tools/cli
# The AppKit frontend's bridged tables (mitcad#1355), as the Qt frontend
# has them (tools/ui-configuration-rows-test.sh, mitcad#1044; the
# Appearances headings of tools/ui-scripts/filament-appearance.mitcad-ui,
# mitcad#1133), on a design mitcad-cli makes
# (tools/ui-scripts/native-rows-appearances.json: a closed table, rows M3
# to M6): the Configurations window's Rows sheet selects a 2 x 2 block of
# cells (a press, Shift+Down, Shift+Right), copies it (Command+C), pastes
# it two rows down (Command+V) and empties a cell (Delete); the Appearances
# window's Metal heading folds and unfolds by a press and, reached with Up,
# by Space, Return, Left and Right
# (tools/ui-scripts/native-macos-rows-appearances.mitcad-ui). Presses are
# the in-process native pointer and keys AppKit key events of this
# process, no system input; the clipboard is the application's own.
# Usage: tools/ui-macos-native-rows-appearances-test.sh <app> <mitcad-cli> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/native-rows-appearances.json" --save "$MITCAD_TEST_WORK/rows-appearances.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-rows-appearances.mitcad-ui \
  ui_run_app native-rows-appearances --ui=native --no-recovery --no-mcp \
  --open "$MITCAD_TEST_WORK/rows-appearances.mitcad"
ui_expect_log '^Configurations copied 2 x 2 cells$' 'the Rows sheet copies a 2 x 2 block'
ui_expect_log '^Configurations pasted 2 x 2 cells at M5 d: added none$' 'and pastes it two rows down'
ui_expect_log '^Configurations cleared 1 cells$' 'Delete empties a cell'
ui_expect_log '^Appearances group Metal collapsed$' 'a heading folds its group'
ui_expect_log '^Appearances group Metal expanded$' 'and unfolds it'
ui_expect_log '^TestDriver: done' 'the Rows sheet and the Appearances headings work in the AppKit frontend'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: AppKit Rows sheet cells and Appearances headings'

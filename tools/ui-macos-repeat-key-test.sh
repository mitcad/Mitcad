#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/framework/EnterRepeat.cpp
# check-all sources: app/framework/EnterRepeat.hpp app/framework/ShortcutDialog.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/repeat-key.mitcad-ui tools/ui-scripts/repeat-key.json
# The command Repeat's own key (mitcad#2052) in the Qt frontend on macOS,
# as tools/ui-repeat-key-test.sh on Linux: the shared steps
# tools/ui-scripts/repeat-key.mitcad-ui on a design that mitcad-cli makes
# (tools/ui-scripts/repeat-key.json): Space given to Repeat in Keyboard
# Shortcuts repeats Extrude in the 3D view, and nowhere the focus or the
# mode keeps it.
# Usage: tools/ui-macos-repeat-key-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-repeat-key-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/repeat.mitcad
"$cli" run "$here/ui-scripts/repeat-key.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run repeat-key.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/repeat-key.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app repeat-key --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Keyboard Shortcuts key: Repeat \| 3D View = Space$' 'Space given to Repeat in Keyboard Shortcuts'
ui_expect_log '^Repeat Extrude \(Space\)$' 'Space in the 3D view repeats Extrude'
ui_expect_log '^TestDriver: done' 'the view repeats by Space; panels, sketches and the browser keep it'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Repeat key assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Repeat key'

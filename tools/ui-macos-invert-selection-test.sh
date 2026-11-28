#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowInvertSelection.cpp app/OcctViewer.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindow.cpp app/resources/layout/default.json app/framework/ShortcutDialog.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/invert-selection.mitcad-ui tools/ui-scripts/invert-selection.json
# Invert Selection (mitcad#2647) in the Qt frontend on macOS: the shared
# steps tools/ui-scripts/invert-selection.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/invert-selection.json): F7 bound in Keyboard
# Shortcuts; bodies inverted (the hidden one not taken), the faces of an
# isolated body, a sketch's lines (by the key and the view's context menu),
# and Interference's Bodies input. As tools/ui-invert-selection-test.sh
# (Linux) and tools/ui-windows-invert-selection-test.ps1 (Windows).
# Usage: bash tools/ui-macos-invert-selection-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-invert-selection}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/invert-selection.json" --save "$MITCAD_TEST_WORK/invert-selection.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/invert-selection.mitcad-ui \
  ui_run_app invert-selection --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/invert-selection.mitcad"
ui_expect_log 'Invert Selection: 1 face to 5 faces' 'the faces of the isolated body inverted'
ui_expect_log 'Invert Selection in Interference Bodies: nothing to 2 bodies' 'the input took the shown bodies'
ui_expect_log 'TestDriver: done' 'bodies, faces, sketch curves and a command input inverted'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: invert selection; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: invert selection; evidence in $UI_OUT"

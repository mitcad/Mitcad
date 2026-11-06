#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeTestInput.mm
# check-all sources: app/view/ViewController.cpp app/MainWindowNative.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/native-macos-turn-keys.mitcad-ui tools/cli
# The arrow keys in the AppKit frontend (mitcad#1351), on the design of the
# Qt frontend's test (tools/ui-view-arrow-keys-test.sh, mitcad#1211): the
# turn commands' keys are the 3D view's only, so Down in the focused
# Browser outline selects the next row and Left in the focused timeline
# the feature before, the camera unchanged, while Left with the 3D view
# focused still turns it (tools/ui-scripts/native-macos-turn-keys.mitcad-ui).
# The keys go through the document window's own dispatch, no system input.
# Usage: tools/ui-macos-native-turn-keys-test.sh <app> <mitcad-cli> <output directory>
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
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
cat > "$MITCAD_TEST_WORK/turn-keys.json" << 'EOF'
[
  {"cmd":"add_feature","name":"Box1","def":{"type":"box","plane":"xy","corner":[0,0],
    "length":10,"width":10,"height":10,"operation":"new_body"}},
  {"cmd":"add_feature","name":"Box2","def":{"type":"box","plane":"xy","corner":[20,0],
    "length":10,"width":10,"height":20,"operation":"new_body"}}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/turn-keys.json" --save "$MITCAD_TEST_WORK/turn-keys.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-turn-keys.mitcad-ui \
  ui_run_app native-turn-keys --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/turn-keys.mitcad"
ui_expect_log '^Native Browser selection: body:F2[.]b0@$' 'Down in the Browser outline selects the next row'
ui_expect_log '^View turned left by 15 degrees$' 'Left in the 3D view turns it'
ui_expect_log '^TestDriver: done' 'the turn keys are the 3D view only in the AppKit frontend'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: AppKit turn keys stay with the focused Browser and timeline'

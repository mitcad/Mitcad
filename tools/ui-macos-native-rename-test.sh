#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel app/browser/BrowserController.cpp app/browser/TimelineWidget
# check-all sources: app/MainWindowNative.cpp app/platform/macos/NativeMacWindow.mm app/platform/NativeMacWindow.hpp
# check-all sources: app/framework/TestDriver app/platform/macos/NativeTestInput.mm tools/cli
# Rename in the AppKit frontend's Browser and timeline menus (mitcad#1356):
# the Browser menu's Rename and a double-click on a body name it in the
# visible AppKit outline's text field, the timeline menu's Rename asks in
# the timeline's Rename Feature dialog as F2 does, never in the hidden Qt
# tree or strip (tools/ui-scripts/native-macos-rename.mitcad-ui). The
# saved design is checked with mitcad-cli: the body is Holder and the
# feature Base block. Input is the TestDriver's in-process native input,
# no system events. tools/ui-macos-tree-rename-test.sh checks the Qt
# frontend's rename editors.
# Usage: tools/ui-macos-native-rename-test.sh <app> <mitcad-cli> <output directory>
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
# Base (F1), a 10 mm cube whose body is Body1.
cat > "$MITCAD_TEST_WORK/rename.json" << 'EOF'
[
  {"cmd":"add_feature","name":"Base","def":{"type":"box","plane":"xy","corner":[0,0],
    "length":10,"width":10,"height":10,"operation":"new_body"}}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/rename.json" --save "$MITCAD_TEST_WORK/rename.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-rename.mitcad-ui \
  ui_run_app native-rename --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/rename.mitcad"
ui_expect_log '^Native Browser editor: body:F1\.b0@$' 'the Browser menu edits the AppKit outline row'
ui_expect_log '^Renamed Body1 to Holder$' 'the body is renamed from its menu'
ui_expect_log '^Renamed Base to Base block$' 'the feature is renamed from the timeline menu'
ui_expect_log '^TestDriver: done' 'Rename in the AppKit Browser and timeline menus'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done

# The names in the saved model.
cat > "$MITCAD_TEST_WORK/check.json" << 'EOF'
[
  {"expect": {"bodies": 1}},
  {"expect": {"body": "Holder", "volume": 1000}},
  {"expect": {"feature": "Base block", "status": "ok"}}
]
EOF
if "$cli" run "$MITCAD_TEST_WORK/check.json" --open "$MITCAD_TEST_WORK/rename.mitcad" \
     > "$UI_OUT/check.log" 2>&1; then
  ui_pass 'the saved model has the body Holder and the feature Base block'
else
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the saved model does not have the new names'
fi
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: Rename in the AppKit Browser and timeline menus'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos app/platform/NativeForms.hpp app/platform/NativeMacWindow.hpp app/platform/NativeTestInput.hpp
# check-all sources: app/framework/SelectionCreate app/framework/ParameterField app/framework/ParameterSuggest app/framework/FormulaEditor app/framework/TestDriver.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowNative.cpp app/MainWindowParameterEdits.cpp app/files/MainWindowPrint.cpp app/commands/ExtrudeCommands.cpp tools/cli
# check-all sources: app/commands/HoleCommands.cpp app/commands/HoleSizes app/framework/CommandSession.cpp
# Compact AppKit selection and command cards (mitcad#815): the selection
# card's whole Create actions as pointer targets, Extrude's aligned rows
# with OK/Cancel and Local bodies in view, the fx expression tools,
# 3D Print's Send/Cancel, and a Hole field's mark and Reset to size
# (mitcad#980), at the normal and a narrow window, light and dark.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-native-forms}"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# A synthetic 40 x 30 x 20 mm box: its planar faces offer Extrude, Create
# Sketch and Hole.
cat > "$MITCAD_TEST_WORK/body.json" <<'JSON'
[{"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":40,"width":30,"height":20,"operation":"new_body"}}]
JSON
"$cli" run "$MITCAD_TEST_WORK/body.json" --save "$MITCAD_TEST_WORK/body.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
for scheme in light dark; do
  MITCAD_FORMS_APPEARANCE=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-forms.mitcad-ui \
    ui_run_app "forms-$scheme" --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/body.mitcad"
  ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$scheme: native AppKit shell was used"
  ui_expect_log 'TestDriver: done' "$scheme: selection card, Extrude, fx tools, 3D Print and Hole passed"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed' \
                 'Unable to simultaneously satisfy constraints'; do
    ui_expect_no_log "$problem" "$scheme: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
done
echo "PASS: compact native selection and command cards, normal/narrow, light/dark; evidence in $UI_OUT"

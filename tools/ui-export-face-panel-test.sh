#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMake.cpp app/MainWindow.cpp app/framework/Command.hpp
# check-all sources: app/framework/CommandSession.cpp core/model-api/src/exchange.rs tools/cli
# Export Face as DXF… with nothing selected (mitcad#1963, GitHub
# mitcad/Mitcad#408) through the real UI, on a design mitcad-cli makes: a
# 60 x 40 x 5 plate (Box1 F1, Body1).
#   - Run with nothing selected, the command opens its panel with a Face
#     input instead of only a hint in the status bar, and asks for a
#     planar face; Esc cancels it.
#   - Opened again, the plate's top picked in the panel and Export (Enter):
#     the file dialog, and the plate's outline written from that face.
# tools/ui-windows-export-face-panel-test.ps1 does the same on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-export-face-panel-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-export-face-panel.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/plate.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 40, "height": 5, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$WORK/plate.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli plate"; }

ui_start_display
ui_start_app --open "$WORK/plate.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Nothing selected: the panel with its Face input"
ui_mark
ui_step "Export Face as DXF"               ui_command "Export Face as DXF"
ui_expect_new "Export Face as DXF: no planar face selected, the panel opens" "no face selected"
ui_expect_new "Command Export Face as DXF... started" "the panel opened"
ui_expect_new "Panel Export Face as DXF... input face at" "the panel has the Face input"
ui_expect_new "Pick face F1.b0/F1:top" "the input takes the plate's top"
grep -q "Exported .*: dxf outline" "$UI_LOG" && ui_fail "written without a face"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Export Face as DXF... cancelled" "cancelled"

echo "--- The top picked in the panel and written"
ui_mark
ui_step "Export Face as DXF"               ui_command "Export Face as DXF"
ui_expect_new "Command Export Face as DXF... started" "the panel opened again"
ui_step "click the plate's top"            ui_click_pick "face F1.b0/F1:top"
ui_expect_new "Export Face as DXF... Face: 1 face" "the top picked"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "Export (Enter)"                   ui_key Return
ui_type_path "Export Face as DXF" "$WORK/face.dxf"
ui_focus_main
ui_expect_new "Exported $WORK/face.dxf: dxf outline of Body1 along F1:top" "the plate's outline written" 30
ui_expect_new "Done Export Face as DXF..." "the panel closed"
[ -s "$WORK/face.dxf" ] || ui_fail "no face DXF"
grep -q "CUT_OUTER" "$WORK/face.dxf" || ui_fail "no outer outline in the DXF"
echo "ok   the DXF holds the outer outline"

ui_finish "UI export face panel test"

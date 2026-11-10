#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/commands/PrintAnalysis.cpp app/commands/PrintAnalysis.hpp
# check-all sources: app/MainWindowPrintAnalysis.cpp app/view/PrintOverlay.cpp app/view/PrintOverlay.hpp
# check-all sources: app/OcctViewer.hpp app/framework/Theme.cpp app/framework/Theme.hpp
# check-all sources: core/model/src/document/analyses.rs core/model-api/src core/model/src/print_analysis.rs
# check-all sources: core/model/src/print_fits.rs tools/cli
# Print Analysis' narrow gaps through the real UI (mitcad#1519), on two
# blocks of 20 mm printed in place 0.2 mm apart along X (Body1 at the
# origin, Body2 from x = 20.2), with the design's printed fit parameter
# print_in_place_gap at 0.25 mm:
#   - Make > Print Analysis opens with both bodies; the Minimum gap field
#     follows print_in_place_gap, so the analysis checks 0.25 mm and finds
#     Body1-Body2 0.20 mm apart; the view draws the gap with its marker
#     "Gap 0.20 mm" and the legend's entry.
#   - A 0.1 mm gap finds none; OK keeps it as Print1 with that gap, shown,
#     and saved with the file.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-gaps-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-print-gaps.XXXXXX)
FILE=$WORK/hinge.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/hinge.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [20.2, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_parameter", "name": "print_in_place_gap", "expression": "0.25 mm"}
]
EOF
"$CLI" run "$WORK/hinge.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_new_re "extended regex" "description": as ui_expect_new, a pattern.
expect_new_re() {
  for _ in $(seq 1 30); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line like '$1' since the mark"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the blocks shown"

echo "--- The panel: the design's gap"
ui_mark
ui_step "print analysis (search)"          ui_command "Print Analysis"
ui_expect_new "Command Print Analysis started" "its panel opens"
ui_expect_new "Print Analysis Bodies: 2 bodies" "with both bodies"
ui_expect_new "narrow gaps under 0.25 mm: Body1-Body2 0.20 mm" \
  "print_in_place_gap's 0.25 mm finds the blocks 0.2 mm apart" 20
expect_new_re "^Print overlay: .*, 1 narrow gap\(s\) #[0-9a-f]{6}, marker Gap 0\.20 mm at \(" \
  "the view draws the gap and its marker"
ui_expect_new "Narrow gap, under 0.25 mm" "the legend's entry"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- A smaller gap"
ui_mark
ui_step "minimum gap 0.1"                  ui_type_in "Panel Print Analysis input gap" "0.1"
ui_expect_new "narrow gaps under 0.1 mm: none" "0.2 mm is no narrow gap at 0.1 mm" 20

echo "--- Kept"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Kept Print Analysis as Print1" "kept in the document"
ui_expect_new "narrow gaps under 0.1 mm: none" "shown when the panel has closed" 20
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
grep -q '"min_gap"' "$FILE" || ui_fail "no narrow gap settings in the file"
echo "ok   the narrow gap settings are in the file"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI print gaps test"

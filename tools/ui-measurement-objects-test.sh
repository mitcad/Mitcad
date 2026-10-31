#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework app/browser app/view core/model-api/src core/model/src/document tools/cli
# check-all sources: app/MainWindowMeasurements.cpp app/MainWindowMeasureHistory.cpp app/MainWindowAnnotations.cpp
# Measurement objects (mitcad#649), now annotations (mitcad#702), through
# the real UI, in the Docked layout, on two cylinders of radius 5, 20 high
# (Body1 about Z through the origin, Body2 through (30, 40)) and a block
# (Body3):
#   - Keep as Dimension in the Measure panel: the two round faces kept as
#     AxisDistance1 (the axes 50 apart), drawn in its plane in Auto
#     (mitcad#894); the Annotate menu's entries, its Annotations… opening
#     the window; Close (mitcad#1010).
#   - Keep as Dimension in a history row's menu: Body1's top rim shown
#     again from its row's menu (Show) and kept as Diameter1 (Ø10), in
#     Auto too.
#   - The browser's Annotations folder lists them in one row.
#   - View > Export Snapshot at 4 x the view with a transparent background:
#     a PNG of four times the view's width with an alpha channel, the shown
#     annotations drawn.
#   - Saved, and opened again: both measured and drawn.
# tools/ui-annotations-test.sh edits them; tools/ui-windows-measurement-
# objects-test.ps1 does the same as this in the Windows VM.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-measurement-objects-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-measurement-objects.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/axes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 40], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [20, -30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/axes.json" --save "$WORK/axes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# wait_enabled "place": until the last "<place> at x,y" line is not
# " (disabled)": the button's place logged once it is enabled.
wait_enabled() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$last" ] && [[ $last != *"(disabled)" ]] && return 0
    sleep 0.2
  done
  ui_fail "'$1' is not enabled: $last"
}

# png_header file: the PNG's width, height and colour type (6: RGBA).
png_header() {
  od -An -tu1 -j16 -N10 "$1" |
    awk '{ printf "%d %d %d\n", $1 * 16777216 + $2 * 65536 + $3 * 256 + $4, $5 * 16777216 + $6 * 65536 + $7 * 256 + $8, $10 }'
}

ui_start_display
ui_start_app --open "$WORK/axes.mitcad"
ui_expect_log "Annotations: none" "no annotations yet"
ui_step "fit (F6)"                         ui_key F6

echo "--- Keep as Dimension in the Measure panel"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_expect_new "Measure Keep as Dimension at" "the panel has Keep as Dimension"
ui_expect_new "Measure Annotate at" "and Annotate beside it"
ui_expect_new "Measure Annotate entries: Annotations…" "nothing picked: only Annotations… in its menu"
ui_step "pick Body1's round face"          ui_click_pick "face F2.b0/F2:side(c1)"
ui_expect_new "Measure Annotate entries: Note…, Datum Label…, Frame…, Hole Callout, Annotations…" \
  "one face picked: every entry"
ui_step "pick Body2's round face"          ui_click_pick "face F4.b0/F4:side(c1)"
ui_expect_new "distance 40, angle 0 deg, axis distance 50, axis lines 2" "the axes 50 apart"
ui_expect_new "Measure Keep as Dimension enabled" "Keep as Dimension enabled for the model's picks"
wait_enabled "Measure Keep as Dimension"
grep -F "Measure Annotate entries: " "$UI_LOG" | tail -1 | grep -q "entries: Annotations…$" ||
  ui_fail "two picks, yet a leader can be chosen: $(grep -F "Measure Annotate entries: " "$UI_LOG" | tail -1)"
echo "ok   two picks: no leader, only Annotations…"
# At the panel's default width (mitcad#1010): no control's text clipped,
# every result row on one line.
ui_sync
width=$(grep -o "Panel Measure width [0-9]* of" "$UI_LOG" | tail -1 | sed 's/[^0-9]//g')
[ -n "$width" ] && [ "$width" -le 300 ] || ui_fail "the panel is '$width' wide, not within the default 300"
grep -F "Measure controls" "$UI_LOG" | tail -1 | grep -qF "Measure controls: none clipped" ||
  ui_fail "a control's text is clipped at $width: $(grep -F "Measure controls" "$UI_LOG" | tail -1)"
grep -F "Panel Measure inputs" "$UI_LOG" | tail -1 | grep -qF "Panel Measure inputs: none clipped" ||
  ui_fail "a pick's label or the filter's text is clipped at $width: $(grep -F "Panel Measure inputs" "$UI_LOG" | tail -1)"
grep -F "Panel Measure result: " "$UI_LOG" | tail -1 | grep -q "rows, one line each$" ||
  ui_fail "a result row does not fit its line: $(grep -F "Panel Measure result: " "$UI_LOG" | tail -1)"
echo "ok   no control clipped, every result row on one line at $width"
ui_mark
ui_step "Keep as Dimension"                ui_click_logged "Measure Keep as Dimension"
ui_expect_new "Measure Keep as Dimension clicked" "Keep as Dimension clicked"
ui_expect_new "Annotation kept: AxisDistance1" "kept as the axes' distance"
ui_expect_new "Annotations: AxisDistance1 axis_distance" "listed by the model"
ui_expect_new "Annotations drawn: AxisDistance1 = 50 (auto)" "drawn in its plane, in Auto"
ui_expect_new "Annotation label AxisDistance1 at" "its label placed in the view"
ui_mark
ui_step "Annotate"                         ui_click_logged "Measure Annotate"
ui_expect_new "Measure Annotate clicked" "Annotate clicked"
ui_expect_new "Context menu: Note… | Datum Label… | Frame… | Hole Callout | Annotations…" \
  "its menu: the leaders, then Annotations…"
ui_step "Annotations… (A)"                 ui_key a
ui_expect_new "Measure Annotations clicked" "Annotations… chosen"
ui_expect_new "Annotations window opened: 1 annotation(s)" "the window opened from the panel"
ui_focus_dialog '^Annotations$'
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main
ui_mark
ui_step "Close"                            ui_click_logged "Panel Measure Close"
ui_expect_new "Closed Measure" "measure closed"
grep -q "Annotations: none" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "closing the panel took the annotation back"
ui_expect_log "Browser Root/Annotations at" "the browser has the Annotations folder"

echo "--- Keep as Dimension in a history row's menu"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_step "pick Body1's top rim"             ui_click_pick "edge F2.b0/E{F2:end(r{c1})|"
ui_expect_new "Inspect Measure: length 31.416, radius 5, diameter 10" "the rim measured"
ui_expect_new "Measure history row 2: Edge of Body1" "its row in the history"
ui_expect_new "Measure history at" "the history's header laid out"
ui_step "unfold the history"               ui_click_logged "Measure history"
ui_expect_new "Measure history opened" "unfolded"
ui_expect_new "Measure history row 2 at" "its row laid out"
ui_mark
ui_step "right-click the rim's row"        ui_click_logged "Measure history row 2" 3
ui_expect_new "Context menu: Show | Keep as Dimension | Copy" "its menu"
ui_step "Show"                             ui_menu_choose "Show"
ui_expect_new "Measure history showed row 2" "shown again"
ui_sync
ui_mark
ui_step "right-click the rim's row"        ui_click_logged "Measure history row 2" 3
ui_expect_new "Context menu: Show | Keep as Dimension | Copy" "its menu again"
ui_step "Keep as Dimension"                ui_menu_choose "Keep as Dimension"
ui_expect_new "Measure history Keep as Dimension clicked: row 2" "Keep as Dimension chosen for the row"
ui_expect_new "Annotation kept: Diameter1" "kept as the rim's diameter"
ui_expect_new "Annotations drawn: AxisDistance1 = 50 (auto) | Diameter1 = Ø10 (auto)" \
  "both drawn, in Auto"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"

echo "--- Export Snapshot: 4 x the view, transparent"
read -r _ _ _ _ vw vh <<< "$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)"
[ -n "$vw" ] || ui_fail "the app did not log its view area"
ui_mark
ui_step "Export Snapshot (search)"         ui_command "Export Snapshot"
ui_expect_new "Snapshot dialog opened: size 2x, transparent off, annotations all (2 shown)" \
  "the dialog with its defaults and the two shown annotations"
ui_focus_dialog '^Export Snapshot$'
ui_step "the file"                         ui_type_text "$WORK/snapshot.png"
ui_step "Size: 4 x (Alt+Z, Down)"          ui_key alt+z Down
ui_expect_new "Snapshot dialog: size = 4x" "four times the view"
ui_step "Transparent background (Alt+T)"   ui_key alt+t
ui_expect_new "Snapshot dialog: transparent = on" "a transparent background"
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Snapshot dialog: 4x $((4 * vw)) x $((4 * vh)) to $WORK/snapshot.png" "the choices taken"
ui_expect_new "Snapshot saved $WORK/snapshot.png: $((4 * vw)) x $((4 * vh)), transparent, 2 annotation(s)" \
  "the snapshot written" 30
[ -s "$WORK/snapshot.png" ] || ui_fail "no snapshot file"
read -r pw ph pt <<< "$(png_header "$WORK/snapshot.png")"
[ "$pw" = $((4 * vw)) ] && [ "$ph" = $((4 * vh)) ] ||
  ui_fail "the snapshot is $pw x $ph, not 4 x the view's $vw x $vh"
[ "$pt" = 6 ] || ui_fail "the snapshot has no alpha channel (PNG colour type $pt)"
echo "ok   snapshot.png is $pw x $ph with an alpha channel, $(stat -c %s "$WORK/snapshot.png") bytes"

echo "--- Saved and opened again"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $WORK/axes.mitcad" "saved"
grep -q '"annotations"' "$WORK/axes.mitcad" || ui_fail "the design file has no annotations"
ui_stop_app
ui_start_app --open "$WORK/axes.mitcad"
ui_expect_log "Annotations: AxisDistance1 axis_distance | Diameter1 diameter" "both read back"
ui_expect_log "Annotations drawn: AxisDistance1 = 50 (auto) | Diameter1 = Ø10 (auto)" "and drawn"
ui_expect_log "Browser Root/Annotations at" "the browser has the Annotations folder"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI measurement objects test"

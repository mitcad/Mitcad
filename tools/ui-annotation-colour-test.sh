#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework app/view core/model-api/src core/model/src/document core/model/src/file tools/cli
# check-all sources: app/MainWindowAnnotations.cpp app/MainWindowMeasurements.cpp app/MainWindowVisibility.cpp
# Edit Annotation's Colour through the real UI (mitcad#764), in the Docked
# layout, with the Measure panel closed and open. Box:1 (O1) is a 30 x 20 x
# 10 block moved 60 along X; the root has a 10 x 10 x 15 block. Offset1 is
# the offset between the top and bottom faces of Box's block in O1 (its
# picks in an occurrence), Offset2 the same of the root's block; both
# stored in the old default orange #d9480f, which the model reads as Auto
# (mitcad#894: the six rotating defaults were the program's choice).
#   - Measure closed (the Annotations window from the command search):
#     Offset1 made Dark Grey (two rows down from Auto: Auto, Black, Dark
#     Grey and Custom… are all the menu offers, mitcad#902) and Offset2
#     #5c940d through Custom… (the colour dialog); the model's annotations,
#     the view and the window show the new colours at once; Offset2's own
#     colour offered as it is by Colour…; Undo and Redo take Offset2's back
#     and again.
#   - Measure open (its panel's Annotate ▾ > Annotations…): Offset1 made Black and
#     Offset2 #862e9c through Custom…; shown at once; kept when the panel
#     closes; Undo and Redo after it; saved, the design file has them.
#   - The Annotations window's Text Height… (mitcad#894): Auto (2.5 mm for
#     these small blocks) offered, 5 mm chosen; saved, the design file has
#     it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-annotation-colour-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-annotation-colour.XXXXXX)
FILE=$WORK/colours.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Box:1 (O1, C1): Sketch1 (F1), Extrude1 (F2): Box's block (F2.b0). The
# root: Sketch2 (F3), Extrude2 (F4): its block (F4.b0).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Box", "transform": [[1, 0, 0, 60], [0, 1, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}},
  {"cmd": "add_annotation", "name": "Offset1", "kind": "offset", "color": "#d9480f",
    "a": {"body": "F2.b0", "face": "F2:end($R)", "occurrence": "O1"},
    "b": {"body": "F2.b0", "face": "F2:start($R)", "occurrence": "O1"},
    "view": {"direction": [0, 1, 0], "up": [0, 0, 1]}},
  {"cmd": "add_annotation", "name": "Offset2", "kind": "offset", "color": "#d9480f",
    "a": {"body": "F4.b0", "face": "F4:end($R)"},
    "b": {"body": "F4.b0", "face": "F4:start($R)"},
    "view": {"direction": [0, 1, 0], "up": [0, 0, 1]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# wait_enabled "place": until the last "<place> at x,y" line is not
# " (disabled)".
wait_enabled() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$last" ] && [[ $last != *"(disabled)" ]] && return 0
    sleep 0.2
  done
  ui_fail "'$1' is not enabled: $last"
}

# no_refusal: no annotation command refused since the mark.
no_refusal() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Annotation command refused" &&
    ui_fail "$1: $(grep -F "Annotation command refused" "$UI_LOG" | tail -1)"
  echo "ok   $1"
}

# shown_in name colour value what: the model's annotation in that colour,
# drawn so in the view and listed so in the Annotations window, since the
# mark.
shown_in() {
  ui_expect_new "$1 = $3 ($2)" "$4: drawn in $2"
  ui_expect_new "Annotations row $1: offset, $3, -, -, $2, shown" "$4: listed in $2"
}

# edit_named name keys colour value what: the editor opened from the
# window's row, a named colour chosen with these keys ("Down Down"), OK.
edit_named() {
  ui_double_click_expect "Annotations row $1" "Edit Annotation dialog opened: $1 offset" "$5: $1's editor opened"
  ui_mark
  ui_focus_dialog '^Edit Annotation$'
  # shellcheck disable=SC2086 # the keys, one word each
  ui_step "Colour (Alt+C, $2)"              ui_key alt+c $2
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edit Annotation dialog: {\"color\":\"$3\"}" "$5: the colour asked for"
  shown_in "$1" "$3" "$4" "$5"
  no_refusal "$5: the edit not refused"
}

# edit_other name colour value what: the editor opened from the window's
# row, Custom… (the last row) and the colour typed into the colour dialog's
# HTML field, OK in both.
edit_other() {
  ui_double_click_expect "Annotations row $1" "Edit Annotation dialog opened: $1 offset" "$4: $1's editor opened"
  ui_mark
  ui_focus_dialog '^Edit Annotation$'
  ui_step "Colour: Custom… (Alt+C, End)"   ui_key alt+c End
  ui_focus_dialog '^Annotation Colour$'
  ui_step "HTML (Alt+H)"                   ui_key alt+h
  ui_step "$2"                             ui_type_text "$2"
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_dialog '^Edit Annotation$'
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edit Annotation dialog: {\"color\":\"$2\"}" "$4: the colour asked for"
  shown_in "$1" "$2" "$3" "$4"
  no_refusal "$4: the edit not refused"
}

# saved_colour name: the annotation's colour in the design file.
saved_colour() { jq -r --arg n "$1" '(.annotations // .measurements)[] | select(.name == $n) | .color' "$FILE"; }

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Annotations drawn: Offset1 = 10 (auto) | Offset2 = 15 (auto)" "both stored in the old orange, drawn as Auto"
ui_step "fit (F6)"                         ui_key F6

echo "--- Measure closed"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_new "Annotations window opened: 2 annotation(s)" "the window opened"
ui_expect_new "Annotations row Offset2 at" "its rows laid out"
edit_named Offset1 "Down Down" "#495057" 10 "Offset1 (in O1), Measure closed"
edit_other Offset2 "#5c940d" 15 "Offset2 (root), Measure closed"
# Its own colour (no longer a preset) is kept and offered as it is. Its row
# is still selected from the double-click that opened its editor.
grep "^Annotations selected: " "$UI_LOG" | tail -1 | grep -qx "Annotations selected: Offset2" ||
  ui_fail "Offset2 is not the row selected: $(grep "^Annotations selected: " "$UI_LOG" | tail -1)"
ui_mark
ui_focus_dialog '^Annotations$'
ui_step "Colour… (Alt+C)"                 ui_key alt+c
ui_expect_new "Annotation Colour dialog opened: Offset2 #5c940d (Auto | Black | Dark Grey | #5c940d | Custom…)" \
  "Auto, Black, Dark Grey, its own and Custom… offered"
ui_focus_dialog '^Annotation Colour$'
ui_step "cancel (Escape)"                 ui_key Escape
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Annotation Colour dialog: " && ui_fail "the cancelled dialog chose a colour"
echo "ok   the stored colour kept"
ui_focus_dialog '^Annotations$'

ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Annotations drawn: Offset1 = 10 (#495057) | Offset2 = 15 (auto)" "Undo takes Offset2's colour back"
ui_mark
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Annotations drawn: Offset1 = 10 (#495057) | Offset2 = 15 (#5c940d)" "Redo gives it again"

echo "--- Measure open"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_expect_new "Measure Annotate at" "the panel has Annotate"
ui_step "Annotate"                         ui_click_logged "Measure Annotate"
ui_expect_new "Context menu: Note… | Datum Label… | Frame… | Hole Callout | Annotations…" "Annotate's menu"
ui_step "Annotations… (A)"                 ui_key a
ui_expect_new "Annotations window opened: 2 annotation(s)" "the window opened from the panel"
# Its rows where they were laid out before (logged when they move).
ui_sync
# Dark Grey is the third row: Black one up from it.
edit_named Offset1 "Up" "#212529" 10 "Offset1 (in O1), Measure open"
edit_other Offset2 "#862e9c" 15 "Offset2 (root), Measure open"
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main
ui_mark
ui_step "close Measure (Enter)"            ui_key Return
ui_expect_new "Closed Measure" "measure closed"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Annotations drawn: " | grep -vqF "Offset1 = 10 (#212529) | Offset2 = 15 (#862e9c)" &&
  ui_fail "closing Measure changed the colours: $(grep "^Annotations drawn: " "$UI_LOG" | tail -1)"
echo "ok   the colours kept as Measure closed"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Edit Offset2" "its own undo step"
ui_expect_new "Annotations drawn: Offset1 = 10 (#212529) | Offset2 = 15 (#5c940d)" "Undo takes Offset2's colour back"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Edit Offset1" "Offset1's edit its own undo step too"
ui_expect_new "Annotations drawn: Offset1 = 10 (#495057) | Offset2 = 15 (#5c940d)" "and Offset1's"
ui_mark
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Annotations drawn: Offset1 = 10 (#212529) | Offset2 = 15 (#862e9c)" "Redo gives both again"

echo "--- Saved"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
[ "$(saved_colour Offset1)" = "#212529" ] || ui_fail "the file has Offset1 in $(saved_colour Offset1), not #212529"
[ "$(saved_colour Offset2)" = "#862e9c" ] || ui_fail "the file has Offset2 in $(saved_colour Offset2), not #862e9c"
echo "ok   the design file has both new colours"

echo "--- The annotations' text height"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_new "Annotations window opened: 2 annotation(s)" "the window opened"
wait_enabled "Annotations Text Height"
ui_focus_dialog '^Annotations$'
ui_step "Text Height… (Alt+X)"             ui_key alt+x
ui_expect_new "Annotations Text Height clicked" "Text Height clicked"
ui_expect_new "Annotation Text Height dialog opened: null (Auto (2.5 mm))" "Auto offered, 2.5 mm for the small blocks"
ui_focus_dialog '^Annotation Text Height$'
ui_step "5 mm (Down x3, Enter)"            ui_key Down Down Down Return
ui_expect_new "Annotation Text Height dialog: 5" "5 mm chosen"
ui_expect_new 'Annotation command: {"cmd":"set_annotation_text_height","text_height":5}' "sent to the model"
no_refusal "the height not refused"
ui_focus_dialog '^Annotations$'
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved again"
# A number as the file's other millimetres are written (5.0).
jq -e '.annotation_text_height == 5' "$FILE" > /dev/null ||
  ui_fail "the file has the text height $(jq -r '.annotation_text_height' "$FILE"), not 5"
echo "ok   the design file has the text height"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI annotation colour test"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/build app/MainWindow.cpp app/MainWindowContextMenu.cpp tools/cli core/model-base/src/properties.rs core/model/src/document/properties.rs core/model-api/src/properties.rs
# check-all sources: app/files/MainWindowFiles.cpp core/model/src/document/design_summary.rs core/model/src/file core/model-api/src/design_properties.rs
# Property Sets (mitcad#496) through the real UI, on a design with a
# component Wall placed twice (Wall:1, Wall:2) holding the body Panel, and
# an own type FireRating (EI30 / EI60 / EI90):
#   - File › Design Properties… (mitcad#1088) sets Title and Author: one
#     command; Property Sets (nothing selected) shows them on the root as
#     its set Summary; the file, written by mitcad-cli without statistics,
#     shows them unknown, and Save writes them, Created still unknown.
#   - The browser's menu of Wall:1 opens Property Sets with the occurrence's
#     and the component's levels: Wall:1 overrides the component's Mark,
#     and shows its FireRating as inherited.
#   - The component's Mark edited in place (W-20); FireRating picked from
#     its own type's values (EI60), as a choice.
#   - The body's menu shows the body's level too: an imported value (ifc)
#     edited shows that it differs from its source; the computed volume is
#     listed, read-only, and never saved.
#   - Find in Design searches by property: FireRating = EI60 finds both
#     occurrences, Mark = W-13 only Wall:1.
#   - The Bill of Materials takes Mark and FireRating as columns.
#   - The design file keeps the edited values and no computed one.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-property-sets-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-property-sets.XXXXXX)
FILE=$WORK/design.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Wall (C1, O1): Sketch1 (F1) 20 x 20 and Extrude1 (F2) 10 up: Panel
# (F2.b0); Wall:2 (O2) a copy. FireRating on the component, Mark on the
# component and Wall:1's own over it, an imported Reference on the body.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Wall"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Panel"},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "Wall:1", "transform": {"translation": [60, 0, 0]}},
  {"cmd": "define_property_type", "name": "FireRating", "base": "text",
   "values": ["EI30", "EI60", "EI90"], "description": "Fire resistance class"},
  {"cmd": "set_property", "owner": "C1", "set": "Pset_WallCommon", "key": "FireRating",
   "type": "FireRating", "value": "EI30"},
  {"cmd": "set_property", "owner": "C1", "set": "Identity", "key": "Mark", "value": "W-12"},
  {"cmd": "set_property", "owner": "O1", "set": "Identity", "key": "Mark", "value": "W-13"},
  {"cmd": "set_property", "owner": "F2.b0", "set": "Identity", "key": "Reference", "value": "R-1",
   "source": "ifc"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# edit_value "level set/key" text: double-click a value, replace it, Enter.
edit_value() {
  ui_focus_dialog '^Property Sets$'
  ui_double_click_logged "Property sets $1 value"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "$2"
  ui_key Return
}

# search "text": the Find in Design window's field.
search() {
  ui_focus_dialog '^Find in Design$'
  ui_click_logged "Find in Design field"
  ui_type_text "$1"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Wall:1 at" "the browser lists Wall:1"

echo "--- Design Properties: the summary on the root (mitcad#1088)"
ui_mark
ui_step "Design Properties (search)"     ui_command "Design Properties"
ui_expect_new "Design Properties opened: Created unknown, Last saved unknown, Last saved by unknown, Last saved with unknown" \
  "a file saved without statistics: nothing known of them"
ui_focus_dialog '^Design Properties$'
ui_step "Title"                          ui_type_field "Design Properties" "Wall study"
ui_step "to Author (Tab, Tab)"           ui_key Tab Tab
ui_step "Author"                         ui_type_field "Design Properties" "A. Maker"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Design Properties set: Title, Author" "one command with the two fields"
ui_focus_main
ui_mark
ui_step "Property Sets (search)"         ui_command "Property Sets"
ui_expect_new "Property Sets opened for C0" "the design's (the root's), nothing selected"
ui_expect_new "Property sets row: component Summary/Title = Wall study (user)" "the Title on the root"
ui_expect_new "Property sets row: component Summary/Author = A. Maker (user)" "the Author on the root"
ui_focus_dialog '^Property Sets$'
ui_step "close"                          ui_click_logged "Property sets close"
ui_focus_main

echo "--- The occurrence's and the component's levels"
ui_mark
ui_step "right-click Wall:1"             ui_click_logged "Browser Root/Wall:1" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Property Sets..."        ui_menu_choose "Property Sets..."
ui_expect_new "Property Sets opened for O1" "the window opened for Wall:1"
ui_expect_new "Property sets of Occurrence Wall:1, Component Wall:" "the occurrence's and the component's levels"
ui_expect_new "Property sets row: occurrence Identity/Mark = W-13 (user, overrides)" \
  "Wall:1's own Mark over the component's"
ui_expect_new "Property sets row: occurrence Pset_WallCommon/FireRating = EI30 (user, inherited)" \
  "the component's FireRating, inherited"
ui_expect_new "Property sets row: component Identity/Mark = W-12 (user)" "the component's Mark"
ui_expect_new "Property sets component Identity/Mark value at" "its cells are logged"

echo "--- A value edited in place"
ui_mark
ui_step "Mark = W-20"                    edit_value "component Identity/Mark" "W-20"
ui_expect_new "Property component Identity/Mark set: W-20" "the edit is a model command"
ui_expect_new "Property sets row: component Identity/Mark = W-20 (user)" "the component's Mark is W-20"
ui_expect_new "Property sets row: occurrence Identity/Mark = W-13 (user, overrides)" "Wall:1 keeps its own"

echo "--- An own type's value picked as a choice"
ui_mark
ui_focus_dialog '^Property Sets$'
ui_step "double-click FireRating"        ui_double_click_logged "Property sets component Pset_WallCommon/FireRating value"
ui_expect_new "Editing property component Pset_WallCommon/FireRating as a choice: EI30 | EI60 | EI90" \
  "the own type's values offered"
ui_expect_new "Property sets choice at" "the choice is logged"
ui_step "pick EI60"                      ui_choose "Property sets choice" 1
ui_expect_new "Property component Pset_WallCommon/FireRating chosen: EI60" "EI60 chosen"
ui_expect_new "Property sets row: occurrence Pset_WallCommon/FireRating = EI60 (user, inherited)" \
  "Wall:1 inherits it"

echo "--- The body's level: an imported value edited, a computed one"
ui_mark
ui_focus_main
ui_step "right-click Panel in Wall:1"    ui_click_logged "Browser Root/Wall:1/Bodies/Panel" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Property Sets..."        ui_menu_choose "Property Sets..."
# A body's level is titled by its name alone (mitcad#655).
ui_expect_new "Property sets of Panel, Occurrence Wall:1, Component Wall:" "the body's level first"
ui_expect_new "Property sets row: body Identity/Reference = R-1 (ifc)" "the imported value with its source"
ui_expect_new "Property sets row: body Computed/Volume = 4000 mm^3 (computed)" "the computed volume, read-only"
ui_mark
ui_step "Reference = R-2"                edit_value "body Identity/Reference" "R-2"
ui_expect_new "Property body Identity/Reference set: R-2" "the imported value edited"
ui_expect_new "Property sets row: body Identity/Reference = R-2 (ifc, edited)" "it shows that it differs from its source"
ui_step "close"                          ui_click_logged "Property sets close"

echo "--- Find in Design by property"
ui_focus_main
ui_mark
ui_step "Find in Design"                 ui_command "Find in Design"
ui_expect_new "Find in Design opened" "the window opened"
ui_mark
search "FireRating = EI60"
ui_expect_new "Find in Design found: 0 bodies, 2 components" "both occurrences inherit EI60"
ui_mark
search "Mark = W-13"
ui_expect_new "Find in Design found: 0 bodies, 1 component" "only Wall:1 has W-13"
ui_mark
search "Reference ~ r-"
ui_expect_new "Find in Design found: 2 bodies, 0 components" "the body in both places"
ui_focus_dialog '^Find in Design$'
ui_key Escape

echo "--- The Bill of Materials' property columns"
ui_focus_main
ui_mark
ui_step "Bill of Materials (command search)"    ui_command "Bill of Materials"
ui_focus_dialog '^Bill of Materials$'
ui_expect_new "Parts list columns at" "the columns field is logged"
ui_type_in "Parts list columns" "Mark, FireRating"
ui_key Return
ui_expect_new "Build parts list columns: Mark, FireRating" "two property columns"
ui_expect_new "Build parts list row: Wall: W-20 | EI60" "the component's values in them"
ui_focus_dialog '^Bill of Materials$'
ui_key Escape

echo "--- The design file: the edited values, no computed one"
ui_focus_main
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[.properties[] | select(.owner == "C1") | .sets[] | select(.name == "Identity") | .properties[0].value]' \
  '["W-20"]' "the component's Mark saved"
expect_file '[.properties[] | select(.owner == "F2.b0") | .sets[0].properties[0] | [.value, .source, .imported]]' \
  '[["R-2","ifc","R-1"]]' "the body's Reference with its source's value"
expect_file '[.properties[].sets[].name] | index("Computed")' 'null' "no computed set in the file"
expect_file '.property_types[0].values' '["EI30","EI60","EI90"]' "the own type saved"
expect_file '[.properties[] | select(.owner == "C0") | .sets[] | select(.name == "Summary") | .properties[] | [.key, .value]]' \
  '[["Title","Wall study"],["Author","A. Maker"]]' "Design Properties' summary saved on the root"
# The file mitcad-cli wrote had no statistics: Save writes the last save,
# its creation stays unknown (mitcad#1088).
expect_file '.statistics | [has("created"), has("last_saved"), (.last_saved_with | startswith("MitCad "))]' \
  '[false,true,true]' "the statistics written, created unknown"

ui_finish "UI property sets test"

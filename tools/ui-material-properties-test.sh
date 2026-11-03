#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print/MaterialProperties.cpp app/print/MaterialsDialog.cpp app/view/ViewController.cpp
# check-all sources: core/model/src/material.rs core/model/src/material_units.rs core/model/src/material_exchange.rs
# check-all sources: core/model/src/material_dataset.rs core/model/src/document/material_import.rs
# check-all sources: core/model-api/src/materials.rs third_party/material-properties tools/cli
# Materials' mechanical and thermal properties (mitcad#1213) through the
# real UI, in an inch design of one box with a material of its own, Shop
# steel. My library is the test's own folder (MITCAD_PRINT_LIBRARY_DIR).
#   1. Edit Materials: Shop steel's Young's modulus typed in psi, the
#      design's unit, shown in psi again and kept in MPa (the saved file).
#   2. New > Browse Material Dataset...: the dataset searched for "6061
#      t6", Aluminum 6061, T6 picked, its values shown in psi and lb/in3,
#      created in This design with its values and source, and saved.
#   3. Import...: a CSV of three materials, one of them not valid, into
#      This design: two imported, the invalid row reported with its line
#      and reason on the message line.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-material-properties-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-material-properties.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/box.mitcad
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library

# cli script [args...]: mitcad-cli run, failing the test when it fails.
cli() {
  "$CLI" run "$@" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli run $*"; }
}

# saved_material name filter expected description: a jq filter on the
# saved design's material of that name.
saved_material() {
  local actual
  actual=$(jq -c --arg name "$1" ".materials[] | select(.name == \$name) | $2" "$FILE") ||
    ui_fail "$4: cannot read $FILE"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

# The design: inches, one box, its own material Shop steel.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "set_units", "length": "in"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-0.5, -0.5],
    "length": 1, "width": 1, "height": 1, "operation": "new_body"}},
  {"cmd": "create_material", "id": "shop_steel", "name": "Shop steel", "density": 7.85, "class": "metal"}
]
EOF
cli "$WORK/model.json" --save "$FILE"
# A CSV of three materials, the third not valid (a negative yield
# strength), in metric units the file states.
printf '%s\r\n' 'name,class,density [g/cm3],yield_strength [MPa],poissons_ratio' \
  'Brass CZ121,metal,8.5,300,0.34' \
  'Bronze PB1,metal,8.8,250,' \
  'Broken,metal,8.5,-3,' > "$WORK/materials.csv"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the box is in the view"

echo "--- 1. Shop steel's Young's modulus typed in psi, the inch design's unit"
ui_mark
ui_command "Edit Materials"
ui_expect_new "Materials opened" "the Materials manager opened"
ui_expect_new "Materials design item Shop steel at " "Shop steel is listed in This design"
ui_mark
ui_step "Shop steel"                       ui_click_logged "Materials design item Shop steel"
ui_expect_new "Material selected: Shop steel (design)" "Shop steel is selected"
ui_expect_new "Materials properties: Shop steel: Young's modulus not given | Poisson's ratio not given | Yield strength not given" \
  "its values are not given, never 0"
ui_expect_new "Materials property youngs_modulus at " "Young's modulus has its field"
ui_mark
ui_step "Young's modulus 30000000"         ui_click_logged "Materials property youngs_modulus"
ui_key ctrl+a
xdotool type --delay 40 "30000000"
ui_step "Enter"                            ui_key Return
ui_expect_new "Material Shop steel (design) youngs_modulus = 30000000 psi" "typed in psi"
ui_expect_new "Materials properties: Shop steel: Young's modulus 30000000 psi | Poisson's ratio not given" \
  "shown in psi again"

echo "--- 2. Browse Material Dataset...: Aluminum 6061, T6 into This design"
ui_mark
ui_step "New"                              ui_click_logged "Materials new"
ui_expect_new "Materials new menu: Metal | General Plastic | FDM Plastic | Resin | Other | Browse Material Dataset" \
  "New ends with Browse Material Dataset..."
ui_step "Browse Material Dataset..."       ui_key Down Down Down Down Down Down Return
ui_expect_new "Material dataset opened: 119 materials" "the dataset opened, offline"
ui_focus_dialog '^Material Dataset$'
ui_mark
xdotool type --delay 40 "6061 t6"
ui_expect_new "Material dataset search: 6061 t6, 1 shown" "the search finds the material and grade"
ui_expect_new "Material dataset item Aluminum 6061, T6 at " "Aluminum 6061, T6 is listed"
ui_mark
ui_step "Aluminum 6061, T6"                ui_click_logged "Material dataset item Aluminum 6061, T6"
ui_expect_new "Material dataset selected: Aluminum 6061, T6: Density: 0.098 lb/in³ | Young's modulus: 10300000 psi | Poisson's ratio: 0.33 | Yield strength: 35000 psi" \
  "its values in the design's units"
ui_mark
ui_step "Create (Enter)"                   ui_key Return
ui_expect_new "Materials dataset menu: This design" "Create asks where, This design first"
ui_step "This design"                      ui_key Down Return
ui_expect_new "Material created from the dataset: Aluminum 6061, T6 (aluminum/6061/t6) as " "made in the design"
ui_expect_new "Materials properties: Aluminum 6061, T6: Young's modulus 10300000 psi | Poisson's ratio 0.33 | Yield strength 35000 psi" \
  "its values filled in, in psi"
ui_expect_new "Materials source: material-properties dataset, 6061, T6" "its source and grade kept"
ui_focus_main
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
# Kept in the model's units: 30 000 000 psi is 206 842.7 MPa.
saved_material "Shop steel" '.properties.youngs_modulus | . * 10 | round' '2068427' "Shop steel's modulus kept in MPa"
saved_material "Aluminum 6061, T6" '[.reference.material, .reference.grade, .class]' '["6061","T6","metal"]' \
  "the dataset's material saved with its source and grade"
saved_material "Aluminum 6061, T6" '.properties.yield_strength | round' '241' "its yield strength in MPa"

echo "--- 3. Import...: a CSV, one invalid row reported"
ui_mark
ui_step "Import..."                        ui_click_logged "Materials import_materials"
ui_step "the CSV"                          ui_type_path "Import Materials" "$WORK/materials.csv"
ui_expect_new "Materials import_materials menu: This design" "Import asks where"
ui_step "This design"                      ui_key Down Return
ui_expect_new "Materials import: materials.csv: added 2, replaced 0, kept 0, 1 not valid" "two imported, one not"
ui_expect_new "Materials import row 4 Broken: Yield strength must be greater than zero, got -3" \
  "the invalid row named with its line and reason"
ui_expect_new "Materials row: metals/" "the list shows the imported materials"
grep -qF "Materials row: metals/" "$UI_LOG" || ui_fail "no rows"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Materials row: .* Brass CZ121 (design)" ||
  ui_fail "Brass CZ121 is not listed in This design"
echo "ok   Brass CZ121 is in This design"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Materials row: .* Broken (design)" &&
  ui_fail "the invalid row was imported"
echo "ok   the invalid row was not imported"

ui_finish "UI material properties"

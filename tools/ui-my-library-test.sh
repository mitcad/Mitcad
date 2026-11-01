#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserController.cpp app/MainWindow.cpp app/files/MainWindowPrint.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs tools/cli
# check-all sources: core/model/src/print_profile.rs core/model/src/print_files/installed.rs core/model/src/document/slicer_presets.rs
# My Library (mitcad#704) through the real UI: the user's materials,
# printers and processes managed without a design, on a design of one box
# that must not change. My library is the test's own folder
# (MITCAD_PRINT_LIBRARY_DIR), which mitcad-cli gives a printer, Bench X.
#   1. Tools › My Library… (the command search): the Materials tab lists
#      the built-in materials under Built-in; Duplicate of PLA Plastic
#      makes PLA Plastic (copy) in My library (its file), its name typed
#      (Bench PLA) and its density changed (1.3), each written to the
#      file; Delete asks ("Delete Bench PLA from My library?") and removes
#      the file.
#   2. The Printers tab: Bench X; Duplicate makes Bench X (copy) in My library,
#      renamed Bench Y; Delete removes it.
#   3. The design did not change: no modified mark in the title, its file
#      as it was; Edit Printers and Processes' Edit Library… (after
#      Delete, left of Close, mitcad#1019) opens the
#      window again.
#   4. A fake Bambu Studio data folder (mitcad#830): Import from Slicer ▾
#      offers its presets; the new process is ticked, the printer named as
#      Bench X is marked and unticked; Import brings the process into My
#      library and leaves Bench X as it was; the line names it and My
#      Library shows it on the Processes tab, selected. Then again, with
#      both names in My library: the keyboard moves to Fine, Space ticks
#      and unticks it, Tab reaches its choice; Override ticks the row and
#      All same names shows no value while the rows differ; Copy again;
#      Import names the count ("Import 1 Preset"); the filter "bench"
#      hides Fine, and Import still brings the ticked Fine in as Fine 2,
#      named in the line and selected.
#   5. The first opening on a library of slicer imports (mitcad#1018): a
#      slicer's data folder made here like an installed one's (a vendor's
#      1800 system presets, the user's 41 on them), its presets imported
#      into a fresh My library; the app again, Edit Materials…, Edit
#      Library… of the Materials window (after Delete, left of Close; its
#      mnemonic Alt+L, mitcad#1019): My Library opens, logs its
#      opening's parts ("My Library timing: ...") and fills its panels
#      when the items were read off the UI thread ("My Library loaded:
#      ..."), the slicer's presets looked for as well ("My Library
#      slicers: ..."). The times are printed, none asserted.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-my-library-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-my-library.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/box.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
LIB=$MITCAD_PRINT_LIBRARY_DIR

# expect_item path filter expected description: a jq filter on a library
# item's file.
expect_item() {
  local actual
  [ -f "$1" ] || ui_fail "$4: no $1"
  actual=$(jq -c "$2" "$1") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

# logged_after "text" sed-expression: the last line since the mark with
# the text, through the expression.
logged_after() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1 | sed "$2"
}

# make_slicer_folder data: a slicer's data folder like an installed
# one's (mitcad#1018), synthetic: the system presets of a vendor Acme
# (30 printer models of 4 nozzles each, 16 processes and 40 filaments
# per model, on common bases with a full set of values), its app config
# with Acme M1 0.4 enabled, and the user's own presets on Acme M1: 30
# filaments, 3 printers, 8 processes, each with a few values of its own.
# Writes $WORK/slicer-import.json, which imports the user's presets into
# My library.
make_slicer_folder() {
  local data=$1 sys=$1/system/Acme user=$1/user/default
  local filament_keys=(filament_diameter filament_density filament_cost filament_shrink temperature_vitrification
    filament_flow_ratio pressure_advance nozzle_temperature nozzle_temperature_initial_layer
    nozzle_temperature_range_low nozzle_temperature_range_high cool_plate_temp eng_plate_temp hot_plate_temp
    textured_plate_temp filament_max_volumetric_speed fan_min_speed fan_max_speed fan_cooling_layer_time
    close_fan_the_first_x_layers full_fan_speed_layer slow_down_layer_time slow_down_min_speed overhang_fan_speed
    additional_cooling_fan_speed during_print_exhaust_fan_speed complete_print_exhaust_fan_speed
    filament_retraction_length filament_z_hop filament_retraction_speed filament_deretraction_speed
    filament_retraction_minimum_travel filament_wipe_distance filament_flush_temp filament_flush_volumetric_speed
    filament_minimal_purge_on_wipe_tower filament_prime_volume filament_change_length counter_coef_1 counter_coef_2
    counter_coef_3 hole_coef_1 hole_coef_2 hole_coef_3 counter_limit_min counter_limit_max hole_limit_min
    hole_limit_max diameter_limit)
  local process_keys=(initial_layer_print_height line_width initial_layer_line_width outer_wall_line_width
    inner_wall_line_width top_surface_line_width sparse_infill_line_width internal_solid_infill_line_width
    support_line_width seam_gap wipe_speed xy_contour_compensation xy_hole_compensation elefant_foot_compensation
    resolution slice_closing_radius ironing_speed ironing_flow ironing_spacing ironing_inset print_flow_ratio
    initial_layer_flow_ratio bridge_flow max_travel_detour_distance wall_loops top_shell_layers top_shell_thickness
    bottom_shell_layers bottom_shell_thickness sparse_infill_density infill_direction skin_infill_depth
    infill_lock_depth bridge_speed gap_infill_speed initial_layer_speed outer_wall_speed inner_wall_speed
    sparse_infill_speed internal_solid_infill_speed top_surface_speed travel_speed default_acceleration
    initial_layer_acceleration outer_wall_acceleration inner_wall_acceleration top_surface_acceleration
    travel_acceleration brim_width brim_object_gap raft_layers support_threshold_angle support_top_z_distance
    support_bottom_z_distance support_base_pattern_spacing support_interface_spacing tree_support_branch_angle
    tree_support_branch_distance prime_tower_width prime_volume)
  local values="" key k n t h q i name names="" presets=""
  mkdir -p "$sys/machine" "$sys/process" "$sys/filament/base" "$user/filament" "$user/machine" "$user/process"
  printf '{"name": "Acme", "version": "01.00.00.00"}\n' > "$data/system/Acme.json"
  # The common bases: every value the presets inherit.
  values=""
  for key in "${filament_keys[@]}"; do values+="\"$key\": [\"1\"], "; done
  printf '{"type": "filament", "name": "fdm_filament_common", "from": "system", "instantiation": "false", %s"filament_type": ["PLA"]}\n' \
    "$values" > "$sys/filament/base/fdm_filament_common.json"
  values=""
  for key in "${process_keys[@]}"; do values+="\"$key\": \"1\", "; done
  printf '{"type": "process", "name": "fdm_process_common", "from": "system", "instantiation": "false", %s"layer_height": "0.2"}\n' \
    "$values" > "$sys/process/fdm_process_common.json"
  printf '{"type": "machine", "name": "fdm_machine_common", "from": "system", "instantiation": "false", "printer_variant": "0.4", "printable_area": ["0x0", "180x0", "180x180", "0x180"], "printable_height": "180", "nozzle_diameter": ["0.4"], "machine_start_gcode": "%s"}\n' \
    "$(printf 'G1 X%s Y%s F3000\\n' $(seq 1 200))" > "$sys/machine/fdm_machine_common.json"
  for t in $(seq 1 40); do
    printf '{"type": "filament", "name": "Acme F%s @base", "from": "system", "instantiation": "false", "inherits": "fdm_filament_common", "nozzle_temperature": ["%s"], "filament_density": ["1.2%s"]}\n' \
      "$t" $((200 + t)) "$t" > "$sys/filament/base/Acme F$t @base.json"
  done
  for k in $(seq 1 30); do
    printf '{"type": "machine_model", "name": "Acme M%s", "nozzle_diameter": "0.2;0.4;0.6;0.8", "default_materials": "Acme F1 @Acme M%s"}\n' \
      "$k" "$k" > "$sys/machine/Acme M$k.json"
    for n in 0.2 0.4 0.6 0.8; do
      printf '{"type": "machine", "name": "Acme M%s %s nozzle", "from": "system", "instantiation": "true", "inherits": "fdm_machine_common", "printer_model": "Acme M%s", "printer_variant": "%s", "nozzle_diameter": ["%s"]}\n' \
        "$k" "$n" "$k" "$n" "$n" > "$sys/machine/Acme M$k $n nozzle.json"
    done
    for h in 08 12 16 20 24 28 32 36; do
      for q in 1 2; do
        printf '{"type": "process", "name": "0.%smm Q%s @Acme M%s", "from": "system", "instantiation": "true", "inherits": "fdm_process_common", "layer_height": "0.%s", "compatible_printers": ["Acme M%s 0.4 nozzle"]}\n' \
          "$h" "$q" "$k" "$h" "$k" > "$sys/process/0.${h}mm Q$q @Acme M$k.json"
      done
    done
    for t in $(seq 1 40); do
      printf '{"type": "filament", "name": "Acme F%s @Acme M%s", "from": "system", "instantiation": "true", "inherits": "Acme F%s @base", "filament_max_volumetric_speed": ["%s"], "compatible_printers": ["Acme M%s 0.4 nozzle"]}\n' \
        "$t" "$k" "$t" $((10 + t % 12)) "$k" > "$sys/filament/Acme F$t @Acme M$k.json"
    done
  done
  for t in $(seq 1 20); do names+="\"Acme F$t @Acme M1\", "; done
  printf '{"models": [{"vendor": "Acme", "model": "Acme M1", "nozzle_diameter": "0.4"}], "filaments": [%s"Acme F21 @Acme M1"], "presets": {"machine": "Acme M1 0.4 nozzle"}}\n# MD5 checksum 0123456789ABCDEF\n' \
    "$names" > "$data/BambuStudio.conf"
  # The user's own presets, each beside its .info.
  for i in $(seq 1 30); do
    name="My F$i"
    printf '{"name": "%s", "from": "User", "inherits": "Acme F%s @Acme M1", "nozzle_temperature": ["%s"], "filament_flow_ratio": ["0.9%s"], "filament_cost": ["2%s"]}\n' \
      "$name" "$i" $((205 + i)) "$((i % 10))" "$i" > "$user/filament/$name.json"
    printf 'sync_info = \nuser_id = \nsetting_id = \nbase_id = GFSA%s\nupdated_time = 0\n' "$i" > "$user/filament/$name.info"
    presets+="{\"path\": \"$user/filament/$name.json\", \"same_name\": \"copy\"}, "
  done
  for i in 1 2 3; do
    name="My M1 $i"
    printf '{"name": "%s", "from": "User", "inherits": "Acme M1 0.4 nozzle", "printable_height": "1%s0"}\n' \
      "$name" "$((7 + i))" > "$user/machine/$name.json"
    presets+="{\"path\": \"$user/machine/$name.json\", \"same_name\": \"copy\"}, "
  done
  for i in $(seq 1 8); do
    name="My 0.20mm Q$i"
    printf '{"name": "%s", "from": "User", "inherits": "0.20mm Q%s @Acme M1", "wall_loops": "%s", "sparse_infill_density": "%s%%"}\n' \
      "$name" "$(((i - 1) % 2 + 1))" $((2 + i % 3)) $((10 + 5 * i)) > "$user/process/$name.json"
    presets+="{\"path\": \"$user/process/$name.json\", \"same_name\": \"copy\"}, "
  done
  printf '[{"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": "%s", "into": "user", "presets": [%s]}]\n' \
    "$data" "${presets%, }" > "$WORK/slicer-import.json"
}

# logged_place text: "x y" of the last "<text> at x,y" logged.
logged_place() {
  grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed 's/^.* at \([-0-9]*\),\([-0-9]*\)$/\1 \2/'
}

# expect_list_button log: Edit Library… of the window logged as `log`
# (mitcad#1019) is in the row under the list after Delete, left of Close:
# no result button.
expect_list_button() {
  local lx ly dx dy cx cy
  read -r lx ly <<< "$(logged_place "$1 edit_library")"
  read -r dx dy <<< "$(logged_place "$1 delete")"
  read -r cx cy <<< "$(logged_place "$1 close")"
  [ "$ly" = "$dy" ] && [ "$ly" = "$cy" ] && [ "$lx" -gt "$dx" ] && [ "$lx" -lt "$cx" ] ||
    ui_fail "$1: Edit Library at $lx,$ly is not after Delete ($dx,$dy) and before Close ($cx,$cy)"
  echo "ok   $1: Edit Library… after Delete, left of Close"
}

# The design: one box. My library: a printer.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
cat > "$WORK/printer.json" << 'EOF'
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench X",
   "values": {"bed_shape": [[0, 0], [220, 0], [220, 220], [0, 220]], "max_print_height": 250,
              "nozzle_diameter": 0.4}}
]
EOF
"$CLI" run "$WORK/printer.json" --save "$WORK/other.mitcad" > "$WORK/cli-printer.log" 2>&1 ||
  { cat "$WORK/cli-printer.log"; ui_fail "mitcad-cli (Bench X into My library)"; }
[ -f "$LIB/print/printers/bench-x.json" ] || ui_fail "Bench X is not in My library"
DESIGN_SUM=$(sha256sum "$FILE" | cut -c1-64)

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the box is in the view"

echo "--- 1. My Library: a built-in material duplicated, renamed, its density changed, deleted"
ui_mark
ui_command "My Library"
ui_expect_new "My Library opened" "the window opened"
ui_expect_new "My Library materials row: fdm_plastics/pla PLA Plastic (mitcad)" "PLA is listed, built in"
ui_expect_new "My Library tab printers at" "the tabs have their places"
ui_expect_new "My Library import at" "Import from Slicer… has its place"
ui_expect_new "My Library materials item PLA Plastic at" "its row has its place"
ui_focus_dialog '^My Library$'
ui_mark
ui_step "PLA Plastic"                      ui_click_logged "My Library materials item PLA Plastic"
ui_expect_new "Material selected: PLA Plastic (mitcad)" "PLA is selected"
ui_expect_new "My Library materials duplicate at" "Duplicate under the list"
ui_step "Duplicate"                        ui_click_logged "My Library materials duplicate"
ui_expect_new "Material duplicated: PLA Plastic as " "a copy made"
id=$(logged_after "Material duplicated: PLA Plastic as " 's/.* as \(.*\) in \(.*\)$/\1/')
place=$(logged_after "Material duplicated: PLA Plastic as " 's/.* as .* in \(.*\)$/\1/')
[ "$place" = user ] || ui_fail "the copy went to '$place', not My library"
echo "ok   in My library, not in the design"
MATERIAL=$LIB/materials/$id.json
expect_item "$MATERIAL" '.material.name' '"PLA Plastic (copy)"' "its file, named after PLA"
ui_expect_new "My Library materials row: fdm_plastics/$id PLA Plastic (copy) (user)" "the list shows it"
ui_sync
ui_mark
xdotool type --delay 40 "Bench PLA"
ui_key Return
ui_expect_new "Material PLA Plastic (copy) (user) name = Bench PLA" "Enter renamed it"
expect_item "$MATERIAL" '.material.name' '"Bench PLA"' "the file has the new name"
ui_expect_new "My Library materials density at" "Density has its place"
ui_step "Density"                          ui_click_logged "My Library materials density"
ui_key ctrl+a
xdotool type --delay 40 "1.3"
ui_key Return
ui_expect_new "Material Bench PLA (user) density = 1.3" "Enter saved the density"
expect_item "$MATERIAL" '.material.density' '1.3' "the file has the new density"
ui_mark
ui_step "Delete"                           ui_click_logged "My Library materials delete"
ui_expect_new "My Library materials delete: Delete Bench PLA from My library?" \
  "Delete asks first, naming the library"
ui_focus_dialog '^Delete$'
ui_step "Delete (Alt+D)"                   ui_key alt+d
ui_expect_new "Material deleted: Bench PLA (user)" "deleted"
[ ! -e "$MATERIAL" ] || ui_fail "the file is still there"
echo "ok   its file is gone"

echo "--- 2. Printers: Bench X duplicated, renamed, deleted"
ui_focus_dialog '^My Library$'
ui_mark
ui_step "the Printers tab"                 ui_click_logged "My Library tab printers"
ui_expect_new "My Library tab: printers" "the Printers tab"
ui_expect_new "My Library printers row: printer/bench-x Bench X (user)" "Bench X is listed"
ui_expect_new "My Library printers item Bench X at" "its row has its place"
ui_step "Bench X"                          ui_click_logged "My Library printers item Bench X"
ui_expect_new "Profile selected: printer Bench X (user)" "Bench X is selected"
ui_expect_new "My Library printers duplicate at" "Duplicate under the list"
ui_mark
ui_step "Duplicate"                        ui_click_logged "My Library printers duplicate"
ui_expect_new "Profile duplicated: Bench X as " "a copy made"
pid=$(logged_after "Profile duplicated: Bench X as " 's/.* as \(.*\) in \(.*\)$/\1/')
place=$(logged_after "Profile duplicated: Bench X as " 's/.* as .* in \(.*\)$/\1/')
[ "$place" = user ] || ui_fail "the copy went to '$place', not My library"
PRINTER=$LIB/print/printers/$pid.json
expect_item "$PRINTER" '.profile.name' '"Bench X (copy)"' "its file, named after Bench X"
expect_item "$PRINTER" '.profile.values.max_print_height == 250' 'true' "with Bench X's values"
ui_sync
ui_mark
xdotool type --delay 40 "Bench Y"
ui_key Return
ui_expect_new "Profile Bench X (copy) (user) name = Bench Y" "Enter renamed it"
expect_item "$PRINTER" '.profile.name' '"Bench Y"' "the file has the new name"
ui_mark
ui_step "Delete"                           ui_click_logged "My Library printers delete"
ui_expect_new "My Library printers delete: Delete Bench Y from My library?" "Delete asks first"
ui_focus_dialog '^Delete$'
ui_step "Delete (Alt+D)"                   ui_key alt+d
ui_expect_new "Profile deleted: Bench Y (user)" "deleted"
[ ! -e "$PRINTER" ] || ui_fail "the printer's file is still there"
[ -f "$LIB/print/printers/bench-x.json" ] || ui_fail "Bench X went too"
echo "ok   its file is gone, Bench X stays"
ui_focus_dialog '^My Library$'
ui_mark
ui_step "close"                            ui_click_logged "My Library printers close"
ui_expect_new "My Library closed after changes" "closed"

echo "--- 3. The design did not change; Printers and Processes links to My Library"
ui_focus_main
title=$(xdotool getwindowname "$UI_WINDOW")
case $title in
  *"*"*) ui_fail "the design is marked changed: $title" ;;
esac
[ "$(sha256sum "$FILE" | cut -c1-64)" = "$DESIGN_SUM" ] || ui_fail "the design's file changed"
echo "ok   the design is unchanged ($title)"
ui_mark
ui_command "Edit Printers and Processes"
ui_expect_new "Printers and processes opened" "Printers and Processes opened"
ui_expect_new "Profiles edit_library at" "Edit Library… has its place"
ui_expect_new "Profiles close at" "Close has its place"
expect_list_button Profiles
ui_focus_dialog '^Printers and Processes$'
ui_mark
ui_step "Edit Library…"                    ui_click_logged "Profiles edit_library"
ui_expect_new "Profiles: Edit Library…" "the link"
ui_expect_new "My Library opened" "My Library opened again"

echo "--- 4. Presets of an installed slicer (mitcad#830): a fake Bambu Studio folder"
# The slicer's data folder in the test's own config folder: a process of
# the user's, and a printer named as My library's Bench X.
PRESETS=$XDG_CONFIG_HOME/BambuStudio/user/default
mkdir -p "$PRESETS/process" "$PRESETS/machine"
printf '{"name": "Fine", "from": "User", "layer_height": "0.12"}\n' > "$PRESETS/process/Fine.json"
printf '{"name": "Bench X", "from": "User", "printable_height": "180"}\n' > "$PRESETS/machine/Bench X.json"
BENCH_SUM=$(sha256sum "$LIB/print/printers/bench-x.json" | cut -c1-64)
ui_focus_dialog '^My Library$'
ui_mark
ui_step "Import from Slicer"               ui_click_logged "My Library import"
ui_expect_new "My Library import menu: From a file… | Presets of Bambu Studio…" "a file or the slicer's presets"
ui_key Down Down Return
ui_expect_new "Slicer presets opened: Bambu Studio, 2 presets" "the slicer's presets listed"
ui_expect_new "Slicer presets row: process Fine, ticked" "a new one ticked"
ui_expect_new "Slicer presets row: printer Bench X, unticked, same name: My library has a printer of this name, copy" \
  "the name My library has: marked, unticked, Copy"
ui_expect_new "Slicer presets ticked: 1, Import 1 Preset" "Import says how many"
ui_expect_new "Slicer presets all same names: copy" "All same names shows the rows' Copy"
ui_focus_dialog '^Import Presets$'
ui_step "Import (Alt+I)"                   ui_key alt+i
ui_expect_new "My Library import: Fine imported into My library" "the ticked one imported, by its name"
ui_expect_new "My Library tab: processes" "My Library shows the Processes tab"
ui_expect_new "Profile selected: process Fine (user)" "with Fine selected"
expect_item "$LIB/print/processes/fine.json" '.profile.values.layer_height' '0.12' "Fine is in My library"
# Its number as a number: the file keeps it as the model wrote it (250.0).
expect_item "$LIB/print/printers/bench-x.json" '.profile.values.max_print_height == 250' 'true' \
  "Bench X keeps its printable height"
[ "$(sha256sum "$LIB/print/printers/bench-x.json" | cut -c1-64)" = "$BENCH_SUM" ] ||
  ui_fail "the unticked Bench X's file changed"
echo "ok   Bench X's file as it was"

echo "--- 4b. Both names in My library now: the keyboard, a choice that ticks its row, Fine as Fine 2"
FINE_SUM=$(sha256sum "$LIB/print/processes/fine.json" | cut -c1-64)
ui_focus_dialog '^My Library$'
ui_mark
ui_step "Import from Slicer"               ui_click_logged "My Library import"
ui_expect_new "My Library import menu: From a file… | Presets of Bambu Studio…" "the menu again"
ui_key Down Down Return
ui_expect_new "Slicer presets row: process Fine, unticked, same name: My library has a process of this name, copy" \
  "Fine is now a name My library has: unticked, Copy"
ui_expect_new "Slicer presets ticked: 0, Import" "nothing ticked: plain Import"
ui_expect_new "Slicer presets all same names: copy" "All same names: Copy, as both rows"
ui_focus_dialog '^Import Presets$'
# The list has the keyboard on its first preset, Bench X: down past the
# Processes heading to Fine.
ui_mark
ui_step "Down Down"                        ui_key Down Down
ui_expect_new "Slicer presets current: Fine" "the arrows move to Fine"
ui_mark
ui_step "Space"                            ui_key space
ui_expect_new "Slicer presets tick: Fine ticked" "Space ticks it"
ui_mark
ui_step "Space"                            ui_key space
ui_expect_new "Slicer presets tick: Fine unticked" "Space again unticks it"
ui_mark
ui_step "Tab"                              ui_key Tab
ui_expect_new "Slicer presets focus: Fine choice" "Tab reaches Fine's Copy or Override"
ui_mark
ui_step "Down (Override)"                  ui_key Down
ui_expect_new "Slicer presets choice: Fine override" "Override chosen"
ui_expect_new "Slicer presets tick: Fine ticked" "the choice ticked its unticked row"
ui_expect_new "Slicer presets ticked: 1, Import 1 Preset" "only Fine ticked: Import 1 Preset"
ui_expect_new "Slicer presets all same names: none" "the rows differ: All same names shows no value"
ui_mark
ui_step "Up (Copy)"                        ui_key Up
ui_expect_new "Slicer presets choice: Fine copy" "Copy chosen again"
ui_expect_new "Slicer presets all same names: copy" "the rows agree again: Copy"
# The filter (mitcad#871 review): Shift+Tab back to the row, again to the
# filter; "bench" hides Fine, which stays ticked and is imported.
ui_mark
ui_step "Shift+Tab Shift+Tab (filter)"     ui_key shift+Tab shift+Tab
ui_step "type bench"                       xdotool type --delay 40 "bench"
ui_expect_new "Slicer presets filter: bench, 1 shown" "the filter shows Bench X alone"
ui_mark
ui_step "Import (Alt+I)"                   ui_key alt+i
ui_expect_new "My Library import: Fine imported into My library as Fine 2" "the line names the copy's name"
ui_expect_new "Profile selected: process Fine 2 (user)" "My Library shows Fine 2 selected"
expect_item "$LIB/print/processes/fine-2.json" '.profile.name' '"Fine 2"' "Fine 2 is in My library"
[ "$(sha256sum "$LIB/print/processes/fine.json" | cut -c1-64)" = "$FINE_SUM" ] ||
  ui_fail "Fine's file changed by a Copy"
[ "$(sha256sum "$LIB/print/printers/bench-x.json" | cut -c1-64)" = "$BENCH_SUM" ] ||
  ui_fail "the unticked Bench X's file changed"
echo "ok   Fine and Bench X as they were"
ui_focus_dialog '^My Library$'
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "My Library closed" "closed"

echo "--- 5. The first opening on a library of slicer imports (mitcad#1018), from the Materials window"
# A slicer's data folder like an installed one's: a vendor of system
# presets (30 printer models, 120 printers, 480 processes, 1200
# filaments on 41 bases), one model enabled in its app config, and the
# user's own presets on them: 30 filaments, 3 printers, 8 processes,
# imported into a fresh My library. Made here, synthetic.
SLICER=$XDG_CONFIG_HOME/BambuStudio
make_slicer_folder "$SLICER"
export MITCAD_PRINT_LIBRARY_DIR=$WORK/slicer-library
"$CLI" run "$WORK/slicer-import.json" --save "$WORK/slicer.mitcad" > "$WORK/cli-slicer.log" 2>&1 ||
  { cat "$WORK/cli-slicer.log"; ui_fail "mitcad-cli (the slicer's presets into My library)"; }
count=$(find "$MITCAD_PRINT_LIBRARY_DIR" -name '*.json' | wc -l)
[ "$count" -ge 41 ] || ui_fail "My library has $count items, not the 41 imported"
echo "ok   $count items imported into My library"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_mark
ui_command "Edit Materials"
ui_expect_new "Materials opened" "the Materials window opened"
ui_expect_new "Materials timing: " "the Materials window logs its opening's time"
ui_expect_new "Materials edit_library at" "Edit Library… has its place"
ui_expect_new "Materials close at" "Close has its place"
expect_list_button Materials
ui_focus_dialog '^Materials$'
ui_mark
ui_step "Edit Library… (Alt+L)"            ui_key alt+l
ui_expect_new "Materials: Edit Library…" "the button, by its mnemonic"
ui_expect_new "My Library opened" "My Library opened"
ui_expect_new "My Library timing: " "its first opening logs each part's time"
ui_expect_new "My Library materials: " "the materials listed"
ui_expect_new "My Library loaded: " "the panels filled when the items were read"
ui_expect_new "My Library slicers: " "the slicer's presets looked for"
# The times as logged, for the record; none is asserted.
for line in "Materials timing: " "My Library timing: " "My Library loaded: " "My Library slicers: "; do
  echo "     $(grep -F -- "$line" "$UI_LOG" | tail -1 | sed "s/^.*\($line\)/\1/")"
done
ui_focus_dialog '^My Library$'
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "My Library closed" "closed"

ui_finish "UI My Library"

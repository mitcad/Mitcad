#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/library app/files app/commands/DerivedPartCommands.cpp app/build/BuildCatalog.cpp
# check-all sources: core/model/src/document/component_options.rs core/model/src/features/derived_part.rs
# check-all sources: core/model/src/part_options.rs core/vcs/src/library tools/cli
# check-all sources: tools/libraries/make-profile-library.py tools/ui-scripts/cut-length-groups.json
# check-all sources: tools/ui-scripts/cut-length-profile.json tools/ui-scripts/cut-length-rail.json
# Components cut to length from the library (mitcad#1950, GitHub
# mitcad/Mitcad#398): a slot profile and a rail made by
# tools/libraries/make-profile-library.py from supplier files made here
# (tools/ui-scripts/cut-length-*.json written as STEP; nothing of a real
# supplier's), fetched from a local git repository, no network:
#   1. Insert from Library on the profile: its length L is an option from
#      50 mm to 3000 mm, at its design's 100 mm (its default row), with its
#      stock lengths above it; L 750 mm typed, inserted with it (no row):
#      the section 261.345576 mm2 times 750 mm.
#   2. Derived Component (a double-click on its feature in the timeline):
#      L is there, 1200 mm, OK: the profile follows.
#   3. Insert from Library on the rail: Stock length 600 mm chosen sets L
#      to 600 mm (the row 600, its part number), inserted: 15 x 12 x 600.
#   4. Insert from Library on a timber board of sizes (a closed table): its
#      Thickness and Width by its rows, L beside them; Stock length 3300 mm
#      (from its stock lengths: no row sets only a length) sets L, inserted
#      in its row 22x100 with L alone: 22 x 100 x 3300.
# The Windows test is tools/ui-windows-cut-length-library-test.ps1, with
# the same steps.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cut-length-library-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
SCRIPTS=$ROOT/tools/ui-scripts
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cut-length-library.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=$WORK/gitconfig
: > "$WORK/gitconfig"
LIB=$WORK/mitcad-parts
SCENE=$WORK/scene.mitcad

# The supplier's files and the group file, then the library, fetched.
mkdir -p "$WORK/supplier" "$WORK/groups"
for name in profile rail; do
  "$CLI" run "$SCRIPTS/cut-length-$name.json" --save "$WORK/$name.mitcad" > "$WORK/$name.log" 2>&1 ||
    { cat "$WORK/$name.log"; ui_fail "mitcad-cli: the $name"; }
done
"$CLI" export "$WORK/profile.mitcad" "$WORK/supplier/p20x20.step" > /dev/null || ui_fail "the profile's STEP"
"$CLI" export "$WORK/rail.mitcad" "$WORK/supplier/rail.step" > /dev/null || ui_fail "the rail's STEP"
cp "$SCRIPTS/cut-length-groups.json" "$WORK/groups/a-cut-length.json"
python3 "$ROOT/tools/libraries/make-profile-library.py" --cli "$CLI" --source "$WORK/supplier" --out "$LIB" \
  --groups "$WORK/groups" --work "$WORK" --version 1.0.0 --no-render --commit \
  > "$WORK/generator.log" 2>&1 || { cat "$WORK/generator.log"; ui_fail "the generator failed"; }
grep -q "mitcad-parts 1.0.0: 3 components" "$WORK/generator.log" ||
  { cat "$WORK/generator.log"; ui_fail "the generator did not make all three components"; }
"$CLI" library fetch "$LIB" > "$WORK/fetch.log" 2>&1 || { cat "$WORK/fetch.log"; ui_fail "the library fetch failed"; }
printf '[{"cmd": "add_parameter", "name": "Spare", "expression": "1 mm"}]\n' > "$WORK/scene.json"
"$CLI" run "$WORK/scene.json" --save "$SCENE" > "$WORK/scene.log" 2>&1 ||
  { cat "$WORK/scene.log"; ui_fail "mitcad-cli: the scene"; }
echo "ok   the library of the profile, the rail and the board"

# The bodies' volumes: the last one logged since the mark.
expect_volume_new() { # sed pattern with one group, expected, description
  local volume=""
  for _ in $(seq 1 150); do
    volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n "s/.*$1.*/\\1/p" | tail -1)
    [ -n "$volume" ] && break
    sleep 0.2
  done
  [ -n "$volume" ] || ui_fail "$3: no volume logged"
  awk -v v="$volume" -v e="$2" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$3: volume $volume mm3, expected $2"
  echo "ok   $3: $volume mm3"
}

ui_start_display
ui_start_app --no-native-dialogs --open "$SCENE"
ui_step "fit (F6)"                         ui_key F6

echo "--- 1. Insert from Library: the profile's length an option"
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Browse Components$'
ui_step "search the profile"               xdotool type --delay 40 "Profile 20x20 cut test"
ui_expect_new "Library search 'Profile 20x20 cut test': 1 components" "the search finds it"
ui_expect_new "Library browser selected mitcad-parts/p20x20-cut-test: version v1.0.0 (" "the profile selected"
ui_expect_new "Library browser options of mitcad-parts/p20x20-cut-test: L = 100" "L starts at the design's length"
ui_expect_new "(row 100 mm)" "its default row"
ui_expect_new "Library option L allowed 50 mm … 3000 mm" "L within the profile's shortest and longest"
ui_expect_new "Library fields at x: Version " "the fields laid out"
ui_expect_new ", Stock length " "the stock lengths above L"
ui_mark
ui_step "L 750 (Alt+V, Tab, Tab, type, Tab)" eval 'ui_key alt+v; ui_sync; ui_key Tab; ui_sync; ui_key Tab; ui_sync;
  ui_key ctrl+a; xdotool type --delay 60 "750"; ui_key Tab'
ui_expect_new "Library option L chosen: 750 mm" "750 mm chosen"
ui_expect_new "Library options: L = 750" "L 750 mm"
ui_expect_new "(no row)" "a length of no stock row"
ui_mark
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component " "the profile inserted"
ui_expect_new "with options L = 750 mm" "with the length typed"
expect_volume_new "New body [^(]* ([^)]*): volume \([0-9.]*\) mm3" 196009.182298 "the profile 750 mm long"

echo "--- 2. Derived Component: its length changed"
ui_double_click_expect "Timeline DerivedComponent1" " with Derived Component" "Derived Component opens on its feature"
ui_expect_new ", options, rows 100 mm|3000 mm|1000 mm, " "the profile's rows"
ui_expect_new ", parameters L, " "its length L among its values"
ui_mark
ui_step "L = 1200 mm"                      ui_type_in "Panel Derived Component input param0" "1200 mm"
ui_step "OK"                               ui_click_logged "Panel Derived Component OK"
ui_expect_new " edited: L = 1200 mm" "one command with the new length"
expect_volume_new "Body [^(]* ([^)]*): volume [0-9.]* -> \([0-9.]*\) mm3" 313614.691677 "the profile follows, 1200 mm"
ui_focus_main

echo "--- 3. The rail: a stock length sets its length"
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Browse Components$'
ui_step "search the rail"                  xdotool type --delay 40 "Rail cut test"
ui_expect_new "Library search 'Rail cut test': 1 components" "the search finds the rail"
ui_expect_new "Library browser selected mitcad-parts/rail-cut-test: version v1.0.0 (" "the rail selected"
ui_expect_new "Library browser options of mitcad-parts/rail-cut-test: L = 300" "the rail at its first stock row"
ui_mark
ui_step "Stock length 600 (Alt+V, Tab, 6)" eval 'ui_key alt+v; ui_sync; ui_key Tab; ui_sync; xdotool type --delay 80 "6"'
ui_expect_new "Library stock length 600: 600 mm" "the stock length 600 mm chosen"
ui_expect_new "Library options: L = 600" "L set to 600 mm"
ui_expect_new "(row 600)" "the row 600"
ui_expect_new "Library browser row 600: designation " "the row's designation"
ui_mark
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component " "the rail inserted"
ui_expect_new "with options L = 600 mm" "with its stock length"
expect_volume_new "New body [^(]* ([^)]*): volume \([0-9.]*\) mm3" 108000 "the rail 15 x 12 x 600"

echo "--- 4. A board of sizes: L beside its closed rows"
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Browse Components$'
ui_step "search the board"                 xdotool type --delay 40 "Board cut test"
ui_expect_new "Library search 'Board cut test': 1 components" "the search finds the board"
ui_expect_new "Library browser selected mitcad-parts/board-cut-test: version v1.0.0 (" "the board selected"
ui_expect_new "Library browser options of mitcad-parts/board-cut-test: Thickness = 22, Width = 100, L = 3000" \
  "its sizes by its default row, L at the design's length"
ui_expect_new "(row 22x100)" "the row 22x100"
ui_expect_new "Library option Width allowed 100, 150 (Only these rows)" "Width by the rows of 22"
ui_expect_new "Library option L allowed 100 mm … 6000 mm" "L within the board's shortest and longest"
ui_mark
ui_step "Stock length 3300 (Alt+V, Tab, 33)" eval 'ui_key alt+v; ui_sync; ui_key Tab; ui_sync; xdotool type --delay 80 "33"'
ui_expect_new "Library stock length 3300: 3300 mm" "the stock length 3300 mm chosen"
ui_expect_new "Library options: Thickness = 22, Width = 100, L = 3300" "L set to 3300 mm, the size kept"
ui_expect_new "(row 22x100)" "still the row 22x100"
ui_mark
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component " "the board inserted"
ui_expect_new "with options L = 3300 mm (row 22x100)" "in its row with its length alone"
expect_volume_new "New body [^(]* ([^)]*): volume \([0-9.]*\) mm3" 7260000 "the board 22 x 100 x 3300"

ui_finish "UI cut length library test"

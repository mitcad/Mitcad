#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp
# check-all sources: app/MainWindowContextMenu.cpp core/model/src/build core/model-build core/model-api/src core/vcs/src/library tools/cli
# The fasteners of the TIMBER group's walls and spacing sets, and Swap
# Fastener (mitcad#497), through the real UI, with a small library of
# synthetic parts made here (mitcad-cli: a 48 x 98 timber member, a wood
# screw, a ring nail and a pan-head connector screw in two sizes each,
# their heads large boxes outside the members to click on; no supplier's
# files), fetched from a local git repository:
#   1. Wall (mitcad#520): Fastener, Size and Count under Fasteners; the
#      first time the countersunk wood screw the library marks as the
#      default (`wood.default`, mitcad#516), not the connector screw whose
#      name comes first; the
#      wood screw chosen in the part picker, its row 5.0x90, three at each
#      junction; a wall 1200 x 600 placed with them. The next Wall start
#      remembers them.
#   2. Two of the wall's screws selected in the view: the context menu's
#      BUILD has Swap Fastener; it starts with them, the ring nail chosen
#      in its picker, 3.1x90; Swap makes them nails in one undo step. The
#      picker's design parts count the screws in their size only
#      (mitcad#518).
#   3. Another design: a member on the ground (after Place nothing is
#      previewed at the origin on top of it, mitcad#513), then Spacing on
#      its side with Fasteners on: every member of the set fastened to it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timber-fasteners-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-timber-fasteners.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/test-timber
FILE=$WORK/wall.mitcad
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# frame NAME ORIGIN Z X: a mount frame (docs/libraries.md, "BUILD parts").
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
design() { # design FILE: the script on stdin saved as a design
  local script=$WORK/$(basename "$1" .mitcad).json
  cat > "$script"
  mkdir -p "$(dirname "$1")"
  "$CLI" run "$script" --save "$1" > /dev/null || ui_fail "mitcad-cli run $(basename "$script") failed"
}
# fastener SIZE ROW ROW: a head SIZE x SIZE x 10 above its axis frame (the
# entry point; a shank is not needed to click on it), in two rows.
fastener() {
  printf '[\n  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-%s, -%s], "length": %s, "width": %s, "height": 10, "operation": "new_body"}},\n' \
    "$(($1 / 2))" "$(($1 / 2))" "$1" "$1"
  frame axis "[0, 0, 10]" "[0, 0, 1]" "[1, 0, 0]"
  printf '  {"cmd": "set_configurations", "configurations": {"parameters": [], "default": "%s",
    "rows": [{"name": "%s"}, {"name": "%s"}]}},\n' "$2" "$2" "$3"
  printf '  {"expect": {"bodies": 1}}\n]\n'
}

"$CLI" library init "$LIB" --id test-timber --name "Test timber" --license MIT > /dev/null ||
  ui_fail "library init"
{ printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "600 mm"},\n'
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-49, -24], "width": 98, "height": 48},\n'
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
  frame face_xp "[49, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame face_xn "[-49, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame face_yp "[0, 24, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame face_yn "[0, -24, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/dimensioned-timber/timber-48-test.mitcad"
fastener 30 5.0x70 5.0x90 | design "$LIB/fasteners/wood-screw-test.mitcad"
fastener 20 2.8x50 3.1x90 | design "$LIB/fasteners/ring-nail-test.mitcad"
fastener 24 5.0x40 5.0x60 | design "$LIB/fasteners/connector-screw-test.mitcad"

cat > "$LIB/mitcad-library.json" <<'EOF'
{
  "format": "mitcad-library", "version": 1, "id": "test-timber", "name": "Test timber",
  "description": "Synthetic BUILD parts of the timber fasteners UI test.", "library_version": "1.0.0",
  "license": "MIT", "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "dimensioned-timber", "name": "Dimensioned timber"}, {"id": "fasteners", "name": "Fasteners"}],
  "components": [
    {"id": "timber-48-test", "path": "dimensioned-timber/timber-48-test.mitcad", "name": "Timber 48x98 test",
     "category": "dimensioned-timber", "designation": "Timber 48x98 test L={L}",
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
       "availability": "standard",
       "section": {"width": "98 mm", "height": "48 mm"}, "nominal": {"width": "98 mm", "height": "48 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "600 mm",
                  "stock": ["3000 mm", "3600 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}]}},
    {"id": "connector-screw-test", "path": "fasteners/connector-screw-test.mitcad", "name": "Connector screw test",
     "category": "fasteners", "designation": "Connector screw test {config}",
     "build": {"role": "fastener", "wood": {"kind": "screw", "head": "pan", "seat": "head_underside",
                                            "diameter": "5 mm", "length": "40 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}],
       "rows": {"5.0x40": {"wood": {"diameter": "5 mm", "length": "40 mm"}},
                "5.0x60": {"wood": {"diameter": "5 mm", "length": "60 mm"}}}}},
    {"id": "wood-screw-test", "path": "fasteners/wood-screw-test.mitcad", "name": "Wood screw test",
     "category": "fasteners", "designation": "Wood screw test {config}",
     "build": {"role": "fastener", "wood": {"kind": "screw", "head": "countersunk", "seat": "head_top", "default": true,
                                            "diameter": "5 mm", "length": "70 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}],
       "rows": {"5.0x70": {"wood": {"diameter": "5 mm", "length": "70 mm"}},
                "5.0x90": {"wood": {"diameter": "5 mm", "length": "90 mm"}}}}},
    {"id": "ring-nail-test", "path": "fasteners/ring-nail-test.mitcad", "name": "Ring nail test",
     "category": "fasteners", "designation": "Ring nail test {config}",
     "build": {"role": "fastener", "wood": {"kind": "nail", "diameter": "3.1 mm", "length": "90 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}],
       "rows": {"2.8x50": {"wood": {"diameter": "2.8 mm", "length": "50 mm"}},
                "3.1x90": {"wood": {"diameter": "3.1 mm", "length": "90 mm"}}}}}
  ]
}
EOF
g "$LIB" add -A
g "$LIB" commit -q -m "Test parts"
g "$LIB" tag v1.0.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch"

# expect_inspect CMD TEXT... DESCRIPTION: an "Inspect CMD:" line since the
# mark has every TEXT.
expect_inspect() {
  local cmd=$1 description=${!#} line text ok
  shift
  for _ in $(seq 1 50); do
    while IFS= read -r line; do
      ok=1
      for text in "${@:1:$#-1}"; do
        [[ $line == *"$text"* ]] || ok=0
      done
      [ $ok = 1 ] && { echo "ok   $description"; return; }
    done < <(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect $cmd: ")
    sleep 0.2
  done
  ui_fail "$description: no 'Inspect $cmd:' line since the mark with: ${*:1:$#-1}"
}
# choose_in_picker CMD INPUT PART [USED]: the input's tile opens the part
# picker, a click on the part's tile there chooses it, Escape closes the
# picker. With USED, the picker's "In this design" line has that text.
choose_in_picker() {
  ui_mark
  ui_step "the $2 tile"                    ui_click_logged "Panel $1 input $2"
  ui_expect_new "Part picker opened: " "the part picker for $1's $2"
  if [ -n "${4:-}" ]; then
    ui_expect_new "$4" "the picker's design parts: $4"
  fi
  ui_expect_new "Part picker tile $3 at " "$3's tile in the picker"
  ui_focus_dialog "^Component Picker - "
  ui_mark
  ui_step "$3 in the picker"               ui_click_logged "Part picker tile $3"
  ui_expect_new "Part picker chose $3" "$3 chosen in the picker"
  ui_step "close the picker (Escape)"      ui_key Escape
  ui_focus_main
}
# fastener_bodies: the bodies of the fasteners' heads made since the mark
# (their volume: a 30 x 30 x 10 head).
fastener_bodies() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^New body .* (\(F[0-9]*\.b[0-9]*\)): volume 9000\b.*/\1/p'
}

ui_start_display
ui_start_app --no-native-dialogs

echo "--- 1. Wall: the fastener, its size and count"
ui_mark
ui_step "the BUILD tab"                    ui_click_logged "Ribbon tab BUILD"
ui_expect_new "Ribbon BUILD/TIMBER at" "its TIMBER group (mitcad#520)"
ui_mark
ui_step "Wall (command search)"            ui_command "Wall"
ui_expect_new "Command Wall started" "Wall started"
ui_expect_new "BUILD parts wood: " "the library's wood fasteners"
expect_inspect Wall "frame wall Wall 1" "fasteners yes" \
  "fastener 2 × Wood screw test at each junction, sized from the members" \
  "the first time (fresh settings): two of the countersunk wood screw the library marks as the default, not the pan-head connector screw (mitcad#516)"
ui_expect_log "Panel Wall input fastener_size at" "Size under Fasteners"
ui_expect_log "Panel Wall input fastener_count at" "Count under Fasteners"
choose_in_picker Wall fastener test-timber/wood-screw-test
ui_mark
ui_step "Size 5.0x90"                      ui_choose "Panel Wall input fastener_size" 2
ui_step "three at each junction"           ui_type_in "Panel Wall input fastener_count" "3"
ui_step "1200 mm long"                     ui_type_in "Panel Wall input frame_length" "1200 mm"
ui_step "600 mm high"                      ui_type_in "Panel Wall input frame_height" "600 mm"
expect_inspect Wall "frame wall Wall 1, wall 1200 × 600 mm" \
  "fastener 3 × Wood screw test 5.0x90 at each junction" "the wall with three 5.0x90 screws at each junction"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall frame placed" 60
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Placed frame Wall 1 " | tail -1)
fastened=$(sed -n 's/.*member(s), \([0-9]*\) junction(s) fastened$/\1/p' <<< "$line")
[ "${fastened:-0}" -gt 0 ] || ui_fail "no junction fastened: $line"
screws=$(fastener_bodies | wc -l)
[ "$screws" -eq $((3 * fastened)) ] || ui_fail "$screws screw(s) at $fastened junction(s), not three each"
echo "ok   $fastened junction(s), three screws each"
seats=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Timber fastener .*: head top ')
[ "$(printf '%s\n' "$seats" | grep -c 'head top 0 mm from entry face$')" -eq "$screws" ] ||
  ui_fail "the physical screw heads are not flush within 1e-6 mm: $seats"
echo "ok   every frame screw head is flush within 1e-6 mm"

SCREWS=$(fastener_bodies)
ui_step "close Wall (Escape)"              ui_key Escape
ui_mark
ui_step "Wall again (command search)"      ui_command "Wall"
ui_expect_new "BUILD wall adds to Component1; fasteners on; 3 × Wood screw test 5.0x90 at each junction" \
  "the next start remembers the fastener, its size and count"
ui_step "close Wall (Escape)"              ui_key Escape

echo "--- 2. Swap Fastener from the context menu"
ui_step "fit (F6)"                         ui_key F6
ui_sync
# The two heads highest in the view (the top plate's).
mapfile -t TOP < <(for body in $SCREWS; do
    grep -E "^Body $body in [O0-9/]+ at " "$UI_LOG" | tail -1
  done | sed -n 's/^Body \(.*\) at \([0-9]*\),\([0-9]*\)$/\3 \1/p' | sort -n | head -2 | cut -d' ' -f2-)
[ "${#TOP[@]}" = 2 ] || ui_fail "no two screw heads in the view: ${TOP[*]}"
ui_step "click a screw"                    ui_click_logged "Body ${TOP[0]}"
ui_step "Ctrl+click another"               xdotool mousemove $(ui_logged_at "Body ${TOP[1]}") keydown ctrl click 1 keyup ctrl
ui_mark
ui_step "right-click it"                   ui_click_logged "Body ${TOP[1]}" 3
ui_expect_new "Context menu Build: " "the context menu's BUILD"
menu=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu Build: " | tail -1)
[[ $menu == *"Swap Fastener"* ]] || ui_fail "BUILD has no Swap Fastener: $menu"
echo "ok   BUILD offers Swap Fastener"
ui_mark
ui_step "Build > Swap Fastener"            ui_menu_choose "Build" "Swap Fastener"
ui_expect_new "Command Swap Fastener started" "Swap Fastener started"
ui_expect_new "BUILD swap fastener starts with 2 fastener(s), test-timber|wood-screw-test" \
  "with the two screws selected, their own part"
expect_inspect "Swap Fastener" "swap 2 fastener(s) (wood, 2 × Wood screw test 5.0x90)" \
  "the two 5.0x90 screws to swap"
# The use count is per size (mitcad#518): the screws are all 5.0x90, the
# wood screw's other size 5.0x70 is not listed in the design's parts.
choose_in_picker "Swap Fastener" part test-timber/ring-nail-test \
  "test-timber/wood-screw-test x$screws [5.0x90 x$screws]"
ui_mark
ui_step "Size 3.1x90"                      ui_choose "Panel Swap Fastener input fastener_size" 2
expect_inspect "Swap Fastener" "for Ring nail test 3.1x90" "they become 3.1x90 ring nails"
ui_mark
ui_step "Swap (Enter)"                     ui_key Return
ui_expect_new "Swapped 2 fastener(s) for Ring nail test 3.1x90 (3.1x90)" "the two screws are ring nails" 30
ui_expect_new "head top 0 mm from entry face" "the swapped nails are seated flush"
seats=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Timber fastener .*: head top ')
[ -n "$seats" ] && ! printf '%s\n' "$seats" | grep -v 'head top 0 mm from entry face$' ||
  ui_fail "a swapped fastener stands proud: $seats"

ui_step "close the panel (Escape)"         ui_key Escape
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Swap 2 fastener(s) for Ring nail test 3.1x90" "one undo step takes the swap back"
ui_stop_app

echo "--- 3. Spacing with fasteners"
ui_start_app --no-native-dialogs
ui_step "the BUILD tab"                    ui_click_logged "Ribbon tab BUILD"
ui_mark
ui_step "Timber (command search)"          ui_command "Timber"
ui_expect_new "Command Timber started" "Timber started"
ui_mark
ui_step "1800 mm long"                     ui_type_in "Panel Timber input length" "1800"
ui_step "a member on the ground (Enter)"   ui_key Return
ui_expect_new "Placed Timber 48x98 test as " "a member on the ground" 30
beam=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^New body .* (\(F[0-9]*\.b[0-9]*\)): volume.*/\1/p' | tail -1)
[ -n "$beam" ] || ui_fail "no body of the member"
# After Place no ghost (mitcad#513): nothing previewed at the origin on top
# of the member; the next waits for a click on a member's side or end. The
# lines after "Placed" only: the member's own preview before Enter (the
# length typed) is no ghost.
UI_MARK=$(grep -n "^Placed Timber 48x98 test as " "$UI_LOG" | tail -1 | cut -d: -f1)
expect_inspect Timber "failed: Click a member's side or end." "after Place the panel waits for a click on a member"
sleep 1.5
ui_sync
! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Inspect Timber: place grounded" ||
  ui_fail "a member previewed at the origin after Place: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    grep "^Inspect Timber: place grounded" | head -1)"
echo "ok   no member previewed at the origin on top of the one placed"
ui_mark
ui_step "Mode: Spacing"                    ui_click_logged "Panel Timber input how=spacing"
ui_expect_new "BUILD parts wood: " "the wood fasteners read"
ui_step "fit (F6)"                         ui_key F6
ui_step "Timber's Place On again"          ui_click_logged "Panel Timber input at"
ui_sync
ui_mark
ui_step "the member's side"                ui_click_pick "$beam/${beam%%.*}:part(F2:side("
expect_inspect Timber "place side" "; set of " "; fasteners no" "a set on the member, no fasteners by default"
ui_mark
ui_step "Fasteners on"                     ui_click_logged "Panel Timber input set_fasteners"
expect_inspect Timber "place side" "; set of " \
  "; fasteners 2 × Wood screw test at each junction, sized from the members" "a set on the member, fastened"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Timber 48x98 test as " "the set placed" 60
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Placed Timber 48x98 test as " | tail -1)
[[ $line =~ a\ set\ of\ ([0-9]+),\ ([0-9]+)\ junction\(s\)\ fastened ]] || ui_fail "no set fastened: $line"
[ "${BASH_REMATCH[1]}" = "${BASH_REMATCH[2]}" ] && [ "${BASH_REMATCH[1]}" -gt 1 ] ||
  ui_fail "not every member fastened to the member it stands on: $line"
echo "ok   ${BASH_REMATCH[1]} members, each fastened"
ui_step "close Timber (Escape)"            ui_key Escape

ui_finish "UI timber fasteners test"

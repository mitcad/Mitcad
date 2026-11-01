#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowContextMenu.cpp app/framework/Command.cpp
# check-all sources: app/framework/Command.hpp app/browser app/build app/commands tools/cli
# check-all sources: core/model/src/build core/model-build core/vcs/src/library
# The view's context menu (mitcad#494) through the real UI, on a design
# with an ordinary body in a component, a surface body, and BUILD timber
# members (a synthetic library made here: a 200 x 300 member; no supplier's
# files), one alone and a spacing set of them:
#   - A face of the ordinary body: Solid (the commands the face completes
#     first: Press Pull, Fillet, Chamfer, Shell, Offset Face, Delete Face;
#     under More the multi-step ones and the creation commands that would
#     only stand on it: Draft, Hole, Split Face, Replace Face, Plastic
#     Feature, Joint, Box, Cylinder, Sphere, Torus, Coil, Gear, Loft), no
#     surface-only command; Construct (Offset Plane first, Midplane and the
#     planes and axes at a point under More); no BUILD; Create Sketch,
#     Measure, Appearance...; Delete: Body, Component, Feature; Hide.
#   - An edge of it: Fillet and Chamfer in Solid's first entries.
#   - A face of the surface body: Untrim, Unstitch and Reverse Normal.
#   - A face of the lone member: BUILD with Length, Ends, Flip, Rotate,
#     Timber, Wall and Panel first, Profile and Linear Component under More, no
#     Spacing Set and no Floor / Roof (its roof is offered on wall frames'
#     members only, mitcad#520); Delete: Member, Body, Component.
#   - A face of a member of the set: Spacing Set in BUILD, and Delete's
#     Spacing Set.
#   - BUILD > Timber on the lone member's face starts with the face as
#     Place On, previewed on its side; Place makes a member there. BUILD >
#     Panel starts with the face as its Frame Face.
#   - Delete > Delete Member removes the lone member, Delete > Delete Body
#     the ordinary body; one undo brings each back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-context-menu-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-context-menu.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/test-timber
FILE=$WORK/menu.mitcad
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# frame NAME ORIGIN Z X: a mount frame (docs/libraries.md, "BUILD parts").
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
# A timber member 300 wide (along X) and 200 thick (along Y), large to
# click, centred on the
# Z axis, extruded by L from z = 0, with its end frames and a face frame on
# each side (docs/libraries.md, "Timber parts").
{ printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "600 mm"},\n'
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-150, -100], "width": 300, "height": 200},\n'
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
  frame face_xp "[150, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame face_xn "[-150, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame face_yp "[0, 100, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame face_yn "[0, -100, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } > "$WORK/member.json"
"$CLI" library init "$LIB" --id test-timber --name "Test timber" --license MIT > /dev/null || ui_fail "library init"
mkdir -p "$LIB/dimensioned-timber"
"$CLI" run "$WORK/member.json" --save "$LIB/dimensioned-timber/timber-200-test.mitcad" > /dev/null ||
  ui_fail "mitcad-cli run member.json failed"
cat > "$LIB/mitcad-library.json" <<'EOF'
{
  "format": "mitcad-library", "version": 1, "id": "test-timber", "name": "Test timber",
  "description": "A synthetic BUILD part of the context menu UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "dimensioned-timber", "name": "Dimensioned timber"}],
  "components": [
    {"id": "timber-200-test", "path": "dimensioned-timber/timber-200-test.mitcad", "name": "Timber 200x300 test",
     "category": "dimensioned-timber", "designation": "Timber 200x300 test L={L}",
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
       "availability": "standard",
       "section": {"width": "300 mm", "height": "200 mm"}, "nominal": {"width": "300 mm", "height": "200 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "600 mm",
                  "stock": ["3000 mm", "3600 mm", "4200 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}]}}
  ]
}
EOF
g "$LIB" add -A
g "$LIB" commit -q -m "Test part"
g "$LIB" tag v1.0.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch"

# F1 (Sketch1) 400 x 300 at (-900, 0), F2 extruded 250 up as a new
# component: Component1:1 (O1, F2.b0). F3 (Sketch2) a line, F4 its surface
# extruded 300 up in the root (F4.b0). The lone member (O2) at the origin,
# 600 long; a set of three members (O3, ...) 500 apart along X from (0, 900).
cat > "$WORK/design.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-900, 0], "width": 400, "height": 300},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 250}, "operation": "new_component"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F3", "start": [-900, 600], "end": [-500, 600]},
  {"cmd": "add_feature", "def": {"type": "extrude", "surface": true, "operation": "new_body",
    "curves": {"sketch": "F3", "curves": ["c1"]}, "extent": {"type": "distance", "distance": 300}}},
  {"cmd": "build_place", "part": {"library": "test-timber", "url": "$LIB", "rev": "HEAD", "component": "timber-200-test"},
   "at": {"point": [0, 0, 0]}, "length": 600},
  {"cmd": "build_place", "part": {"source": "S1"}, "at": {"point": [0, 900, 0]}, "length": 600,
   "spacing": {"along": [1, 0, 0], "length": 1300, "distance": 500}}
]
EOF
"$CLI" run "$WORK/design.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli design"; }

# The last "Context menu <name>: ..." line's entries, " | " between them.
menu_line() { grep -F "Context menu$1: " "$UI_LOG" | tail -1 | sed "s/.*Context menu$1: //"; }
# has NAME ENTRY...: each entry is one of the submenu's (NAME "" the menu).
has() {
  local name=$1 entries entry
  shift
  # (The labels' single ellipsis matches three dots, mitcad#645.)
  entries="| $(menu_line "$name" | sed "s/…/.../g") |"
  for entry in "$@"; do
    [[ $entries == *"| ${entry//…/...} |"* ]] || ui_fail "'$entry' is not in the context menu$name: $(menu_line "$name")"
  done
  echo "ok   the context menu${name:- itself} has: $*"
}
# lacks NAME ENTRY...: none of the entries is the submenu's.
lacks() {
  local name=$1 entries entry
  shift
  entries="| $(menu_line "$name") |"
  for entry in "$@"; do
    [[ $entries != *"| $entry |"* ]] || ui_fail "'$entry' is in the context menu$name: $(menu_line "$name")"
  done
  echo "ok   the context menu${name:- itself} has none of: $*"
}
# right_click KIND BODY [OCCURRENCE [FACE]]: a right click on a face or an edge
# of the body, where Offset Face (faces) or Fillet (edges) logs it picked (FACE: a part of its name, the largest face to hit).
right_click() {
  local at
  if [ "$1" = edge ]; then
    ui_step "fillet (search)"              ui_command "Fillet"
  else
    ui_step "offset face (search)"         ui_command "Offset Face"
  fi
  ui_expect_log "Pick $1 $2/${4:-}" "where a $1 of $2 is picked"
  at=$(ui_pick_at "$1 $2/${4:-}")
  ui_step "Escape (cancel it)"             ui_key Escape
  ui_mark
  ui_step "right-click a $1 of $2"         xdotool mousemove $at click 3
  ui_expect_new "Selected: 1 $1 [$1 " "a $1 selected"
  grep "^Selected: " "$UI_LOG" | tail -1 | grep -qF " of $2${3:+ in $3}]" || ui_fail "not a $1 of $2${3:+ in $3}: $(grep "^Selected: " "$UI_LOG" | tail -1)"
  ui_expect_new "Context menu: " "the context menu opened"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F2.b0 in O1 at" "the ordinary body is in the view"
ui_expect_log "Body F4.b0 at" "the surface"
member=$(sed -n 's/^Body \(.*\) in O2 at .*/\1/p' "$UI_LOG" | tail -1)
[ -n "$member" ] || ui_fail "the lone member's body (in O2) is not in the view"
setMember=$(sed -n 's/^Body \(.*\) in O3 at .*/\1/p' "$UI_LOG" | tail -1)
[ -n "$setMember" ] || ui_fail "a member of the set (in O3) is not in the view"
shown=$(grep "^Bodies shown: " "$UI_LOG" | tail -1)

echo "--- A face of an ordinary body"
right_click face F2.b0 O1 "F2:end("
has "" Solid Construct "Create Sketch" Measure "Appearance..." Delete Hide "Find in Browser" "Clear Selection"
lacks "" Build
has " Solid" "Press Pull" Fillet Chamfer Shell "Offset Face" "Delete Face" More
has " Solid More" Draft Hole "Split Face" "Replace Face" "Plastic Feature" Joint Box Cylinder Sphere Torus Coil Gear Loft
lacks " Solid" Draft Hole Box Loft Untrim Unstitch "Reverse Normal"
lacks " Solid More" Untrim Unstitch "Reverse Normal" Thicken Stitch "Press Pull"
has " Construct" "Offset Plane" More
has " Construct More" Midplane "Plane Tangent to Face at Point" "Offset Plane Through Point" \
  "Axis Perpendicular to Face at Point" "Point Through Three Planes"
[ "$(menu_line " Delete")" = "Delete Body | Delete Component | Delete Feature" ] ||
  ui_fail "Delete of an ordinary body's face: $(menu_line " Delete")"
echo "ok   Delete: Body, Component, Feature"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- An edge of it"
right_click edge F2.b0 O1
solid=$(menu_line " Solid")
[[ $solid == *"Fillet | Chamfer"* ]] || ui_fail "Fillet and Chamfer are not Solid's first entries: $solid"
echo "ok   Solid: $solid"
lacks "" Build "Create Sketch"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- A face of a surface body"
right_click face F4.b0
has " Solid" Untrim Unstitch "Reverse Normal"
[ "$(menu_line " Delete")" = "Delete Body | Delete Feature" ] ||
  ui_fail "Delete of a surface's face in the root: $(menu_line " Delete")"
echo "ok   Delete: Body, Feature"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- A face of the lone timber member"
right_click face "$member" O2 "${member%%.*}:part("
has "" Solid Build Construct Delete Hide
has " Build" Length Ends Flip "Rotate 90°" Timber Wall Panel More
has " Build More" Profile "Linear Component"
lacks " Build" "Spacing Set" Profile "Linear Component"
lacks " Build More" "Spacing Set" "Floor / Roof"
lacks " Build" "Floor / Roof"
[ "$(menu_line " Delete")" = "Delete Member | Delete Body | Delete Component" ] ||
  ui_fail "Delete of a member's face: $(menu_line " Delete")"
echo "ok   Delete: Member, Body, Component"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- A face of a member of the set"
right_click face "$setMember" O3 "${setMember%%.*}:part("
has " Build" "Spacing Set"
[ "$(menu_line " Delete")" = "Delete Member | Delete Spacing Set | Delete Body | Delete Component" ] ||
  ui_fail "Delete of a set member's face: $(menu_line " Delete")"
echo "ok   Delete: Member, Spacing Set, Body, Component"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- BUILD > Timber on the member's face"
right_click face "$member" O2 "${member%%.*}:part("
ui_step "Build > Timber"                   ui_menu_choose "Build" "Timber"
ui_expect_new "Command Timber started" "Timber started"
ui_expect_new "Timber Place On: 1 face" "the face is where it goes"
for _ in $(seq 1 50); do
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Timber: " | grep -qF "place side" && break
  sleep 0.2
done
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Timber: " | grep -qF "place side" ||
  ui_fail "Timber's preview is not on the member's side: $(grep "^Inspect Timber: " "$UI_LOG" | tail -1)"
echo "ok   previewed on the side"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Timber 200x300 test" "a member placed on that face"
ui_step "Close (Esc)"                      ui_key Escape
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "$shown" "one undo takes it away"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- BUILD > Panel on the member's face"
right_click face "$member" O2 "${member%%.*}:part("
ui_step "Build > Panel"                    ui_menu_choose "Build" "Panel"
ui_expect_new "Command Panel started" "Panel started"
ui_expect_new "Panel Frame Face: 1 face" "the face is its frame face"
ui_step "Cancel (Esc)"                     ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- Delete > Delete Member"
right_click face "$member" O2 "${member%%.*}:part("
ui_step "Delete > Delete Member"           ui_menu_choose "Delete" "Delete Member"
ui_expect_new "Deleted member Timber 200x300 test" "the member is deleted"
ui_expect_new "Bodies shown: " "the bodies shown again"
grep "^Bodies shown: " "$UI_LOG" | tail -1 | grep -qF "$member in O2" && ui_fail "the member is still shown"
echo "ok   the member's body is gone"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: " "undone"
ui_expect_new "$shown" "one undo brings the member back"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- Delete > Delete Body"
right_click face F2.b0 O1 "F2:end("
ui_step "Delete > Delete Body"             ui_menu_choose "Delete" "Delete Body"
ui_expect_new "Deleted body" "the body is deleted"
ui_expect_new "Bodies shown: " "the bodies shown again"
grep "^Bodies shown: " "$UI_LOG" | tail -1 | grep -qF "F2.b0 in O1" && ui_fail "the body is still shown"
echo "ok   the ordinary body is gone"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: " "undone"
ui_expect_new "$shown" "one undo brings the body back"

ui_finish "UI context menu"

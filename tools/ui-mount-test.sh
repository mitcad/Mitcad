#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MountCommands.cpp app/commands/JointCommands.cpp app/library app/files
# Mount frames of any part and Place Part (mitcad#525) through the real UI,
# on synthetic parts mitcad-cli makes (no model is downloaded) in local git
# repositories (no network):
#   1. Gridfinity, a baseplate of 3 x 2 cells of 42 mm (each cell a block of
#      its own, so that a click picks the cell) and bins of 1 x 1 and 2 x 1
#      cells: the baseplate dropped in as a copy with Enter (the traditional
#      insert, no choices), a 1 x 1 bin on cell (2, 1), a 2 x 1 bin turned
#      90 degrees refused on (2, 0) and (2, 1) (one is taken) and placed on
#      (0, 0) and (0, 1), a 1 x 1 bin on (1, 0); saved, the bins where their
#      cells are.
#      With MITCAD_GRIDFINITY_LIBRARY (a library of a ready-made model from
#      the test files, its grid and foot frames defined) its foot part on
#      its grid part too; skipped without.
#   2. Free on a face: a lamp clicked onto the plate's top: a planar joint;
#      moving the plate carries it, a drag slides it on the face.
#   3. An own screw: Joint Origin on its head's underside made a mount frame
#      (Mount Frame: into a hole, turns and slides), published to a library
#      of one's own, placed into the plate's hole: coaxial, its head on the
#      plate, a cylindrical joint.
#   4. A drop-in: a lamp dropped in with Enter, then joined with Joint by its
#      mount frame (the browser's row of its joint origin) onto a plate's
#      top, in a design of its own.
#   5. A selection made before (mitcad#1197): a hole's rim selected, then
#      Insert from Library > ISO 4762 of the parts library's generator
#      (its mount frame `seat` under the head): Place Component starts
#      with the rim as Place On, the screw shown in the hole; Enter places
#      it, head on the face, a rigid joint to the hole; no other screw at
#      the origin.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mount-test.sh
# check-all sources: core/model/src/document/mounts.rs core/model-api/src/mounts.rs core/vcs/src/library tools/cli
# check-all sources: tools/libraries/make-fastener-library.py tools/libraries/library_previews.py

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-mount.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
export GIT_AUTHOR_NAME="Mount Tester" GIT_AUTHOR_EMAIL=tester@example.invalid
export GIT_COMMITTER_NAME="Mount Tester" GIT_COMMITTER_EMAIL=tester@example.invalid
LIB=$WORK/mounts
MINE=$WORK/screws
SCENE=$WORK/scene.mitcad
SCREW=$WORK/screw.mitcad
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# design name 'commands...': a design from mitcad-cli's commands.
design() {
  printf '[%s]\n' "$2" > "$WORK/$1.json"
  "$CLI" run "$WORK/$1.json" --save "$WORK/$1.mitcad" > "$WORK/$1.log" 2>&1 ||
    { cat "$WORK/$1.log"; ui_fail "mitcad-cli: $1"; }
}
box() { # corner-x corner-y length width height
  printf '{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [%s, %s], "length": %s, "width": %s, "height": %s, "operation": "new_body"}}' "$@"
}
frame() { # origin normal mount
  printf '{"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": {"origin": %s, "normal": %s, "x_axis": [1, 0, 0]}, "mount": %s}}' "$@"
}

# The baseplate: six cells of 42 x 42 x 5, its grid on their top.
design baseplate "$(box 0 0 42 42 5), $(box 42 0 42 42 5), $(box 84 0 42 42 5),
  $(box 0 42 42 42 5), $(box 42 42 42 42 5), $(box 84 42 42 42 5),
  $(frame '[0, 0, 5]' '[0, 0, 1]' '{"name": "grid", "kind": "grid", "pitch": [42, 42], "cells": [3, 2]}')"
# The bins: 1 x 1 and 2 x 1 cells, 21 high, their feet in the middle of their bottoms.
design bin1x1 "$(box 0 0 42 42 21),
  $(frame '[21, 21, 0]' '[0, 0, -1]' '{"name": "foot", "kind": "foot", "footprint": [1, 1], "pitch": [42, 42]}')"
design bin2x1 "$(box 0 0 84 42 21),
  $(frame '[42, 21, 0]' '[0, 0, -1]' '{"name": "foot", "kind": "foot", "footprint": [2, 1], "pitch": [42, 42]}')"
# The lamp: 10 x 10 x 5, free on a face from the middle of its bottom.
design lamp "$(box 0 0 10 10 5),
  $(frame '[5, 5, 0]' '[0, 0, -1]' '{"name": "base", "kind": "face", "joint": "planar"}')"
# The screw, no frame yet: a head of 14 mm under z = 0 (3 high) and a shank
# of 8 mm, 20 long, joined (F1.b0; the head's top face F1:top, where it bears).
design screw '{"cmd": "add_feature", "def": {"type": "cylinder", "plane": {"origin": [0, 0, -3], "normal": [0, 0, 1]},
    "center": [0, 0], "diameter": 14, "height": 3, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 8, "height": 20,
    "operation": "join", "participants": ["F1.b0"]}}'
# The scene: Plate:1, 100 x 60 x 10 at x = 200 with a hole of 10 mm at
# (270, 30) through it (F1.b0: its top F1:top, the hole's wall F2:side0).
design scene '{"cmd": "create_component", "name": "Plate"},
  '"$(box 200 0 100 60 10)"',
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [270, 30], "diameter": 10, "height": 10,
    "operation": "cut", "participants": ["F1.b0"]}},
  {"cmd": "activate_component", "component": "Root"}'
# The plate a dropped-in lamp is joined to (4.): Plate:1, 100 x 60 x 10 at
# (20, 20), its top's middle at (70, 50, 10).
JOIN=$WORK/join.mitcad
design join '{"cmd": "create_component", "name": "Plate"}, '"$(box 20 20 100 60 10)"',
  {"cmd": "activate_component", "component": "Root"}'
# The design save and check work on.
OPEN=$SCENE

# The library of the synthetic parts: their frames come from the designs.
"$CLI" library init "$LIB" --id test-mounts --name "Test mounts" --license CC0-1.0 --author "Mount Tester" \
  > /dev/null || ui_fail "library init"
for part in "baseplate Baseplate baseplates" "bin1x1 Bin_1x1 bins" "bin2x1 Bin_2x1 bins" "lamp Lamp lamps"; do
  read -r id name category <<< "$part"
  name=${name//_/ }
  "$CLI" library add "$LIB" "$WORK/$id.mitcad" --id "$id" --name "$name" --designation "$name" \
    --category "$category" > /dev/null || ui_fail "library add $id"
done
"$CLI" library check "$LIB" > /dev/null || ui_fail "the library has errors"
grep -q '"kind": "foot"' "$LIB/mitcad-library.json" || ui_fail "library add wrote no mounts"
g "$LIB" add -A
g "$LIB" commit -q -m "Mount parts"
g "$LIB" tag v1.0.0
echo "ok   the library of the synthetic parts, its mounts from the designs"

# The parts library's socket head cap screws, M5, as its generator makes
# them (5.): their mount frame `seat` under the head, in the manifest.
PARTS=$WORK/parts
python3 "$ROOT/tools/libraries/make-fastener-library.py" --cli "$CLI" --out "$PARTS" --sizes M5 \
  --standards iso4762 --version 1.0.0 --no-render --commit > "$WORK/parts.log" 2>&1 ||
  { cat "$WORK/parts.log"; ui_fail "the fastener generator failed"; }
"$CLI" library check "$PARTS" > /dev/null || ui_fail "the generated fasteners have errors"
grep -q '"name": "seat"' "$PARTS/mitcad-library.json" || ui_fail "the generated screw lists no mount frame"
echo "ok   the generated ISO 4762 lists its mount frame"
# The plate with a hole of 5.5 mm (5.): Plate:1, 100 x 60 x 10 at (20,
# 20), the hole at (70, 50) through it (F1.b0: its top F1:top, the
# hole's wall F2:side0, the rim their edge).
HOLED=$WORK/holed.mitcad
design holed '{"cmd": "create_component", "name": "Plate"}, '"$(box 20 20 100 60 10)"',
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [70, 50], "diameter": 5.5, "height": 10,
    "operation": "cut", "participants": ["F1.b0"]}},
  {"cmd": "activate_component", "component": "Root"}'

# check "description" expectations...: the saved design opened by mitcad-cli.
check() {
  local description=$1; shift
  local IFS=,
  echo "[$*]" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$OPEN" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$description"; }
  echo "ok   $description"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $OPEN" "saved"
}

# insert "search" id [copy]: Insert from Library, the part searched for (its
# id), linked (or a copy), Insert: Place Part starts for a part with frames.
insert() {
  ui_mark
  ui_step "Insert from Library"            ui_command "Insert from Library"
  ui_focus_dialog '^Browse Components$'
  ui_step "search $1"                      xdotool type --delay 40 "$1"
  ui_expect_new "Library search '$1': 1 components" "the search finds it"
  ui_expect_new "Library browser selected $2: version " "the part selected"
  if [ "${3:-}" = copy ]; then
    ui_step "a copy (Alt+P)"               ui_key alt+p
  fi
  ui_step "Insert (Alt+I)"                 ui_key alt+i
  ui_focus_main
  ui_expect_new "Command Place Component started" "Place Part started"
}

# The bodies a placement or a drop-in made, from its log line.
bodies_of() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1 | sed -n 's/.*; bodies \([^;]*\).*/\1/p'; }
at_of() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1 | sed -n 's/.* at (\([^)]*\)); bodies .*/\1/p'; }

ui_start_display
ui_start_app --open "$SCENE"
ui_step "fit (F6)"                         ui_key F6

echo "--- 0. Library Sources: the synthetic parts' library only"
ui_mark
ui_step "Library Sources (command search)" ui_command "Library Sources"
ui_focus_dialog '^Library Sources$'
ui_step "remove the defaults (Alt+R twice)" eval 'ui_key alt+r; ui_sync; ui_key alt+r'
ui_step "Add URL (Alt+A)"                  ui_key alt+a
ui_focus_dialog '^Add Library$'
ui_step "the folder"                       eval 'ui_type_text "$LIB"; ui_key Return'
ui_focus_dialog '^Library Sources$'
ui_expect_new "Library source added: $LIB" "the source added"
ui_step "Fetch All (Alt+F)"                ui_key alt+f
ui_expect_new "Library fetched: library Test mounts (test-mounts) from $LIB" "fetched" 30
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main

echo "--- 1. Gridfinity: a baseplate dropped in, bins on its cells"
insert "Baseplate" test-mounts/baseplate copy
ui_expect_new "Insert from Library: test-mounts/baseplate has mount frames grid (a grid, fixed): Place Part" \
  "a part with frames: Place Part"
ui_expect_new "View hint Place Component: Click where Baseplate mounts" "the view says to click or press Enter"
ui_mark
ui_step "drop it in (Enter)"               ui_key Return
ui_expect_new "Inserted component Baseplate (copy) from test-mounts v1.0.0 (" "dropped in as before" 30
ui_expect_new "mount frames grid (grid)" "its grid is a frame of its component"
read -r -a CELLS <<< "$(bodies_of 'Inserted component Baseplate' | tr -d ',')"
[ "${#CELLS[@]}" = 6 ] || ui_fail "the baseplate's six cells: ${CELLS[*]}"
# cell body: a face of a cell to click, as the last "Pick places:" logs
# them: its top, else its front (side0, -y), else any (a click anywhere on
# a cell takes the cell; a bin standing on it hides its top).
cell() {
  local start places face
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  places=$(tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick face $1/" || true)
  for face in top side0; do
    if grep -qF "Pick face $1/${1%.b0}:$face at " <<< "$places"; then
      echo "face $1/${1%.b0}:$face"
      return
    fi
  done
  echo "face $1/"
}
ui_step "close (Escape)"                   ui_key Escape
# The scene was fitted before the baseplate came in at the origin, out of
# the view: fitted again, its cells' faces are logged where they are.
ui_step "fit (F6)"                         ui_key F6

# The view looks from the front right (-y, +x): the bins stand where they
# hide no cell clicked later. A taken cell is under its bin, out of sight;
# the long bin, turned, is refused on the free cell (2, 0) in front of the
# first bin, as it would cover (2, 1) too. A foot takes faces only: the
# planes of the frames of the bins placed lie in the cells' tops.
insert "Bin 1x1" test-mounts/bin1x1
ui_expect_new "Place Part Bin 1x1 from test-mounts v1.0.0 (" "Place Part with the bin"
ui_expect_new ": frames foot (on a grid, fixed)" "the bin's foot frame"
ui_mark
ui_step "click cell (2, 1)"                ui_click_pick "$(cell "${CELLS[5]}")"
ui_expect_new "Inspect Place Component: place part Bin 1x1: frame foot (foot), rigid joint, on Baseplate:1, cell (2, 1)" \
  "the foot snaps to the cell clicked" 20
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed part Bin 1x1 (linked) from test-mounts v1.0.0 (" "the bin placed" 30
ui_expect_new "by its frame foot on Baseplate:1, cell (2, 1): rigid joint" "on its cell, a rigid joint"
BIN1=$(bodies_of 'Placed part Bin 1x1')
ui_step "close (Escape)"                   ui_key Escape

insert "Bin 2x1" test-mounts/bin2x1
ui_step "Rotate 90"                        ui_type_in "Panel Place Component input rotate" "90"
ui_mark
ui_step "click cell (2, 0)"                ui_click_pick "$(cell "${CELLS[2]}")"
ui_expect_new "the cell (2, 1) of Baseplate:1's grid is taken by Bin 1x1:1" \
  "a taken cell is refused with who stands there" 20
ui_mark
ui_step "click cell (0, 1)"                ui_click_pick "$(cell "${CELLS[3]}")"
ui_expect_new "on Baseplate:1, cells (0, 0) and (0, 1)" "turned, it takes two cells along Y" 20
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed part Bin 2x1 (linked) from test-mounts" "the long bin placed" 30
BIN2=$(bodies_of 'Placed part Bin 2x1')
ui_step "close (Escape)"                   ui_key Escape

insert "Bin 1x1" test-mounts/bin1x1
ui_mark
ui_step "click cell (1, 0)"                ui_click_pick "$(cell "${CELLS[1]}")"
ui_expect_new "on Baseplate:1, cell (1, 0)" "a free cell" 20
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed part Bin 1x1 (linked) from test-mounts" "the second bin placed" 30
BIN3=$(bodies_of 'Placed part Bin 1x1')
ui_step "close (Escape)"                   ui_key Escape
save
check "the bins stand on their cells, the long one turned" \
  "{\"expect\": {\"occurrence\": \"Bin 1x1:1\", \"body\": \"$BIN1\", \"center\": [105, 63, 15.5], \"tolerance\": 1e-6}}" \
  "{\"expect\": {\"occurrence\": \"Bin 2x1:1\", \"body\": \"$BIN2\", \"center\": [21, 42, 15.5], \"tolerance\": 1e-6}}" \
  "{\"expect\": {\"occurrence\": \"Bin 1x1 (1):1\", \"body\": \"$BIN3\", \"center\": [63, 21, 15.5], \"tolerance\": 1e-6}}" \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"kind": "rigid", "solved": true}, {"kind": "rigid", "solved": true}, {"kind": "rigid", "solved": true}]}}}'

if [ -n "${MITCAD_GRIDFINITY_LIBRARY:-}" ] && [ -f "$MITCAD_GRIDFINITY_LIBRARY/mitcad-library.json" ]; then
  echo "--- 1b. A ready-made Gridfinity model of the test files"
  # Its parts with a grid and with a foot, by the manifest's mounts.
  read -r GRID_NAME FOOT_NAME <<< "$(python3 -I -c '
import json, sys
m = json.load(open(sys.argv[1]))
def first(kind):
    return next(c["name"] for c in m["components"] if any(f.get("kind") == kind for f in c.get("mounts", [])))
print(first("grid").replace(" ", "_"), first("foot").replace(" ", "_"))' "$MITCAD_GRIDFINITY_LIBRARY/mitcad-library.json")"
  GRID_NAME=${GRID_NAME//_/ }
  FOOT_NAME=${FOOT_NAME//_/ }
  ui_mark
  ui_step "Library Sources (command search)" ui_command "Library Sources"
  ui_focus_dialog '^Library Sources$'
  ui_step "Add URL (Alt+A)"                ui_key alt+a
  ui_focus_dialog '^Add Library$'
  ui_step "the test files' library"        eval 'ui_type_text "$MITCAD_GRIDFINITY_LIBRARY"; ui_key Return'
  ui_focus_dialog '^Library Sources$'
  ui_step "Fetch All (Alt+F)"              ui_key alt+f
  ui_expect_new "Library fetched: library " "the model's library fetched" 60
  ui_step "close (Alt+C)"                  ui_key alt+c
  ui_focus_main
  ui_mark
  ui_step "Insert from Library"            ui_command "Insert from Library"
  ui_focus_dialog '^Browse Components$'
  ui_step "search $GRID_NAME"              xdotool type --delay 40 "$GRID_NAME"
  ui_step "a copy (Alt+P), Insert (Alt+I)" eval 'ui_key alt+p; ui_sync; ui_key alt+i'
  ui_focus_main
  ui_expect_new "Command Place Component started" "Place Part with the model's baseplate"
  ui_mark
  ui_step "drop it in (Enter)"             ui_key Return
  ui_expect_new "Inserted component $GRID_NAME (copy)" "the model's baseplate dropped in" 60
  GRID_BODY=$(bodies_of "Inserted component $GRID_NAME" | cut -d, -f1)
  ui_step "close (Escape)"                 ui_key Escape
  ui_mark
  ui_step "Insert from Library"            ui_command "Insert from Library"
  ui_focus_dialog '^Browse Components$'
  ui_step "search $FOOT_NAME"              xdotool type --delay 40 "$FOOT_NAME"
  ui_step "Insert (Alt+I)"                 ui_key alt+i
  ui_focus_main
  ui_expect_new "Command Place Component started" "Place Part with the model's bin"
  ui_mark
  ui_step "click the baseplate"            ui_click_pick "face $GRID_BODY/"
  ui_expect_new "Inspect Place Component: place part $FOOT_NAME: frame " "the model's bin on the model's grid" 60
  ui_mark
  ui_step "Place (Enter)"                  ui_key Return
  ui_expect_new "Placed part $FOOT_NAME (linked)" "the model's bin placed" 60
  ui_step "close (Escape)"                 ui_key Escape
else
  echo "skip the ready-made Gridfinity model (MITCAD_GRIDFINITY_LIBRARY is not set)"
fi

echo "--- 2. Free on a face: a lamp on the plate's top"
insert "Lamp" test-mounts/lamp
ui_expect_new ": frames base (on a face, free on the face)" "the lamp's frame"
ui_mark
# Shift + click: the pick and Place in one (as BUILD's, mitcad#451).
ui_step "Shift+click on the plate's top"   eval 'xdotool keydown shift; ui_click_pick "face F1.b0/F1:top"; xdotool keyup shift'
ui_expect_new "Inspect Place Component: place part Lamp: frame base (face), planar joint, on Plate:1" \
  "the lamp lies where clicked, free on the face" 20
ui_expect_new "Placed part Lamp (linked) from test-mounts" "Shift+click placed it at once" 30
LAMP=$(bodies_of 'Placed part Lamp')
read -r LX LY LZ <<< "$(at_of 'Placed part Lamp' | tr -d ',')"
[ "$LZ" = 10 ] || ui_fail "the lamp's frame is on the plate's top (z 10), not $LZ"
ui_step "close (Escape)"                   ui_key Escape
save
lx=$(awk -v v="$LX" 'BEGIN { print v + 10 }')
ly=$(awk -v v="$LY" 'BEGIN { print v + 5 }')
# The click's place is logged to 0.001 mm.
check "the lamp is free on the plate's top: carried by the plate, slid by a drag" \
  "{\"expect\": {\"occurrence\": \"Lamp:1\", \"body\": \"$LAMP\", \"center\": [$LX, $LY, 12.5], \"tolerance\": 1e-3}}" \
  '{"expect": {"query": {"query": "joint_dof", "occurrence": "Lamp:1"}, "result": {"dof": 3}}}' \
  '{"cmd": "set_occurrence_transform", "occurrence": "Plate:1", "transform": {"translation": [0, 0, 50]}}' \
  "{\"expect\": {\"occurrence\": \"Lamp:1\", \"body\": \"$LAMP\", \"center\": [$LX, $LY, 62.5], \"tolerance\": 1e-3}}" \
  '{"cmd": "ground_occurrence", "occurrence": "Plate:1"}' \
  "{\"cmd\": \"drag_occurrence\", \"occurrence\": \"Lamp:1\", \"point\": [$LX, $LY, 60], \"target\": [$lx, $ly, 90]}" \
  "{\"expect\": {\"occurrence\": \"Lamp:1\", \"body\": \"$LAMP\", \"center\": [$lx, $ly, 62.5], \"tolerance\": 1e-2}}"

echo "--- 3. An own screw: a mount frame under its head, published, placed into the hole"
ui_stop_app
ui_start_app --open "$SCREW"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "Joint Origin (command search)"    ui_command "Joint Origin"
ui_expect_new "Command Joint Origin started" "Joint Origin started"
ui_step "the head's top, where it bears"   ui_click_pick "face F1.b0/F1:top"
ui_step "Mount Frame on"                   ui_click_logged "Panel Joint Origin input mount"
ui_step "its name"                         ui_type_in "Panel Joint Origin input mount_name" "head"
ui_step "into a hole"                      ui_choose "Panel Joint Origin input mount_kind" 1
ui_step "turns and slides"                 ui_choose "Panel Joint Origin input mount_joint" 2
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added joint origin JointOrigin1: origin (0, 0, 0), normal (0, 0, 1), mount head (shaft, cylindrical)" \
  "a mount frame under the head, z along the shank"
ui_mark
ui_step "Publish to Library"               ui_command "Publish to Library"
ui_focus_dialog '^Publish to Library$'
ui_step "the folder"                       eval 'ui_key alt+f; ui_type_text "$MINE"'
ui_step "the library's id"                 eval 'ui_key alt+i; ui_type_text "my-screws"'
ui_step "the library's name"               eval 'ui_key alt+n; ui_type_text "My screws"'
ui_step "the author"                       eval 'ui_key alt+a; ui_type_text "Mount Tester"'
ui_step "the component's id"               eval 'ui_key alt+d; ui_type_text "screw-m8"'
ui_step "its name"                         eval 'ui_key alt+m; ui_type_text "Screw M8"'
ui_step "its category"                     eval 'ui_key alt+g; ui_type_text "screws"'
ui_step "Add to Library (Alt+L)"           ui_key alt+l
ui_expect_new "Publish: Added screw-m8 to the library, recorded as version " "published and recorded" 30
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main
"$CLI" library check "$MINE" > /dev/null || ui_fail "the published library has errors"
grep -q '"kind": "shaft"' "$MINE/mitcad-library.json" || ui_fail "the published screw lists no shaft frame"
echo "ok   the published screw lists its frame"

ui_stop_app
ui_start_app --open "$SCENE"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "Library Sources (command search)" ui_command "Library Sources"
ui_focus_dialog '^Library Sources$'
ui_step "Add URL (Alt+A)"                  ui_key alt+a
ui_focus_dialog '^Add Library$'
ui_step "the own library"                  eval 'ui_type_text "$MINE"; ui_key Return'
ui_focus_dialog '^Library Sources$'
ui_step "Fetch All (Alt+F)"                ui_key alt+f
ui_expect_new "Library fetched: library My screws (my-screws) from $MINE" "the own library fetched" 30
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Browse Components$'
ui_step "search Screw M8"                  xdotool type --delay 40 "Screw M8"
ui_expect_new "Library browser selected my-screws/screw-m8" "the own screw"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "frames head (into a hole, turns and slides)" "Place Part with the screw's frame"
ui_expect_new "Command Place Component started" "Place Part started"
ui_mark
# The hole's wall (F2's face), else its rim on the plate's top: whichever
# the view shows (the wall's middle can be out of sight in the hole).
hole_pick() {
  local start
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | sed -n 's/^Pick \(.*\) at [0-9]*,[0-9]*$/\1/p' |
    grep -E '^(face F1\.b0/F2:|edge F1\.b0/E\{.*F1:top.*F2:|edge F1\.b0/E\{.*F2:.*F1:top)' | head -1
}
HOLE=$(hole_pick)
[ -n "$HOLE" ] || ui_fail "neither the hole's wall nor its rim among the pick places"
ui_step "click the hole ($HOLE)"           ui_click_pick "$HOLE at"
ui_expect_new "Inspect Place Component: place part Screw M8: frame head (shaft), cylindrical joint, on Plate:1" \
  "the screw goes into the hole" 20
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed part Screw M8 (linked) from my-screws " "the screw placed" 30
ui_expect_new "cylindrical joint Joint" "a cylindrical joint"
ui_expect_new "at (270, 30, 10)" "its head on the hole's rim, coaxial"
SCREW_BODY=$(bodies_of 'Placed part Screw M8')
ui_step "close (Escape)"                   ui_key Escape
save
# The head (14 x 3) on the plate's top, the shank (8 x 20) down through the
# hole: the centre of mass on the hole's axis, 10 - 6.380086 mm up.
check "the screw is coaxial with the hole, its head on the plate" \
  "{\"expect\": {\"occurrence\": \"Screw M8:1\", \"body\": \"$SCREW_BODY\", \"center\": [270, 30, 3.619914], \"tolerance\": 1e-3}}"

echo "--- 4. A drop-in joined afterwards with Joint by its mount frame"
# A design of its own: in the scene the plate's top is under the first
# lamp and its sides are thin in a small view, so that Joint may find none
# of its faces; here the plate's top is in sight.
ui_stop_app
OPEN=$JOIN
ui_start_app --open "$JOIN"
ui_step "fit (F6)"                         ui_key F6
insert "Lamp" test-mounts/lamp
ui_mark
ui_step "drop it in (Enter)"               ui_key Return
ui_expect_new "Inserted component Lamp (linked) from test-mounts" "the lamp dropped in, no choices" 30
ui_expect_new "mount frames base (face)" "its frame is a joint origin of its component"
DROPPED=$(bodies_of 'Inserted component Lamp')
ui_step "close (Escape)"                   ui_key Escape
ui_mark
ui_step "joint (J)"                        ui_key j
ui_expect_new "Command Joint started" "Joint started"
ui_step "the lamp's frame in the browser"  ui_click_logged "Browser Root/Lamp:1/Joints/mount:base"
ui_expect_log "Joint Origin A: 1 " "origin A is the mount frame"
ui_step "the plate's top"                  ui_click_pick "face F1.b0/F1:top"
ui_step "flip"                             ui_click_logged "Panel Joint input flip"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added joint Joint" "joined by its mount frame"
save
# Its frame (the middle of its bottom, z out of it) flipped onto the top's
# middle (70, 50, 10): the lamp stands on it.
check "the dropped-in lamp stands on the plate's top, by its mount frame" \
  "{\"expect\": {\"occurrence\": \"Lamp:1\", \"body\": \"$DROPPED\", \"center\": [70, 50, 12.5], \"tolerance\": 1e-6}}"

echo "--- 5. A hole's rim selected before Insert from Library: ISO 4762 placed in the hole (mitcad#1197)"
ui_stop_app
OPEN=$HOLED
ui_start_app --open "$HOLED"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "Library Sources (command search)" ui_command "Library Sources"
ui_focus_dialog '^Library Sources$'
ui_step "Add URL (Alt+A)"                  ui_key alt+a
ui_focus_dialog '^Add Library$'
ui_step "the generated fasteners"          eval 'ui_type_text "$PARTS"; ui_key Return'
ui_focus_dialog '^Library Sources$'
ui_expect_new "Library source added: $PARTS" "the source added"
ui_step "Fetch All (Alt+F)"                ui_key alt+f
ui_expect_new "Library fetched: library Mitcad parts (mitcad-parts) from $PARTS" "the fasteners fetched" 30
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main
# Where a click picks the hole's rim on the plate's top: Fillet logs its
# edges' places (an idle view logs none), cancelled before the click.
ui_mark
ui_step "Fillet (command search)"          ui_command "Fillet"
ui_expect_new "Pick places:" "where the edges are picked"
RIM=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Pick \(.*\) at [0-9]*,[0-9]*$/\1/p' |
  grep -E '^(edge F1\.b0/E\{.*F1:top.*F2:|edge F1\.b0/E\{.*F2:.*F1:top)' | head -1)
[ -n "$RIM" ] || ui_fail "the hole's rim on the plate's top is not among the pick places"
ui_step "cancel it (Esc)"                  ui_key Escape
ui_mark
ui_step "select the rim ($RIM)"            ui_click_pick "$RIM at"
ui_expect_new "Selected: 1 edge" "the hole's rim selected"
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Browse Components$'
ui_step "search 4762"                      xdotool type --delay 40 "4762"
ui_expect_new "Library search '4762': 1 components" "the search finds the screw"
ui_expect_new "Library browser selected mitcad-parts/iso4762: version v1.0.0 (" "the generated screw"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Insert from Library: mitcad-parts/iso4762 has mount frames seat (into a hole, fixed): Place Part" \
  "the screw has its mount frame: Place Component"
ui_expect_new "Command Place Component started" "Place Component started"
ui_expect_new "Place Component: Place On edge" "the rim selected before is Place On"
ui_expect_new ": frame seat (shaft), rigid joint, on Plate:1" "the screw shown in the hole at once, no click" 20
no_free=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c "^Inserted component " || true)
[ "$no_free" = 0 ] || ui_fail "a component was dropped in at the origin"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed part " "the screw placed" 30
ui_expect_new "by its frame seat on Plate:1: rigid joint Joint" "a rigid joint to the hole (logged)"
ui_expect_new "at (70, 50, 10)" "its head on the plate's top, on the hole's axis"
ui_step "close (Escape)"                   ui_key Escape
save
check "one screw, fixed in the hole by its joint: none free at the origin" \
  '{"expect": {"bodies": 2}}' \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"kind": "rigid", "solved": true}]}}}' \
  '{"expect": {"query": {"query": "joint_dof", "occurrence": "ISO 4762 M5x16:1"}, "result": {"dof": 0}}}'

ui_stop_app
ui_finish "mount frames of any part and Place Part"

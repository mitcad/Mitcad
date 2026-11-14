#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The pictures of the user guide's Rib and Web sections (mitcad#1754): a
# one-off run that makes them, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). It builds two small designs with
# mitcad-cli, opens each, and saves the 3D view from the isometric view,
# fitted, each picture checked to be under 200 kB:
#   <dir>/rib.png  a 100 x 40 x 10 base with two bosses of radius 10, one
#                  60 mm and one 30 mm high, and a 4 mm rib on the middle
#                  plane: a slanted line between the bosses, grown
#                  straight down to the base (Direction Sketch Y) with
#                  Extend Ends (the model of cli.f3_rib_direction)
#   <dir>/web.png  an 80 x 60 x 4 plate with a grid of webs on its top:
#                  three lines along X and three along Y, 2 mm thick and
#                  10 mm deep
# The pictures go into docs/images/rib-web/, where the guide expects them:
# run it into a folder under build/ and copy the two PNG files. The
# designs, the commands and the app's logs stay in <dir>/work.
#
# What to look for: the whole design in each picture; in rib.png the rib
# under the slanted line, filling up to both bosses' round faces and down
# to the base; in web.png the six walls crossing, one body with the plate.
#
# Runs headless on Xvfb (see ui-test-lib.sh), in the Floating layout (the
# default).
# Usage: tools/ui-rib-web-guide-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-rib-web-guide-look.sh <dir>}
mkdir -p "$DIR/work"
DIR=$(cd "$DIR" && pwd)
WORK=$DIR/work
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$WORK/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# The largest picture the guide takes (bytes).
MAX_BYTES=200000
TOO_LARGE=""

# The rib: the base (Sketch1 F1, Extrude1 F2), the bosses (F3 to F6), the
# middle plane y = 20 (Plane1 F7, whose sketch y points down), the slanted
# line (Sketch4 F8) and Rib1 (F9).
cat > "$WORK/rib.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 100, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 20], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "start": {"type": "offset", "offset": 10}, "extent": {"type": "distance", "distance": 50},
    "operation": "join", "participants": ["F2.b0"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [80, 20], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "start": {"type": "offset", "offset": 10}, "extent": {"type": "distance", "distance": 20},
    "operation": "join", "participants": ["F2.b0"]}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xz", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F7"},
  {"cmd": "sketch.add_line", "sketch": "F8", "start": [30, -50], "end": [70, -30]},
  {"cmd": "add_feature", "def": {"type": "rib", "curves": {"sketch": "F8", "curves": ["c1"]}, "thickness": 4,
    "extent": {"type": "to_next"}, "direction": "sketch_y", "extend_ends": true}},
  {"expect": {"feature": "Rib1", "status": "ok"}},
  {"expect": {"bodies": 1}}
]
EOF

# The web: the plate (Sketch1 F1, Extrude1 F2), the plane on its top
# (Plane1 F3), the grid (Sketch2 F4: each line takes three ids, its own and
# its two points) and Web1 (F5).
cat > "$WORK/web.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 80, "height": 60},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 4}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 4}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [0, 15], "end": [80, 15]},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [0, 30], "end": [80, 30]},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [0, 45], "end": [80, 45]},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [20, 0], "end": [20, 60]},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [40, 0], "end": [40, 60]},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [60, 0], "end": [60, 60]},
  {"cmd": "add_feature", "def": {"type": "web",
    "curves": {"sketch": "F4", "curves": ["c1", "c4", "c7", "c10", "c13", "c16"]}, "thickness": 2,
    "extent": {"type": "depth", "depth": 10}, "participants": ["F2.b0"]}},
  {"expect": {"feature": "Web1", "status": "ok"}},
  {"expect": {"bodies": 1}}
]
EOF

for name in rib web; do
  "$CLI" run "$WORK/$name.json" --save "$WORK/$name.mitcad" > "$WORK/$name-cli.log" 2>&1 ||
    { cat "$WORK/$name-cli.log"; ui_fail "mitcad-cli (the $name design)"; }
done

# away: the mouse off the window (no hover, no tooltip).
away() { xdotool mousemove 5000 5000; }

# shot name: the 3D view, once it has settled, as <dir>/<name>.png; noted
# when it is too large for the guide.
shot() {
  local dump="$WORK/screen.xwd" bytes
  local -a from to
  away
  ui_wait_idle
  ui_sync
  sleep 2
  xwd -root -silent > "$dump"
  read -r -a from <<< "$(ui_view_at 0 0)"
  read -r -a to <<< "$(ui_view_at 100 100)"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1.png" \
    "${from[0]}" "${from[1]}" $((to[0] - from[0])) $((to[1] - from[1]))
  bytes=$(stat -c %s "$DIR/$1.png")
  if [ "$bytes" -gt "$MAX_BYTES" ]; then
    TOO_LARGE="$TOO_LARGE $1.png ($bytes bytes)"
    echo "LARGE $DIR/$1.png: $bytes bytes"
  else
    echo "ok   $DIR/$1.png ($bytes bytes)"
  fi
}

ui_start_display
for name in rib web; do
  echo "--- The $name"
  ui_start_app --open "$WORK/$name.mitcad" --no-native-dialogs --chrome=floating
  ui_expect_log "Browser Root at" "the design is open"
  grep -q "Recompute failed\|Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"
  ui_step "isometric view (search)"        ui_command "Isometric View"
  ui_step "fit (F6)"                       ui_key F6
  shot "$name"
  cp -f "$UI_LOG" "$WORK/$name-look.log"
  ui_stop_app
done

[ -z "$TOO_LARGE" ] || ui_fail "pictures over $MAX_BYTES bytes:$TOO_LARGE"
ui_finish "Rib and Web guide pictures in $DIR"

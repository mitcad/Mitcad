#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Hole preview By Pick (mitcad#1743): a one-off visual check, not a
# recurring test (its name is not ui-*-test.sh, so no check picks it up).
# The design of tools/ui-hole-pick-test.sh: a 60 x 40 x 20 block in the
# root and Bar:1, a 30 x 20 x 10 block turned 90 degrees about Z and moved
# by (100, 0, 0). Hole, a click on the root block's top face: the picture
# <dir>/hole-on-face.png (the hole's tool see-through red over the block,
# its handles at the hole); a click on Bar's top face: <dir>/hole-on-component.png
# (the hole on Bar, its handles there, not where Bar's own coordinates
# would put them). The design is <dir>/look.mitcad, the app's log
# <dir>/look.log.
#
# Before and after a change: run it once with the build before it, once
# with the build after it, each into its own folder:
#   UI_APP=<build before>/app/mitcad UI_CLI=<build before>/tools/cli/mitcad-cli \
#     tools/ui-hole-preview-look.sh <dir>/before
#   tools/ui-hole-preview-look.sh <dir>/after
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hole-preview-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-hole-preview-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$DIR/look.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "create_component", "name": "Bar", "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$DIR/look.json" --save "$DIR/look.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the design)"; }

# picture name: the mouse off the window (no hover, no tooltip), drawn.
picture() {
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$1"
  echo "ok   $DIR/$1"
}

ui_start_display
ui_start_app --open "$DIR/look.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the design shown"
ui_mark
ui_step "hole (H)"                         ui_key h
ui_expect_new "Command Hole started" "hole started"
ui_expect_new "Pick places:" "the picks are logged"
ui_step "click the top face"               ui_click_pick "face F2.b0/F2:end($R)"
ui_expect_new "Place Hole: face face F2:end(" "on the block's top face"
ui_expect_new "Preview Hole: ok" "previewed" 30
picture hole-on-face.png
ui_mark
ui_step "click Bar's top face"             ui_click_pick "face F4.b0/F4:end($R)"
ui_expect_new "Place Hole: face face F4:end(" "on Bar's top face"
# Previewed, or refused (a build before mitcad#1743 left the hole in the
# root and failed it): the picture shows either.
for _ in $(seq 1 60); do
  grep -qE "^Preview Hole: (ok|failed)" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && break
  sleep 0.5
done
grep -E "^Preview Hole: (ok|failed)" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") | tail -1
picture hole-on-component.png
ui_step "Esc (cancel)"                     ui_key Escape
ui_crashed && ui_fail "the application crashed"
ui_finish "Hole preview pictures in $DIR"

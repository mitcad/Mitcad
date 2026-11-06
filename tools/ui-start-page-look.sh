#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125): a one-off visual
# check, not a recurring test (its name is not ui-*-test.sh, so no check
# picks it up). Four designs that mitcad-cli makes (a plate, a block with a
# hole, a bracket, a ring) are opened, changed and saved in one start
# (each save writes its preview), the ring's file is then moved away (a
# missing file on the list), two projects made in the projects folder (Shop,
# Local, with plate, parts/gear and parts/sub/block; Team, Cloud through a
# bare repository, with bolts/bracket), and MitCad started again on the Start tab
# (MITCAD_START_PAGE=1), the bracket pinned with its star, on the light and
# the dark theme (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME), in the
# Floating layout (the default) and the Docked one:
#   <dir>/start-page-floating-light.png  <dir>/start-page-floating-dark.png
#   <dir>/start-page-docked-light.png    <dir>/start-page-docked-dark.png
#                                        the page in the tests' 1280 x 800
#   <dir>/start-page-<layout>-<theme>-1600.png  the same in 1600 x 1000
#   <dir>/start-page-<layout>-light-list.png     the compact list
#   <dir>/start-page-<layout>-light-solid.png    the Solid tab after it:
#                                        the cards (or docks) back
#   <dir>/start-page-<layout>-<theme>-project.png  the project browser
#                                        (mitcad#1301): Shop's folder parts
#   <dir>/start-page-<layout>-<theme>-search.png   the search "pla": Shop's
#                                        plate and the recent plate, each
#                                        with its folder
# and the app's logs as <dir>/<name>.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-start-page-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-start-page-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT
WORK=$DIR/designs
mkdir -p "$WORK"
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-60, -40], "width": 120, "height": 80},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 6}, "operation": "new_body"}}
]
EOF
cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 15], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}}
]
EOF
cat > "$WORK/bracket.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 8},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 8, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "join"}}
]
EOF
cat > "$WORK/ring.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 50},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 12}, "operation": "new_body"}}
]
EOF
for name in plate block bracket ring; do
  "$CLI" run "$WORK/$name.json" --save "$WORK/$name.mitcad" > "$WORK/cli-$name.log" 2>&1 ||
    { cat "$WORK/cli-$name.log"; ui_fail "mitcad-cli: $name"; }
done

# The fixture: each design opened, changed and saved, the ring first and
# the plate last (the newest first on the list).
cat > "$WORK/fixture.mitcad-ui" << EOF
# SPDX-License-Identifier: MIT
expect-ever 'View area' 30
wait-idle
set-setting documents/openInCurrentWindow true
EOF
first=1
for name in ring bracket block plate; do
  if [ "$first" = 0 ]; then
    printf "mark\nopen-file %s\nwait-log '^Opened .*%s[.]mitcad\$' 60\nwait-idle\n" "$WORK/$name.mitcad" "$name" \
      >> "$WORK/fixture.mitcad-ui"
  fi
  first=0
  printf "mark\nmodel-command '{\"cmd\":\"add_parameter\",\"name\":\"Note\",\"value\":1}'\nwait-idle\n" \
    >> "$WORK/fixture.mitcad-ui"
  printf "command view.fit\nwait-idle\nsleep 500\ncommand file.save\nwait-log '^Design preview saved: %s[.]mitcad' 30\n" \
    "$name" >> "$WORK/fixture.mitcad-ui"
done

ui_start_display
export MITCAD_TEST_INPUT=$WORK/fixture.mitcad-ui MITCAD_TEST_STEP_MS=100
ui_start_app --ui=qt --no-mcp --open "$WORK/ring.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "the fixture's steps"; }
cp -f "$UI_LOG" "$DIR/fixture.log"
ui_expect_log "Design preview saved: plate.mitcad" "the four designs saved with their previews"
unset MITCAD_TEST_INPUT
# A missing file on the list.
mv -f "$WORK/ring.mitcad" "$WORK/ring-moved.mitcad"

# The projects of the browser (as tools/ui-start-page-test.sh makes them).
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
AUTHOR="Ada Tester <ada@example.invalid>"
export MITCAD_PROJECTS_DIR=$WORK/projects
SHOP=$MITCAD_PROJECTS_DIR/Shop
TEAM=$MITCAD_PROJECTS_DIR/Team
mkdir -p "$MITCAD_PROJECTS_DIR" "$WORK/remotes"
pcli() { "$CLI" "$@" >> "$WORK/cli-projects.log" 2>&1 || { cat "$WORK/cli-projects.log"; ui_fail "mitcad-cli $*"; }; }
pcli project init "$SHOP" --author "$AUTHOR"
mkdir -p "$SHOP/parts/sub" "$SHOP/drawings"
pcli run "$WORK/plate.json" --save "$SHOP/plate.mitcad"
pcli run "$WORK/block.json" --save "$SHOP/parts/gear.mitcad"
pcli run "$WORK/bracket.json" --save "$SHOP/parts/bracket.mitcad"
pcli run "$WORK/block.json" --save "$SHOP/parts/sub/block.mitcad"
for design in plate parts/gear parts/bracket parts/sub/block; do
  pcli version save "$SHOP/$design.mitcad" -m "Add $design" --author "$AUTHOR"
done
git init -q --bare "$WORK/remotes/team.git"
pcli project create "$TEAM" --author "$AUTHOR" --url "$WORK/remotes/team.git"
mkdir -p "$TEAM/bolts"
pcli run "$WORK/bracket.json" --save "$TEAM/bolts/bracket.mitcad"
pcli version save "$TEAM/bolts/bracket.mitcad" -m "Add the bracket" --author "$AUTHOR"
pcli project settings "$TEAM" --set '{"local": {"name": "Team"}}' --author "$AUTHOR"

# shot name: the mouse off the window (no hover, no tooltip), the screen
# saved.
shot() {
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

export MITCAD_START_PAGE=1
pinned=0
# picture layout theme: a start on the Start tab in the layout and theme,
# the screens saved.
picture() {
  local layout=$1 theme=$2 name=start-page-$1-$2
  export MITCAD_TEST_COLOR_SCHEME=$theme
  ui_start_app --no-native-dialogs --chrome="$layout"
  ui_expect_log "Start page shown" "$layout, $theme: the Start page"
  ui_expect_log "Start page card plate.mitcad at" "$layout, $theme: the cards"
  if [ "$pinned" = 0 ]; then
    ui_mark
    ui_step "the bracket's star"             ui_click_logged "Start page star bracket.mitcad"
    ui_expect_new "Start page pinned: bracket.mitcad" "the bracket pinned"
    pinned=1
  fi
  shot "$name"
  # The project browser: Shop's folder parts, then a search.
  ui_expect_log "Start page project Shop at" "$layout, $theme: the projects"
  ui_mark
  ui_step "the project Shop"               ui_click_logged "Start page project Shop"
  ui_expect_new "Start page place: Shop (" "Shop shown"
  ui_mark
  ui_step "its folder parts"               ui_click_logged "Start page folder Shop/parts/"
  ui_expect_new "Start page cards: Shop/parts/" "parts shown"
  shot "$name-project"
  ui_mark
  ui_step "the search"                     ui_click_logged "Start page search"
  xdotool type --delay 20 -- "pla"
  ui_expect_new "Start page search results: 2" "the two plates"
  shot "$name-search"
  ui_mark
  ui_step "the search cleared"             ui_key ctrl+a BackSpace
  ui_expect_new "Start page place: Shop/parts/" "parts again"
  ui_mark
  ui_step "Recent"                         ui_click_logged "Start page filter Recent"
  ui_expect_new "Start page filter: all" "Recent again"
  ui_mark
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 1000
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  shot "$name-1600"
  if [ "$theme" = light ]; then
    ui_mark
    ui_step "the compact list"             ui_click_logged "Start page view List"
    ui_expect_new "Start page view: list" "the list"
    shot "$name-list"
    ui_mark
    ui_step "the cards again"              ui_click_logged "Start page view Cards"
    ui_expect_new "Start page view: cards" "the cards"
    ui_mark
    ui_step "the Solid tab"                ui_click_logged "Ribbon tab SOLID"
    ui_expect_new "Start page hidden" "the page left"
    shot "$name-solid"
  fi
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
ui_finish "Start page pictures in $DIR"

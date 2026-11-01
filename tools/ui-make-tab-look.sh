#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Make tab (mitcad#1049): a one-off visual check, not a recurring test
# (its name is not ui-*-test.sh, so no check picks it up). A design
# mitcad-cli makes (a block whose process sets Walls 5 of its own, beside a
# second block; the design prints with Bench and Standard) is opened, the
# Make tab shown and Body2 selected in the Browser, and the screen saved
# with the tab's groups, the body's card on the right and the Browser's
# marker, on the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME), in the Floating layout (the default) and the
# Docked one:
#   <dir>/make-<layout>-<theme>.png        the Make tab, Body2's card
#   <dir>/make-<layout>-<theme>-menu.png   the Annotate group's menu open
# and the app's logs as <dir>/make-<layout>-<theme>.log. The native macOS
# pictures come from tools/ui-macos-make-tab-test.sh's screenshot on a Mac.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-make-tab-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-make-tab-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$DIR/make.mitcad"
cat > "$DIR/make.json" << EOF
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[-50,0],"width":40,"height":30},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F1","region":"$R"}],
    "extent":{"type":"distance","distance":10},"operation":"new_body"}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F3","corner":[0,0],"width":40,"height":30},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F3","region":"$R"}],
    "extent":{"type":"distance","distance":10},"operation":"new_body"}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"process","name":"Standard","values":{"layer_height":0.2,"perimeters":2}},
  {"cmd":"set_property","owner":"C0","set":"Print Printer","key":"profile","value":"bench"},
  {"cmd":"set_property","owner":"C0","set":"Print Process","key":"profile","value":"standard"},
  {"cmd":"set_property","owner":"F4.b0","set":"Print Process","key":"perimeters","value":5},
  {"cmd":"set_property","owner":"F4.b0","set":"Print Process","key":"layer_height","value":0.12}
]
EOF

# shot name: the mouse off the window (no hover, no tooltip), the screen
# saved.
shot() {
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

# picture layout theme: a start in the layout and theme, the Make tab with
# Body2's card, the screens saved.
picture() {
  local layout=$1 theme=$2 name=make-$1-$2
  export MITCAD_TEST_COLOR_SCHEME=$theme
  cp -f "$FILE" "$DIR/make-$layout-$theme.mitcad"
  ui_start_app --open "$DIR/make-$layout-$theme.mitcad" --no-native-dialogs --chrome="$layout"
  ui_expect_log "make-$layout-$theme.mitcad" "$layout, $theme: the design opened"
  ui_step "fit (F6)"                       ui_key F6
  ui_mark
  ui_step "the Make tab"                   ui_click_logged "Ribbon tab MAKE"
  ui_expect_new "Ribbon MAKE/LASER at" "$layout, $theme: the Make tab's groups"
  ui_expect_new "Browser Make markers: Root/Bodies/Body2: Own print settings: " "Body2 marked"
  ui_mark
  ui_step "select Body2"                   ui_click_logged "Browser Root/Bodies/Body2"
  ui_expect_new "Make card: Body2: " "$layout, $theme: the body's card"
  ui_expect_new "Make card value perimeters at" "its fields placed"
  shot "$name"
  ui_mark
  ui_step "the Annotate menu"              ui_click_logged "Ribbon MAKE/ANNOTATE"
  ui_expect_new "Ribbon menu MAKE/ANNOTATE: " "the menu open"
  ui_sync
  sleep 1
  ui_capture "$DIR/$name-menu.png"
  echo "ok   $DIR/$name-menu.png"
  ui_step "close it (Esc)"                 ui_key Escape
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

"$CLI" run "$DIR/make.json" --save "$FILE" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }
ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
ui_finish "Make tab pictures in $DIR"

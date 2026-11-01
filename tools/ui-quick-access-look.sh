#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The quick-access group (mitcad#1090): a one-off visual check, not a
# recurring test (its name is not ui-*-test.sh, so no check picks it up).
# The screen is saved with the group in the Floating layout's title row
# (after the workspace tabs) and the Docked toolbar (after its tabs),
# on the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME), with the default commands and View > Look At
# added (quickAccess/commands in the settings), on a saved plate whose Save
# is disabled:
#   <dir>/quick-access-floating-light.png  <dir>/quick-access-floating-dark.png
#   <dir>/quick-access-docked-light.png    <dir>/quick-access-docked-dark.png
#   <dir>/quick-access-menu-<layout>-light.png  the group's menu open, its
#                                          View submenu shown
#   <dir>/quick-access-folded-light.png    Floating in a 760 px wide window:
#                                          the group folded into its ⋮
# and the app's logs as <dir>/<name>.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-quick-access-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-quick-access-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
WORK=$(mktemp -d /tmp/mitcad-ui-quick-access-look.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
PLATE=$WORK/plate.mitcad
cat > "$WORK/plate.json" << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
JSON
"$CLI" run "$WORK/plate.json" --save "$PLATE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# chosen: the default commands and Look At in the settings, before each start.
chosen() {
  mkdir -p "$XDG_CONFIG_HOME/Mitcad"
  printf '[quickAccess]\ncommands="file.new_design,file.open,file.save,file.print,file.design_properties,tools.preferences,view.look_at"\n' \
    > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
}

# picture layout theme: a start in the layout and theme, the group checked
# in the log, the screen saved; on the light theme its menu too.
picture() {
  local layout=$1 theme=$2 name=quick-access-$1-$2
  chosen
  export MITCAD_TEST_COLOR_SCHEME=$theme
  ui_start_app --chrome="$layout" --no-native-dialogs --open "$PLATE"
  ui_expect_log "Quick access: " "$layout, $theme: the group"
  ui_expect_log "Quick access view.look_at at " "$layout, $theme: Look At's button"
  ui_save_state disabled "$layout, $theme: Save disabled"
  # The mouse off the window (no hover, no tooltip).
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$name.png"
  echo "ok   $DIR/$name.png"
  if [ "$theme" = light ]; then
    ui_mark
    ui_step "the group's menu"             ui_click_logged "Quick access menu"
    ui_expect_new "Context menu: " "$layout: the menu"
    # The View submenu open (Down to View, Right).
    ui_key Down Down Down Right
    ui_sync
    sleep 1
    ui_capture "$DIR/quick-access-menu-$layout-$theme.png"
    echo "ok   $DIR/quick-access-menu-$layout-$theme.png"
    ui_key Escape Escape
    if [ "$layout" = floating ]; then
      ui_mark
      ui_step "a narrow window"            xdotool windowsize "$UI_WINDOW" 760 800
      ui_expect_new "Quick access folded" "folded"
      xdotool mousemove 5000 5000
      ui_sync
      sleep 1
      ui_capture "$DIR/quick-access-folded-$theme.png"
      echo "ok   $DIR/quick-access-folded-$theme.png"
    fi
  fi
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
ui_finish "Quick-access group pictures in $DIR"

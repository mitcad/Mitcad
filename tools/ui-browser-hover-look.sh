#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The browser's hover highlight of a hidden part (mitcad#1022): a one-off
# visual check, not a recurring test (its name is not ui-*-test.sh, so no
# check picks it up). A design made with mitcad-cli: Housing (a solid box
# 60 x 40 x 30 mm in the root) and Core:1, a component with a 10 mm cube
# placed inside the box, out of sight. On the light and the dark theme
# (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME) it saves:
#   <dir>/browser-hover-<theme>-none.png     the window, no row under the pointer
#   <dir>/browser-hover-<theme>-core.png     the pointer on Core:1's row
#   <dir>/browser-hover-<theme>-housing.png  the pointer on Housing's row
# The app's log as <dir>/look-<theme>.log, with its "Browser hover
# highlight:" lines (the last dump stays as <dir>/window.xwd).
#
# What to look for: with Core:1's row under the pointer the cube inside
# the box shows through the box in the hover colour (its faces faint, its
# edges clear), where nothing showed before; with Housing's row the box is
# highlighted, its hidden back edges showing through; the selection and
# the view as they were; readable on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-hover-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-browser-hover-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Housing (F2.b0) in the root; Core (C1, Core:1) with a 10 mm cube at
# (25, 15, 10), inside the box.
cat > "$DIR/design.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Housing"},
  {"cmd": "create_component", "name": "Core"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [25, 15], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "set_occurrence_transform", "occurrence": "Core:1", "transform": {"translation": [0, 0, 10]}}
]
EOF
"$CLI" run "$DIR/design.json" --save "$DIR/hidden-core.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }

# shot name: the main window as <dir>/browser-hover-<theme>-<name>.png,
# once the app has drawn what the pointer set going.
shot() {
  ui_wait_idle
  ui_sync
  sleep 2
  xwd -id "$UI_WINDOW" -silent > "$DIR/window.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/window.xwd" "$DIR/browser-hover-$theme-$1.png"
  echo "ok   $DIR/browser-hover-$theme-$1.png"
}

ui_start_display
for theme in light dark; do
  echo "--- The $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --open "$DIR/hidden-core.mitcad"
  ui_expect_log "Window style: $theme, " "the window's style is $theme"
  ui_step "fit (F6)"                       ui_key F6
  ui_expect_log "Browser Root/Core:1 at" "the browser lists Core:1"
  ui_expect_log "Browser Root/Bodies/Housing at" "and Housing"
  xdotool mousemove 5000 5000
  shot none
  ui_mark
  ui_step "point at Core:1's row"          xdotool mousemove $(ui_logged_at "Browser Root/Core:1")
  ui_expect_new "Browser hover highlight: Root/Core:1 (1 bodies), " "Core:1's cube highlighted"
  shot core
  ui_mark
  ui_step "point at Housing's row"         xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Housing")
  ui_expect_new "Browser hover highlight: Root/Bodies/Housing (1 bodies), " "Housing highlighted"
  shot housing
  grep "^Browser hover highlight: " "$UI_LOG"
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Browser hover look pictures in $DIR"

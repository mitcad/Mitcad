#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The popups' border over Qt's portable style (mitcad#2081, mitcad#2101): a
# one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). A design made with mitcad-cli: a
# box of 60 x 40 x 30 mm. In the Floating and the Docked layout, on the
# Light and the Dark theme (Preferences › Display › Theme, the setting
# appearance/theme), it saves the whole test screen with:
#   <dir>/popup-border-<layout>-<theme>-context-menu.png  the view's context
#       menu of the box, its first entry highlighted (Down)
#   <dir>/popup-border-<layout>-<theme>-dropdown.png      Extrude's Operation
#       drop-down open in its command panel, a row highlighted (Down)
# The app's log as <dir>/look-<layout>-<theme>.log, with its "Style icon
# size:" and "Theme:" lines.
#
# What to look for: each popup has a whole 1 px frame in the role
# PopupBorder (#7c828b on the Light theme, #707580 on the Dark one) that
# sets it apart from the panel or the view under it; the highlighted row's
# band (mitcad#2084) stays inside that frame, which runs unbroken past it.
#
# Runs headless on Xvfb (see ui-test-lib.sh), with Qt's generic platform
# theme, so the style is the portable one: the start-up log's "Style icon
# size:" line names it "with the popups' border (Icon size 100%)".
# Usage: tools/ui-popup-border-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-popup-border-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
# The portable style and MitCad's own palettes: no platform theme or style
# of the caller's, no test colour scheme; the Theme preference decides.
unset QT_QPA_PLATFORMTHEME QT_STYLE_OVERRIDE GTK_THEME MITCAD_TEST_COLOR_SCHEME
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# The box (F2.b0) in the root.
cat > "$DIR/design.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}
]
EOF
"$CLI" run "$DIR/design.json" --save "$DIR/box.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }

# shot name: the test screen (with the open popup, a window of its own) as
# <dir>/popup-border-<layout>-<theme>-<name>.png, once the app has drawn it.
shot() {
  ui_sync
  sleep 1
  ui_capture "$DIR/popup-border-$layout-$theme-$1.png"
  echo "ok   $DIR/popup-border-$layout-$theme-$1.png"
}

ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    echo "--- The $layout layout, the $theme theme"
    # The Theme preference, written over the settings before each start.
    mkdir -p "$UI_CONFIG/Mitcad"
    printf '[appearance]\ntheme=%s\n' "$theme" > "$UI_CONFIG/Mitcad/Mitcad.conf"
    ui_start_app --open "$DIR/box.mitcad" --chrome=$layout
    ui_expect_log "Chrome: $layout (--chrome" "the $layout layout"
    ui_expect_log "with the popups' border" "the popups' border drawn over the portable style"
    ui_expect_log "Theme: $theme (preference" "the $theme theme"
    ui_expect_log "Window style: $theme, " "the window's style is $theme"
    ui_step "fit (F6)"                       ui_key F6
    ui_expect_log "Body F2.b0 at" "the box is in the view"

    # The view's context menu of the box.
    ui_mark
    ui_step "right-click the box"            ui_click_logged "Body F2.b0" 3
    ui_expect_new "Context menu: " "the view's context menu opened"
    ui_step "highlight its first entry"      ui_key Down
    shot context-menu
    ui_step "close the menu"                 ui_key Escape

    # Extrude's Operation drop-down in its command panel.
    ui_mark
    ui_step "Extrude (search)"               ui_command "Extrude"
    ui_expect_new "Command Extrude started" "Extrude started"
    ui_expect_new "Panel Extrude input operation at" "the panel shows its Operation"
    ui_step "open Operation"                 ui_click_logged "Panel Extrude input operation"
    # The pointer off the list, so that the key's row stays highlighted.
    xdotool mousemove $((X + WIDTH - 2)) $((Y + HEIGHT - 2))
    ui_step "highlight the next entry"       ui_key Down
    shot dropdown
    ui_step "close the drop-down"            ui_key Escape

    grep -E "^(Style|Style icon size|Theme|Window style): " "$UI_LOG"
    cp -f "$UI_LOG" "$DIR/look-$layout-$theme.log"
    ui_crashed && ui_fail "the application crashed"
    ui_stop_app
  done
done

ui_finish "Popup border pictures in $DIR"

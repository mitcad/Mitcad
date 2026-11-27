#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowVisibility.cpp tools/cli app/MainWindowContextMenu.cpp
# The view's context menu Hide submenu (mitcad#347) through the real UI, on
# a design with a component (Component1) placed twice, Component1:1 (O1)
# and Component1:2 (O2) 50 mm apart, and a body of the root beside them:
#   - A right-click on a face of a body inside Component1:1 offers Hide >
#     Hide Body and Hide Component; Hide Body hides the body (both its
#     occurrences share it), the browser's light bulb shows it hidden, and
#     undo brings it back in one step.
#   - Hide Component hides that occurrence only: Component1:2 stays shown,
#     and undo brings the occurrence back.
#   - Two faces of two bodies selected: Hide 2 Bodies hides both at once,
#     one undo step takes them back.
#   - A face of the root's body has Hide Body but no Hide Component.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hide-menu-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-hide-menu.XXXXXX)
FILE=$WORK/design.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Sketch1 (F1) 30 x 20 around the origin, Extrude1 (F2) 10 up: Body1 (F2.b0)
# in the root. Extrude2 (F3) 4 down as a new component: Component1:1 (O1,
# F3.b0), copied 50 mm along Y as Component1:2 (O2).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 50, 0]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_file filter expected description: a jq filter on the saved project file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# choose_hide "entry": opens the Hide submenu of the context menu just shown
# (Return on its entry) and picks an entry of it, with the arrow keys, from
# the logged "Context menu Hide: a | b".
choose_hide() {
  local entries index=0 found="" item
  ui_menu_choose "Hide"
  ui_sync
  entries=$(grep "Context menu Hide: " "$UI_LOG" | tail -1 | sed 's/.*Context menu Hide: //')
  IFS='|' read -ra items <<< "$entries"
  for item in "${items[@]}"; do
    index=$((index + 1))
    item=${item# }
    item=${item% }
    if [ "$item" = "$1" ]; then
      found=$index
      break
    fi
  done
  [ -n "$found" ] || ui_fail "'$1' is not in the Hide submenu: $entries"
  [ "$found" -le 1 ] || xdotool key --delay 80 --repeat $((found - 1)) Down
  ui_key Return
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Component1:1/Bodies/Body2 at" "the browser lists Body2 in Component1:1"
ui_expect_log "Browser Root/Component1:2/Bodies/Body2 at" "and in Component1:2"
ui_expect_log "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "Body1 and both occurrences shown"

echo "--- Hide Body"
ui_mark
ui_step "right-click Body2 in Component1:1" ui_click_logged "Body F3.b0 in O1" 3
ui_expect_new "Context menu Hide: Hide Body | Hide Component" "the Hide submenu offers both"
ui_step "Hide > Hide Body"               choose_hide "Hide Body"
ui_expect_new "Hid body Body2" "the body is hidden"
ui_expect_new "Bodies shown: F2.b0" "only the root's body is shown"
save
expect_file '[.bodies[].visible]' '[null,false]' "the file keeps the body hidden: its light bulb shows it so"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Hide Body2" "undo brings the body back in one step"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "both occurrences shown again"

echo "--- Hide Component"
ui_mark
ui_step "right-click Body2 in Component1:1" ui_click_logged "Body F3.b0 in O1" 3
ui_step "Hide > Hide Component"          choose_hide "Hide Component"
ui_expect_new "Hid component Component1:1" "the occurrence is hidden"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O2" "Component1:2 stays shown"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Hide Component1:1" "undo brings the occurrence back"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "all shown again"

echo "--- Several items: counts, one undo step"
ui_mark
ui_step "click Body1"                    ui_click_logged "Body F2.b0"
ui_step "Ctrl+click Body2 in Component1:1" xdotool mousemove $(ui_logged_at "Body F3.b0 in O1") keydown ctrl click 1 keyup ctrl
ui_step "Ctrl+click Body2 in Component1:2" xdotool mousemove $(ui_logged_at "Body F3.b0 in O2") keydown ctrl click 1 keyup ctrl
ui_step "right-click Body2 in Component1:2" ui_click_logged "Body F3.b0 in O2" 3
ui_expect_new "Context menu Hide: Hide 2 Bodies | Hide 2 Components" "the entries count the bodies and the occurrences, each once"
ui_step "Hide > Hide 2 Components"      choose_hide "Hide 2 Components"
ui_expect_new "Hid component Component1:1, Component1:2" "both occurrences are hidden"
ui_expect_new "Bodies shown: F2.b0" "only the root's body is shown"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Hide Components" "one undo step brings both back"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "all shown again"

echo "--- A body of the root has no Hide Component"
ui_step "Escape (clear the selection)"   ui_key Escape
ui_mark
ui_step "right-click Body1"              ui_click_logged "Body F2.b0" 3
ui_expect_new "Context menu Hide: Hide Body" "only Hide Body"
ui_step "Escape (close the menu)"        ui_key Escape

ui_finish "UI hide menu"

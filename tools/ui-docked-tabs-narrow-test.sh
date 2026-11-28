#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/QuickAccess.cpp
# check-all sources: app/framework/TitleBar.cpp app/MainWindow.cpp tools/cli
# The Docked layout's tabs in a 1024 x 768 window and then a 900 x 700 one
# (mitcad#2079), through the real UI, on a 20 mm cube mitcad-cli makes.
# Since mitcad#2082 the Docked layout of Linux has the Floating layout's
# toolbar panel: its tab row has the shown workspace's tabs (Design's:
# Solid, Surface, Sheet Parts, Build and Manage), Start is the top bar's
# Home and Render the workspace selector's. (The pill tabs that fold into a
# », with Boost development compact first, are the macOS Docked toolbar's,
# docs/ui-conventions.md 13.) Checked:
#   - every Design tab is in sight ("Ribbon tab SOLID at x,y"), inside the
#     window, left to right, before the + of the tabs' menu; no Design tab
#     out of sight or folded;
#   - each tab is chosen in turn (ui_click_tab): it is shown ("Ribbon tab
#     shown: MANAGE") and every Design tab stays in sight;
#   - at 900 px the same for Solid and Manage shown.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-docked-tabs-narrow-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-docked-tabs-narrow.XXXXXX")
FILE=$WORK/cube.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/cube.json" << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"bodies": 1}}
]
JSON
"$CLI" run "$WORK/cube.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

DESIGN_TABS=(SOLID SURFACE "SHEET PARTS" BUILD MANAGE)

# expect_row description shown: each Design tab's last logged place is in
# sight inside the window, left to right, before the + of the tabs' menu.
expect_row() {
  local what=$1 shown=$2 line tab x previous=-1 plus width
  width=$(sed -n 's/^Window layout: window \([0-9]*\)x.*/\1/p' "$UI_LOG" | tail -1)
  plus=$(grep '^Ribbon tabs button at ' "$UI_LOG" | tail -1 | sed 's/.* at \([0-9]*\),.*/\1/')
  [ -n "$plus" ] || ui_fail "$what: the + of the tabs' menu is not logged"
  for tab in "${DESIGN_TABS[@]}"; do
    line=$(grep -E "^Ribbon tab $tab (at [0-9]+,[0-9]+|out of sight|folded)$" "$UI_LOG" | tail -1)
    case $line in
    *" at "*)
      x=$(sed 's/.* at \([0-9]*\),.*/\1/' <<< "$line")
      [ "$x" -gt "$previous" ] || ui_fail "$what: $tab at x $x is not right of the tab before it (x $previous)"
      [ "$x" -lt "$plus" ] || ui_fail "$what: $tab at x $x is not before the + (x $plus)"
      [ -z "$width" ] || [ "$x" -lt "$width" ] || ui_fail "$what: $tab at x $x is outside the $width px window"
      previous=$x
      ;;
    "") ui_fail "$what: no place logged for the tab $tab" ;;
    *) ui_fail "$what: $line" ;;
    esac
  done
  echo "ok   $what: $shown shown; every Design tab in sight before the + (x $plus)"
}

ui_start_display

echo "--- The Docked layout in a 1024 x 768 window"
ui_start_app --open "$FILE" --chrome=docked
ui_expect_log "Opened $FILE" "the design opened"
ui_expect_log "Ribbon tab SOLID at " "the Solid tab logged its place"
ui_wait_idle
ui_mark
ui_step "the window 1024 x 768"         xdotool windowsize "$UI_WINDOW" $((1024 * UI_SCALE)) $((768 * UI_SCALE))
ui_expect_new "Window layout: window 1024x768" "the window is 1024 x 768"
ui_wait_idle
ui_sync
eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
expect_row "1024 px" SOLID

echo "--- Each tab chosen in turn"
for tab in "${DESIGN_TABS[@]:1}" SOLID; do
  ui_mark
  ui_step "choose $tab"                   ui_click_tab "$tab"
  ui_expect_new "Ribbon tab shown: $tab" "$tab shown"
  ui_wait_idle
  ui_sync
  sleep 0.3
  ui_sync
  expect_row "1024 px, $tab chosen" "$tab"
done

echo "--- The window 900 x 700"
ui_mark
ui_step "the window 900 x 700"          xdotool windowsize "$UI_WINDOW" $((900 * UI_SCALE)) $((700 * UI_SCALE))
ui_expect_new "Window layout: window 900x700" "the window is 900 x 700"
ui_wait_idle
ui_sync
eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
expect_row "900 px" SOLID
for tab in MANAGE SOLID; do
  ui_mark
  ui_step "choose $tab"                   ui_click_tab "$tab"
  ui_expect_new "Ribbon tab shown: $tab" "$tab shown"
  ui_wait_idle
  ui_sync
  sleep 0.3
  ui_sync
  expect_row "900 px, $tab chosen" "$tab"
done
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI Docked tabs at 1024 and 900 px"

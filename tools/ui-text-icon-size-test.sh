#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/UiSizes.cpp app/framework/UiSizes.hpp app/main.cpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriver.cpp app/framework/TestDriverChrome.cpp
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandPanel.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp app/framework/TitleBar.cpp
# check-all sources: app/framework/WindowStyle.cpp app/browser/TimelineWidget.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/text-icon-size.mitcad-ui
# check-all sources: app/framework/IconSizeStyle.cpp app/framework/IconSizeStyle.hpp
# check-all sources: app/print/MaterialsDialog.cpp
# Preferences › Display › Text size, Icon size and Font (mitcad#1556,
# GitHub mitcad/Mitcad#207) through the real UI, the input script
# tools/ui-scripts/text-icon-size.mitcad-ui: MitCad started with the
# settings appearance/textSize, appearance/iconSize and
# appearance/fontFamily, at Interface size 100 %:
#   - Text size 80 %, Icon size 150 %, the system's font (Floating);
#   - Text size 200 %, Icon size 80 %, the system's font (Floating, then
#     Docked);
#   - Text size 120 %, Icon size 120 %, the font DejaVu Sans (Floating).
# Each time the start-up logs the sizes, the application font and the icon
# sizes; with Box's command panel open the application font is in that
# family at the platform's size times Text size, the Browser's and the
# ribbon's icons at their sizes times Icon size, the icons Qt's style sizes
# (a menu's and a combo box's, mitcad#1561) at the style's sizes times Icon
# size, the application's style inside Icon size's proxy, and the window, the
# ribbon, the Browser and the panel lay out whole, no input of the panel
# clipped; the Materials window's appearance swatches 20 px times Icon size
# (mitcad#1610); Preferences shows the
# sizes, and Icon size one step up is saved for the next start.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-text-icon-size-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
# The sizes alone: no factor of the test library's (UI_SCALE) nor one kept
# from another start.
unset QT_SCALE_FACTOR MITCAD_USER_QT_SCALE_FACTOR
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/text-icon-size.mitcad-ui
UI_SCREEN_HEIGHT=1080
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
# A family every Linux test host has (the fonts the UI tests draw with).
FONT='DejaVu Sans'

# style_icons name icon: the icons Qt's style sizes (mitcad#1561): the
# application's style inside Icon size's proxy, and the menu's and the combo
# box's icons in expect-text-icon-size's line at the style's sizes times
# Icon size, rounded to whole pixels (counted here).
style_icons() {
  local name=$1 icon=$2 line menu menuBase combo comboHigh comboBase
  grep -qE "^Style icon size: [^ ]+ with the style's icons at $icon%\$" "$UI_LOG" ||
    ui_fail "$name: the application's style is not inside Icon size's proxy"
  line=$(grep -E '^Text and icon size layout: ' "$UI_LOG" | tail -1)
  local re="menu icons ([0-9]+)x([0-9]+) \\(the style's ([0-9]+) px\\), "
  re+="combo box icons ([0-9]+)x([0-9]+) \\(the style's ([0-9]+) px\\)"
  [[ $line =~ $re ]] || ui_fail "$name: no menu and combo box icons in: $line"
  [ "${BASH_REMATCH[1]}" = "${BASH_REMATCH[2]}" ] || ui_fail "$name: the menu's icons are not square: $line"
  menu=${BASH_REMATCH[1]} menuBase=${BASH_REMATCH[3]}
  combo=${BASH_REMATCH[4]} comboHigh=${BASH_REMATCH[5]} comboBase=${BASH_REMATCH[6]}
  [ "$menu" -eq $(((menuBase * icon + 50) / 100)) ] ||
    ui_fail "$name: the menu's icons $menu px, not the style's $menuBase px times $icon%"
  if [ "$combo" -ne $(((comboBase * icon + 50) / 100)) ] || [ "$comboHigh" -ne "$combo" ]; then
    ui_fail "$name: the combo box's icons ${combo}x$comboHigh, not the style's $comboBase px times $icon%"
  fi
  echo "ok   $name: menu icons ${menu} px and combo box icons ${combo} px (the style's $menuBase and $comboBase px)"
}

# run layout text icon font: one start of MitCad with those settings; the
# app ends with the script's steps.
run() {
  local layout=$1 text=$2 icon=$3 font=$4
  local name="$layout, text $text%, icons $icon%, font $font"
  mkdir -p "$(dirname "$CONF")"
  {
    printf '[appearance]\ntextSize=%s\niconSize=%s\n' "$text" "$icon"
    [ "$font" = system ] || printf 'fontFamily=%s\n' "$font"
  } > "$CONF"
  export MITCAD_TEST_TEXT_SIZE=$text MITCAD_TEST_ICON_SIZE=$icon MITCAD_TEST_FONT=$font
  export MITCAD_TEST_ICON_NEXT=$((icon + 5))
  # The Materials window's appearance swatches (mitcad#1610).
  export MITCAD_TEST_SWATCH_SIZE=$(((20 * icon + 50) / 100))
  if [ "$font" = system ]; then
    export MITCAD_TEST_SYSTEM_FONT=true
  else
    export MITCAD_TEST_SYSTEM_FONT=false
  fi
  echo "--- $name"
  ui_start_app --ui=qt --no-mcp "--chrome=$layout"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the text and icon size driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$name: the text and icon size driver failed: $(grep -E 'TestDriver|Text and icon size' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$name: the font, the Browser's and the ribbon's icons, and the layout whole"
  grep -E '^Text and icon size|^Style icon size: |^Application font: |^Icon sizes: |^Interface size layout: ' \
    "$UI_LOG" | sed 's/^/     /'
  style_icons "$name" "$icon"
  # No label or choice of Box's panel cut at these sizes (CommandPanel's
  # "Panel Box inputs: ..." lines).
  grep -E '^Panel [^ ]+ inputs clipped: ' "$UI_LOG" | sort -u | sed 's/^/     /' | grep . &&
    ui_fail "$name: a command panel's input is clipped"
  grep -q '^Panel Box inputs: none clipped$' "$UI_LOG" || ui_fail "$name: Box's panel did not log its inputs"
  echo "ok   $name: no input of Box's panel clipped"
  grep -q "^iconSize=$((icon + 5))\$" "$CONF" || ui_fail "$name: Icon size $((icon + 5))% is not saved"
  echo "ok   $name: Icon size $((icon + 5))% saved for the next start"
}

ui_start_display

run floating 80 150 system
run floating 200 80 system
run docked 200 80 system
run floating 120 120 "$FONT"

ui_finish 'UI text and icon size test'

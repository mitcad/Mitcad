#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/AppTheme.cpp app/framework/AppTheme.hpp app/framework/Theme.cpp app/framework/Theme.hpp
# check-all sources: app/main.cpp app/view/ViewController.cpp app/files/StartPage.cpp app/browser app/framework/GlassCard.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/theme-preference.mitcad-ui
# Preferences › Display › Theme (mitcad#1330, GitHub mitcad/Mitcad#151)
# through the real UI, the shared input script
# tools/ui-scripts/theme-preference.mitcad-ui (also run by
# tools/ui-windows-theme-preference-test.ps1): on a light platform scheme,
# MitCad started with the setting appearance/theme=dark shows the Start
# page, the Browser's card, the Browser and the timeline dark; Light chosen
# in Preferences repaints them light without a restart, Dark dark again,
# and System follows the platform again (light, then its switch to dark
# and back). Three starts:
#   1. gtk3-floating: Qt's GTK platform theme (QT_QPA_PLATFORMTHEME=gtk3),
#      which honours the request, in the Floating layout.
#   2. gtk3-docked: the same in the Docked layout.
#   3. generic-floating: Qt's generic platform theme, which has no dark
#      scheme and keeps its palette: MitCad's own palette of the theme's
#      roles for Dark (at launch and when chosen again), none for Light and
#      System; the desktop's switch under System changes nothing there.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-theme-preference-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/theme-preference.mitcad-ui
export MITCAD_START_PAGE=1
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

# run name theme desktop-dark layout: one start of MitCad on the platform
# theme with the Theme preference Dark stored; the app ends with the steps.
run() {
  export QT_QPA_PLATFORMTHEME=$2
  export MITCAD_TEST_THEME_DESKTOP_DARK=$3
  mkdir -p "$(dirname "$CONF")"
  printf '[appearance]\ntheme=dark\n' > "$CONF"
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp "--chrome=$4"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: the theme driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$1: the theme driver failed: $(grep -E 'TestDriver|Theme:' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$1: dark at launch, light and dark from Preferences, System follows the platform"
  grep -E '^Theme: |^Preferences: theme |TestDriver: widget theme' "$UI_LOG" | sed 's/^/     /'
  grep -q '^theme=system$' "$CONF" || ui_fail "$1: System is not saved as appearance/theme"
  echo "ok   $1: saved as appearance/theme"
}

ui_start_display

run gtk3-floating gtk3 dark floating
run gtk3-docked gtk3 dark docked

run generic-floating generic light floating
own=$(grep -c "^Theme: dark (preference; MitCad's own palette, window #" "$UI_LOG")
[ "$own" -eq 2 ] || ui_fail "generic-floating: MitCad's own dark palette $own times, not 2 (at launch and chosen again)"
echo "ok   generic-floating: MitCad's own dark palette where the platform kept its light one"
grep -q '^Theme: light (preference)$' "$UI_LOG" ||
  ui_fail "generic-floating: Light did not take the platform's light palette"
echo "ok   generic-floating: Light on the platform's own palette"

ui_finish 'UI theme preference test'

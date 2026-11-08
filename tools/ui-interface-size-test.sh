#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/InterfaceSize.cpp app/framework/InterfaceSize.hpp app/main.cpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriver.cpp app/framework/TestDriverChrome.cpp
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandPanel.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/interface-size.mitcad-ui
# Preferences › Display › Interface size (mitcad#1495, GitHub
# mitcad/Mitcad#194) through the real UI, the input script
# tools/ui-scripts/interface-size.mitcad-ui: MitCad started with the
# setting appearance/interfaceSize=150 and no QT_SCALE_FACTOR of its own,
# on a test screen of 1600 x 1080 pixels (1067 x 720 logical pixels at
# 150 %):
#   - the start-up logs 150 % as Qt's scale factor 1.5 and the window's
#     device pixel ratio 1.5;
#   - with Box's command panel open, the window lies within the screen, the
#     ribbon's captions are complete and its buttons as large as their
#     icons, the Browser's rows as high as their icons, and the panel's OK
#     and fields whole ("Interface size layout: ...");
#   - Preferences shows 150%, and 100% chosen there is saved for the next
#     start.
# Twice: in the Docked layout and in the Floating one.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-interface-size-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
# The Interface size alone scales the app here: no factor of the test
# library's (UI_SCALE) nor one kept from another start.
unset QT_SCALE_FACTOR MITCAD_USER_QT_SCALE_FACTOR
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/interface-size.mitcad-ui
UI_SCREEN_HEIGHT=1080
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

# run layout: one start of MitCad at 150 % in that layout; the app ends
# with the script's steps.
run() {
  mkdir -p "$(dirname "$CONF")"
  printf '[appearance]\ninterfaceSize=150\n' > "$CONF"
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp "--chrome=$1"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: the interface size driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$1: the interface size driver failed: $(grep -E 'TestDriver|Interface size' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$1: 150 % at device pixel ratio 1.5, the ribbon, Browser and Box's panel whole"
  grep -E '^Interface size|^Device pixel ratio: |^Preferences: interface size ' "$UI_LOG" | sed 's/^/     /'
  grep -q '^interfaceSize=100$' "$CONF" || ui_fail "$1: 100% is not saved as appearance/interfaceSize"
  echo "ok   $1: 100% saved for the next start"
}

ui_start_display

run docked
run floating

ui_finish 'UI interface size test'

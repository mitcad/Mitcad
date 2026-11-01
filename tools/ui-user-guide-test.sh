#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/CMakeLists.txt app/report/ReportCenter.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/user-guide.mitcad-ui
# Help › User Guide (mitcad#1032) through the real UI, Docked and Floating:
# the in-process test driver (tools/ui-scripts/user-guide.mitcad-ui)
# chooses the entry in the menu bar's Help menu and then the command by the
# command search's id; each time the app would open the user guide on
# GitHub, at the release's tag or main. No browser opens:
# MITCAD_TEST_LOG_URLS=1 (ui-test-lib.sh) logs the address instead.
# The Windows counterpart is tools/ui-windows-user-guide-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-user-guide-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

SCRIPT=$(cd "$(dirname "$0")" && pwd)/ui-scripts/user-guide.mitcad-ui
GUIDE='https://github\.com/mitcad/Mitcad/blob/(main|v[0-9]+\.[0-9]+\.[0-9]+)/docs/user-guide\.md$'

ui_start_display

for chrome in docked floating; do
  echo "--- Help > User Guide, $chrome"
  MITCAD_TEST_INPUT=$SCRIPT ui_start_app --demo --chrome=$chrome
  for _ in $(seq 1 300); do
    grep -q '^TestDriver: \(done\|FAIL\)' "$UI_LOG" && break
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  grep -q '^TestDriver: done' "$UI_LOG" ||
    ui_fail "the test script failed ($chrome): $(grep -m1 'TestDriver: FAIL' "$UI_LOG")"
  opened=$(grep -cE "Open URL \(test, not opened\): $GUIDE" "$UI_LOG")
  [ "$opened" = 2 ] || ui_fail "$chrome: the guide's address logged $opened times, not 2"
  echo "ok   $chrome: Help > User Guide and the command opened $(grep -oE "$GUIDE" "$UI_LOG" | tail -1)"
  ui_stop_app
done

ui_finish "user guide"

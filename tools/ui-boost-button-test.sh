#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BoostButton.cpp app/framework/BoostButton.hpp app/framework/TitleBar.cpp
# check-all sources: app/framework/TitleBar.hpp app/framework/Ribbon.cpp app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: app/framework/Theme.cpp app/icons/boost.svg app/report/ReportCenter.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/boost-button.mitcad-ui tools/ui-scripts/boost-button.json
# The Boost development button (mitcad#1990) through the real UI, the
# shared input script tools/ui-scripts/boost-button.mitcad-ui (also run by
# tools/ui-windows-boost-button-test.ps1 and, Docked, by
# tools/ui-macos-boost-button-test.sh) on a box that mitcad-cli makes
# (tools/ui-scripts/boost-button.json), in a 1024 x 768 window, in the
# Floating layout and in the Docked one, both with the top bar (its part
# titleBoost, left of the indicators: "Floating cards: ..." and "Docked
# top bar: ...", mitcad#2082), each with a fresh
# profile: the button shown, a click logs the Ko-fi link (the URL log of
# the tests, no browser), its × hides it and saves it, Preferences ›
# Display › Show the Boost development button brings it back. The
# pictures (boost-button-<layout>.png) are kept in the test's folder
# (MITCAD_UI_LOG_DIR when check-all gives one).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-boost-button-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-boost-button.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/boost-button.mitcad-ui
"$cli" run "$here/ui-scripts/boost-button.json" --save "$work/boost.mitcad" || exit 1
pictures=${MITCAD_UI_LOG_DIR:-$root/build}/ui-boost-button-pictures
mkdir -p "$pictures"
conf=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
ui_start_display

for layout in floating docked; do
  echo "=== $layout"
  rm -f "$conf"
  export MITCAD_TEST_WORK=$work/$layout
  mkdir -p "$MITCAD_TEST_WORK"
  ui_start_app --ui=qt --no-mcp "--chrome=$layout" --open "$work/boost.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "boost button driver timed out ($layout)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'Boost button shown' "$layout: the button shown"
  ui_expect_log 'Boost link opened: https://ko-fi.com/mitcad' "$layout: a click opens the Ko-fi page"
  ui_expect_log 'Open URL (test, not opened): https://ko-fi.com/mitcad' "$layout: only logged in the test"
  ui_expect_log 'Boost button hidden' "$layout: the × hides it"
  ui_expect_log 'Preferences: Boost development button shown' "$layout: Preferences shows it again"
  ui_expect_log 'TestDriver: done' "$layout: the Boost development button"
  # In the top bar, left of the project indicator, in both layouts.
  cards=$(grep -E '^(Floating cards|Docked top bar): ' "$UI_LOG" | tail -1)
  boost=$(tr ';' '\n' <<< "${cards#*; }" | sed 's/^ *//' | awk '$1 == "titleBoost" { print $2 }')
  indicator=$(tr ';' '\n' <<< "${cards#*; }" | sed 's/^ *//' | awk '$1 == "titleIndicator" { print $2 }')
  [ -n "$boost" ] && [ -n "$indicator" ] || ui_fail "no titleBoost or titleIndicator in the top bar: '$cards'"
  [ "${boost%,*}" -lt "${indicator%,*}" ] || ui_fail "the Boost button is not left of the project indicator: '$cards'"
  echo "ok   $layout: the Boost button in the top bar, left of the project indicator"
  [ -s "$MITCAD_TEST_WORK/boost-button.png" ] || ui_fail "no picture boost-button.png ($layout)"
  cp "$MITCAD_TEST_WORK/boost-button.png" "$pictures/boost-button-$layout.png"
done
echo "ok   the pictures in $pictures"
ui_finish 'UI Boost development button'

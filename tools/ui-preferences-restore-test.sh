#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/PreferencesRestore.cpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/framework/SettingsWindow.cpp app/framework/SettingsWindow.hpp app/framework/TestDriver.cpp
# check-all sources: app/framework/ChromeStyle.cpp app/browser/ActivateQuestion.cpp app/sketch/SketchPreferences.cpp
# check-all sources: app/agent/ChatSettings.cpp app/library/SupplierShops.cpp
# check-all sources: tools/ui-scripts/preferences-restore.mitcad-ui
# Preferences › Restore Defaults (mitcad#1887, GitHub mitcad/Mitcad#357)
# through the real UI, the shared input script
# tools/ui-scripts/preferences-restore.mitcad-ui (also run by
# tools/ui-windows-preferences-restore-test.ps1 and
# tools/ui-macos-preferences-restore-test.sh) on a new design: preferences
# changed on Display, General and Sketch; Restore Defaults › Current Group
# on Display, confirmed, puts only Display back to its defaults (shown at
# once and saved, General and Sketch keep their changes); › All Settings,
# confirmed, puts every page back, while a recent file and a library
# source stay.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preferences-restore-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/preferences-restore.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'preferences restore driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preferences: small circles full' 'preferences changed on Display'
ui_expect_log 'Preferences: click adds to the selection in sketches on' 'and on Sketch'
ui_expect_log 'Preferences restore defaults asked: Restore the defaults of the Display settings?' \
  'Current Group asks first, naming the page'
ui_expect_log 'Kept: recent files, library sources and fetched libraries, favorites, window sizes and places' \
  'the question says what is kept'
ui_expect_log 'Preferences: restore defaults, current group: display' 'Display back to its defaults'
ui_expect_log 'Preferences restore defaults asked: Restore the defaults of all settings?' 'All Settings asks first'
ui_expect_log 'Preferences: restore defaults, all settings: general, navigation, display' 'every page back to its defaults'
ui_expect_log 'TestDriver: done' 'the pages show the defaults, the store holds them, a recent file and a library source stay'
ui_finish 'UI preferences restore defaults'

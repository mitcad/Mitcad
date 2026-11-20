#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreferencesSearch.cpp app/framework/PreferencesSearch.hpp
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/view/NavigationPreferences.hpp
# check-all sources: app/framework/SettingsWindow.cpp app/framework/SettingsWindow.hpp
# check-all sources: app/framework/CommandSearch.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/preferences-search.mitcad-ui
# Preferences' search (mitcad#2074, GitHub mitcad/Mitcad#450) through the
# real UI, the shared input script tools/ui-scripts/preferences-search.mitcad-ui
# (also run by tools/ui-windows-preferences-search-test.ps1 and
# tools/ui-macos-preferences-search-test.sh) on a new design: "temperature"
# shows General with Temperature unit highlighted and the other pages
# greyed; Return focuses its box and Preferences stays open; a greyed page
# can be clicked and the text stays; "dark" finds Display's theme, "orbit"
# Navigation's bindings table; Esc clears; "3d mouse" opens Navigation's
# 3D Mouse tab; the command search's "temperature" lists Preferences:
# General and opens it with Temperature unit highlighted.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preferences-search-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/preferences-search.mitcad-ui
# Closes Preferences once its search field is empty (Cancel: nothing changed).
export MITCAD_TEST_PREFERENCES_DONE=Escape
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'preferences search driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preferences search: "temperature": ' 'the search field takes what is typed when Preferences opens'
ui_expect_log 'Preferences search on General: Temperature unit' 'General shown with Temperature unit highlighted'
ui_expect_log 'Preferences search: Return focuses temperatureUnit on General' 'Return focuses the first match'
ui_expect_log 'Preferences search: "orbit": ' 'a table row is found'
ui_expect_log 'Preferences search: "3d mouse": ' 'a sub-tab is found by its name'
ui_expect_log 'Command search chose tools.preferences.general' 'the command search finds the page by its keyword'
ui_expect_log 'TestDriver: done' 'counts, greyed pages, highlights, Return, Esc and the sub-tab as expected'
ui_finish 'UI preferences search'

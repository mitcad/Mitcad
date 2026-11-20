#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreferencesSearch.cpp app/framework/PreferencesSearch.hpp
# check-all sources: app/view/ViewController.cpp app/framework/SettingsWindow.cpp app/framework/CommandSearch.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/preferences-search.mitcad-ui
# Preferences' search (mitcad#2074, GitHub mitcad/Mitcad#450) in the Settings
# window of the Qt frontend: the shared input script
# tools/ui-scripts/preferences-search.mitcad-ui of
# tools/ui-preferences-search-test.sh on a new design. "temperature" shows
# General with Temperature unit highlighted and the other panes greyed in
# the page list; Return focuses its box; a greyed pane can be clicked and
# the text stays; "dark" finds Display's theme, "orbit" Navigation's
# bindings table; Esc clears; "3d mouse" opens Navigation's 3D Mouse tab;
# the command search's "temperature" lists Preferences: General and opens
# the Settings window with Temperature unit highlighted.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-preferences-search-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=300
# Closes the Settings window, as in ui-macos-navigation-test.sh.
export MITCAD_TEST_PREFERENCES_DONE=ctrl+w
MITCAD_TEST_INPUT=$here/ui-scripts/preferences-search.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app preferences-search --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log 'Preferences search on General: Temperature unit' 'General shown with Temperature unit highlighted'
ui_expect_log 'Preferences search: Return focuses temperatureUnit on General' 'Return focuses the first match'
ui_expect_log 'Preferences search: "3d mouse": ' 'a sub-tab is found by its name'
ui_expect_log 'Command search chose tools.preferences.general' 'the command search finds the page by its keyword'
ui_expect_log '^TestDriver: done' 'counts, greyed panes, highlights, Return, Esc and the sub-tab as expected'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt preferences search'

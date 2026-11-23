#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/view/PreferencesRestore.cpp
# check-all sources: app/framework/AppTheme.cpp app/framework/Theme.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/view-background-theme.mitcad-ui
# The 3D view's Follow System background follows MitCad's theme
# (mitcad#2145) in the Settings window of the Qt frontend: the shared input
# script tools/ui-scripts/view-background-theme.mitcad-ui of
# tools/ui-view-background-theme-test.sh. MitCad started with the setting
# appearance/theme=dark and no saved view background shows the dark
# gradient; Light and Dark chosen in Settings repaint it at once; System
# follows the appearance asked (light, then dark); a chosen White
# Background stays; Restore Defaults › All Settings while the appearance is
# dark gives Follow System back, dark.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-view-background-theme-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=300
# The Theme Dark; no view/background: the default, Follow System. On macOS
# Qt names the settings folder by the organization domain (main.cpp).
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[appearance]\ntheme=dark\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
MITCAD_TEST_INPUT=$here/ui-scripts/view-background-theme.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_THEME_DESKTOP_DARK=dark \
  ui_run_app view-background-theme --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log 'TestDriver: view Background #3b424d #1c2027' 'the dark gradient while the theme is dark'
ui_expect_log 'TestDriver: view Background #f7f8fa #dfe3ea' 'the light gradient while the theme is light'
ui_expect_log 'Preferences: restore defaults, all settings: ' 'Restore Defaults › All Settings'
ui_expect_log '^TestDriver: done' 'Follow System: dark with the dark theme, light with the light one, after Restore Defaults too'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
if grep -q '^background=5$' "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"; then
  ui_pass 'Follow System saved after Restore Defaults'
else
  ui_fail 'Restore Defaults did not save Follow System as view/background'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: the view background follows the theme'

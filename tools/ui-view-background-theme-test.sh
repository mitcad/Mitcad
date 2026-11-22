#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/view/PreferencesRestore.cpp
# check-all sources: app/framework/AppTheme.cpp app/framework/Theme.cpp app/framework/Theme.hpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/view-background-theme.mitcad-ui
# The 3D view's Follow System background follows MitCad's theme
# (mitcad#2145) through the real UI, the shared input script
# tools/ui-scripts/view-background-theme.mitcad-ui (also run by
# tools/ui-windows-view-background-theme-test.ps1 and
# tools/ui-macos-view-background-theme-test.sh): MitCad started with the
# setting appearance/theme=dark and no saved view background shows the
# dark gradient; Light and Dark chosen in Preferences repaint it at once;
# System follows the platform (light, then its switch to dark); a chosen
# White Background stays; Restore Defaults › All Settings while the system
# is dark gives Follow System back, dark. Qt's GTK platform theme
# (QT_QPA_PLATFORMTHEME=gtk3) gives the dark scheme; the generic one has
# none. The Floating layout, as a new installation has it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-background-theme-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/view-background-theme.mitcad-ui
export QT_QPA_PLATFORMTHEME=gtk3
export MITCAD_TEST_THEME_DESKTOP_DARK=dark
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

ui_start_display

mkdir -p "$(dirname "$CONF")"
# The Theme Dark; no view/background: the default, Follow System.
printf '[appearance]\ntheme=dark\n' > "$CONF"
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the background driver timed out"
wait "$UI_RUNNER" ||
  ui_fail "the background driver failed: $(grep -E 'TestDriver|Theme:|^Background ' "$UI_LOG" | tail -8)"
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'Follow System: dark with the dark theme, light with the light one, after Restore Defaults too'
grep -E '^Theme: |^Preferences: theme |^Background |TestDriver: view Background' "$UI_LOG" | sed 's/^/     /'
ui_expect_log 'TestDriver: view Background #3b424d #1c2027' 'the dark gradient while the theme is dark'
ui_expect_log 'TestDriver: view Background #f7f8fa #dfe3ea' 'the light gradient while the theme is light'
grep -q '^background=5$' "$CONF" || ui_fail "Restore Defaults did not save Follow System as view/background"
echo "ok   Follow System saved after Restore Defaults"

ui_finish 'UI view background theme test'

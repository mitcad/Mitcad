#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/WindowPlace.cpp app/framework/WindowPlace.hpp app/WindowManager.cpp
# check-all sources: app/main.cpp app/MainWindow.cpp app/framework/FitToScreen.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverWindowFit.cpp
# check-all sources: tools/ui-scripts/window-place-save.mitcad-ui tools/ui-scripts/window-place-restore.mitcad-ui
# check-all sources: tools/ui-scripts/window-place-maximized.mitcad-ui tools/ui-scripts/window-place-on-screen.mitcad-ui
# The design window's size, place and state kept between starts
# (mitcad#1710, GitHub mitcad/Mitcad#283) on macOS, the shared input
# scripts of tools/ui-window-place-test.sh (Linux) and
# tools/ui-windows-window-place-test.ps1 (Windows), in the Qt frontend:
#   1. Nothing kept: the start size; the window resized to 900 x 600 and
#      moved to 70, 50 from the screen's visible area's top left (below the
#      menu bar); kept when MitCad ends.
#   2. Started again: the same size and place; zoomed (maximized), kept so.
#   3. Started again: zoomed; made normal, the same size and place; a new
#      window at that size, cascaded.
#   4. A kept place on a monitor no longer connected, one half off the
#      screen with its title bar above the top, and one larger than the
#      screen (written into the settings): the whole window within the
#      primary screen's visible area.
# In the Floating layout; 1 and 2 also in the Docked one.
# Usage: tools/ui-macos-window-place-test.sh <app> <output directory>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo "usage: $0 <app> <output directory>" >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=120
export MITCAD_TEST_STEP_MS=100
runs=0

# run description script layout: one start of MitCad running
# tools/ui-scripts/window-place-<script>.mitcad-ui; the app ends with it.
run() {
  runs=$((runs + 1))
  echo "--- $1"
  MITCAD_TEST_INPUT=$here/ui-scripts/window-place-$2.mitcad-ui \
    ui_run_app "window-place-$runs-$2-$3" --ui=qt --no-recovery --no-mcp "--chrome=$3"
  ui_expect_log '^TestDriver: done' "$1"
  grep -E '^Window place|^TestDriver: window ' "$UI_LOG_FILE" | sed 's/^/     /'
  [ "$UI_FAILURES" = 0 ] || exit 1
}

# The settings file of the runs (an INI file under MITCAD_SETTINGS_DIR).
settings_file() {
  find "$MITCAD_SETTINGS_DIR" -name '*.ini' | head -n 1
}

# expect_kept line description: the settings file has the line.
expect_kept() {
  local file
  file=$(settings_file)
  if [ -n "$file" ] && grep -qxF -- "$1" "$file"; then ui_pass "$2"; else ui_fail "$2: '$1' not in the settings"; fi
  [ "$UI_FAILURES" = 0 ] || exit 1
}

# seed geometry description: a kept place written into the settings, on no
# connected screen's area as a whole; MitCad moves it onto the primary one.
seed() {
  local file
  file=$(settings_file)
  [ -n "$file" ] || { ui_fail "$2: no settings file to write the place into"; exit 1; }
  printf '[mainWindow]\ngeometry=@Rect(%s)\nscreen=gone-monitor\nmaximized=false\n' "$1" > "$file"
  run "$2" on-screen floating
}

for layout in floating docked; do
  find "$MITCAD_SETTINGS_DIR" -name '*.ini' -exec rm -f {} +
  run "$layout: nothing kept, resized and moved" save "$layout"
  ui_expect_log '^Window place saved: ' "$layout: the place kept when MitCad ends"
  expect_kept 'maximized=false' "$layout: kept as a normal window"
  run "$layout: started again at the same size and place" restore "$layout"
  expect_kept 'maximized=true' "$layout: kept as zoomed"
done

run "floating: started again zoomed, normal at the same place" maximized floating

seed '6000 200 900 600' "a removed monitor: on the primary screen"
seed '-450 -20 900 600' "half off the screen, the title bar above the top: on the screen"
seed '-100 -100 6000 4000' "larger than the screen: within it"

[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt: the window place kept between starts'

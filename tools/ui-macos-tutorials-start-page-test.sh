#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials app/webbrowser core/vcs/src/library tools/cli
# check-all sources: app/MainWindowStart.cpp app/WindowManager.cpp app/files/StartPage.cpp
# A tutorial opened over the Start page (mitcad#2168) on macOS: the cases
# of tools/ui-tutorials-test.py start-page (the page at launch, the last
# design closed to the page, a modified design under the page), each in
# the Floating and the Docked layout, the application run through
# ui-macos-lib.sh in the Aqua session. Each time the Start page's Tutorials
# button shows the example tutorial's card (mitcad#2346), whose Start starts
# it in an empty design of its own; its card shows its progress, and the
# tutorial is done to its end.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 4 ] && [ "$#" != 5 ]; then
  echo 'usage: ui-macos-tutorials-start-page-test.sh <app> <cli> <output> <the built web browser component> [<example>]' >&2
  exit 2
fi
example=${5:-$here/../docs/examples/tutorials}
ui_init "$1" "$3"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 "$here/ui-tutorials-test.py" prepare-start-page "$MITCAD_TEST_WORK" "$example" || exit 1
export MITCAD_REMEMBER_PANEL_VALUES=0 MITCAD_LOG_TYPED_TEXT=1
# The web browser component the build made (no download).
export MITCAD_BROWSER_COMPONENT_DIR=$4
UI_TIMEOUT=300
for case in launch closed modified; do
  start_page=0
  [ "$case" = launch ] && start_page=1
  for layout in floating docked; do
    run=$MITCAD_TEST_WORK/$case-$layout
    export MITCAD_LIBRARIES_DIR=$run/cache
    export MITCAD_TUTORIALS_DIR=$run/installed
    export MITCAD_WEB_PROFILE_DIR=$run/profile
    export MITCAD_BROWSER_COMPONENTS_DIR=$run/components
    # Settings and the app's other files of this run's own, as
    # ui-tutorials-test.py's run_app gives each run a fresh settings folder:
    # a tutorial's progress is kept in the settings (mitcad#2346), so a run
    # after the first saw the example's card Done, not Not started (0.72.0-rc3:
    # tutorials-launch-docked failed after launch-floating passed).
    own=$UI_ROOT/runs/$case-$layout
    export MITCAD_SETTINGS_DIR=$own/settings MITCAD_AUTOSAVE_DIR=$own/autosave
    export MITCAD_PROJECTS_DIR=$own/projects MITCAD_THUMBNAIL_DIR=$own/thumbnails MITCAD_CRASH_DIR=$own/crashes
    mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_AUTOSAVE_DIR" "$MITCAD_PROJECTS_DIR" "$MITCAD_THUMBNAIL_DIR" || exit 1
    MITCAD_START_PAGE=$start_page MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/$case.mitcad-ui MITCAD_TEST_STEP_MS=100 \
      ui_run_app "tutorials-$case-$layout" --ui=qt --no-native-dialogs --no-recovery --no-mcp --chrome="$layout"
    ui_expect_log '^TestDriver: quit' "the example tutorial was done to its end over the Start page ($case, $layout)"
    ui_expect_no_log 'TestDriver: FAIL' "all its assertions passed ($case, $layout)"
    [ "$UI_FAILURES" = 0 ] || exit 1
  done
done
echo 'PASS: Tutorials over the Start page: at launch, after the last design closed and over a modified design, in both layouts'

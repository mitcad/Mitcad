#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials app/webbrowser core/vcs/src/library tools/cli
# In-app tutorials (mitcad#2012) on macOS: the tutorials repository, the
# page outside it and the steps of tools/ui-tutorials-test.py, the
# application run through ui-macos-lib.sh in the Aqua session; then the
# example tutorials repository's tutorial taken to its end (mitcad#2078);
# then the tutorial whose page reads the design (mitcad#2371). Each starts
# from its card on the Start page (mitcad#2346).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 4 ] && [ "$#" != 5 ]; then
  echo 'usage: ui-macos-tutorials-test.sh <app> <cli> <output> <the built web browser component> [<example>]' >&2
  exit 2
fi
example=${5:-$here/../docs/examples/tutorials}
ui_init "$1" "$3"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 "$here/ui-tutorials-test.py" prepare "$MITCAD_TEST_WORK" "$example" || exit 1
export MITCAD_LIBRARIES_DIR=$MITCAD_TEST_WORK/cache
export MITCAD_TUTORIALS_DIR=$MITCAD_TEST_WORK/installed
export MITCAD_WEB_PROFILE_DIR=$MITCAD_TEST_WORK/profile
export MITCAD_BROWSER_COMPONENTS_DIR=$MITCAD_TEST_WORK/components
export MITCAD_REMEMBER_PANEL_VALUES=0 MITCAD_LOG_TYPED_TEXT=1
# The web browser component the build made (no download).
export MITCAD_BROWSER_COMPONENT_DIR=$4
UI_TIMEOUT=300
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/tutorials.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app tutorials --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '^TestDriver: quit' 'the tutorial was opened, guided and left'
ui_expect_no_log 'TestDriver: FAIL' 'all the tutorials'"'"' assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
# The example's tutorial, its source and folders its own, each step done.
example_work=$MITCAD_TEST_WORK/example
export MITCAD_LIBRARIES_DIR=$example_work/cache
export MITCAD_TUTORIALS_DIR=$example_work/installed
export MITCAD_WEB_PROFILE_DIR=$example_work/profile
export MITCAD_BROWSER_COMPONENTS_DIR=$example_work/components
# Settings of its own, as ui-tutorials-test.py's run_app gives each run:
# tutorials keep their progress and sources in the settings (mitcad#2346).
own=$UI_ROOT/runs/example
export MITCAD_SETTINGS_DIR=$own/settings MITCAD_AUTOSAVE_DIR=$own/autosave
export MITCAD_PROJECTS_DIR=$own/projects MITCAD_THUMBNAIL_DIR=$own/thumbnails MITCAD_CRASH_DIR=$own/crashes
mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_AUTOSAVE_DIR" "$MITCAD_PROJECTS_DIR" "$MITCAD_THUMBNAIL_DIR" || exit 1
MITCAD_TEST_INPUT=$example_work/example.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app tutorials-example --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '^TestDriver: quit' 'the example tutorial was taken to its end'
ui_expect_no_log 'TestDriver: FAIL' 'all the example tutorial'"'"'s assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
# The reading tutorial (mitcad#2371): its page reads the design through the
# bridge's query; its source, folders and settings its own.
reading_work=$MITCAD_TEST_WORK/reading
export MITCAD_LIBRARIES_DIR=$reading_work/cache
export MITCAD_TUTORIALS_DIR=$reading_work/installed
export MITCAD_WEB_PROFILE_DIR=$reading_work/profile
export MITCAD_BROWSER_COMPONENTS_DIR=$reading_work/components
own=$UI_ROOT/runs/reading
export MITCAD_SETTINGS_DIR=$own/settings MITCAD_AUTOSAVE_DIR=$own/autosave
export MITCAD_PROJECTS_DIR=$own/projects MITCAD_THUMBNAIL_DIR=$own/thumbnails MITCAD_CRASH_DIR=$own/crashes
mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_AUTOSAVE_DIR" "$MITCAD_PROJECTS_DIR" "$MITCAD_THUMBNAIL_DIR" || exit 1
MITCAD_TEST_INPUT=$reading_work/reading.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app tutorials-reading --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '^TestDriver: quit' 'the reading tutorial read the design and was refused the rest'
ui_expect_no_log 'TestDriver: FAIL' 'all the reading tutorial'"'"'s assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
python3 "$here/ui-tutorials-test.py" verify "$MITCAD_TEST_WORK" || exit 1
echo 'PASS: Tutorials: listed and opened, the bridge'"'"'s steps, a command refused, no bridge outside, Leave Tutorial; the example tutorial done; the design read through the bridge'

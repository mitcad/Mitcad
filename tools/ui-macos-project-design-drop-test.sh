#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files/MainWindowFiles.cpp app/files/MainWindowArrivals.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp
# check-all sources: core/model/src/document/link_copies.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/project-design-drop.mitcad-ui
# A design of the Projects panel dragged onto the 3D view (mitcad#2428) on
# macOS in Qt, as tools/ui-project-design-drop-test.sh on Linux: the shared
# input script tools/ui-scripts/project-design-drop.mitcad-ui, once in the
# Floating layout (project Shop) and once in the Docked one (project Lab).
# mitcad-cli makes each Local project with assembly.mitcad (a 20 mm cube),
# bracket.mitcad (a block) and back.mitcad (linking assembly.mitcad); the
# app opens assembly.mitcad. The script drags bracket.mitcad onto the cube's
# top face (Linked, at the face's point, one undo step), again while
# Extrude is open (it waits, then Copy), and back.mitcad and assembly.mitcad
# (refused).
# Usage: tools/ui-macos-project-design-drop-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-project-design-drop-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=420
author='UI Tester <ui@example.invalid>'

# A Local project of the three designs, back.mitcad made in its folder so
# that its link is assembly.mitcad.
make_project() {
  local dir=$MITCAD_PROJECTS_DIR/$1
  mkdir -p "$dir"
  {
    "$cli" run "$here/ui-scripts/project-design-drop-assembly.json" --save "$dir/assembly.mitcad" &&
      "$cli" run "$here/ui-scripts/project-design-drop-part.json" --save "$dir/bracket.mitcad" &&
      (cd "$dir" && "$cli" run "$here/ui-scripts/project-design-drop-back.json" --save "$dir/back.mitcad") &&
      "$cli" project create "$dir" --author "$author"
  } >> "$UI_OUT/setup.log" 2>&1 ||
    { cat "$UI_OUT/setup.log" >&2; echo "FAIL: mitcad-cli: the project $1" >&2; exit 1; }
}
make_project Shop
make_project Lab

export MITCAD_TEST_INPUT=$here/ui-scripts/project-design-drop.mitcad-ui MITCAD_TEST_STEP_MS=100
for run in floating:Shop docked:Lab; do
  chrome=${run%%:*}
  export MITCAD_TEST_PROJECT=${run#*:}
  # The panel shown at the start, as for a user who turned it on
  # (mitcad#1020: hidden until then), in the macOS settings file.
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$chrome
  mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
  printf '[projects]\npanel=true\npanelTopBar=true\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
  ui_run_app "project-design-drop-$chrome" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --no-native-dialogs \
    --open "$MITCAD_PROJECTS_DIR/$MITCAD_TEST_PROJECT/assembly.mitcad"
  ui_expect_log '^TestDriver: done' \
    "$chrome: inserted linked and as a copy where dropped, the drop waited for Extrude, links back refused"
  ui_expect_no_log 'TestDriver: FAIL' "$chrome: no failed step"
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drop of a project design onto the 3D view'

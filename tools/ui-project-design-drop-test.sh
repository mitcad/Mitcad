#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files/MainWindowFiles.cpp app/files/MainWindowArrivals.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp
# check-all sources: core/model/src/document/link_copies.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/project-design-drop.mitcad-ui tools/ui-scripts/project-design-drop-assembly.json
# check-all sources: tools/ui-scripts/project-design-drop-part.json tools/ui-scripts/project-design-drop-back.json
# A design of the Projects panel dragged onto the 3D view (mitcad#2428,
# GitHub mitcad/Mitcad#561) through the real UI, the shared input script
# tools/ui-scripts/project-design-drop.mitcad-ui (also run by
# tools/ui-windows-project-design-drop-test.ps1 and
# tools/ui-macos-project-design-drop-test.sh), once in the Floating layout
# (project Shop) and once in the Docked one (project Lab). mitcad-cli makes
# each Local project with assembly.mitcad (a 20 mm cube), bracket.mitcad (a
# block) and back.mitcad (linking assembly.mitcad); the app opens
# assembly.mitcad. The script drags bracket.mitcad onto the cube's top face
# (Linked, at the face's point, one undo step), again while Extrude is open
# (it waits, then Copy), and back.mitcad and assembly.mitcad (refused).
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-project-design-drop-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-project-design-drop.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
AUTHOR="Ada Tester <ada@example.invalid>"
# The projects folder: the panel lists its projects.
export MITCAD_PROJECTS_DIR=$work/projects
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/project-design-drop.mitcad-ui

cli() {
  "$cli" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; exit 1; }
}

# wait_driver what: waits for the driver's run of the application to end.
wait_driver() {
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed"; exit 1; }
  UI_RUNNER=""
}

# make_project name: a Local project of the three designs, back.mitcad made
# in its folder so that its link is assembly.mitcad.
make_project() {
  local dir=$MITCAD_PROJECTS_DIR/$1
  mkdir -p "$dir"
  cli run "$here/ui-scripts/project-design-drop-assembly.json" --save "$dir/assembly.mitcad"
  cli run "$here/ui-scripts/project-design-drop-part.json" --save "$dir/bracket.mitcad"
  (cd "$dir" && cli run "$here/ui-scripts/project-design-drop-back.json" --save "$dir/back.mitcad") || exit 1
  cli project create "$dir" --author "$AUTHOR"
  echo "ok   the project $1 is made"
}

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
mkdir -p "$MITCAD_PROJECTS_DIR"
make_project Shop
make_project Lab

# The panel shown at the start, as for a user who turned it on (mitcad#1020:
# hidden until then).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\npanelTopBar=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

ui_start_display

echo "--- 1. Floating: the project Shop"
MITCAD_TEST_PROJECT=Shop ui_start_app --ui=qt --no-mcp --no-recovery --no-native-dialogs --chrome=floating \
  --open "$MITCAD_PROJECTS_DIR/Shop/assembly.mitcad"
wait_driver 'Floating'
ui_expect_log 'TestDriver: done' 'Floating: inserted linked and as a copy where dropped, the drop waited for Extrude, links back refused'

echo "--- 2. Docked: the project Lab"
UI_MARK=0 # a new log
MITCAD_TEST_PROJECT=Lab ui_start_app --ui=qt --no-mcp --no-recovery --no-native-dialogs --chrome=docked \
  --open "$MITCAD_PROJECTS_DIR/Lab/assembly.mitcad"
wait_driver 'Docked'
ui_expect_log 'TestDriver: done' 'Docked: inserted linked and as a copy where dropped, the drop waited for Extrude, links back refused'

ui_finish 'UI project design drop'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/RecentDesigns.cpp app/MainWindowStart.cpp app/main.cpp
# check-all sources: app/framework/Ribbon.cpp app/files/MainWindowFiles.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/start-page.json tools/ui-scripts/start-page-fixture.mitcad-ui
# check-all sources: tools/ui-scripts/start-page.mitcad-ui tools/ui-scripts/start-page-off.mitcad-ui
# check-all sources: app/files/StartBrowser.cpp app/projects/ProjectBrowserData.cpp app/projects/ProjectsTree.cpp
# check-all sources: app/MainWindowStart.cpp core/vcs core/ffi/src/projects.rs
# The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125) through the real UI,
# the shared input scripts tools/ui-scripts/start-page*.mitcad-ui (also run
# by tools/ui-windows-start-page-test.ps1 and tools/ui-macos-start-page-test.sh)
# on three designs that mitcad-cli makes (tools/ui-scripts/start-page.json)
# and two projects of the projects folder for the page's project browser
# (mitcad#1301): Shop (Local) with plate at its top, parts/gear and
# parts/sub/pin, and Team (Cloud: a bare repository in a folder stands in
# for the cloud, no network; its display name Team) with bolts/hex-bolt; in
# three starts of MitCad:
#   1. start-page-fixture: the three opened, changed and saved one after
#      another (each save writes its preview), the page off at launch
#      (a file opens, and MITCAD_START_PAGE=0 as in every UI test);
#   2. start-page, with MITCAD_START_PAGE=1 and Show start page at launch
#      at its default (on): the Start tab at launch with the three designs
#      and their previews, a card opens its design, the star pins and the
#      Pinned filter and the search filter the list, Show start page at
#      launch turned off; the projects: Shop, its folder and sub-folder and
#      back up by the breadcrumb, the search finds Team's design with its
#      project and folder, a project's card opens its design, which joins
#      Recent; New Design gives an empty design in the default units
#      (inches, set by the fixture);
#   3. start-page-off, MITCAD_START_PAGE=1 again: MitCad starts on an empty
#      design on the Solid tab, as before the Start tab.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-start-page-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-start-page.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
for name in a b c; do
  "$cli" run "$here/ui-scripts/start-page.json" --save "$work/start-$name.mitcad" || exit 1
done

# The two projects (as tools/ui-open-from-project-test.sh makes them).
command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
export MITCAD_PROJECTS_DIR=$work/projects
shop=$MITCAD_PROJECTS_DIR/Shop
team=$MITCAD_PROJECTS_DIR/Team
mkdir -p "$MITCAD_PROJECTS_DIR" "$work/remotes"
project_cli() {
  "$cli" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log" >&2; ui_fail "mitcad-cli $*"; exit 1; }
}
project_cli project init "$shop" --author "$author"
mkdir -p "$shop/parts/sub"
for design in plate parts/gear parts/sub/pin; do
  project_cli run "$here/ui-scripts/start-page.json" --save "$shop/$design.mitcad"
  project_cli version save "$shop/$design.mitcad" -m "Add $design" --author "$author"
done
git init -q --bare "$work/remotes/team.git"
project_cli project create "$team" --author "$author" --url "$work/remotes/team.git"
mkdir -p "$team/bolts"
project_cli run "$here/ui-scripts/start-page.json" --save "$team/bolts/hex-bolt.mitcad"
project_cli version save "$team/bolts/hex-bolt.mitcad" -m "Add the bolt" --author "$author"
project_cli project settings "$team" --set '{"local": {"name": "Team"}}' --author "$author"
echo "ok   the projects are made"
ui_start_display

# run_script <name> <script> [app arguments...]: one start of MitCad with
# the shared steps; the app ends with them.
run_script() {
  local name=$1 script=$2
  shift 2
  export MITCAD_TEST_INPUT=$here/ui-scripts/$script
  ui_start_app --ui=qt --no-mcp "$@"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$name: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$name: the driver failed"; }
  ui_expect_log 'TestDriver: done' "$name: the steps"
}

run_script fixture start-page-fixture.mitcad-ui --open "$work/start-a.mitcad"
for name in a b c; do
  ui_expect_log "Design preview saved: start-$name.mitcad (" "start-$name.mitcad saved with its preview"
done

export MITCAD_START_PAGE=1
run_script start start-page.mitcad-ui
ui_expect_log 'Start page at launch: shown' 'the Start tab at launch'
ui_expect_log 'Start page recent designs (all): start-c.mitcad (preview) | start-b.mitcad (preview) | start-a.mitcad (preview)' \
  'the three designs with their previews, newest first'
ui_expect_log 'Start page pinned: start-b.mitcad' 'the star pins a design'
ui_expect_log 'Start page recent designs (pinned): start-b.mitcad (preview)' 'Pinned shows the pinned design alone'
ui_expect_log 'Start page search results: 1 (start-a.mitcad)' 'the search finds a recent design'
ui_expect_log 'Start page projects: Shop (local) | Team (cloud)' 'the projects in the left column'
ui_expect_log 'Start page place: Shop/parts/sub/ (0 folders, 1 designs)' 'a project'"'"'s folder and its sub-folder'
ui_expect_log 'Start page crumb Shop at ' 'the breadcrumb'
ui_expect_log 'Start page search results: 1 (Team/bolts/hex-bolt.mitcad)' 'the search finds a design of the other project'
ui_expect_log 'Start page search result: Team/bolts/hex-bolt.mitcad (Team / bolts)' 'with its project and folder'
ui_expect_log 'Start page chosen: open gear.mitcad' 'a project'"'"'s card opens its design'
ui_expect_log 'Start page recent designs (all): gear.mitcad (no preview) | start-c.mitcad (preview)' \
  'the project'"'"'s design joined Recent'
ui_expect_log 'Start page chosen: open start-c.mitcad' 'a card opens its design'
ui_expect_log 'Start page at launch: off' 'Show start page at launch turned off'
ui_expect_log 'New design in in' 'New Design: an empty design in the default units'

run_script off start-page-off.mitcad-ui
ui_expect_log 'Start page at launch: off (Show start page at launch)' 'off: no Start page at launch'
ui_finish 'UI Start tab: recent designs with previews, pins, the projects'"'"' browser and its search, New Design and the launch choice'

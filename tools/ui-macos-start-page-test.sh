#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/StartBrowser.cpp app/files/RecentDesigns.cpp app/MainWindowStart.cpp
# check-all sources: app/projects/ProjectBrowserData.cpp app/projects/ProjectsTree.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/start-page.json tools/ui-scripts/start-page-fixture.mitcad-ui
# check-all sources: tools/ui-scripts/start-page.mitcad-ui tools/ui-scripts/start-page-off.mitcad-ui
# The Start tab (mitcad#1258) and its project browser (mitcad#1301) on
# macOS, in the Qt frontend or AppKit's, through the shared scripts of
# tools/ui-start-page-test.sh (Linux) and tools/ui-windows-start-page-test.ps1
# (Windows): three designs that mitcad-cli makes (start-page.json) and two
# projects of the projects folder, Shop (Local) with plate, parts/gear and
# parts/sub/pin and Team (Cloud through a bare repository in a folder; its
# display name Team) with bolts/hex-bolt; three starts of MitCad:
#   1. start-page-fixture: the three opened, changed and saved (each save
#      writes its preview), no Start page at launch;
#   2. start-page with MITCAD_START_PAGE=1: the Start tab at launch with the
#      three designs and their previews, the star and Pinned, the search,
#      Show start page at launch turned off, the projects (a folder, its
#      sub-folder, back up by the breadcrumb, the search finds Team's design
#      with its project and folder, a project's card opens its design, which
#      joins Recent), a card opens its design, New Design in inches;
#   3. start-page-off: MitCad starts on an empty design, as before.
# Usage: tools/ui-macos-start-page-test.sh <app> <mitcad-cli> <output directory> [qt|native]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory> [qt|native]" >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
case "$frontend" in qt|native) ;; *) echo 'expected qt or native' >&2; exit 2 ;; esac
command -v git > /dev/null || { echo "FAIL: git is required" >&2; exit 2; }
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_ROOT/files MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK" "$UI_ROOT/remotes"
setup() {
  "$cli" "$@" >> "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; echo "FAIL: mitcad-cli $*"; exit 1; }
}
for name in a b c; do
  setup run "$here/ui-scripts/start-page.json" --save "$MITCAD_TEST_WORK/start-$name.mitcad"
done
# The two projects in the run's projects folder (ui_init's MITCAD_PROJECTS_DIR).
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
shop=$MITCAD_PROJECTS_DIR/Shop
team=$MITCAD_PROJECTS_DIR/Team
setup project init "$shop" --author "$author"
mkdir -p "$shop/parts/sub"
for design in plate parts/gear parts/sub/pin; do
  setup run "$here/ui-scripts/start-page.json" --save "$shop/$design.mitcad"
  setup version save "$shop/$design.mitcad" -m "Add $design" --author "$author"
done
git init -q --bare "$UI_ROOT/remotes/team.git" || { echo "FAIL: git init --bare"; exit 1; }
setup project create "$team" --author "$author" --url "$UI_ROOT/remotes/team.git"
mkdir -p "$team/bolts"
setup run "$here/ui-scripts/start-page.json" --save "$team/bolts/hex-bolt.mitcad"
setup version save "$team/bolts/hex-bolt.mitcad" -m "Add the bolt" --author "$author"
setup project settings "$team" --set '{"local": {"name": "Team"}}' --author "$author"
ui_pass "the designs and the projects are made"

MITCAD_START_PAGE=0 MITCAD_TEST_INPUT=$here/ui-scripts/start-page-fixture.mitcad-ui \
  ui_run_app start-page-fixture --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/start-a.mitcad"
ui_expect_log '^TestDriver: done' 'start-page-fixture: the steps'
for name in a b c; do
  ui_expect_log "^Design preview saved: start-$name\\.mitcad \\(" "start-$name.mitcad saved with its preview"
done

MITCAD_START_PAGE=1 MITCAD_TEST_INPUT=$here/ui-scripts/start-page.mitcad-ui \
  ui_run_app start-page --ui="$frontend" --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'start-page: the steps'
ui_expect_log '^Start page at launch: shown$' 'the Start tab at launch'
ui_expect_log '^Start page pinned: start-b\.mitcad$' 'the star pins a design'
ui_expect_log '^Start page search results: 1 \(start-a\.mitcad\)$' 'the search finds a recent design'
ui_expect_log '^Start page projects: Shop \(local\) \| Team \(cloud\)$' 'the projects in the left column'
ui_expect_log '^Start page place: Shop/parts/sub/ \(0 folders, 1 designs\)$' "a project's folder and its sub-folder"
ui_expect_log '^Start page search result: Team/bolts/hex-bolt\.mitcad \(Team / bolts\)$' \
  'the search finds a design of the other project with its project and folder'
ui_expect_log '^Start page recent designs \(all\): gear\.mitcad \(no preview\) \| start-c\.mitcad \(preview\)' \
  "a project's card opened its design, which joined Recent"
ui_expect_log '^New design in in$' 'New Design: an empty design in the default units'
if [ "$frontend" = native ]; then
  ui_expect_log '^Native cards: none \(the Start page is shown\)$' 'AppKit: no card over the page'
fi

MITCAD_START_PAGE=1 MITCAD_TEST_INPUT=$here/ui-scripts/start-page-off.mitcad-ui \
  ui_run_app start-page-off --ui="$frontend" --no-recovery --no-mcp
ui_expect_log '^Start page at launch: off \(Show start page at launch\)$' 'off: no Start page at launch'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend: the Start tab, its project browser and its search"

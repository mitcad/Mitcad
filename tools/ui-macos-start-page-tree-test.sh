#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/StartBrowser.cpp app/MainWindowStart.cpp app/framework/TestDriver.cpp
# check-all sources: app/projects/ProjectBrowserData.cpp app/projects/ProjectsTree.cpp core/vcs tools/cli
# check-all sources: tools/ui-scripts/start-page.json tools/ui-scripts/start-page-tree.mitcad-ui
# check-all sources: tools/ui-scripts/start-page-tree-again.mitcad-ui
# The Start page's Tree / Cards toggle (mitcad#2093) on macOS, through the
# shared scripts of tools/ui-start-page-tree-test.sh (Linux) and
# tools/ui-windows-start-page-tree-test.ps1 (Windows): one Local project of
# the projects folder, Shop, with plate, parts/gear and parts/sub/pin; two
# starts of MitCad with the Start page at launch:
#   1. start-page-tree: Tree shows Shop's folders, choosing parts and then
#      parts/sub in it shows their cards, pin's card opens it; Cards goes
#      back to the breadcrumb view at the same folder; Tree again;
#   2. start-page-tree-again: after the restart Tree at parts/sub; the
#      arrow keys walk and fold the tree, Enter opens gear.
# Usage: tools/ui-macos-start-page-tree-test.sh <app> <mitcad-cli> <output directory>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
command -v git > /dev/null || { echo "FAIL: git is required" >&2; exit 2; }
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_ROOT/files MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
setup() {
  "$cli" "$@" >> "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; echo "FAIL: mitcad-cli $*"; exit 1; }
}
# The project in the run's projects folder (ui_init's MITCAD_PROJECTS_DIR).
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
shop=$MITCAD_PROJECTS_DIR/Shop
setup project init "$shop" --author "$author"
mkdir -p "$shop/parts/sub"
for design in plate parts/gear parts/sub/pin; do
  setup run "$here/ui-scripts/start-page.json" --save "$shop/$design.mitcad"
  setup version save "$shop/$design.mitcad" -m "Add $design" --author "$author"
done
ui_pass "the project is made"

MITCAD_START_PAGE=1 MITCAD_TEST_INPUT=$here/ui-scripts/start-page-tree.mitcad-ui \
  ui_run_app start-page-tree --ui=qt --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'start-page-tree: the steps'
ui_expect_log '^Start page tree: Shop \| Shop/parts/ \| Shop/parts/sub/ \(current Shop\)$' "Tree shows the project's folders"
ui_expect_log '^Start page place: Shop/parts/sub/ \(0 folders, 1 designs\)$' 'the inner folder chosen in the tree'
ui_expect_log '^Start page chosen: open pin\.mitcad$' 'its card opens its design'
ui_expect_log '^Start page browse view: cards$' 'Cards goes back to the breadcrumb view'

MITCAD_START_PAGE=1 MITCAD_TEST_INPUT=$here/ui-scripts/start-page-tree-again.mitcad-ui \
  ui_run_app start-page-tree-again --ui=qt --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'start-page-tree-again: the steps'
ui_expect_log '^Start page tree: Shop \| Shop/parts/ \| Shop/parts/sub/ \(current Shop/parts/sub/\)$' \
  'Tree at the same folder after the restart'
ui_expect_log '^Start page tree folded: Shop/parts/$' 'Left folds a folder'
ui_expect_log '^Start page chosen: open gear\.mitcad$' "Enter opens a design from the tree's folder"
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail "no failed driver assertion: 'TestDriver: FAIL' in the log"
else
  ui_pass 'no failed driver assertion'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: the Start page's Tree / Cards toggle"

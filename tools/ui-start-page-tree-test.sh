#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/StartBrowser.cpp app/MainWindowStart.cpp app/framework/TestDriver.cpp
# check-all sources: app/projects/ProjectBrowserData.cpp app/projects/ProjectsTree.cpp core/vcs tools/cli
# check-all sources: tools/ui-scripts/start-page.json tools/ui-scripts/start-page-tree.mitcad-ui
# check-all sources: tools/ui-scripts/start-page-tree-again.mitcad-ui
# The Start page's Tree / Cards toggle (mitcad#2093) through the real UI,
# the shared input scripts tools/ui-scripts/start-page-tree*.mitcad-ui
# (also run by tools/ui-windows-start-page-tree-test.ps1 and
# tools/ui-macos-start-page-tree-test.sh) on one Local project of the
# projects folder, Shop, with plate at its top, parts/gear and
# parts/sub/pin, in two starts of MitCad with the Start page at launch:
#   1. start-page-tree: Tree shows Shop's folders, choosing parts and then
#      parts/sub in it shows their cards, pin's card opens it; Cards goes
#      back to the breadcrumb view at the same folder; Tree again;
#   2. start-page-tree-again: after the restart Tree at parts/sub; the
#      arrow keys walk and fold the tree, Enter goes into a folder's cards
#      and Enter opens gear.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-start-page-tree-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-start-page-tree.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

# The project (as tools/ui-start-page-test.sh makes Shop).
command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
export MITCAD_PROJECTS_DIR=$work/projects
shop=$MITCAD_PROJECTS_DIR/Shop
mkdir -p "$MITCAD_PROJECTS_DIR"
project_cli() {
  "$cli" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log" >&2; ui_fail "mitcad-cli $*"; exit 1; }
}
project_cli project init "$shop" --author "$author"
mkdir -p "$shop/parts/sub"
for design in plate parts/gear parts/sub/pin; do
  project_cli run "$here/ui-scripts/start-page.json" --save "$shop/$design.mitcad"
  project_cli version save "$shop/$design.mitcad" -m "Add $design" --author "$author"
done
echo "ok   the project is made"
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

export MITCAD_START_PAGE=1
run_script tree start-page-tree.mitcad-ui
ui_expect_log 'Start page tree: Shop | Shop/parts/ | Shop/parts/sub/ (current Shop)' 'Tree shows the project'"'"'s folders'
ui_expect_log 'Start page place: Shop/parts/sub/ (0 folders, 1 designs)' 'the inner folder chosen in the tree'
ui_expect_log 'Start page chosen: open pin.mitcad' 'its card opens its design'
ui_expect_log 'Start page browse view: cards' 'Cards goes back to the breadcrumb view'
ui_expect_log 'Start page crumb Shop/parts/sub/ at ' 'at the same folder'

run_script again start-page-tree-again.mitcad-ui
ui_expect_log 'Start page tree: Shop | Shop/parts/ | Shop/parts/sub/ (current Shop/parts/sub/)' \
  'Tree at the same folder after the restart'
ui_expect_log 'Start page tree folded: Shop/parts/' 'Left folds a folder'
ui_expect_log 'Start page tree unfolded: Shop/parts/' 'Right unfolds it'
ui_expect_log 'Start page chosen: open gear.mitcad' 'Enter opens a design from the tree'"'"'s folder'
ui_finish 'UI Start page Tree / Cards: the project'"'"'s folders as a tree, its cards, the keyboard and the choice after a restart'

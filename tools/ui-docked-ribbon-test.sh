#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowNavigationBar.cpp app/DocumentFrame.cpp app/WindowManager.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverChrome.cpp app/view/NavigationBar.cpp
# check-all sources: app/build app/commands/BuildCommands.cpp core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/docked-ribbon.mitcad-ui tools/ui-scripts/docked-ribbon-bar.json
# check-all sources: tools/ui-scripts/docked-ribbon-library.json
# The ribbon, the Browser and the navigation bar stay with the design in
# front (mitcad#1451) through the real UI, the shared input script
# tools/ui-scripts/docked-ribbon.mitcad-ui (also run by
# tools/ui-windows-docked-ribbon-test.ps1), in the Docked layout and then
# in the Floating one, with a library of one synthetic BUILD profile made
# here (mitcad-cli: a 20 x 20 bar, tools/ui-scripts/docked-ribbon-bar.json;
# no supplier's files), fetched from a local git repository:
#   - At the start, after a member placed with Build > Profile, in a second
#     design and back in the first: the ribbon and the navigation bar are
#     shown and the view's top is below the ribbon ("Design chrome: ...").
#   - A right-click on the ribbon's tab row opens the window's menu of its
#     panels (Browser, Timeline, Command, ...) without Toolbar: in the
#     Docked layout that entry hid the whole ribbon of that design, with
#     nothing to bring it back but a new design, and the hidden ribbon came
#     back with the design whenever it was in front again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-docked-ribbon-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-docked-ribbon.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=${UI_DOCKED_RIBBON_SCRIPT:-$here/ui-scripts/docked-ribbon.mitcad-ui}
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-ribbon
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

"$cli" library init "$lib" --id test-ribbon --name "Test ribbon" --license MIT > /dev/null ||
  { ui_fail "library init"; exit 1; }
mkdir -p "$lib/profiles"
"$cli" run "$here/ui-scripts/docked-ribbon-bar.json" --save "$lib/profiles/bar-20-test.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the bar"; exit 1; }
cp "$here/ui-scripts/docked-ribbon-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m "Test profile"
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || { ui_fail "library fetch"; exit 1; }
ui_start_display

# run_layout docked|floating: the script in that layout, to its end.
run_layout() {
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the ribbon driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the ribbon driver failed"; exit 1; }
  grep -E '^(Design chrome|Window context menu): ' "$UI_LOG"
  if [ "$1" = docked ]; then
    ui_expect_log 'Window context menu: Browser | ' "docked: the right-click on the ribbon's tab row opened the window's menu"
  fi
  ui_expect_log 'Placed Bar 20 test as ' "$1: a member placed with Build > Profile"
  ui_expect_log 'Document window created: window-2 ' "$1: a second design"
  ui_expect_log 'TestDriver: done' "$1: the ribbon and the navigation bar stay with the design in front"
}

run_layout docked
run_layout floating
ui_finish "UI ribbon with the design in front"

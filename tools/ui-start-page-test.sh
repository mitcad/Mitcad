#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/RecentDesigns.cpp app/MainWindowStart.cpp app/main.cpp
# check-all sources: app/framework/Ribbon.cpp app/files/MainWindowFiles.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/start-page.json tools/ui-scripts/start-page-fixture.mitcad-ui
# check-all sources: tools/ui-scripts/start-page.mitcad-ui tools/ui-scripts/start-page-off.mitcad-ui
# The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125) through the real UI,
# the shared input scripts tools/ui-scripts/start-page*.mitcad-ui (also run
# by tools/ui-windows-start-page-test.ps1) on three designs that mitcad-cli
# makes (tools/ui-scripts/start-page.json), in three starts of MitCad:
#   1. start-page-fixture: the three opened, changed and saved one after
#      another (each save writes its preview), the page off at launch
#      (a file opens, and MITCAD_START_PAGE=0 as in every UI test);
#   2. start-page, with MITCAD_START_PAGE=1 and Show start page at launch
#      at its default (on): the Start tab at launch with the three designs
#      and their previews, a card opens its design, the star pins and the
#      Pinned filter and the search filter the list, Show start page at
#      launch turned off, New Design gives an empty design in the default
#      units (inches, set by the fixture);
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
ui_expect_log 'Start page recent designs (all, "start-a"): start-a.mitcad (preview)' 'the search finds a design'
ui_expect_log 'Start page chosen: open start-c.mitcad' 'a card opens its design'
ui_expect_log 'Start page at launch: off' 'Show start page at launch turned off'
ui_expect_log 'New design in in' 'New Design: an empty design in the default units'

run_script off start-page-off.mitcad-ui
ui_expect_log 'Start page at launch: off (Show start page at launch)' 'off: no Start page at launch'
ui_finish 'UI Start tab: recent designs with previews, pins, search, New Design and the launch choice'

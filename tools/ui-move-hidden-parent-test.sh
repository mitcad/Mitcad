#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/commands/MoveCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/browser/BrowserPanel.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/move-hidden-parent.mitcad-ui
# Move on a component hidden with its parent (mitcad#1995, GitHub
# mitcad/Mitcad#430), through the real UI: the shared input script
# tools/ui-scripts/move-hidden-parent.mitcad-ui (also run by
# tools/ui-windows-move-hidden-parent-test.ps1 and
# tools/ui-macos-move-hidden-parent-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, github-430; never committed),
# unpacked into the test's own folder: the component inserted under the
# hidden Fairlead:1, picked in the Browser, is moved 40 mm along X by
# Move/Copy, whose panel once said "Select what moves." for it. Skipped
# without the design.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-hidden-parent-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
zip=$(find "${MITCAD_MITCAD_REPORTED:-/nonexistent}/github-430" -maxdepth 1 -name '*.zip' -type f 2> /dev/null | sort | head -1)
if [ -z "$zip" ] || ! command -v unzip > /dev/null; then
  echo "skip: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-430) or unzip is not here"
  exit 0
fi
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-move-hidden-parent.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/move-hidden-parent.mitcad-ui
# The design in the test's folder: the run writes nothing next to the
# private test files.
unzip -o -q "$zip" -d "$work/reported" || { echo "FAIL: the reported design could not be unpacked" >&2; exit 1; }
design=$(find "$work/reported" -name '*.mitcad' -type f | sort | head -1)
[ -n "$design" ] || { echo "FAIL: no design in the reported zip" >&2; exit 1; }
ui_start_display
UI_WINDOW_WAIT=180 ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'move hidden parent driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log ': hidden with Fairlead:1' 'the rows in Fairlead:1 are hidden with it'
ui_expect_log 'Move/Copy Objects: 1 component [component C3 in O1/O2/O3]' 'Move/Copy takes the hidden component'
ui_expect_log 'Moved 1 components (' 'OK moves it'
ui_expect_log 'TestDriver: done' 'every step passed'
ui_finish 'UI Move of a component hidden with its parent'

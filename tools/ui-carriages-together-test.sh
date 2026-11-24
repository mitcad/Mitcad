#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: core/model/src/build/follow.rs core/model/src/joints.rs core/model/src/document.rs
# check-all sources: app/MainWindowJoints.cpp app/commands/JointCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverModel.cpp
# check-all sources: tools/ui-scripts/carriages-together.mitcad-ui
# Two bearings on one shaft joined by a Rigid joint or a Rigid Group move
# together (mitcad#2274, GitHub mitcad/Mitcad#575), through the real UI:
# the shared input script tools/ui-scripts/carriages-together.mitcad-ui
# (also run by tools/ui-windows-carriages-together-test.ps1 and
# tools/ui-macos-carriages-together-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, github-575; never
# committed), unpacked into the test's own folder: a Rigid joint between
# the bearings at their distance holds, a drag of one moves both (both
# Placement Offsets change), and a drag past one's travel limit stops both
# there; undone, the same with a Rigid Group. Skipped without the design.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-carriages-together-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
zip=$(find "${MITCAD_MITCAD_REPORTED:-/nonexistent}/github-575" -maxdepth 1 -name '*.zip' -type f 2> /dev/null | sort | head -1)
if [ -z "$zip" ] || ! command -v unzip > /dev/null; then
  echo "skip: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-575) or unzip is not here"
  exit 0
fi
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-carriages-together.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/carriages-together.mitcad-ui
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
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'carriages together driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Joint Origin B: 1 face' 'the Rigid joint picks both bearings'"'"' end faces'
ui_expect_log ' (rigid)' 'the Rigid joint at their distance is added'
ui_expect_log 'Added rigid group ' 'the Rigid Group of the two bearings is added'
ui_expect_log 'Dragged ' 'the bearings are dragged along the shaft'
ui_expect_log 'TestDriver: done' 'both move together and stop at the first travel limit'
ui_finish 'UI bearings joined rigidly move together'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/browser/DocumentSnapshot.cpp app/browser/TimelineWidget.cpp app/browser/FeatureStatus.cpp
# check-all sources: core/model/src/features/mod.rs core/model/src/recompute.rs core/model/src/document
# check-all sources: core/model-api/src/lib.rs core/ffi/src/kernel/dressup.rs core/cpp/bridge/dressup.cpp
# check-all sources: geometry/src/dressup.cpp geometry/include/mitcad/geometry/shape.hpp
# check-all sources: tools/ui-scripts/fillet-chain-remark.mitcad-ui app/framework/TestDriver.cpp app/main.cpp
# A fillet's short tangent chain is a remark, not a warning (mitcad#1914),
# through the real UI: the shared input script
# tools/ui-scripts/fillet-chain-remark.mitcad-ui (also run by
# tools/ui-windows-fillet-chain-remark-test.ps1 and
# tools/ui-macos-fillet-chain-remark-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, fillet-chain-note/; never
# committed), copied into the test's own folder. Its accepted Fillet12
# marks nothing on the Timeline; edited again, the panel points the short
# chain out as information and the pointer over the message paints the
# edges left sharp. Skipped without the design.
#
# The script's steps start while the start-up open still computes the
# design: its first step waits for the view as long as the open takes under
# check-all's load, and a step that fails before the main event loop runs
# ends the app at once (mitcad#1933; it used to keep running until the
# timeout below).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-chain-remark-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
reported=${MITCAD_MITCAD_REPORTED:-}/fillet-chain-note
if [ -z "${MITCAD_MITCAD_REPORTED:-}" ] || [ ! -d "$reported" ]; then
  echo "skip: the reported design of the private test files (MITCAD_MITCAD_REPORTED/fillet-chain-note) is not here"
  exit 0
fi
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-chain-remark.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-chain-remark.mitcad-ui
# The project with its design, in the test's folder: the run writes nothing
# next to the private test files.
cp -R "$reported" "$work/reported" || { echo "FAIL: the reported design could not be copied" >&2; exit 1; }
design=$(find "$work/reported" -name '*.mitcad' -type f | sort | head -1)
[ -n "$design" ] || { echo "FAIL: no design in the reported folder" >&2; exit 1; }
ui_start_display
UI_WINDOW_WAIT=180 ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 6000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'fillet chain remark driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Fillet: ok, remark: ' 'the panel points the short chain out'
ui_expect_log 'Panel Fillet message: information: ' 'as information, not a warning'
ui_expect_log 'TestDriver: done' 'nothing on the timeline after OK, the remark in the open panel'
ui_finish 'UI fillet chain remark'

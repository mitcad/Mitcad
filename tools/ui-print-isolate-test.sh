#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPrint.cpp app/files/Print3d.cpp app/browser/DocumentSnapshot.cpp
# check-all sources: app/framework/TestDriverModel.cpp core/model/src/exchange
# Send to Slicer with Isolate (mitcad#2475) on tools/ui-scripts/print-isolate.json:
# Pin placed twice (Pin:1, and Pin:2 turned a quarter), Cube in the design
# itself and one print plate.
#   1. STL (tools/ui-scripts/print-isolate-stl.mitcad-ui): Pin:2 isolated
#      and its pin selected, the panel starts with that placement alone
#      and Send writes one STL file, Pin:2's.
#   2. 3MF (tools/ui-scripts/print-isolate.mitcad-ui): with Pin:2 isolated
#      and nothing selected Bodies starts with the placement shown and
#      Send the Design's Plates off, so Send writes the bodies; the box
#      ticked sends the plates; with Isolate off the plates start on
#      again; Pin:2 isolated and selected sends one 3MF placement, Pin:2's.
# The files are read by tools/ui-scripts/print-isolate-verify.py.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-isolate-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-print-isolate.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
# The app's temporary folder, where the files for the slicer go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR" "$XDG_CONFIG_HOME/Mitcad"
FOLDER=$TMPDIR/mitcad-print/isolate
"$CLI" run "$here/ui-scripts/print-isolate.json" --save "$WORK/isolate.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
for format in stl 3mf; do
  script=$here/ui-scripts/print-isolate.mitcad-ui
  [ "$format" = stl ] && script=$here/ui-scripts/print-isolate-stl.mitcad-ui
  export MITCAD_TEST_INPUT=$script
  # A slicer that does nothing: the files stay where they were written.
  printf '[print]\nslicerName=Fixture slicer\nslicerProgram=%s\nformat=%s\nreuseWindow=false\n' \
    "$(command -v true)" "$format" > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
  ui_start_app --ui=qt --no-mcp --no-recovery --open "$WORK/isolate.mitcad"
  for _ in $(seq 1 1200); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the $format driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  if [ "$format" = stl ]; then
    ui_expect_log 'TestDriver: done' "STL: Pin:2 isolated and selected, its placement alone sent"
    python3 -I "$here/ui-scripts/print-isolate-verify.py" stl "$FOLDER" || ui_fail "the STL file of Pin:2"
  else
    ui_expect_log 'TestDriver: done' \
      "3MF: Bodies follows Isolate and the selection, the plates off until ticked, on again without Isolate"
    python3 -I "$here/ui-scripts/print-isolate-verify.py" 3mf "$FOLDER/isolate.3mf" || ui_fail "the 3MF of Pin:2"
  fi
done
ui_finish 'UI Send to Slicer with Isolate and a selection'

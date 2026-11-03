#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPlates.cpp app/files/MainWindowPrint.cpp app/files/Print3d.cpp
# check-all sources: app/print/OwnSettingsDialog.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/print_plates.rs core/model/src/document/print_plates.rs
# check-all sources: core/model/src/exchange core/model/src/print_files core/model-api/src/print_plates.rs tools/cli
# Plates… (mitcad#1050), the shared input script
# tools/ui-scripts/print-plates.mitcad-ui (also run by
# tools/ui-windows-print-plates-test.ps1 and tools/ui-macos-print-plates-test.sh)
# on two boxes: Plates… adds Plate 1 and Plate 2, Body2 picked goes on
# Plate 2, Spiral Vase is set for it and the plate is drawn beside the
# design; Send to Slicer (a fixture slicer that does nothing) sends the
# design's plates, and the 3MF written has two plates with the spiral
# mode on Plate 2 only (tools/ui-scripts/print-plates-verify.py). Light in
# the Floating layout, dark in the Docked one, each with its picture.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-plates-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-print-plates.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/print-plates.mitcad-ui
# The app's temporary folder, where the files for the slicer go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR" "$XDG_CONFIG_HOME/Mitcad"
ui_start_display
for run in light:floating dark:docked; do
  scheme=${run%%:*}
  chrome=${run#*:}
  "$CLI" run "$here/ui-scripts/print-plates.json" --save "$WORK/plates.mitcad" > "$WORK/cli.log" 2>&1 ||
    { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
  # A slicer that does nothing: the 3MF stays where it was written.
  printf '[print]\nslicerName=Fixture slicer\nslicerProgram=%s\nformat=3mf\nreuseWindow=false\n' \
    "$(command -v true)" > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
  export MITCAD_TEST_COLOR_SCHEME=$scheme
  ui_start_app --ui=qt --no-mcp --chrome="$chrome" --open "$WORK/plates.mitcad"
  for _ in $(seq 1 1200); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "Plates… driver timed out ($scheme)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'TestDriver: done' "Plates… adds plates, puts Body2 on Plate 2 in vase mode, Send writes them ($scheme, $chrome)"
  python3 -I "$here/ui-scripts/print-plates-verify.py" "$TMPDIR/mitcad-print/plates/plates.3mf" ||
    ui_fail "the 3MF of the plates ($scheme)"
  [ -s "$WORK/print-plates-$scheme.png" ] || ui_fail "no picture of the plate ($scheme)"
  # The pictures for the ticket, kept beside the test's log.
  cp "$WORK/print-plates-$scheme.png" "$ROOT/build/print-plates-$chrome-$scheme.png" 2> /dev/null
done
ui_finish 'UI Plates… and the plates sent to the slicer'

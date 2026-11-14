#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPrint.cpp app/MainWindowMake.cpp app/print/MakeBodyCard.cpp
# check-all sources: app/framework/PickerComboBox.cpp app/browser/LibraryPicker.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/print_settings.rs core/model/src/exchange core/model/src/print_files tools/cli
# Send to Slicer's Process row shows the process set on the body
# (mitcad#1799), the shared input script
# tools/ui-scripts/body-process-send.mitcad-ui (also run by
# tools/ui-windows-body-process-send-test.ps1) on two boxes and the
# design's processes Fixture Fine and Fixture Draft, none on the root: a
# process set on Body1 on the Make card is the row's choice, Body2 with
# another one gives Per body, and Fixture Fine chosen in the panel is set
# on both bodies in one undo step; Send (a fixture slicer that does
# nothing, 3MF) writes the file, read back here
# (tools/ui-scripts/body-process-send-verify.py): Fixture Fine for both
# bodies, no part of its own with Fixture Draft's layers.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-body-process-send-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-body-process-send.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/body-process-send.mitcad-ui
# The app's temporary folder, where the files for the slicer go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR" "$XDG_CONFIG_HOME/Mitcad"
"$CLI" run "$here/ui-scripts/body-process-send.json" --save "$WORK/body-process.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
# A slicer that does nothing, 3MF: the file stays where it was written.
printf '[print]\nslicerName=Fixture slicer\nslicerProgram=%s\nformat=3mf\nreuseWindow=false\n' \
  "$(command -v true)" > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$WORK/body-process.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "Send to Slicer process driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' \
  "the body's process is the Process row's choice, Per body for two, a choice sets it on both bodies"
python3 -I "$here/ui-scripts/body-process-send-verify.py" "$TMPDIR/mitcad-print/body-process/body-process.3mf" ||
  ui_fail "the 3MF of the two bodies"
ui_finish "UI Send to Slicer's Process row and the bodies' own process"

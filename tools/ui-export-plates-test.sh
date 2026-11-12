#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/files/MainWindowFiles.cpp app/files/MainWindowPlates.cpp
# check-all sources: app/print/PrintKeys.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/print_plates.rs core/model/src/document/print_plates.rs
# check-all sources: core/model/src/exchange core/model/src/print_files core/model-api/src/print_plates.rs tools/cli
# File > Export of a 3MF file with the design's print plates (mitcad#1679),
# the shared input script tools/ui-scripts/export-plates.mitcad-ui (also run
# by tools/ui-windows-export-plates-test.ps1) on two boxes on two plates,
# Plate 2 in vase mode: the Export dialog offers the plates; all of them in
# one file write a project of two plates with the spiral mode on Plate 2 only
# (tools/ui-scripts/print-plates-verify.py, as Send to Slicer's), a file per
# plate writes one file each, and Only Plate 2 that plate alone.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-export-plates-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-export-plates.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/export-plates.mitcad-ui
ui_start_display
"$CLI" run "$here/ui-scripts/export-plates.json" --save "$WORK/plates.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_app --ui=qt --no-mcp --open "$WORK/plates.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "Export driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "File > Export writes the plates: in one file, a file per plate, one plate"
python3 -I "$here/ui-scripts/print-plates-verify.py" "$WORK/kit.3mf" || ui_fail "the 3MF of both plates"
for file in "each - Plate 1.3mf" "each - Plate 2.3mf" "two.3mf"; do
  [ -s "$WORK/$file" ] || ui_fail "no $file"
done
echo "ok   a file per plate and Plate 2 alone written"
ui_finish 'UI File > Export of the print plates'

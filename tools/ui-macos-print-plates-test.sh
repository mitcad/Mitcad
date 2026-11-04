#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPlates.cpp app/files/MainWindowPrint.cpp app/files/Print3d.cpp
# check-all sources: app/print/OwnSettingsDialog.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/exchange core/model/src/print_files core/model-api/src/print_plates.rs tools/cli
# Plates… (mitcad#1050) on macOS in Qt, light and dark, the shared input
# script tools/ui-scripts/print-plates.mitcad-ui as
# tools/ui-print-plates-test.sh runs it on Linux: Plates… adds Plate 1 and
# Plate 2, Body2 picked goes on Plate 2, Spiral Vase is set for it and the
# plate is drawn beside the design (a picture per scheme); Send to Slicer
# (a fixture slicer that does nothing) sends the design's plates, and the
# 3MF written has two plates with the spiral mode on Plate 2 only.
# Usage: tools/ui-macos-print-plates-test.sh <app> <cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=300
# QDir::tempPath and the exported files belong to this evidence directory.
export TMPDIR=$UI_OUT/tmp
mkdir -p "$TMPDIR"
for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR"
  # QSettings uses the macOS organization domain even with IniFormat.
  mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
  printf '[print]\nslicerName=Fixture slicer\nslicerProgram=/usr/bin/true\nformat=3mf\nreuseWindow=false\n' \
    > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
  export MITCAD_TEST_WORK=$UI_OUT/files/$scheme
  rm -rf "$MITCAD_TEST_WORK"
  mkdir -p "$MITCAD_TEST_WORK"
  "$cli" run "$here/ui-scripts/print-plates.json" --save "$MITCAD_TEST_WORK/plates.mitcad" \
    > "$UI_OUT/setup-$scheme.log" 2>&1 || { cat "$UI_OUT/setup-$scheme.log" >&2; exit 1; }
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=90 MITCAD_TEST_INPUT=$here/ui-scripts/print-plates.mitcad-ui \
    ui_run_app "print-plates-$scheme" --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plates.mitcad"
  ui_expect_log '^TestDriver: done' "$scheme: Plates… adds plates, puts Body2 on Plate 2 in vase mode, Send writes them"
  for problem in '^TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'OCCT view event failed:'; do
    ui_expect_no_log "$problem" "$scheme: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
  python3 -I "$here/ui-scripts/print-plates-verify.py" "$TMPDIR/mitcad-print/plates/plates.3mf" || exit 1
  [ -s "$MITCAD_TEST_WORK/print-plates-$scheme.png" ] || { echo "FAIL: $scheme: no picture" >&2; exit 1; }
done
echo 'PASS: Plates… in Qt on macOS, light and dark'

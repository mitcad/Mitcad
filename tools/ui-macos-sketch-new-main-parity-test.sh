#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands app/commands/InspectCommands app/framework/TestDriver app/commands/MeasureHistory app/MainWindowMeasureHistory.cpp app/MainWindowMeasurements.cpp app/platform/NativeForms app/platform/macos/NativeForms.mm app/platform/macos/NativeDialogs.mm
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/MainWindowNative.cpp
# Small current-main canvas regressions, paired Qt/AppKit and light/dark.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ] || { [ "$4" != qt ] && [ "$4" != native ]; }; then
  echo "usage: $0 <app> <mitcad-cli> <output> <qt|native>" >&2
  exit 2
fi
cli=$2
frontend=$4
ui_init "$1" "$3"
UI_TIMEOUT=180
for scheme in light dark; do
  for scenario in rounded fillet symmetry first-dimension measure; do
    export MITCAD_TEST_WORK=$UI_OUT/$scenario-$scheme/files
    export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scenario-$scheme
    mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_SETTINGS_DIR"
    python3 "$here/ui-scripts/sketch-new-main-parity-fixture.py" "$scenario" \
      > "$MITCAD_TEST_WORK/setup.json" || exit 1
    "$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_WORK/model.mitcad" \
      > "$UI_OUT/setup-$scenario-$scheme.log" 2>&1 || { cat "$UI_OUT/setup-$scenario-$scheme.log" >&2; exit 1; }
    cp "$MITCAD_TEST_WORK/model.mitcad" "$MITCAD_TEST_WORK/baseline.mitcad" || exit 1
    python3 "$here/ui-scripts/sketch-new-main-parity-script.py" "$frontend" "$scenario" \
      > "$MITCAD_TEST_WORK/workflow.mitcad-ui" || exit 1
    scale=0
    if [ "$scenario" = first-dimension ]; then scale=1; fi
    MITCAD_SKETCH_FIRST_DIMENSION_SCALE=$scale MITCAD_TEST_COLOR_SCHEME=$scheme \
      MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
      ui_run_app "$scenario-$scheme" --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/model.mitcad"
    ui_expect_log 'TestDriver: done' "$frontend/$scenario/$scheme: genuine input completed"
    ui_expect_no_log 'TestDriver: FAIL' "$frontend/$scenario/$scheme: assertions passed"
    for problem in 'OCCT view event failed:' 'Native Mac canvas:' 'The document was read while a job computes it'; do
      ui_expect_no_log "$problem" "$frontend/$scenario/$scheme: no $problem"
    done
    python3 "$here/ui-scripts/sketch-new-main-parity-verify.py" "$scenario" "$MITCAD_TEST_WORK" || \
      ui_fail "$frontend/$scenario/$scheme: saved geometry/undo verification failed"
    [ "$UI_FAILURES" = 0 ] || exit 1
  done
done
echo "PASS: $frontend current-main rounded rectangles, corner fillets, Symmetry, first dimension and Measure picks"

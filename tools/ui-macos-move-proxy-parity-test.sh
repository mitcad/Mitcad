#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands app/commands/MoveDragMotion app/framework/CommandSessionProxy app/framework/DragProxy app/framework/ManipulatorOverlay app/framework/TestDriver app/platform/macos/NativeForms app/OcctViewer cmake/MacUiTests.cmake tools/cli
# Paired real held handles for #838/#854; synthetic public fixtures only.
# Usage: ui-macos-move-proxy-parity-test.sh <app> <cli> <output> <qt|native>
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
[ "$#" = 4 ] || exit 2
cli=$2
frontend=$4
case "$frontend" in qt|native) ;; *) exit 2 ;; esac
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
for fixture in motion face; do
  python3 "$here/ui-scripts/move-proxy-parity-fixture.py" "$fixture" > "$UI_OUT/$fixture.json"
  "$cli" run "$UI_OUT/$fixture.json" --save "$MITCAD_TEST_WORK/$fixture.mitcad" > "$UI_OUT/setup-$fixture.log" 2>&1 || {
    cat "$UI_OUT/setup-$fixture.log" >&2
    exit 1
  }
done
for scheme in light dark; do
  for case in body-arrow rotate-ring point-arrow face-ring; do
    rm -rf "$MITCAD_SETTINGS_DIR"
    mkdir -p "$MITCAD_SETTINGS_DIR"
    fixture=motion
    [ "$case" != face-ring ] || fixture=face
    model=$MITCAD_TEST_WORK/$case-$frontend-$scheme.mitcad
    cp "$MITCAD_TEST_WORK/$fixture.mitcad" "$model"
    script=$UI_OUT/$case-$frontend-$scheme.mitcad-ui
    python3 "$here/ui-scripts/move-proxy-parity-script.py" "$frontend" "$case" > "$script"
    # Rigid motions intentionally defer their exact preview until release.
    # Move Face instead computes exact changing solids while the ring moves.
    # No timing/throughput assertion is inferred from the 64 delivered moves.
    MITCAD_LOG_TIMING=1 MITCAD_MESH_TRACE=1 MITCAD_TEST_STEP_MS=16 MITCAD_TEST_SETTLE_MS=0 \
      MITCAD_TEST_DRAG_STEPS=64 MITCAD_TEST_DRAG_PAUSE_TICKS=6 MITCAD_TEST_DRAG_WAIT_PREVIEW=0 \
      MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$script \
      ui_run_app "$case-$frontend-$scheme" --ui="$frontend" --no-recovery --no-mcp --open "$model"
    ui_expect_log 'TestDriver: done' "$case/$frontend/$scheme: geometry and history assertions completed"
    for problem in 'TestDriver: FAIL' 'OCCT view event failed:' 'Native Mac canvas:' 'Recompute failed'; do
      ui_expect_no_log "$problem" "$case/$frontend/$scheme: no $problem"
    done
    [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
    python3 "$here/ui-scripts/move-proxy-parity-verify.py" "$UI_LOG_FILE" "$model" "$case" \
      "$UI_OUT/evidence-$case-$frontend-$scheme.json"
    [ -s "$MITCAD_TEST_WORK/$case-$frontend-$scheme-preview.png" ] || exit 1
  done
done
echo "PASS: held Move/Copy proxies, Move Face exact ring, geometry and undo/redo ($frontend, light and dark)"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandMessage.cpp app/MainWindowNoticeHover.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-conflict.mitcad-ui tools/ui-scripts/sketch-conflict.json
# check-all sources: tools/ui-scripts/sketch-conflict-replace.mitcad-ui tools/ui-scripts/sketch-conflict-replace.json
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/MainWindow.cpp
# Conflicting sketch constraints in the Qt frontend on macOS, as
# tools/ui-sketch-conflict-test.sh (Linux) and
# tools/ui-windows-sketch-conflict-test.ps1 (Windows): the shared steps
# tools/ui-scripts/sketch-conflict.mitcad-ui (mitcad#1566: a refused Sketch
# Fillet names and paints the Equal that blocks it) and
# tools/ui-scripts/sketch-conflict-replace.mitcad-ui (mitcad#2090: Replace
# for a constraint or dimension one item blocks, Shift at the click or
# Shift+Enter as a second way, no button for fixed geometry or several
# items, Esc taking it away, a stale button replacing nothing), each on the
# design mitcad-cli makes from its .json.
# Usage: bash tools/ui-macos-sketch-conflict-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-conflict}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
UI_TIMEOUT=300
for name in sketch-conflict sketch-conflict-replace; do
  "$cli" run "$here/ui-scripts/$name.json" --save "$MITCAD_TEST_WORK/$name.mitcad" \
    > "$UI_OUT/$name-setup.log" 2>&1 || { cat "$UI_OUT/$name-setup.log" >&2; exit 1; }
  MITCAD_TEST_INPUT=$here/ui-scripts/$name.mitcad-ui \
    ui_run_app "$name" --ui=qt --no-recovery --no-mcp --chrome=floating --open "$MITCAD_TEST_WORK/$name.mitcad"
  ui_expect_log 'TestDriver: done' "$name: the steps ran"
  ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' "$name: all assertions passed"
  if [ "$name" = sketch-conflict ]; then
    ui_expect_log 'Sketch conflict painted: k14 on c6, c9 \(hovered\)' 'the Equal is painted, in the hover colour over the message'
  else
    ui_expect_log 'Sketch conflict replaced: k28 by k' 'Replace swaps the Parallel for the Perpendicular'
    ui_expect_log 'Sketch conflict replaced \(Shift\): k28 by k' 'Shift at the click replaces at once'
    ui_expect_log 'Sketch conflict replace withdrawn' 'Esc takes the button away'
    ui_expect_log 'Sketch conflict replaced: k32 by k' 'Replace makes a driven dimension driving'
    ui_expect_log 'Sketch conflict replaced \(Shift\): k33 by k' 'Shift+Enter replaces at once'
    ui_expect_log 'Sketch conflict replace refused: the sketch changed since' 'a stale button replaces nothing'
    # The sketch snapshots: the Parallel gone after Replace and back after
    # Undo, the 20 mm dimension gone after its Replace.
    if grep -q '"k28"' "$MITCAD_TEST_WORK/replace-constraint.json"; then
      ui_fail 'the Parallel k28 is still there after Replace'
    fi
    grep -q '"k28"' "$MITCAD_TEST_WORK/replace-constraint-undone.json" ||
      ui_fail 'Undo did not bring the Parallel k28 back'
    if grep -q '"k32"' "$MITCAD_TEST_WORK/replace-dimension.json"; then
      ui_fail 'the 20 mm dimension k32 is still there after Replace'
    fi
  fi
done
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch conflicts and Replace; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch conflicts and Replace; evidence in $UI_OUT"

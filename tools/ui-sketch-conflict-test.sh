#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandMessage.cpp app/MainWindowNoticeHover.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-conflict.mitcad-ui tools/ui-scripts/sketch-conflict.json
# check-all sources: tools/ui-scripts/sketch-conflict-replace.mitcad-ui tools/ui-scripts/sketch-conflict-replace.json
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/MainWindow.cpp
# check-all sources: app/files/MainWindowLinks.cpp
# A Sketch Fillet refused for conflicting constraints (mitcad#1566), the
# shared input script tools/ui-scripts/sketch-conflict.mitcad-ui (also run
# by tools/ui-windows-sketch-conflict-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/sketch-conflict.json): two Equal lines between two
# arcs whose centres are fixed, the upper one 2 mm from the centre line;
# Sketch Fillet R 2 at the corner of the upper line and the left arc fails,
# the panel says in words, without ids, that the Equal can't hold with the
# fillet, and the Equal's symbol and lines are painted in the view, in the
# hover's colour while the pointer is over the message; Esc clears them.
# The screenshot (sketch-conflict.png) shows the painting.
# Then Replace (mitcad#2090), the shared input script
# tools/ui-scripts/sketch-conflict-replace.mitcad-ui on
# tools/ui-scripts/sketch-conflict-replace.json: a constraint or dimension
# refused for the one item that blocks it offers Replace, which swaps it
# out in one undo step; Shift at the click or Shift+Enter does so at once;
# no button when fixed geometry or several items block it; Esc takes it
# away; an Undo before the press replaces nothing; a constraint key never
# replaces at once.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-conflict-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-conflict.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$WORK}
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-conflict.mitcad-ui
"$CLI" run "$here/ui-scripts/sketch-conflict.json" --save "$WORK/conflict.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$WORK/conflict.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch conflict driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Panel Sketch Fillet error: the Equal between two lines can' "the panel says what can't hold, without ids"
ui_expect_log 'Sketch conflict painted: k14 on c6, c9 (hovered)' 'the Equal is painted, in the hover colour over the message'
ui_expect_log 'TestDriver: done' 'the refused fillet names and paints the Equal'

# Replace (mitcad#2090).
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-conflict-replace.mitcad-ui
"$CLI" run "$here/ui-scripts/sketch-conflict-replace.json" --save "$WORK/replace.mitcad" || exit 1
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$WORK/replace.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch conflict replace driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketch conflict replace offered: k28' 'a refusal for one blocking item offers Replace'
ui_expect_log 'Sketch conflict replaced: k28 by k' 'Replace swaps the Parallel for the Perpendicular'
ui_expect_log 'Sketch conflict replaced (Shift): k28 by k' 'Shift at the click replaces at once'
ui_expect_log 'Sketch conflict replace withdrawn' 'Esc takes the button away'
ui_expect_log 'Sketch conflict replaced: k32 by k' 'Replace makes a driven dimension driving'
ui_expect_log 'Sketch conflict replaced (Shift): k33 by k' 'Shift+Enter replaces at once'
ui_expect_log 'Sketch conflict replace refused: the sketch changed since' 'a stale button replaces nothing'
ui_expect_log 'TestDriver: done' 'Replace, Shift and their refusals'
# The sketch snapshots: the Parallel gone after Replace and back after
# Undo, the 20 mm dimension gone after its Replace.
snapshot() { grep -q "\"$2\"" "$MITCAD_TEST_WORK/$1"; }
if snapshot replace-constraint.json k28; then ui_fail 'the Parallel k28 is still there after Replace'; fi
snapshot replace-constraint-undone.json k28 || ui_fail 'Undo did not bring the Parallel k28 back'
if snapshot replace-dimension.json k32; then ui_fail 'the 20 mm dimension k32 is still there after Replace'; fi
echo "ok   the sketch snapshots: k28 replaced and back after Undo, k32 replaced"
ui_finish 'UI sketch conflicting constraints'

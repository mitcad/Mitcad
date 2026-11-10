#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandMessage.cpp app/MainWindowNoticeHover.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-conflict.mitcad-ui tools/ui-scripts/sketch-conflict.json
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
ui_finish 'UI sketch conflicting constraints'

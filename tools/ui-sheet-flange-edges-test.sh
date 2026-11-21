#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp core/model/src/sheet core/model/src/features/sheet_folded.rs
# check-all sources: tools/ui-scripts/sheet-flange-edges.mitcad-ui tools/ui-scripts/sheet-flange-edges.json tools/cli
# check-all sources: core/model/src/document/flat_patterns.rs core/model-api/src/lib.rs app/MainWindowNoticeHover.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/Command.hpp
# A Flange on several edges with mitred corners (mitcad#1872, GitHub
# mitcad/Mitcad#337) through the real UI, the shared input script
# tools/ui-scripts/sheet-flange-edges.mitcad-ui (also run by
# tools/ui-windows-sheet-flange-edges-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/sheet-flange-edges.json): three side faces of a
# 40 x 30 sheet picked into one Flange show Miter, Own Corner Gap and
# Corners and leave the keyboard in Length (mitcad#2098), Corner Gap 1 mm and Position Inside; one corner's own Miter
# (mitcad#1877: a Corners row, its corner picked, Earlier Edge Runs Past);
# Corner Gap 0 refused in words, the pointer over the message painting the
# two walls and the corner; OK at 1 mm adds one flange whose body has the
# volume cli.sheet_flange_edges gives for that corner's miter
# (2267.5253779686504 mm3), and the flange edited again loads its three
# edges, its gap and its corner's Miter.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-flange-edges-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-flange-edges.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-flange-edges.mitcad-ui
design=$work/sheet-flange-edges.mitcad
"$cli" run "$here/ui-scripts/sheet-flange-edges.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet flange edges driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Panel field length: 12' 'the keyboard in Length after the picks'
ui_expect_log 'Added flange on 3 edges' 'the three edges made one flange'
ui_expect_log 'Added Flange as F4' 'the flange was added'
ui_expect_log 'Notice hover highlighted: panel: face F3:sheet_side' "the refusal's places are painted on hover"
ui_expect_volume 'Body [A-Za-z0-9]* (F3.b0): volume [0-9.]* -> \([0-9.]*\) mm3' 2267.5253779686504 \
  "the three walls mitred with a 1 mm gap, the first corner's earlier wall running past"
ui_expect_log 'TestDriver: done' 'the flange on three edges is made and edited from the Flange panel'
ui_finish 'UI sheet flange edges'

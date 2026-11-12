#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/PickList.cpp app/view/PickListView.cpp app/framework/Selection.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/CommandSession.cpp app/MainWindow.cpp tools/cli
# check-all sources: tools/ui-scripts/pick-list.mitcad-ui tools/ui-scripts/pick-list.json
# The pick list (mitcad#1703, GitHub mitcad/Mitcad#270) through the real
# UI: the shared input script tools/ui-scripts/pick-list.mitcad-ui (also run
# by tools/ui-windows-pick-list-test.ps1) on a design that mitcad-cli makes
# (tools/ui-scripts/pick-list.json): a block below a sketch's square. A
# press held still on the square lists the profile and the block's top face
# behind it, nearest first and in words; the second row hovered highlights
# the face and chosen selects it, Ctrl adds a row, Esc and a click
# elsewhere close the list, and inside Extrude a row goes to its input.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pick-list-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-pick-list.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/pick-list.mitcad-ui
"$cli" run "$here/ui-scripts/pick-list.json" --save "$work/pick-list.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/pick-list.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'pick list driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a held press lists the profile and the face behind it; rows hovered, chosen, added, closed'
ui_finish 'UI pick list'

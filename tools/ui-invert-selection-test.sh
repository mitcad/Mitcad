#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowInvertSelection.cpp app/OcctViewer.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindow.cpp app/resources/layout/default.json app/framework/ShortcutDialog.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/invert-selection.mitcad-ui tools/ui-scripts/invert-selection.json
# Invert Selection (mitcad#2647) through the real UI: the shared input
# script tools/ui-scripts/invert-selection.mitcad-ui (also run by
# tools/ui-windows-invert-selection-test.ps1 and
# tools/ui-macos-invert-selection-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/invert-selection.json): two blocks shown and one hidden,
# and a sketch's rectangle. F7 bound in Keyboard Shortcuts; bodies inverted
# (the hidden one not taken), the faces of an isolated body, the
# rectangle's lines in the sketch (by the key and the view's context menu),
# and Interference's Bodies input.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-invert-selection-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-invert-selection.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/invert-selection.mitcad-ui
"$cli" run "$here/ui-scripts/invert-selection.json" --save "$work/invert-selection.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/invert-selection.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'invert selection driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Invert Selection: 1 face to 5 faces' 'the faces of the isolated body inverted'
ui_expect_log 'Invert Selection in Interference Bodies: nothing to 2 bodies' 'the input took the shown bodies'
ui_expect_log 'TestDriver: done' 'bodies, faces, sketch curves and a command input inverted'
ui_finish 'UI invert selection'

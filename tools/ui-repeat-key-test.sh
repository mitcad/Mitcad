#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/framework/EnterRepeat.cpp
# check-all sources: app/framework/EnterRepeat.hpp app/framework/ShortcutDialog.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/repeat-key.mitcad-ui tools/ui-scripts/repeat-key.json
# The command Repeat's own key (mitcad#2052) through the real UI, the
# shared input script tools/ui-scripts/repeat-key.mitcad-ui (also run by
# tools/ui-windows-repeat-key-test.ps1 and tools/ui-macos-repeat-key-test.sh)
# on a design that mitcad-cli makes (tools/ui-scripts/repeat-key.json): with
# no key bound Space does nothing; Repeat's row in Keyboard Shortcuts has no
# key and takes Space; after an Extrude, Space in the 3D view repeats it
# (`Repeat Extrude (Space)`), not while its panel is open, not in a sketch,
# not with the browser's tree focused, and typed into a rename it is a
# space.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-repeat-key-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-repeat-key.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/repeat-key.mitcad-ui
"$cli" run "$here/ui-scripts/repeat-key.json" --save "$work/repeat.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/repeat.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Repeat key driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Keyboard Shortcuts key: Repeat | 3D View = Space' 'Space given to Repeat in Keyboard Shortcuts'
ui_expect_log 'Repeat Extrude (Space)' 'Space in the 3D view repeats Extrude'
ui_expect_log 'Renamed Body1 to Plate X' 'Space typed into a rename is a space'
ui_expect_log 'TestDriver: done' 'the view repeats by Space; panels, sketches and the browser keep it'
ui_finish 'UI Repeat key'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# check-all sources: tools/ui-scripts/sheet-convert-triangulated.mitcad-ui
# check-all sources: tools/ui-scripts/sheet-convert-triangulated.json
# Convert to Sheet Metal by triangulation (mitcad#2442) through the real
# UI, the shared input script
# tools/ui-scripts/sheet-convert-triangulated.mitcad-ui (also run by
# tools/ui-windows-sheet-convert-triangulated-test.ps1 and
# tools/ui-macos-sheet-convert-triangulated-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/sheet-convert-triangulated.json): a
# shelled rectangle-to-round transition's four outer faces picked with
# Development Approximate, OK, the body kept and its Flat Pattern
# approximate; edited with fewer triangles, its stated deviation grows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-convert-triangulated-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-convert-triangulated.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-convert-triangulated.mitcad-ui
design=$work/sheet-convert-triangulated.mitcad
"$cli" run "$here/ui-scripts/sheet-convert-triangulated.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet convert triangulated driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Convert to Sheet Metal: ok' 'the transition with its four faces previewed as a sheet'
ui_expect_log 'Added Convert to Sheet Metal as F7' 'the conversion was added'
ui_expect_log 'Editing F7 with Convert to Sheet Metal' 'the conversion was edited'
ui_expect_log 'TestDriver: done' 'a transition laid flat by triangulation through the panel'
ui_finish 'UI sheet convert triangulated'

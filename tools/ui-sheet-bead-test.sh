#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/resources/layout/default.json
# check-all sources: core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# check-all sources: tools/ui-scripts/sheet-bead.mitcad-ui tools/ui-scripts/sheet-bead.json
# The Bead command (mitcad#2645) through the real UI, the shared input
# script tools/ui-scripts/sheet-bead.mitcad-ui (also run by
# tools/ui-windows-sheet-bead-test.ps1 and tools/ui-macos-sheet-bead-test.sh)
# on a design that mitcad-cli makes (tools/ui-scripts/sheet-bead.json): a
# round bead pressed along a sketch line on a plate's top face, opened in
# Bead and changed to a trapezoid, then pressed into the face, each with its
# volume.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-bead-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-bead.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-bead.mitcad-ui
design=$work/sheet-bead.mitcad
"$cli" run "$here/ui-scripts/sheet-bead.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet bead driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added Bead as F5' 'the bead was added'
ui_expect_log 'Editing F5 with Bead' 'the bead opened in Bead'
ui_expect_log 'TestDriver: done' 'a bead made, changed to a trapezoid and pressed into the face'
ui_finish 'UI sheet bead'

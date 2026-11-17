#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# check-all sources: tools/ui-scripts/sheet-unfold-fillet.mitcad-ui tools/ui-scripts/sheet-unfold-fillet.json
# An Unfold refused by a fillet on a surface edge (mitcad#1942) through
# the real UI, the shared input script
# tools/ui-scripts/sheet-unfold-fillet.mitcad-ui (also run by
# tools/ui-windows-sheet-unfold-fillet-test.ps1 and
# tools/ui-macos-sheet-unfold-fillet-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/sheet-unfold-fillet.json): the edited Unfold's
# panel says why in words, the fillet by its name, no ids.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-unfold-fillet-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-unfold-fillet.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-unfold-fillet.mitcad-ui
design=$work/sheet-unfold-fillet.mitcad
"$cli" run "$here/ui-scripts/sheet-unfold-fillet.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet unfold fillet driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Panel Unfold error: sheet Unfold needs the sheet' 'the Unfold panel says why it fails'
ui_expect_log 'TestDriver: done' 'the Unfold panel names the fillet in words, without ids'
ui_finish 'UI sheet unfold fillet'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model/src/features/flat_pack.rs core/model/src/features/finger_joint.rs core/model-api/src/sheet_process.rs core/model-api/src/cut_sheets.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/flat-pack.mitcad-ui tools/ui-scripts/flat-pack.json
# Flat Pack (mitcad#1939) through the real UI: the shared input script
# tools/ui-scripts/flat-pack.mitcad-ui (also run by
# tools/ui-windows-flat-pack-test.ps1 and tools/ui-macos-flat-pack-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/flat-pack.json). A box picked in the browser becomes six finger-jointed plates on a cut sheet, taken back by one undo.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flat-pack-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-flat-pack.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/flat-pack.mitcad-ui
design=$work/flat-pack.mitcad
"$cli" run "$here/ui-scripts/flat-pack.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'flat-pack driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Flat pack: six plates of 6' 'the flat pack was made'
ui_expect_log 'TestDriver: done' 'Flat Pack (mitcad#1939): every step of the script'
ui_finish 'UI flat-pack'

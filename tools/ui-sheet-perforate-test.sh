#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/resources/layout/default.json
# check-all sources: core/model/src/features/sheet_perforate.rs core/model/src/sheet/folded/perforate.rs
# check-all sources: tools/ui-scripts/sheet-perforate.mitcad-ui tools/ui-scripts/sheet-perforate.json
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-perforate.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=50 MITCAD_TEST_INPUT=$here/ui-scripts/sheet-perforate.mitcad-ui
design=$work/perforate.mitcad
"$cli" run "$here/ui-scripts/sheet-perforate.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Perforate timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
# Fixed text, not patterns (ui_expect_log): the driver's own trace of a
# wait-log line contains its pattern, which would match itself.
ui_expect_log 'Preview Perforate: ok, info: The perforation cuts 11 holes.' 'the outline limits the field'
ui_expect_log 'TestDriver: done' 'all Perforate steps completed'
ui_finish 'UI Perforate'

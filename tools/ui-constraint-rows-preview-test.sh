#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/CommandSession.cpp app/framework/CommandSession.hpp
# check-all sources: app/framework/Command.hpp app/framework/Command.cpp app/framework/PreviewBodies.cpp
# check-all sources: app/framework/TestDriver.cpp app/OcctViewer.cpp tools/cli
# check-all sources: core/model/src/constraints.rs core/model/src/features/constraint_set.rs
# check-all sources: tools/ui-scripts/constraint-rows-preview.mitcad-ui tools/ui-scripts/constraint-rows-preview.json
# Assembly Constraint's preview with more than one row (mitcad#1813)
# through the real UI, the shared input script
# tools/ui-scripts/constraint-rows-preview.mitcad-ui (also run by
# tools/ui-windows-constraint-rows-preview-test.ps1 and
# tools/ui-macos-constraint-rows-preview-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/constraint-rows-preview.json): row 1
# complete is previewed; Add Constraint leaves the new row out of the
# preview, which still places the block by row 1 and says what the row
# needs; the second row complete places the block by both rows, drawn
# once, with its picks where the preview puts it; OK adds the set as
# previewed.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-constraint-rows-preview-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-constraint-rows-preview.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/constraint-rows-preview.mitcad-ui
"$cli" run "$here/ui-scripts/constraint-rows-preview.json" --save "$work/constraint-rows-preview.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/constraint-rows-preview.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'constraint rows preview driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Assembly Constraint moves O2' 'row 1 previewed: the block moved'
ui_expect_log 'Preview Assembly Constraint without its incomplete rows (rows row 2: Select geometry 1.): ok' \
  'the empty row left out, row 1 still previewed'
ui_expect_log 'Preview Assembly Constraint without its incomplete rows (rows row 2: Select geometry 2.): ok' \
  'the row with only its Geometry 1 left out, row 1 still previewed'
ui_expect_log 'Assembly Constraint picks drawn where the preview places them: 2' \
  'the picks on the block where the preview puts it'
ui_expect_log 'Added constraint Constraint1: align, align placed, moved O2' 'OK added the set'
ui_expect_log 'TestDriver: done' 'the rows'"'"' preview'
grep -q '^Preview Assembly Constraint: not built' "$UI_LOG" && ui_fail 'an incomplete row took the preview away'
echo "ok   no row took the preview away"
ui_finish 'UI Assembly Constraint rows preview'

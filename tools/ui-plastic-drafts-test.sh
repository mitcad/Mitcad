#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PlasticCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/plastic.rs geometry/src/plastic.cpp geometry/src/rib.cpp tools/cli
# check-all sources: tools/ui-scripts/plastic-drafts.mitcad-ui tools/ui-scripts/plastic-drafts.json
# Plastic Feature's fields of mitcad#2511 through the real UI, the shared
# input script tools/ui-scripts/plastic-drafts.mitcad-ui (also run by
# tools/ui-windows-plastic-drafts-test.ps1 and
# tools/ui-macos-plastic-drafts-test.sh) on a shelled box and a plate with a
# wall that mitcad-cli makes (tools/ui-scripts/plastic-drafts.json): a lip
# with Follow Tangency and a Side Draft of its own, reopened with them; a
# boss whose Rib Draft shows its angle and whose rib end row takes a rib up
# to the wall, reopened with it; the analytic volumes; Undo takes both.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-plastic-drafts-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-plastic-drafts.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/plastic-drafts.mitcad-ui
design=$work/plastic-drafts.mitcad
"$cli" run "$here/ui-scripts/plastic-drafts.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'plastic drafts driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added plastic feature (Lip)' 'the lip is added'
ui_expect_log 'Added plastic feature (Boss)' 'the boss is added'
ui_expect_log 'TestDriver: done' 'the lip follows the face beside it and leans; a rib reaches the wall'
ui_finish 'UI plastic drafts'

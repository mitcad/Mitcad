#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MoveOccurrenceDialog.cpp app/framework/MoveOccurrenceDialog.hpp
# check-all sources: app/browser/BrowserController.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/reparent.rs core/model-api/src/components.rs
# check-all sources: tools/ui-scripts/move-occurrence-group.mitcad-ui tools/ui-scripts/move-occurrence-group.json
# Move to Component on a rigid group joined to a shaft (mitcad#1847,
# GitHub mitcad/Mitcad#334) through the real UI, the shared input script
# tools/ui-scripts/move-occurrence-group.mitcad-ui (also run by
# tools/ui-windows-move-occurrence-group-test.ps1 and
# tools/ui-macos-move-occurrence-group-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/move-occurrence-group.json): on two
# selected occurrences every parent they cannot go into says why under
# it, the joint to the shaft with what to do; on one of them the rest of
# its rigid group is offered; with the shaft selected too, all three go
# into the empty component in one step, and one Undo puts them back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-occurrence-group-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-move-occurrence-group.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/move-occurrence-group.mitcad-ui
"$cli" run "$here/ui-scripts/move-occurrence-group.json" --save "$work/move-occurrence-group.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/move-occurrence-group.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'move occurrence group driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Move Component reason shown under Root/Holder:1: Joint1: it joins Bearing:1 to Shaft:1' \
  'the reason under the empty component, in words'
ui_expect_log 'Move Component rigid group offered: Tube:1' 'the rest of the rigid group offered'
ui_expect_log 'Moved Bearing:1, ' 'the group and the shaft moved in one step'
ui_expect_log 'TestDriver: done' 'Move to Component on a rigid group'
ui_finish 'UI Move to Component on a rigid group'

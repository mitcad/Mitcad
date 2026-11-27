#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp core/model/src/features/split_body.rs
# check-all sources: tools/ui-scripts/split-body-keep-selected.mitcad-ui tools/ui-scripts/split-body-keep.json
# Split Body keeping the piece clicked (mitcad#2504) through the real UI,
# tools/ui-scripts/split-body-keep-selected.mitcad-ui, on the design
# mitcad-cli makes (tools/ui-scripts/split-body-keep.json): a block split
# by a plane, Keep set to Selected, the dot of one piece clicked; one body
# after OK, in one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-split-body-keep-selected-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-split-body-keep-selected.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/split-body-keep-selected.mitcad-ui
"$cli" run "$here/ui-scripts/split-body-keep.json" --save "$work/split-body-keep.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/split-body-keep.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'split body keep selected driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the clicked piece kept alone, in one undo step'
ui_finish 'UI split body keep selected'

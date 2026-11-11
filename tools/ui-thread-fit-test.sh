#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/HoleCommands.cpp app/framework core/model/src/document/thread_fit.rs
# check-all sources: core/model/src/features/thread core/model-api/src/lib.rs tools/cli tools/ui-scripts/thread-fit.mitcad-ui
# The Thread panel's fit of a hole too small for the thread (mitcad#1632)
# through the real UI, the shared input script
# tools/ui-scripts/thread-fit.mitcad-ui (also run by
# tools/ui-windows-thread-fit-test.ps1) on a design that mitcad-cli makes
# (tools/ui-scripts/thread-fit.json): M8x1.25 on a 5 mm hole warns, the
# panel's note says so and its button fits the hole; OK adds the thread
# and the fit as one undo step, and Undo takes back both.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-thread-fit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-thread-fit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/thread-fit.mitcad-ui
"$cli" run "$here/ui-scripts/thread-fit.json" --save "$work/thread-fit.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/thread-fit.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'thread fit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Thread panel: the hole fitted to the thread, one undo step'
ui_finish 'UI Thread panel fits the hole to the thread'

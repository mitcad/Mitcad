#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/HoleCommands.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/thread.rs core/model/data/threads.json tools/cli
# check-all sources: tools/ui-scripts/thread-size.mitcad-ui tools/ui-scripts/thread-size.json
# The Thread panel's size from a picked face (mitcad#1781) through the
# real UI, the shared input script tools/ui-scripts/thread-size.mitcad-ui
# (also run by tools/ui-windows-thread-size-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/thread-size.json): an edited M60x3
# thread keeps its size when its face is picked again, a size chosen by
# hand stays when a Ø65 shaft is picked (the fit warning shows), and a Ø60
# shaft with nothing chosen suggests M60x5.5 6g.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-thread-size-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-thread-size.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/thread-size.mitcad-ui
"$cli" run "$here/ui-scripts/thread-size.json" --save "$work/thread-size.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/thread-size.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'thread size driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Thread panel: a chosen or edited size stays, the nearest size suggested'
ui_finish 'UI Thread panel suggests the size only while none is chosen'

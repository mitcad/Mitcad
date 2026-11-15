#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/DocumentSession.cpp app/framework/TestDriver.cpp
# check-all sources: app/commands/SweepCommands.cpp core/model/src/features/pipe.rs
# check-all sources: tools/ui-scripts/pipe-preview-drop.mitcad-ui tools/ui-scripts/pipe-preview-drop.json tools/cli
# A value typed while a slow preview of the value before computes drops
# that preview (mitcad#1788) through the real UI, the shared input script
# tools/ui-scripts/pipe-preview-drop.mitcad-ui (also run by
# tools/ui-windows-pipe-preview-drop-test.ps1) on a 50 mm line that
# mitcad-cli makes (tools/ui-scripts/pipe-preview-drop.json): Pipe with the
# path picked first, its default size's preview held by the test's worker
# gate for more than two seconds, then the size 2 typed: the held preview
# is dropped, the 2 mm pipe previewed once the gate lets it, and OK makes
# it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pipe-preview-drop-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-pipe-preview-drop.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/pipe-preview-drop.mitcad-ui
"$cli" run "$here/ui-scripts/pipe-preview-drop.json" --save "$work/pipe-preview-drop.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/pipe-preview-drop.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'pipe preview drop driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Pipe: dropped after' 'the slow preview is dropped for the typed size'
ui_expect_log 'Pipe Section Size: 2 = ' 'the typed size is evaluated'
ui_expect_log 'TestDriver: done' 'a typed size drops the slow preview of the size before'
ui_finish 'UI typed size drops a slow Pipe preview'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Application.cpp app/framework/ParameterField.cpp app/framework/ValueStep.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/commands/FilletCommands.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/fillet-wheel-busy.mitcad-ui tools/ui-scripts/fillet-wheel-busy.json tools/cli
# The mouse wheel over Fillet's Radius while its preview computes
# (mitcad#1407, GitHub mitcad/Mitcad#162: 0.48.0 ended with "The document
# was read while a job computes it") through the real UI, the shared input
# script tools/ui-scripts/fillet-wheel-busy.mitcad-ui (also run by
# tools/ui-windows-fillet-wheel-busy-test.ps1) on a cylinder that mitcad-cli
# makes (tools/ui-scripts/fillet-wheel-busy.json): Fillet on its top edge,
# Radius 3 mm, then wheel notches over the field faster than the preview
# computes (the preview held by the test's worker gate): each notch steps
# the field, nothing reads the document, the application goes on, the last
# value (6 mm) is evaluated and previewed once the job ends, and OK makes
# that fillet.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-wheel-busy-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-wheel-busy.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-wheel-busy.mitcad-ui
"$cli" run "$here/ui-scripts/fillet-wheel-busy.json" --save "$work/fillet-wheel-busy.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/fillet-wheel-busy.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'fillet wheel busy driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Value field Fillet Radius: wheel stepped 4 mm to 5 mm' 'the wheel steps the radius while the preview computes'
ui_expect_log 'Fillet Radius: 6 mm = 6 mm' 'the last value is evaluated once the job has ended'
ui_expect_log 'Added Fillet' 'OK makes the fillet'
ui_expect_log 'TestDriver: done' 'the wheel over Fillet Radius while its preview computes'
ui_finish 'UI wheel over Fillet Radius while its preview computes'

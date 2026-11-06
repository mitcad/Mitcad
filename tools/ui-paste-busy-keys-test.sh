#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Application.cpp app/framework/ParameterField.cpp app/MainWindow.cpp
# check-all sources: app/browser/BrowserController.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/paste-busy-keys.mitcad-ui tools/ui-scripts/paste-busy-keys.json tools/cli
# Keys while a pasted component is still computed (mitcad#1360, GitHub
# mitcad/Mitcad#156) through the real UI, the shared input script
# tools/ui-scripts/paste-busy-keys.mitcad-ui (also run by
# tools/ui-windows-paste-busy-keys-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/paste-busy-keys.json): Copy and Paste New of a
# component open Move/Copy; while its preview computes, Shift and Up in the
# X Distance field neither read the document nor end the application, Up
# steps 10 mm to 11 mm, and OK places the copy once the job has ended.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-paste-busy-keys-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-paste-busy-keys.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/paste-busy-keys.mitcad-ui
"$cli" run "$here/ui-scripts/paste-busy-keys.json" --save "$work/paste-busy-keys.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/paste-busy-keys.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'paste busy keys driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Value field Move/Copy X Distance: Up stepped 10 mm to 11 mm' 'Up steps the distance while the preview computes'
ui_expect_log ' at (51, 0, 0)' 'the copy is placed once the job has ended'
ui_expect_log 'TestDriver: done' 'keys while a pasted component computes'
ui_finish 'UI keys while a pasted component computes'

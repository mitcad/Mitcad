#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp core/model/src/features/shell.rs geometry/src/faceops.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/shell-own-thickness.mitcad-ui tools/ui-scripts/shell-own-thickness.json
# Shell with faces of their own thickness (mitcad#2443) through the real UI:
# the shared input script tools/ui-scripts/shell-own-thickness.mitcad-ui
# (also run by tools/ui-windows-shell-own-thickness-test.ps1 and
# tools/ui-macos-shell-own-thickness-test.sh) on a box that mitcad-cli makes
# (tools/ui-scripts/shell-own-thickness.json). Shell with the top removed,
# 2 mm inside, the front in an Own Thickness row at 5 mm, then reopened and
# the row edited to 4 mm, with the volumes the script works out.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-shell-own-thickness-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-shell-own-thickness.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/shell-own-thickness.mitcad-ui
design=$work/shell-own-thickness.mitcad
"$cli" run "$here/ui-scripts/shell-own-thickness.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'shell-own-thickness driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Shell: added face_thicknesses row 1' 'the Own Thickness row was added'
ui_expect_log 'Added shell removing 1 face(s), 1 face(s) at their own thickness' 'the shell was added with the row'
ui_expect_log 'TestDriver: done' 'Shell with faces of their own thickness (mitcad#2443): every step of the script'
ui_finish 'UI shell-own-thickness'

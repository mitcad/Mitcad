#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/solver core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-line-extensions.mitcad-ui tools/ui-scripts/sketch-line-extensions.json
# Resting the pointer on a sketch line acquires it, and a drawing tool
# snaps onto its extension and to where two acquired lines' extensions
# meet (mitcad#2035, GitHub mitcad/Mitcad#433), through the real UI on a
# design that mitcad-cli makes (tools/ui-scripts/sketch-line-extensions.json):
# a line ended on a vertical line's extension is held there by a Coincident
# and follows the line when it moves; a rectangle's corner where two
# acquired lines' extensions meet is held on both; with Snap to line
# extensions unchecked nothing is acquired. The steps are
# tools/ui-scripts/sketch-line-extensions.mitcad-ui, which
# tools/ui-windows-sketch-line-extensions-test.ps1 runs on Windows and
# tools/ui-macos-sketch-line-extensions-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-line-extensions-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-line-extensions.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-line-extensions.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-line-extensions.json" --save "$work/lines.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/lines.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch line extensions driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'on the extension of c2' 'a line end held on a line extension'
ui_expect_log 'on the extension of c8' 'a rectangle corner held where two line extensions meet'
ui_expect_log 'TestDriver: done' 'line extensions acquired, snapped to, followed and turned off'
ui_finish 'UI sketch line extensions'

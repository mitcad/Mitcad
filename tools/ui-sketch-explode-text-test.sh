#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model/src/document/sketch_explode_text.rs
# check-all sources: core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-explode-text.mitcad-ui tools/ui-scripts/sketch-explode-text.json
# Explode Text (mitcad#2163) through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-explode-text.json: a text):
# the text right-clicked, Explode Text from its context menu, then Trim on
# a piece of the letter. The steps are
# tools/ui-scripts/sketch-explode-text.mitcad-ui, which
# tools/ui-windows-sketch-explode-text-test.ps1 runs on Windows and
# tools/ui-macos-sketch-explode-text-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-explode-text-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-explode-text.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-explode-text.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-explode-text.json" --save "$work/text.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/text.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch explode text driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Exploded text t1 in Sketch1: ' 'Explode Text from the text'"'"'s context menu'
ui_expect_log 'Trimmed c4 at ' 'Trim on a piece of the letter'
ui_expect_log 'TestDriver: done' 'a text exploded and a piece of it trimmed'
ui_finish 'UI sketch explode text'

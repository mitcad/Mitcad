#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/sketch-value-error-clears.mitcad-ui
# check-all sources: tools/ui-scripts/sketch-value-error-clears.json
# A sketch value editor's error goes when the editor closes (mitcad#1621),
# through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-value-error-clears.json: a 20 x 10 rectangle,
# its width k5 d1 = 20 mm), in the Docked and then the Floating layout:
# k5's value editor emptied and Enter says "Enter a value." without a
# leading colon; Esc closes the editor, and the error goes from the status
# line and the message band, the history still listing it; the error again
# and a value taken: the error goes with the editor. The steps are
# tools/ui-scripts/sketch-value-error-clears.mitcad-ui. An error of no
# source (a failed feature) staying until it is closed or the next command
# is tools/ui-message-band-test.sh's.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-value-error-clears-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-value-error-clears.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-value-error-clears.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-value-error-clears.json" --save "$work/value.mitcad" || exit 1

# run_layout chrome: the steps in that layout, and what they logged.
run_layout() {
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/value.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: value error driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'Status error: Enter a value.' "$1: the empty value's error, without a leading colon"
  ui_expect_log 'Status line cleared (sketch/editor closed)' "$1: Esc took it from the status line"
  ui_expect_log 'Message band source closed: error Enter a value. (sketch/editor)' "$1: and from the message band"
  ui_expect_log 'Dimension k5 = 25 mm (d1)' "$1: the value taken"
  ui_expect_log 'TestDriver: done' "$1: the error goes when the editor closes or takes its value"
}

ui_start_display
run_layout docked
run_layout floating
ui_finish 'UI sketch value error clears'

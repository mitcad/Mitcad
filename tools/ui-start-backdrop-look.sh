#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Start page's rounded backdrop (mitcad#1439): a one-off visual check,
# not a recurring test (its name is not ui-*-test.sh, so no check picks it
# up). The steps tools/ui-scripts/start-backdrop-look.mitcad-ui (also run
# by tools/ui-windows-start-backdrop-look.ps1) on the block mitcad-cli makes
# (tools/ui-scripts/start-page-delete.json), in the Floating and the Docked
# layout, with Qt's GTK platform theme for the dark scheme
# (tools/ui-start-page-look.sh). The pictures go to <dir>:
#   start-backdrop-floating-light.png  start-backdrop-floating-dark.png
#   start-backdrop-docked-light.png    start-backdrop-docked-dark.png
# with the app's logs as <dir>/start-backdrop-look-<layout>.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-start-backdrop-look.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-start-backdrop-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT
export MITCAD_TEST_WORK=$DIR MITCAD_TEST_STEP_MS=20
export MITCAD_TEST_INPUT=$here/ui-scripts/start-backdrop-look.mitcad-ui
"$cli" run "$here/ui-scripts/start-page-delete.json" --save "$DIR/block.mitcad" || exit 1
ui_start_display
for layout in floating docked; do
  export MITCAD_LOOK_LAYOUT=$layout
  ui_start_app --ui=qt --no-mcp --chrome="$layout" --open "$DIR/block.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$layout: the look driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  cp -f "$UI_LOG" "$DIR/start-backdrop-look-$layout.log"
  ui_expect_log 'TestDriver: done' "$layout: the pictures were taken"
  ui_stop_app
done
for picture in "$DIR"/start-backdrop-*.png; do
  echo "ok   $picture"
done

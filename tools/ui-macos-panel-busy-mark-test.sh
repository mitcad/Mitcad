#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BusyMark.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/panel-busy-mark.mitcad-ui
# check-all sources: tools/ui-scripts/panel-message-top.json
# A preview being computed shown without resizing the panel (mitcad#1907,
# GitHub mitcad/Mitcad#361) on macOS, through the test driver
# (tools/ui-scripts/panel-busy-mark.mitcad-ui, the same script as the Linux
# and Windows tests'), in the shared Qt panel, Docked and then Floating,
# every feature a preview evaluates slowed to 1 s: Extrude1 of a cube
# edited, its distance arrow dragged up; the busy mark beside OK and Cancel
# shows and then hides, the message line stays hidden, and every busy line
# from the edit's first preview on gives the panel the same size and needed
# height ("Panel Extrude busy ..., panel WxH, needs H").
# Usage: tools/ui-macos-panel-busy-mark-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-panel-busy-mark}"
UI_TIMEOUT=180
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/panel-busy-mark.mitcad
"$2" run "$here/ui-scripts/panel-message-top.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

for layout in docked floating; do
  echo "--- $layout"
  MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=1000 \
    MITCAD_TEST_INPUT=$here/ui-scripts/panel-busy-mark.mitcad-ui \
    ui_run_app "panel-busy-mark-$layout" --ui=qt --no-recovery --no-mcp --chrome=$layout --open "$design"
  ui_expect_log '^Manipulator Extrude distance dragged to ' "$layout: the distance's arrow dragged"
  ui_expect_log '^Panel Extrude busy mark shown after ' "$layout: the busy mark shown while the preview computes"
  ui_expect_log '^Panel Extrude busy mark hidden after ' "$layout: and hidden once it has ended"
  ui_expect_log 'TestDriver: done' "$layout: a preview being computed resizes nothing"
  ui_expect_no_log 'TestDriver: FAIL' "$layout: no failed driver assertion"
  # The busy lines from the end of the edit's first preview to the panel's
  # end (Esc); BSD sed and awk.
  lines=$(awk '/^Panel Extrude busy (mark hidden|ended) after / { on = 1 }
               on && /^Command Extrude cancelled/ { exit }
               on && /^Panel Extrude busy/ { print }' "$UI_LOG_FILE")
  if ! printf '%s\n' "$lines" | grep -q '^Panel Extrude busy mark shown after '; then
    ui_fail "$layout: the busy mark did not show during the drag"
    continue
  fi
  sizes=$(printf '%s\n' "$lines" | sed -n 's/.*, \(panel [0-9]*x[0-9]*, needs [0-9]*\)$/\1/p' | sort -u)
  if [ "$(printf '%s\n' "$sizes" | grep -c .)" = 1 ]; then
    ui_pass "$layout: $(printf '%s\n' "$lines" | grep -c .) busy lines, all with $sizes"
  else
    ui_fail "$layout: the panel's size changed while previews computed: $(printf '%s' "$sizes" | tr '\n' ';')"
  fi
done
[ "$UI_FAILURES" = 0 ] || exit 1

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render app/MainWindowRender.cpp app/framework/Ribbon.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/render-workspace.mitcad-ui tools/ui-scripts/render-workspace.json
# The Render workspace (mitcad#2013) in the Qt frontend on macOS, as
# tools/ui-render-workspace-test.sh on Linux: the shared steps
# tools/ui-scripts/render-workspace.mitcad-ui on a design that mitcad-cli
# makes (tools/ui-scripts/render-workspace.json): the stages as the tab
# row with the stage's actions under it as the toolbar's groups, one
# header per panel, the docked
# environment and lights, an appearance given to a body, two shots saved,
# renamed and looked through, and Render All Shots with 4 samples. The
# Render workspace is the toolbar panel's (mitcad#1990); a build whose
# macOS window has no workspace selector skips (77) and says so.
# Usage: tools/ui-macos-render-workspace-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-render-workspace-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=600
# The shared script's pictures of the stages (render-look.png, ...).
export MITCAD_TEST_PICTURES=$UI_OUT/pictures
mkdir -p "$MITCAD_TEST_PICTURES"
for layout in floating; do
  export MITCAD_TEST_WORK=$UI_OUT/files-$layout
  rm -rf "$MITCAD_TEST_WORK"
  mkdir -p "$MITCAD_TEST_WORK"
  design=$MITCAD_TEST_WORK/render-workspace.mitcad
  "$cli" run "$here/ui-scripts/render-workspace.json" --save "$design" > "$UI_OUT/setup-$layout.log" 2>&1 ||
    { cat "$UI_OUT/setup-$layout.log"; echo 'FAIL: mitcad-cli run render-workspace.json' >&2; exit 1; }
  MITCAD_RENDER_SAMPLES=16 MITCAD_TEST_INPUT=$here/ui-scripts/render-workspace.mitcad-ui MITCAD_TEST_STEP_MS=100 \
    ui_run_app "render-workspace-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout" --open "$design"
  if ! grep -q "^Ribbon workspace at " "$UI_LOG_FILE"; then
    echo 'SKIP: no workspace selector in this macOS window: the Render workspace is the toolbar panel'"'"'s'
    exit 77
  fi
  ui_expect_log '^Ribbon tab RENDER: its own row$' "the stages as the tab row ($layout)"
  ui_expect_log '^Render groups: Live [|] Pick [|] Look$' "Look's groups after the Live group ($layout)"
  ui_expect_log '^Render panels: Shots to Render [|] Output$' "each stage its own list and card ($layout)"
  ui_expect_log '^Render settings field environment.strength at ' "the environment's settings docked ($layout)"
  ui_expect_log '^Render all shots done: 2 of 2$' "both shots rendered ($layout)"
  ui_expect_log '^TestDriver: done' "the Render workspace ($layout)"
  ui_expect_no_log 'TestDriver: FAIL' "no failed driver assertion ($layout)"
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Render workspace'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowRender.cpp app/drawings/MainWindowDrawings.cpp
# check-all sources: app/framework/ChromeStyle.cpp app/framework/Ribbon.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/workspace-render-access.mitcad-ui tools/ui-scripts/workspaces.json
# Open Render from the workspace selector in both macOS shared Qt layouts,
# then return to Design (mitcad#2080).
# Usage: tools/ui-macos-workspace-render-access-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=90
for layout in floating docked; do
  work=$UI_OUT/files-$layout
  mkdir -p "$work" || exit 1
  design=$work/workspaces.mitcad
  "$cli" run "$here/ui-scripts/workspaces.json" --save "$design" > "$UI_OUT/setup-$layout.log" 2>&1 ||
    { cat "$UI_OUT/setup-$layout.log"; echo "FAIL: mitcad-cli run workspaces.json ($layout)" >&2; exit 1; }
  MITCAD_TEST_INPUT=$here/ui-scripts/workspace-render-access.mitcad-ui MITCAD_TEST_STEP_MS=100 \
    ui_run_app "workspace-render-access-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout" --open "$design"
  ui_expect_log "^Style: .* Chrome: $layout " "the $layout layout is active"
  ui_expect_log '^Render workspace opened$' "the workspace selector opens Render ($layout)"
  ui_expect_log '^Render workspace closed$' "the workspace selector returns to Design ($layout)"
  ui_expect_log '^TestDriver: done' "the workspace workflow completes ($layout)"
  ui_expect_no_log 'TestDriver: FAIL' "no failed driver assertion ($layout)"
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Render access in Qt Floating and Docked layouts'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowToolbarPanel.cpp app/framework/ToolbarPanelReveal.cpp
# check-all sources: app/framework/ToolbarPanelReveal.hpp app/framework/FloatingLayout.cpp
# check-all sources: app/MainWindowStart.cpp app/MainWindow.cpp app/framework/Ribbon.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverPointer.cpp
# check-all sources: tools/ui-scripts/toolbar-panel-hide.mitcad-ui tools/ui-scripts/toolbar-panel-hide-restart.mitcad-ui
# View › Toolbar Panel (mitcad#2426, GitHub mitcad/Mitcad#571) through the
# real UI, in the Floating and the Docked layout, each with a fresh profile
# and two starts of MitCad running the shared input scripts (also run by
# tools/ui-windows-toolbar-panel-hide-test.ps1):
#   1. tools/ui-scripts/toolbar-panel-hide.mitcad-ui: Ctrl+F1 hides the
#      panel (the view's room grows, the way back said once), a rest at the
#      view's top edge reveals it over the view, a group's menu keeps it, a
#      command from it runs and keeps it (taking its room) while its command
#      panel is open (mitcad#2472), it goes once the command ended with the
#      pointer away, the command by its key keeps nothing, a drag across
#      the edge shows nothing, View › Toolbar Panel shows it, a new design
#      and the Start page keep the hidden state.
#   2. tools/ui-scripts/toolbar-panel-hide-restart.mitcad-ui: hidden at the
#      next start, revealed there, Ctrl+F1 shows it and keeps it so.
#
# Runs headless on Xvfb (see ui-test-lib.sh). MitCad is started here, not with
# ui_start_app: the scripts drive the window from their first line and the
# restart's ends within seconds.
# Usage: tools/ui-toolbar-panel-hide-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
CONF=$UI_CONFIG/Mitcad/Mitcad.conf

# run layout script name: one start of MitCad in the layout, running
# tools/ui-scripts/toolbar-panel-hide<script>.mitcad-ui; the app ends with it.
run() {
  local layout=$1 name=$3
  echo "--- $name"
  export MITCAD_TEST_INPUT=$here/ui-scripts/toolbar-panel-hide$2.mitcad-ui
  "$UI_APP" --no-recovery --ui=qt --no-mcp "--chrome=$layout" > "$UI_LOG" 2>&1 &
  UI_RUNNER=$!
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the toolbar panel driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$name: the toolbar panel driver failed: $(grep -E 'TestDriver|Toolbar panel' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$name"
  grep -E '^Toolbar panel ' "$UI_LOG" | sed 's/^/     /'
}

ui_start_display
for layout in floating docked; do
  # A fresh profile for each layout (the way back is said once per profile).
  mkdir -p "$(dirname "$CONF")"
  : > "$CONF"
  run "$layout" "" "$layout: hidden, revealed, a command from it, a new design, the Start page"
  grep -q '^toolbarPanelHidden=true$' "$CONF" || ui_fail "$layout: the hidden state is not kept in $CONF"
  echo "ok   $layout: the hidden state kept in the settings"
  run "$layout" "-restart" "$layout: hidden after the restart, shown again"
  grep -q '^toolbarPanelHidden=false$' "$CONF" || ui_fail "$layout: the shown state is not kept in $CONF"
  echo "ok   $layout: the shown state kept in the settings"
done

ui_finish 'UI toolbar panel hide test'

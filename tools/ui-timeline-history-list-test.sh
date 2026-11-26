#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/TimelineWidget.hpp
# check-all sources: app/browser/TimelineHistoryList.cpp app/browser/TimelineHistoryList.hpp
# check-all sources: app/browser/MainWindowTimelineList.cpp app/browser/BrowserRows.hpp
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPreferences.cpp app/browser/BrowserController.cpp
# check-all sources: app/browser/DocumentSnapshot.cpp app/view/ViewController.cpp app/MainWindowDocks.cpp
# check-all sources: app/framework/GlassCard.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/timeline-history-list.mitcad-ui
# check-all sources: tools/ui-scripts/timeline-history-list-docked.mitcad-ui
# check-all sources: tools/ui-scripts/timeline-history-list.json
# The timeline's History list (mitcad#2427, GitHub mitcad/Mitcad#581)
# through the real UI, the shared input scripts
# tools/ui-scripts/timeline-history-list.mitcad-ui (the Floating layout) and
# timeline-history-list-docked.mitcad-ui (the Docked layout), also run by
# tools/ui-windows-timeline-history-list-test.ps1, on a design that
# mitcad-cli makes (tools/ui-scripts/timeline-history-list.json): Preferences
# › Display › Timeline: History list moves the timeline under the Browser,
# its rows name the items in timeline order with the group as a folder row;
# the marker dragged between two rows rolls the model back; the group's
# arrow folds it and opens it; a sketch's eye hides it (and a feature's its
# body); the context menu's Rename renames in the row; Strip brings the
# strip back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timeline-history-list-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-timeline-history-list.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_WORK=$work
"$cli" run "$here/ui-scripts/timeline-history-list.json" --save "$work/timeline-history-list.mitcad" || exit 1
ui_start_display

# run_script <name> <layout>: one of the shared scripts, the app started on
# the design in that layout, with the settings the runs share (the Floating
# run ends with the strip again for the Docked run).
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp "--chrome=$2" --open "$work/timeline-history-list.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "timeline history list driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the timeline history list ($1)"
}

run_script timeline-history-list floating
run_script timeline-history-list-docked docked
ui_finish 'UI Timeline history list'

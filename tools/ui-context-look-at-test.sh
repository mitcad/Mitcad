#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowContextMenu.cpp app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/context-look-at.mitcad-ui
# Look At in the view's context menu (mitcad#1212) through the real UI, the
# shared input script tools/ui-scripts/context-look-at.mitcad-ui (also run
# by tools/ui-windows-context-look-at-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/context-look-at.json): a right click on the
# cylindrical side of a cylinder offers no Look At; on the planar top of a
# block in a component it offers Look At before Measure, which turns the
# camera to look down the face's normal (logged).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-context-look-at-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-context-look-at.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/context-look-at.mitcad-ui
"$cli" run "$here/ui-scripts/context-look-at.json" --save "$work/look-at.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/look-at.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'context Look At driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "Look At in a planar face's context menu, not in a cylindrical one's"
ui_finish 'UI Look At in the context menu'

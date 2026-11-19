#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFiles.cpp app/browser/BrowserController.cpp app/commands/PlasticCommands.cpp
# check-all sources: app/resources/layout/default.json app/framework/Ribbon.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document.rs core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/manage-commands.mitcad-ui tools/ui-scripts/manage-commands.json
# The Manage tab's Recompute All, Units… and Plastic Rules (mitcad#1982)
# through the real UI, the shared input script
# tools/ui-scripts/manage-commands.mitcad-ui (also run by
# tools/ui-windows-manage-commands-test.ps1 and
# tools/ui-macos-manage-commands-test.sh) on two boxes that mitcad-cli
# makes (tools/ui-scripts/manage-commands.json): each command run from
# its group's menu on the MANAGE tab.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-manage-commands-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-manage-commands.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/manage-commands.mitcad-ui
"$cli" run "$here/ui-scripts/manage-commands.json" --save "$work/boxes.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/boxes.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Manage commands driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Recompute All: 2 features, no undo step' 'Recompute All from Manage › Design'
ui_expect_log 'Units dialog opened: mm' 'Units… from Manage › Design'
ui_expect_log 'Added plastic feature (Plastic Rule)' 'Plastic Rules from Manage › Rules'
ui_expect_log 'TestDriver: done' 'the Manage tab runs Recompute All, Units… and Plastic Rules'
ui_finish 'UI Manage commands'

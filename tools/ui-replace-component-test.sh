#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ReplaceComponentDialog.cpp app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: app/MainWindowContextMenu.cpp core/model/src/document/replace_component.rs
# check-all sources: core/model-api/src/components.rs tools/cli tools/ui-scripts/replace-component.mitcad-ui
# check-all sources: tools/ui-scripts/replace-component-holder.json tools/ui-scripts/replace-component-bracket.json
# check-all sources: tools/ui-scripts/replace-component-bracket-v2.json
# Replace Component (mitcad#2640) through the real UI, the shared input
# script tools/ui-scripts/replace-component.mitcad-ui (also run by
# tools/ui-windows-replace-component-test.ps1 and
# tools/ui-macos-replace-component-test.sh). mitcad-cli writes Bracket.mitcad
# and Bracket_v2.mitcad and, in their folder, the holder that links Bracket
# twice with a joint on Bracket:1. Replace Component... from the browser
# row with Bracket_v2.mitcad shows the replacement in all occurrences with
# the joint kept, OK keeps it as one undo step and Undo takes it back; This
# one from the other row shows it there alone and Cancel takes it back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-replace-component-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-replace-component.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
"$cli" run "$here/ui-scripts/replace-component-bracket.json" --save "$work/Bracket.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: Bracket"; exit 1; }
"$cli" run "$here/ui-scripts/replace-component-bracket-v2.json" --save "$work/Bracket_v2.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: Bracket_v2"; exit 1; }
# The holder made in the brackets' folder: its link is Bracket.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/replace-component-holder.json" --save "$work/holder.mitcad") \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the holder"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/replace-component.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --no-recovery --no-native-dialogs --open "$work/holder.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "replace component driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Replace Component: Bracket replaced by Bracket in 2 occurrences, 0 unmatched' \
  'all occurrences replaced, the joint kept, the name kept'
ui_expect_log 'Replace Component canceled' 'This one previewed and canceled'
ui_expect_log 'TestDriver: done' 'replacing a component'
ui_finish 'UI replace component'

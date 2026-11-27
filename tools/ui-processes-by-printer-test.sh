#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/MainWindowMake.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: app/browser/LibraryPicker.cpp app/browser/PropertySetsDialog.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/print_profile.rs core/model/src/print_files core/model/src/document/library_items.rs
# check-all sources: app/framework/PickerComboBox.cpp
# Processes grouped by printer (mitcad#1216), on a private Xvfb display:
# the test driver's script tools/ui-scripts/processes-by-printer.mitcad-ui
# (shared with the Windows test; it says what it does) on a design
# mitcad-cli makes from tools/ui-scripts/processes-by-printer.json.
# Usage: tools/ui-processes-by-printer-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
ROOT=$(cd "$here/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-processes-by-printer.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
"$CLI" run "$here/ui-scripts/processes-by-printer.json" --save "$WORK/processes.mitcad" > "$WORK/setup.log" 2>&1 || {
  cat "$WORK/setup.log" >&2
  exit 1
}
# The user's library in the test's own folder: empty.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/processes-by-printer.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$WORK/processes.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'processes by printer driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'processes by printer in the Processes manager, the Process picker and the body card passed'
echo 'PASS: processes by printer'

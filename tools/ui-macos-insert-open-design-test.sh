#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.hpp app/framework/TestDriver.cpp
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeDialogs.mm tools/ui-scripts/insert-open-design-script.py tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo 'usage: ui-macos-insert-open-design-test.sh <app> <cli> <output> [qt|native]' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
case "$frontend" in qt|native) ;; *) echo 'expected qt or native frontend' >&2; exit 2 ;; esac
UI_TIMEOUT=360
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
for design in first second; do
  "$cli" run "$here/ui-scripts/insert-open-design-$design.json" --save "$MITCAD_TEST_WORK/$design.mitcad" || exit 1
done
script=$MITCAD_TEST_WORK/workflow.mitcad-ui
python3 "$here/ui-scripts/insert-open-design-script.py" "$frontend" > "$script" || exit 1
MITCAD_TEST_INPUT=$script MITCAD_TEST_STEP_MS=100 \
  ui_run_app insert-open-design --ui="$frontend" --no-native-dialogs --no-mcp --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
ui_expect_log 'TestDriver: quit' 'open-design selection, saved-state insertion, Save/Cancel and independent undo'
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend insert an open design"

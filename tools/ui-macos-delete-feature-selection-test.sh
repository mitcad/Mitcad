#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowContextMenu.cpp app/MainWindowNative.cpp
# check-all sources: app/platform/macos/NativeMacWindow.mm app/framework/TestDriver.cpp tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo 'usage: ui-macos-delete-feature-selection-test.sh <app> <cli> <output> [qt|native]' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
case "$frontend" in qt|native) ;; *) echo 'expected qt or native frontend' >&2; exit 2 ;; esac
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/delete-feature-selection.json" --save "$MITCAD_TEST_WORK/selection.mitcad" || exit 1
script=$here/ui-scripts/delete-feature-selection.mitcad-ui
if [ "$frontend" = native ]; then
  script=$MITCAD_TEST_WORK/workflow.mitcad-ui
  # NSAlert exposes QMessageBox's question as its visible message, rather
  # than the Qt window title. focus-dialog matches that actual label.
  { echo 'native-expand native.modelTree'
    sed -e "s/click-logged '\^Timeline A at '/native-select native.history.F1/" \
        -e "s/click-logged '\^Timeline B at '/native-select native.history.F2/" \
        -e "s/click-logged '\^Timeline C at '/native-select native.history.F3/" \
        -e "s/click-logged '\^Timeline Source at '/native-select native.history.F4/" \
        -e "s/click-logged '\^Browser Root\/Bodies\/Body1 at '/native-select native.model.body:F1.b0@/" \
        -e "s/click-logged '\^Browser Root\/Bodies\/Body2 at '/native-select native.model.body:F2.b0@/" \
        -e "s/click-logged '\^Browser Root\/Bodies\/Body3 at '/native-select native.model.body:F3.b0@/" \
        -e "s/click-logged '\^Browser Root\/Bodies\/Body4 at '/native-select native.model.body:F5.b0@/" \
        -e "s/click-logged '\^Browser Root\/Sketches\/Source at '/native-select native.model.sketch:F4@/" \
        -e "s/click-logged '\^Browser Root\/Sketches\/Source\/Dependent at '/native-select 'native.model.feature:F5<F4@'/" \
        -e "s/click-logged '\^Browser Root\/Origin at '/native-select native.model.origin/" \
        -e "s/context-menu-logged '\^Timeline B at '/native-context-menu native.history.F2/" \
        -e "s/context-menu-logged '\^Timeline C at '/native-context-menu native.history.F3/" \
        -e "s/context-menu-logged '\^Browser Root\/Bodies\/Body2 at '/native-context-menu native.model.body:F2.b0@/" \
        -e 's/^focus-widget modelView$/focus-main/' \
        -e 's/^focus-widget modelBrowserTree$/native-focus native.modelTree/' \
        -e '/^key Right$/d' \
        -e 's/^focus-widget timelineStrip$/native-focus native.historyStrip/' \
        -e 's/^# NATIVE_BROWSER_MIRROR:.*$/expect-native-history F1 F2 F3/' \
        -e "s/focus-dialog '\^Delete\$'/focus-dialog '^Delete 2 features[?]\$'/" \
      "$here/ui-scripts/delete-feature-selection.mitcad-ui"
  } > "$script"
else
  # Qt's macOS QMessageBox drops its window title too; the portable dialog
  # matcher already falls back to the message of that exact message box.
  script=$MITCAD_TEST_WORK/workflow.mitcad-ui
  sed "s/focus-dialog '\^Delete\$'/focus-dialog '^Delete 2 features[?]\$'/" \
    "$here/ui-scripts/delete-feature-selection.mitcad-ui" > "$script"
fi
MITCAD_TEST_INPUT=$script MITCAD_TEST_STEP_MS=100 \
  ui_run_app delete-selection --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/selection.mitcad"
ui_expect_log '^TestDriver: done' "$frontend: collection and view Delete, dependency question and atomic Undo"
ui_expect_no_log 'TestDriver: FAIL' "$frontend: no failed deletion assertions"
questions=$(grep -c '^Delete C, Source: also Dependent?$' "$UI_LOG_FILE" || true)
[ "$questions" = 2 ] || ui_fail "expected one question per attempt, found $questions"
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend feature selection Delete"

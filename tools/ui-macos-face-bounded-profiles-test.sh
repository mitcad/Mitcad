#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: app/platform/macos/NativeTestInput.mm
# check-all sources: core/sketch core/model core/model-base core/model-api core/ffi core/cpp geometry/src/sketch_ref.cpp tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo 'usage: ui-macos-face-bounded-profiles-test.sh <app> <fixture> <output> [qt|native]' >&2
  exit 2
fi
ui_init "$1" "$3"
fixture=$2
frontend=${4:-qt}
case "$frontend" in qt|native) ;; *) echo 'expected qt or native frontend' >&2; exit 2 ;; esac
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$fixture" --write-fixture "$MITCAD_TEST_WORK" "$here/ui-scripts" || exit 1
UI_TIMEOUT=180
for scenario in face-bounded-profiles face-bounded-whole-face face-bounded-open-face; do
  case "$scenario" in
    face-bounded-profiles) design=face-profiles ;;
    face-bounded-open-face) design=open-face-profiles ;;
    *) design=whole-face ;;
  esac
  workflow=$MITCAD_TEST_WORK/$scenario.mitcad-ui
  if [ "$frontend" = native ]; then
    sed -e 's/^button .* clear_profiles$/native-click native.action.clear|profiles/' \
        -e 's/^click-pick /native-viewport-click-pick /' \
        -e 's/^qt-choice input_operation /native-choice input_operation /' "$workflow" \
      > "$MITCAD_TEST_WORK/$scenario-native.mitcad-ui"
    workflow=$MITCAD_TEST_WORK/$scenario-native.mitcad-ui
  fi
  MITCAD_TEST_INPUT=$workflow MITCAD_TEST_STEP_MS=100 \
    ui_run_app "$scenario" --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/$design.mitcad"
  ui_expect_log '^TestDriver: done' "$frontend: $scenario completed"
  ui_expect_no_log '^TestDriver: FAIL' "$frontend: $scenario assertions passed"
  if [ "$frontend" = native ]; then
    ui_expect_log '^Native viewport pointer:' "$scenario: pick used AppKit hit-testing and NSApp dispatch"
  fi
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend solid/open face-bounded and whole-face profile picking"

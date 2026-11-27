#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeForms.mm app/platform/macos/NativeMacWindow.mm app/MainWindowNative.cpp
# check-all sources: app/platform/macos/NativeMessageBand.mm
# check-all sources: app/MainWindowNoticeHover.cpp app/framework/CommandSession.cpp app/framework/NoticePlaces.cpp
# check-all sources: app/framework/TestDriver.cpp app/platform/macos/NativeTestInput.mm
# check-all sources: tools/ui-scripts/native-macos-notice-places.mitcad-ui tools/ui-scripts/notice-places.json tools/cli
# Notice places in the AppKit frontend (mitcad#1333, mitcad#1379), on the design of the
# Qt frontend's test (tools/ui-notice-places-test.sh, mitcad#1327): a rim's
# rounding that runs onto the rounding of the block's front edge warns in
# plain words (tools/ui-scripts/notice-places.json, made by mitcad-cli), and
# the pointer over the warning paints its places in the view (the
# rounding's face, a point) on the AppKit Timeline item, on the AppKit
# message band's warning and in the AppKit command form's message while it
# is edited; moving away clears them
# (tools/ui-scripts/native-macos-notice-places.mitcad-ui). The pointer is
# the in-process native hover, no system input. The screenshots
# (native-notice-places-*.png) show the highlight.
# Usage: tools/ui-macos-native-notice-places-test.sh <app> <mitcad-cli> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_SHOTS=$UI_OUT/shots
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_TEST_SHOTS"
"$cli" run "$here/ui-scripts/notice-places.json" --save "$MITCAD_TEST_WORK/notice-places.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-notice-places.mitcad-ui \
  ui_run_app native-notice-places --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/notice-places.mitcad"
ui_expect_log '^Notice hover highlighted: timeline: ' 'the AppKit Timeline item paints its places'
ui_expect_log '^Notice hover highlighted: message band: ' 'the AppKit message band paints its places'
ui_expect_log '^Notice hover highlighted: panel: ' "the AppKit command form's message paints its places"
ui_expect_log '^Notice hover highlighted: none$' 'leaving clears them'
ui_expect_log '^TestDriver: done' 'the notice places are painted on hover in the AppKit frontend'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: AppKit notice places on hover'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials/GuideStep.cpp app/tutorials/TutorialGuide.cpp app/tutorials/MainWindowGuide.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchSnap.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/guide-sketch-input.mitcad-ui
# A tutorial's guide steps on the learner's sketch input (mitcad#2440) on
# macOS, as tools/ui-guide-sketch-input-test.sh on Linux: the shared steps
# tools/ui-scripts/guide-sketch-input.mitcad-ui in an empty design in the Qt
# frontend (a sketch on XZ held back and one on XY taken; the 2-Point
# Rectangle's first corner off the origin held back and the one at the
# origin taken; the opposite corner without the width typed and Enter with
# another width held back, the width typed taken).
# Usage: tools/ui-macos-guide-sketch-input-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-guide-sketch-input-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=180
MITCAD_TEST_INPUT=$here/ui-scripts/guide-sketch-input.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app guide-sketch-input --ui=qt --no-recovery --no-mcp
ui_expect_log 'Guide: step plane plane xz, not its XY' 'a sketch on another plane is held back'
ui_expect_log 'Guide: step plane done: a sketch on XY' 'a sketch on the plane completes the plane step'
ui_expect_log 'Guide: step corner done: a point at origin' 'the first corner at the origin completes the point step'
ui_expect_log 'Guide: step width value 25 in width, not its 40' 'Enter with another width is held back'
ui_expect_log 'Guide: step width done: 40 typed into width' 'the width typed completes the value step'
ui_expect_log '^TestDriver: done' 'guide sketch input steps completed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed guide sketch input assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt guide steps on sketch input'

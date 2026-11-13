#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/OriginPreference.cpp app/view/OriginPreference.hpp app/view/DatumHandles.cpp
# check-all sources: app/MainWindowOrigin.cpp app/MainWindow.cpp app/OcctViewer.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/document/browser.rs
# check-all sources: tools/ui-scripts/origin-size.mitcad-ui
# Preferences › Display › Origin planes and the resize handles of a
# construction plane (mitcad#1748, GitHub mitcad/Mitcad#273 and #279)
# through the real UI, the input script tools/ui-scripts/origin-size.mitcad-ui
# on a new design: with Size Screen the origin planes keep their size on
# the screen through a zoom in and out (within 3 px), with Size Model 25 mm
# they are 25 mm planes, in the Corner and the Centered style; a click on a
# plane picks it in each; a construction plane's corner handle dragged sets
# the size the model keeps for it, one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-origin-size-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-origin-size.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1
export MITCAD_TEST_INPUT=$here/ui-scripts/origin-size.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'origin size driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'View origin planes: screen size, corner style; XY ' 'the origin drawn at a size on the screen, Corner style'
ui_expect_log 'View origin planes: model size, corner style; XY 25.000 mm a side' 'Size Model 25 mm: a 25 mm plane'
ui_expect_log 'View origin planes: screen size, centred style; XY ' 'the Centered style at a size on the screen'
ui_expect_log 'Datum handle F1 resized to ' "a construction plane's corner handle resized it"
ui_expect_log 'Undo: Resize Plane1' 'the resize was one undo step'
ui_expect_log 'TestDriver: done' 'the origin keeps its size on the screen, takes millimeters and is picked in each mode'
ui_finish 'UI origin size'

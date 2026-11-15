#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/DatumHandles.cpp app/view/DatumHandles.hpp app/MainWindowOrigin.cpp app/MainWindow.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp core/model/src/features/construction
# check-all sources: core/model/src/document/datums.rs core/model-api/src/analysis.rs tools/cli
# check-all sources: tools/ui-scripts/construction-size.mitcad-ui tools/ui-scripts/construction-size.json
# Construction planes drawn at the size and shape of what they come from
# (mitcad#1827, GitHub mitcad/Mitcad#321) in the Qt frontend on macOS, as
# tools/ui-construction-size-test.sh on Linux: the shared steps
# tools/ui-scripts/construction-size.mitcad-ui on a design that mitcad-cli
# makes (tools/ui-scripts/construction-size.json): a plane saved with a
# size of its own opens unchanged; a plane off a long narrow face is a
# rectangle over it, one over a small face is small; a corner handle's
# drag stores a size, Reset Size in its context menu brings the fitted
# rectangle back.
# Usage: tools/ui-macos-construction-size-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-construction-size-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/construction-size.mitcad
"$cli" run "$here/ui-scripts/construction-size.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run construction-size.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/construction-size.mitcad-ui MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1 \
  ui_run_app construction-size --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Datum F5 drawn 60 x 60 mm [(]display_size[)]$' 'a plane saved with a size of its own opens unchanged'
ui_expect_log '^Datum F3 drawn 220 x 30 mm [(]fitted[)]$' 'a plane off a long narrow face is a rectangle over it'
ui_expect_log '^Datum F4 drawn 16 x 16 mm [(]fitted[)]$' 'a plane over a small face is small'
ui_expect_log '^Datum handle F3 resized to ' "a corner handle's drag stored a size"
ui_expect_log '^Datum handle F3 size reset$' "Reset Size in the handle's menu"
ui_expect_log '^TestDriver: done' 'construction planes fitted to their faces, resized and reset'
ui_expect_no_log 'TestDriver: FAIL' 'no failed construction size assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt construction size'

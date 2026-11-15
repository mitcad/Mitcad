#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/DatumHandles.cpp app/view/DatumHandles.hpp app/MainWindowOrigin.cpp app/MainWindow.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp core/model/src/features/construction
# check-all sources: core/model/src/document/datums.rs core/model-api/src/analysis.rs tools/cli
# check-all sources: tools/ui-scripts/construction-size.mitcad-ui tools/ui-scripts/construction-size.json
# Construction planes drawn at the size and shape of what they come from
# (mitcad#1827, GitHub mitcad/Mitcad#321) through the real UI, the shared
# input script tools/ui-scripts/construction-size.mitcad-ui (also run by
# tools/ui-windows-construction-size-test.ps1 and
# tools/ui-macos-construction-size-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/construction-size.json): a plane saved with a
# size of its own opens unchanged; a plane off a long narrow face is a
# rectangle over it, one over a small face is small; a corner handle's
# drag stores a size, Reset Size in its context menu brings the fitted
# rectangle back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construction-size-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-construction-size.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1
export MITCAD_TEST_INPUT=$here/ui-scripts/construction-size.mitcad-ui
"$cli" run "$here/ui-scripts/construction-size.json" --save "$work/construction-size.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli run construction-size.json failed"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/construction-size.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'construction size driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Datum F5 drawn 60 x 60 mm (display_size)' 'a plane saved with a size of its own opens unchanged'
ui_expect_log 'Datum F3 drawn 220 x 30 mm (fitted)' 'a plane off a long narrow face is a rectangle over it'
ui_expect_log 'Datum F4 drawn 16 x 16 mm (fitted)' 'a plane over a small face is small'
ui_expect_log 'Datum handle F3 resized to ' "a corner handle's drag stored a size"
ui_expect_log 'Datum handle F3 size reset' 'Reset Size in the handle'"'"'s menu'
ui_expect_log 'TestDriver: done' 'construction planes fitted to their faces, resized and reset'
ui_finish 'UI construction size'

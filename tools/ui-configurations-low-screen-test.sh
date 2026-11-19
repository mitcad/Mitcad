#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationsDialog.cpp app/configurations/RuleCanvas.cpp
# check-all sources: app/configurations/RulesPage.cpp app/framework/FitToScreen.cpp
# check-all sources: app/framework/TestDriverWindowFit.cpp tools/ui-scripts/configurations-low-screen.mitcad-ui
# Configurations on a screen 720 logical px high (mitcad#2036): within the
# screen on the Options and the Rules tab, shorter than the Rules palette's
# full height, the palette scrolling to its last entry
# (tools/ui-scripts/configurations-low-screen.mitcad-ui).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-configurations-low-screen.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
"$CLI" run "$here/ui-scripts/configurations.json" --save "$WORK/configurations.mitcad" || exit 1
UI_SCREEN_HEIGHT=720
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/configurations-low-screen.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --open "$WORK/configurations.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the Configurations script timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the window fits the low screen and the palette scrolls"
ui_finish 'UI configurations low-screen'

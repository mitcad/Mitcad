#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/MainWindowSheet.cpp app/MainWindow.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSession.hpp app/browser/DocumentHost.hpp
# check-all sources: core/model/src/sheet core/model/src/features/sheet_folded.rs core/model/src/document/flange_profile.rs
# check-all sources: tools/ui-scripts/sheet-flange-profile.mitcad-ui tools/ui-scripts/sheet-flange-profile.json tools/cli
# A Flange's profile sketch edited from its panel (mitcad#1648, GitHub
# mitcad/Mitcad#237) through the real UI, the shared input script
# tools/ui-scripts/sheet-flange-profile.mitcad-ui (also run by
# tools/ui-windows-sheet-flange-profile-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/sheet-flange-profile.json): a new
# Flange's Edit Profile makes the profile sketch on the wall before the
# flange and opens it while the panel waits, Finish Sketch brings the panel
# back, OK adds the flange with the sketch as its profile after it in the
# timeline, and the flange edited again loads its profile and opens the
# same sketch.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-flange-profile-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-flange-profile.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-flange-profile.mitcad-ui
design=$work/sheet-flange-profile.mitcad
"$cli" run "$here/ui-scripts/sheet-flange-profile.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet flange profile driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Flange set aside while a sketch is edited' 'the panel waited while the profile sketch was edited'
ui_expect_log 'Flange back from the sketch Sketch2' 'Finish Sketch brought the panel back'
ui_expect_log 'Added Flange as F6' 'the flange was added with its profile'
ui_expect_log 'TestDriver: done' 'the profile is edited from the Flange panel and linked in the timeline'
ui_finish 'UI sheet flange profile'

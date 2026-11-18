#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser/DocumentHost.hpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/MainWindowJobs.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/ffi/src/kernel/mod.rs core/ffi/src/kernel/abandon.rs
# check-all sources: tools/ui-scripts/drawing-large-view.mitcad-ui tools/ui-scripts/drawing-large-view.json
# A large design's drawing (mitcad#1997, GitHub #425) through the real UI,
# the shared input script tools/ui-scripts/drawing-large-view.mitcad-ui
# (also run by tools/ui-windows-drawing-large-view-test.ps1 and
# tools/ui-macos-drawing-large-view-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/drawing-large-view.json: 36 cylinders and a
# drawing of them), each projection of the jobs held 3 s as a large view's
# takes: the drawing opened is drawn in a job while the window stays
# usable, with its note; a new scale shows the old lines scaled while the
# command projects; a base view's placing cancelled adds nothing, placed
# again it is projected once inside its command; a model change draws the
# sheet in a job.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-large-view-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-large-view.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-large-view.mitcad-ui
export MITCAD_TEST_RECOMPUTE_DELAY_MS='add_drawing_view=3000,edit_drawing_view=3000,drawing sheet=3000'
"$cli" run "$here/ui-scripts/drawing-large-view.json" --save "$work/cylinders.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/cylinders.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'large view driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drawing note: Drawing the views' 'the opened sheet drawn in a job, with its note'
ui_expect_log 'Drawing views updating: 1 by 0.5' 'the old lines scaled while the new scale is projected'
ui_expect_log 'Computing add_drawing_view cancelled after ' 'the placing cancelled'
ui_expect_log 'TestDriver: done' 'placed once, cancelled without a view, drawn in jobs after a model change'
ui_finish 'UI drawing of a large design: jobs, progress, cancel and the old lines scaled'

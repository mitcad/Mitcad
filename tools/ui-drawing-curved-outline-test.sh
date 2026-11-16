#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_silhouettes.rs
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src/annotate.rs core/model-drawing/src/compose.rs
# check-all sources: core/cpp/bridge/face_outline.cpp geometry/src/face_outline.cpp
# check-all sources: tools/ui-scripts/drawing-curved-outline.mitcad-ui tools/ui-scripts/drawing-curved-outline.json
# Dimensions to a curved outline's top and a section's label dragged
# (mitcad#1909, GitHub #363) through the real UI, the shared input script
# tools/ui-scripts/drawing-curved-outline.mitcad-ui (also run by
# tools/ui-windows-drawing-curved-outline-test.ps1 and
# tools/ui-macos-drawing-curved-outline-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/drawing-curved-outline.json: a pipe
# bend seen along its axis and a section through it): Dimension from the
# bend's end to the top of its outer outline (a silhouette of its torus
# face, no edge) reads 70; the section's label dragged keeps its offset,
# and Reset Label Position puts it back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-curved-outline-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-curved-outline.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-curved-outline.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-curved-outline.json" --save "$work/bend.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/bend.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'curved outline driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drawing picked point (2): ' 'the top of the outer outline picked as the second pick'
ui_expect_log 'Drawing view 2 label dragged by ' 'the section label dragged'
ui_expect_log 'TestDriver: done' 'the overall height read 70, the label moved and put back'
ui_finish 'UI drawing dimension to a curved outline and a dragged section label'

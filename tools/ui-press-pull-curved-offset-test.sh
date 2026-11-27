#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/commands/CommandSupport.cpp app/framework/ManipulatorOverlay.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: geometry/src/datum.cpp geometry/src/faceops.cpp core/model/src/features/offset_face.rs
# check-all sources: tools/ui-scripts/press-pull-curved-offset.mitcad-ui tools/ui-scripts/press-pull-curved-offset.json
# Press Pull's Offset on a curved face (mitcad#2509, the arrow of
# mitcad#2478) through the real UI, the shared input script
# tools/ui-scripts/press-pull-curved-offset.mitcad-ui (also run by
# tools/ui-windows-press-pull-curved-offset-test.ps1 and
# tools/ui-macos-press-pull-curved-offset-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/press-pull-curved-offset.json): on a
# tapered Extrude's side the arrow starts on the face along its outward
# normal; dragged, the preview is ok and OK commits a body that grew; the
# dragged distance typed gives the same body, 0.5 mm typed the grown
# frustum's volume.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-press-pull-curved-offset-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-press-pull-curved-offset.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/press-pull-curved-offset.mitcad-ui
"$cli" run "$here/ui-scripts/press-pull-curved-offset.json" --save "$work/press-pull-curved-offset.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/press-pull-curved-offset.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'press pull curved offset driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Press Pull made offset face, distance ' "the side offset by the dragged arrow"
ui_expect_log 'Press Pull made offset face, distance 0.5' 'the side offset by 0.5 mm typed'
ui_expect_log 'TestDriver: done' 'the arrow on the curved face dragged and typed'
ui_finish 'UI Press Pull Offset on a curved face'

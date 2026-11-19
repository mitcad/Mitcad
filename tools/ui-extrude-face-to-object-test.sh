#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/extrude.rs core/ffi/src/kernel/face_extrude.rs
# check-all sources: core/cpp/bridge/face_extrude.cpp geometry/src/face_extrude.cpp geometry/src/extrude.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-face-to-object.mitcad-ui tools/ui-scripts/extrude-face-to-object.json
# Extrude of a body face up to another face of the same body (mitcad#2033)
# through the real UI, the shared input script
# tools/ui-scripts/extrude-face-to-object.mitcad-ui (also run by
# tools/ui-windows-extrude-face-to-object-test.ps1 and
# tools/ui-macos-extrude-face-to-object-test.sh) on a stepped block that
# mitcad-cli makes (tools/ui-scripts/extrude-face-to-object.json): the lower
# step's top is the profile, Extent To Object takes the higher step's top
# (To Selected Face), the automatic Join builds and OK adds it: one level
# body of 72000 mm3.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-face-to-object-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-face-to-object.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100 MITCAD_LOG_PICKS=1

"$cli" run "$here/ui-scripts/extrude-face-to-object.json" --save "$work/stepped.mitcad" || exit 1
ui_start_display

echo "--- The lower step's top extruded up to the higher step's top and joined"
export MITCAD_TEST_INPUT=$here/ui-scripts/extrude-face-to-object.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$work/stepped.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'extrude of a face to an object driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Extrude Profiles: 1 face [face F2:end(' 'the lower top is the profile'
ui_expect_log 'Extrude Object: 1 face [face F4:end(' 'the higher top of the same body is the object'
ui_expect_log 'Added extrude (Join), distance to object, of a face' 'the Join up to the face added'
ui_expect_log 'TestDriver: done' 'Extrude of a face up to a face of its own body'

ui_finish 'UI extrude of a body face up to an object'

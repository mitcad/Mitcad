#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print/MakeBodyCard.cpp app/MainWindowMake.cpp app/files/MainWindowPrint.cpp
# check-all sources: app/files/Print3d.cpp app/framework/TestDriver.cpp core/model/src/exchange tools/cli
# The Make card's Send to Slicer (mitcad#1377), the shared input script
# tools/ui-scripts/make-card-send.mitcad-ui (also run by
# tools/ui-windows-make-card-send-test.ps1) on two shown boxes: a face of
# Body2 selected on the Make tab shows the card with its Send to Slicer
# button, a face of Body1 added takes the card away, and with Body2's face
# alone again the button opens Send to Slicer's panel with Body2 alone in
# Bodies; Send (a fixture slicer that does nothing, STL) writes Body2.stl
# and nothing else in the design's folder for the slicer.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-make-card-send-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-make-card-send.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/make-card-send.mitcad-ui
# The app's temporary folder, where the files for the slicer go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR" "$XDG_CONFIG_HOME/Mitcad"
"$CLI" run "$here/ui-scripts/make-card-send.json" --save "$WORK/two-bodies.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
# A slicer that does nothing, STL: a file per body stays where it was
# written.
printf '[print]\nslicerName=Fixture slicer\nslicerProgram=%s\nformat=stl\nreuseWindow=false\n' \
  "$(command -v true)" > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/two-bodies.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "Make card Send to Slicer driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the Make card's Send to Slicer opens the panel with Body2 alone, Send writes it"
# The folder for the slicer holds Body2's file alone.
FOLDER=$TMPDIR/mitcad-print/two-bodies
[ "$(ls "$FOLDER" 2> /dev/null)" = "Body2.stl" ] || ui_fail "the folder for the slicer has: $(ls "$FOLDER" 2>&1)"
[ "$(stat -c %s "$FOLDER/Body2.stl" 2> /dev/null || echo 0)" -ge 684 ] || ui_fail "Body2.stl is too small"
echo "ok   Body2.stl alone written"
ui_finish "UI the Make card's Send to Slicer"

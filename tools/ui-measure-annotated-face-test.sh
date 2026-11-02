#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/PickContext.cpp app/view/MeasurementOverlay.cpp app/commands/InspectCommands.cpp tools/cli
# check-all sources: app/MainWindowMake.cpp app/MainWindowMeasurements.cpp app/MainWindowAnnotations.cpp
# Measure on a face that carries an annotation (mitcad#1166, GitHub #105):
# a plate with a joined round boss; Measure picks the boss's top face, a
# diameter annotation is added on its rim from Make > Dimension, then
# Measure picks the top face again (logged) and the view's frame shows the
# boss's top and side faces in the colours they had before the annotation
# (not see-through; capture-view-pixel, expect-view-pixel). The Windows twin is
# tools/ui-windows-measure-annotated-face-test.ps1; both run
# tools/ui-scripts/measure-annotated-face.mitcad-ui.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-measure-annotated-face.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/measure-annotated-face.mitcad-ui
"$cli" run "$here/cli/tests/measure_annotated_boss.json" --save "$work/boss.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/boss.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'annotated face Measure driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Measure picks the annotated face, drawn opaque'
ui_finish 'UI Measure on an annotated face'

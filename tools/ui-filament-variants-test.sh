#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/framework/TestDriver.cpp tools/cli app/browser/LibraryPicker.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/material.rs core/model/src/print_files core/model/src/document/slicer_presets.rs
# check-all sources: core/model/src/document/material_variants.rs core/model/src/document/print_settings.rs
# One filament, its variants per printer and nozzle (mitcad#1054), on a
# private Xvfb display: the test driver's script
# tools/ui-scripts/filament-variants.mitcad-ui (shared with the Windows and
# macOS tests; it says what it does) on a design mitcad-cli makes from
# tools/ui-scripts/filament-variants.json, with the synthetic slicer folder
# tools/ui-scripts/filament-variants-slicer as the installed slicer's.
# Usage: tools/ui-filament-variants-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-filament-variants.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
"$CLI" run "$here/ui-scripts/filament-variants.json" --save "$WORK/variants.mitcad" > "$WORK/setup.log" 2>&1 || {
  cat "$WORK/setup.log" >&2
  exit 1
}
# The user's library in the test's own folder: only the design's printers
# are offered.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
export MITCAD_TEST_SLICER_FOLDERS="[{\"dialect\": \"bambu_studio\", \"path\": \"$here/ui-scripts/filament-variants-slicer\"}]"
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/filament-variants.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$WORK/variants.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'filament variants driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'variants in 3D Print, Import Presets and the Materials window passed'
echo 'PASS: filament variants'

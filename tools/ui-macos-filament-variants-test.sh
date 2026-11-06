#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/framework/TestDriver.cpp core/model/src/material.rs core/model/src/print_files app/browser/LibraryPicker.cpp app/framework/CommandPanel.cpp
# One filament, its variants per printer and nozzle (mitcad#1054): the
# shared driver script tools/ui-scripts/filament-variants.mitcad-ui (3D
# Print's Materials row, Import Presets grouped by filament with the
# printers in use and the merge of separate materials, the Materials
# window's Variants tab) through the Qt frontend in the macOS VM.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-filament-variants}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/filament-variants.json" --save "$UI_OUT/files/variants.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
export MITCAD_PRINT_LIBRARY_DIR=$UI_OUT/files/print-library
export MITCAD_TEST_SLICER_FOLDERS="[{\"dialect\": \"bambu_studio\", \"path\": \"$here/ui-scripts/filament-variants-slicer\"}]"
MITCAD_TEST_STEP_MS=${MITCAD_TEST_STEP_MS:-70} MITCAD_TEST_SETTLE_MS=${MITCAD_TEST_SETTLE_MS:-150} \
  MITCAD_TEST_INPUT=$here/ui-scripts/filament-variants.mitcad-ui \
  ui_run_app variants --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$UI_OUT/files/variants.mitcad"
ui_expect_log 'TestDriver: done' 'variants in 3D Print, Import Presets and the Materials window passed'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1

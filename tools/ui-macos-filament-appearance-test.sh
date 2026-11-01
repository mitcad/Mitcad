#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserMaterialMenu.cpp app/framework/TestDriver.cpp core/model/src/material.rs core/model/src/document/materials.rs
# A filament's colour is its appearance (mitcad#1055): the shared driver
# script tools/ui-scripts/filament-appearance.mitcad-ui (Material ▸ PLA
# Basic ▸ an appearance for bodies and the root, a long material name shown
# in full in the Materials window's list, General's list of a material's
# appearances with Add… (the appearance library's popover), Make Default
# and Remove) through the Qt frontend in the
# macOS VM, in the light and the dark appearance; the pictures stay in the
# output folder.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-filament-appearance}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/filament-appearance.json" --save "$UI_OUT/files/appearance.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
export MITCAD_PRINT_LIBRARY_DIR=$UI_OUT/files/print-library
export MITCAD_TEST_WORK=$UI_OUT/files
for scheme in light dark; do
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=70 MITCAD_TEST_SETTLE_MS=150 \
    MITCAD_TEST_INPUT=$here/ui-scripts/filament-appearance.mitcad-ui \
    ui_run_app "$scheme" --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$UI_OUT/files/appearance.mitcad"
  ui_expect_log 'TestDriver: done' "$scheme: Material ▸ material ▸ appearance and the Materials window's appearances passed"
  ui_expect_no_log '^TestDriver: FAIL' "$scheme: no failed driver assertion"
  [ "$UI_FAILURES" = 0 ] || exit 1
done

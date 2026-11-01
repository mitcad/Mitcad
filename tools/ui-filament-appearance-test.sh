#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserMaterialMenu.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/material.rs core/model/src/document/materials.rs core/model/src/document/print_settings.rs
# check-all sources: core/model-base/src/material_class.rs core/model-base/src/appearance.rs
# A filament's colour is its appearance (mitcad#1055), on a private Xvfb
# display: the test driver's script
# tools/ui-scripts/filament-appearance.mitcad-ui (shared with the Windows
# and macOS tests; it says what it does: Material ▸ PLA Basic ▸ an
# appearance for bodies and the root, a long material name shown in full in
# the Materials window's list, General's list of a material's appearances
# with Add… (the appearance library's popover, the material's class first,
# mitcad#1102), Make Default and Remove, a filament's Class fixed)
# on a design mitcad-cli makes from
# tools/ui-scripts/filament-appearance.json, once in the light and once in
# the dark color scheme (Qt's GTK platform theme, as tools/ui-theme-test.sh).
# The pictures of the Materials window and the picker stay in
# build/ui-filament-appearance-pictures (UI_PICTURES).
# Usage: tools/ui-filament-appearance-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
ROOT=$(cd "$here/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-filament-appearance.XXXXXX")
PICTURES=${UI_PICTURES:-$ROOT/build/ui-filament-appearance-pictures}
mkdir -p "$PICTURES"
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
"$CLI" run "$here/ui-scripts/filament-appearance.json" --save "$WORK/appearance.mitcad" > "$WORK/setup.log" 2>&1 || {
  cat "$WORK/setup.log" >&2
  exit 1
}
# The user's library in the test's own folder: no recent materials.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
export MITCAD_TEST_WORK=$PICTURES
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME
ui_start_display
for scheme in light dark; do
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/filament-appearance.mitcad-ui \
    MITCAD_TEST_STEP_MS=70 ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$WORK/appearance.mitcad"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2>/dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail "filament appearance driver timed out ($scheme)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$scheme: Material ▸ material ▸ appearance and the Materials window's appearances passed"
done
echo "PASS: filament appearance (pictures in $PICTURES)"

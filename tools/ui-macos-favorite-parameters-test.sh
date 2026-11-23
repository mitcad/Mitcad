#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/FavoriteParametersCard.cpp app/browser/FavoriteParameterRows.cpp
# check-all sources: app/browser/MainWindowFavoriteParameters.cpp tools/ui-scripts/favorite-parameters.mitcad-ui
# Favorite Parameters (mitcad#2203, GitHub mitcad/Mitcad#547) on macOS in
# the Qt frontend, through the test driver: the shared steps
# tools/ui-scripts/favorite-parameters.mitcad-ui (also run by
# tools/ui-favorite-parameters-test.sh and
# tools/ui-windows-favorite-parameters-test.ps1) on the design mitcad-cli
# makes of tools/ui-scripts/favorite-parameters.json, in the Floating
# layout and then in the Docked one; after each, a second start finds the
# card shown as chosen and its own close button hides it.
# Usage: tools/ui-macos-favorite-parameters-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-favorite-parameters}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/favorite-parameters.json" --save "$files/favorite-parameters.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
for layout in floating docked; do
  MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 FAVORITES_LAYOUT=$layout \
    MITCAD_TEST_INPUT=$here/ui-scripts/favorite-parameters.mitcad-ui \
    ui_run_app "favorite-parameters-$layout" --ui=qt --no-recovery --no-mcp --chrome="$layout" \
    --open "$files/favorite-parameters.mitcad"
  ui_expect_log 'Favorite parameter width set: 900 mm = 900 mm' "$layout: a typed value, the unit added"
  ui_expect_log 'Favorite parameter width refused: 2600: ' "$layout: a value above the strict maximum refused"
  ui_expect_log '^Favorite parameters card: none$' "$layout: the empty card"
  ui_expect_log 'TestDriver: done' "Favorite Parameters ($layout)"
  ui_expect_no_log '^TestDriver: FAIL' "$layout: no failed driver assertion"
  MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 FAVORITES_LAYOUT=$layout \
    MITCAD_TEST_INPUT=$here/ui-scripts/favorite-parameters-close-$layout.mitcad-ui \
    ui_run_app "favorite-parameters-close-$layout" --ui=qt --no-recovery --no-mcp --chrome="$layout" \
    --open "$files/favorite-parameters.mitcad"
  ui_expect_log 'Favorite parameters panel at the start: shown \(as chosen\)' "$layout: shown as chosen"
  ui_expect_log 'Favorite parameters: closed by its button' "$layout: its own close button hides it"
  ui_expect_log 'TestDriver: done' "Favorite Parameters closed ($layout)"
  ui_expect_no_log '^TestDriver: FAIL' "$layout: no failed driver assertion"
done
[ "$UI_FAILURES" = 0 ] || exit 1

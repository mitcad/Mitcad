#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/FavoriteParametersCard.cpp app/browser/FavoriteParametersCard.hpp
# check-all sources: app/browser/FavoriteParameterRows.cpp app/browser/FavoriteParameterRows.hpp
# check-all sources: app/browser/MainWindowFavoriteParameters.cpp app/browser/BrowserController.cpp
# check-all sources: app/browser/ParametersDialog.cpp app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp
# check-all sources: app/MainWindowDocks.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/favorite-parameters.mitcad-ui tools/ui-scripts/favorite-parameters.json
# check-all sources: tools/ui-scripts/favorite-parameters-close-floating.mitcad-ui
# check-all sources: tools/ui-scripts/favorite-parameters-close-docked.mitcad-ui
# Favorite Parameters (mitcad#2203, GitHub mitcad/Mitcad#547) through the
# real UI, the shared input script tools/ui-scripts/favorite-parameters.mitcad-ui
# (also run by tools/ui-windows-favorite-parameters-test.ps1 and
# tools/ui-macos-favorite-parameters-test.sh) on the design mitcad-cli makes
# of tools/ui-scripts/favorite-parameters.json, in the Floating layout and
# then in the Docked one: MANAGE > Favorite Parameters shows the empty
# card, its Open Parameters... the Parameters window where two parameters
# are starred (one with a slider, one a formula); a value typed (one undo
# step), the slider dragged (the field shows the value at the handle, the
# model takes it on release), a value above the strict maximum refused
# (the old value kept, the reason shown, Esc restores), undo followed, a
# favorite unstarred (its row goes), the empty state again; the toggle
# hides and shows it. After each layout's run a second start finds the
# card shown as chosen and its own close button hides it
# (favorite-parameters-close-floating.mitcad-ui, -close-docked.mitcad-ui).
# Pictures of the card, light and dark, in both layouts, stay in the work
# folder (build/ui-favorite-parameters, overwritten by the next run).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-favorite-parameters-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-favorite-parameters
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/favorite-parameters.mitcad
"$cli" run "$here/ui-scripts/favorite-parameters.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_WORK=$work
# Qt's GTK platform theme gives the palettes: the generic one has no dark
# scheme and ignores the script's color-scheme request (the dark picture).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
ui_start_display

# run_script <script> <layout> <what>: the app on the design in that
# layout, driven by the script to its end.
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1
  export FAVORITES_LAYOUT=$2
  ui_start_app --ui=qt --no-mcp --chrome="$2" --open "$file"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$3: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$3: the driver failed"; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$3"
}

for layout in floating docked; do
  echo "--- $layout: the card's rows, fields and slider"
  run_script favorite-parameters.mitcad-ui "$layout" "Favorite Parameters ($layout)"
  ui_expect_log 'Favorite parameter width set: 900 mm = 900 mm' "$layout: a typed value, the unit added"
  ui_expect_log 'Favorite parameter width refused: 2600: ' "$layout: a value above the strict maximum refused"
  ui_expect_log 'Favorite parameters card: none' "$layout: the empty card"
  for picture in empty light dark drag; do
    [ -s "$work/favorite-parameters-$picture-$layout.png" ] || ui_fail "no picture $picture ($layout)"
  done
  echo "ok   pictures: $work/favorite-parameters-{empty,light,dark,drag}-$layout.png"
  echo "--- $layout: shown at the next start, hidden by its own close button"
  run_script "favorite-parameters-close-$layout.mitcad-ui" "$layout" "Favorite Parameters closed ($layout)"
  ui_expect_log 'Favorite parameters panel at the start: shown (as chosen)' "$layout: shown as chosen"
  ui_expect_log 'Favorite parameters: closed by its button' "$layout: its own close button hides it"
done
ui_finish 'UI favorite parameters test'

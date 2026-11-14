#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/order.rs core/sketch/src/steps.rs core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/dispatch.rs core/model/src/document.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# check-all sources: app/browser/FeatureStatus.cpp app/framework/Icons.cpp app/framework/FloatingLayout.cpp
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, after mitcad#1492's list of items), through the real UI on
# a design that mitcad-cli makes (tools/ui-scripts/sketch-item-list.json: a
# box, an empty Sketch2 on its top face, and Sketch3 saved without steps,
# with an inactive Horizontal):
# one Project of the face's outline, one Rectangle and one dimension show
# three cells, icons only; the Project cell's hover list lists its items, a
# row highlights its edge and a click selects it, and the Project cell is
# drawn selected with its icon in its own colors; a double-click on the
# dimension's cell opens its value editor; a refused Parallel puts the
# error badge on the Rectangle cell; a click on the Rectangle cell selects
# its items and Delete removes them and the cell; Sketch3 shows one cell
# per item, the inactive Horizontal's dimmed; Finish Sketch brings the
# timeline back. The steps are tools/ui-scripts/sketch-item-list.mitcad-ui,
# run in the Docked layout and in the Floating one, where the hover list
# is over the message band's card and its low rows take the pointer
# (mitcad#1763). In both the list's icons are in the middle of the bar it
# is in (mitcad#1760): equal room above and below within 1 px. The
# Floating card is a row of cells high in sketch mode; the Docked panel
# keeps the timeline's height, and the view never changes its size.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-item-list-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-item-list.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/sketch-item-list.json" --save "$work/items.mitcad" || exit 1
ui_start_display

# The bar's places (mitcad#1760), from "Sketch steps bar at x,y wxh" and the
# "Sketch step <key> icon at x,y wxh" lines after it: each icon in the
# bar's middle (equal room above and below within 1 px), and the bar no
# more than $2 px taller than an icon: Floating 14 (the cell's 3 px around
# the icon and the card's 4 px margins: a row of cells), Docked 24 (the
# timeline's panel: its marker's band and the panel's 6 px margins too).
check_bar() {
  local verdict
  verdict=$(awk '
    /^Sketch steps bar at / { split($5, at, ","); split($6, size, "x"); top = at[2]; high = size[2]; bar = 1; next }
    bar && /^Sketch step [^ ]+ icon at / {
      split($6, at, ","); split($7, size, "x");
      above = at[2] - top; below = top + high - at[2] - size[2];
      if (above - below > 1 || below - above > 1) { print "icon " $3 " off the middle: " above " px above, " below " below"; bad = 1 }
      if (high - size[2] > most) { print "bar " high " px high for a " size[2] " px icon"; bad = 1 }
      icons++
    }
    END { if (!icons) print "no icon places logged"; else if (!bad) print "ok" }' most="$2" "$UI_LOG")
  [ "$verdict" = ok ] || ui_fail "$1: $verdict"
  echo "ok   $1: the icons in the middle of the bar, at most $2 px taller than they are"
}

# The view's size: the same all along, in sketch mode too (a view that
# changed its height for a sketch drew the design at another size after it).
check_view() {
  local areas
  areas=$(grep '^View area ' "$UI_LOG" | sort -u)
  [ -n "$areas" ] && [ "$(echo "$areas" | wc -l)" = 1 ] || ui_fail "$1: the view changed its size: $areas"
  echo "ok   $1: the view kept its size"
}

# run_app <chrome> <script>: the app in that layout through the script.
run_app() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$2
  ui_start_app --ui=qt --no-mcp --chrome=$1 --open "$work/items.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "sketch step list driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
}

# The whole script in each layout (mitcad#1763: in the Floating one too);
# the bar at most this many px taller than an icon (mitcad#1760): the
# Floating card a row of cells and its margins, the Docked panel the
# timeline's height.
declare -A limit=([docked]=24 [floating]=14)
for chrome in docked floating; do
  run_app $chrome sketch-item-list.mitcad-ui
  ui_expect_log "Sketch item list in the timeline's place" "$chrome: the step list in the timeline's place in sketch mode"
  ui_expect_log ' project x' "$chrome: the face projected as one Project step"
  ui_expect_log ' rectangle x' "$chrome: the rectangle as one step"
  ui_expect_log ' add_dimension x1' "$chrome: the dimension as one step"
  ui_expect_log ' hover list: Project: ' "$chrome: the Project step's hover list"
  ui_expect_log ' hovered' "$chrome: the hover list's rows under the pointer"
  ui_expect_log ' clicked' "$chrome: an item selected from its row"
  ui_expect_log ' drawn: selected, icon normal' "$chrome: a selected cell's icon in its own colors"
  ui_expect_log ' [error]' "$chrome: the conflict badge on its step"
  ui_expect_log 'Sketch steps: p1 point, p2 point, c3 line, k4 horizontal inactive' \
    "$chrome: a sketch without steps: one cell per item"
  ui_expect_log 'Sketch step k4 drawn: dimmed, icon disabled' "$chrome: an inactive item's cell dimmed"
  ui_expect_log 'Timeline back in its place' "$chrome: the timeline back after the sketch"
  ui_expect_log 'TestDriver: done' "$chrome: the steps listed, hovered, selected, deleted and edited"
  check_bar $chrome "${limit[$chrome]}"
  check_view $chrome
done
ui_finish 'UI sketch step list'

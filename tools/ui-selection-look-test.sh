#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/SelectionLook.cpp app/view/SelectionLook.hpp
# check-all sources: app/view/ViewController.cpp app/MainWindow.cpp app/MainWindowSelection.cpp
# check-all sources: app/sketch/SketchOverlay.cpp app/sketch/PointMarkers.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/selection-look.mitcad-ui tools/ui-scripts/selection-look.json
# The selection's look (mitcad#1803, GitHub mitcad/Mitcad#309): a selected
# edge's highlight is drawn Selected line width's logical pixels times the
# device pixel ratio wide, wider than a plain edge, in Preferences ›
# Display › Selection color; a change of either draws the selection anew,
# in the view and in sketch mode. Through the real UI, the input script
# tools/ui-scripts/selection-look.mitcad-ui (also run by
# tools/ui-windows-selection-look-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/selection-look.json). The script also measures
# the lines in the view's own frame (view-line-width, mitcad#2063).
# Runs at device pixel ratio 1 and again at 2 (UI_SCALE, QT_SCALE_FACTOR).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-selection-look-test.sh
set -uo pipefail
if [ -z "${UI_SCALE:-}" ]; then
  UI_SCALE=1 bash "$0" "$@" || exit 1
  UI_SCALE=2 bash "$0" "$@"
  exit
fi
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-selection-look.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=30
export MITCAD_TEST_INPUT=$here/ui-scripts/selection-look.mitcad-ui
export MITCAD_TEST_PREFERENCES_DONE=Return # OK of the Preferences dialog
"$cli" run "$here/ui-scripts/selection-look.json" --save "$work/selection-look.mitcad" || exit 1
echo "=== Device pixel ratio $UI_SCALE"
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/selection-look.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'selection look driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'View selection look: color #1f7aff, line width normal; ' 'Normal and the selection blue by default'
ui_expect_log 'Preferences: selection color #d02090, line width normal' 'Selection color saved'
ui_expect_log 'Preferences: selection color #d02090, line width thick' 'Selected line width saved'
ui_expect_log 'TestDriver: done' 'the selection drawn in the chosen color and width, in the view and in sketch mode'

# Each "View highlight line: <kind> <px> px (<logical> x ratio <r>), color
# <#rrggbb>; plain edges <px> px": drawn at the logical width times this
# run's ratio, wider than a plain edge.
lines=$(grep -E '^View highlight line: ' "$UI_LOG")
[ -n "$lines" ] || ui_fail 'no highlight line logged'
while IFS= read -r line; do
  [ -n "$line" ] || continue
  if [[ $line =~ ^View\ highlight\ line:\ ([a-z ]+)\ ([0-9.]+)\ px\ \(([0-9.]+)\ x\ ratio\ ([0-9.]+)\),\ color\ (#[0-9a-f]{6})\;\ plain\ edges\ ([0-9.]+)\ px$ ]]; then
    kind=${BASH_REMATCH[1]} drawn=${BASH_REMATCH[2]} logical=${BASH_REMATCH[3]} ratio=${BASH_REMATCH[4]}
    color=${BASH_REMATCH[5]} plain=${BASH_REMATCH[6]}
    awk -v d="$drawn" -v l="$logical" -v r="$ratio" -v s="$UI_SCALE" -v p="$plain" \
      'BEGIN { exit !((d - l * r) ^ 2 < 1e-6 && (r - s) ^ 2 < 1e-6 && d > p) }' ||
      ui_fail "$kind drawn $drawn px: not $logical x the ratio $UI_SCALE, or not wider than a plain edge ($plain px)"
    echo "ok   $kind highlight $drawn px ($logical x ratio $ratio) in $color, plain edges $plain px"
  else
    ui_fail "unexpected highlight line: $line"
  fi
done <<< "$lines"
# The widths and the colors of the steps, in this order among the lines
# logged: Normal in the blue, in the chosen color, then Thick, then the
# sketch's line.
expected="edge $((4 * UI_SCALE)) #1f7aff
edge $((4 * UI_SCALE)) #d02090
edge $((6 * UI_SCALE)) #d02090
sketch curve $((6 * UI_SCALE)) #d02090"
seen=$(sed -n 's/^View highlight line: \([a-z ]*\) \([0-9.]*\) px .*, color \(#[0-9a-f]*\);.*/\1 \2 \3/p' "$UI_LOG")
missing=$(awk -v want="${expected//$'\n'/|}" 'BEGIN { n = split(want, w, "|"); i = 1 }
  i <= n && $0 == w[i] { i++ }
  END { if (i <= n) print w[i] }' <<< "$seen")
[ -z "$missing" ] || ui_fail "no highlight line '$missing' in its turn among
$seen"
echo "ok   the edge and the sketch line drawn $((4 * UI_SCALE)) then $((6 * UI_SCALE)) px at ratio $UI_SCALE"
ui_finish "UI selection look at device pixel ratio $UI_SCALE"

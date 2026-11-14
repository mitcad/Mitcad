#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/view/CamStockOverlay.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp app/framework/Ribbon.cpp
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# CAM's CNC group on the Make tab (mitcad#1013, app/COMMANDS.md "The CNC
# group"), the shared input script tools/ui-scripts/cam.mitcad-ui (also run
# by tools/ui-windows-cam-test.ps1) on a block with a second sketch and a
# router (tools/ui-scripts/cam.json): New Setup, Facing, Contour, Pocket
# and Drill from their panels; Generate (every operation up to date and
# drawn: the moves of each operation's toolpath add up to the moves drawn);
# the block's height changed (every operation out of date, drawn muted);
# Simulate to the end (the stock as cut, shaded at the end by the material
# left); Post Process… to grbl (the dialog's notice that CAM is highly
# experimental; the file's first lines: the same notice as four comments,
# the setup's name, the tools, G90 G94 G17, G21, G54; every operation by its
# name; a manual tool change; M30 at the end); Machines and Tools… (a tool
# added); the Browser's CAM rows (an operation dragged, one suppressed, one
# opened in its panel).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam.mitcad-ui
# The user's machine library of the test's own.
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam.json" --save "$WORK/cam.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "setup, four operations, generate, out of date, simulate, post, tools and the Browser's rows"

# The toolpaths drawn after Generate: each operation's moves, and their sum
# the moves drawn.
read -r drawn counted operations < <(awk '
  /^CAM toolpath [^:]+: [0-9]+ moves/ { split($0, a, ": "); split(a[2], b, " "); sum += b[1]; n++; next }
  /^CAM toolpaths drawn: 4 operation\(s\), [0-9]+ moves$/ { split($0, c, ", "); split(c[2], d, " "); print d[1], sum, n; exit }
  /^CAM toolpaths drawn: / { sum = 0; n = 0 }
' "$UI_LOG")
[ -n "${drawn:-}" ] || ui_fail "no toolpaths drawn for four operations"
[ "$operations" = 4 ] || ui_fail "the toolpaths of $operations operations logged, not 4"
[ "$drawn" -gt 0 ] && [ "$drawn" = "$counted" ] || ui_fail "moves drawn $drawn, the operations' moves add up to $counted"
for name in "Facing 1" "Contour 1" "Pocket 1" "Drill 1"; do
  grep -qE "^CAM toolpath $name: [1-9][0-9]* moves \(rapid [0-9]+, feed [0-9]+, entry [0-9]+\), [1-9][0-9]* segments$" \
    "$UI_LOG" || ui_fail "no toolpath of $name drawn"
done
echo "ok   the toolpaths drawn: $drawn moves of four operations"

# The program for grbl: its first lines and every operation.
FILE=$WORK/setup1.nc
[ -s "$FILE" ] || ui_fail "no program written"
[ "$(sed -n 1p "$FILE")" = "(WARNING: MitCad CAM is highly experimental.)" ] ||
  ui_fail "the program does not start with the notice that CAM is experimental: $(head -4 "$FILE")"
sed -n 2,4p "$FILE" | grep -qvE '^\(.*\)$' && ui_fail "the notice is not four comments: $(head -4 "$FILE")"
[ "$(sed -n 5p "$FILE")" = "(Setup 1)" ] || ui_fail "the program does not name the setup after the notice: $(head -7 "$FILE")"
sed -n 6p "$FILE" | grep -qE '^\(T1 6 mm flat end mill' || ui_fail "the first tool is not listed: $(head -8 "$FILE")"
sed -n 7p "$FILE" | grep -qE '^\(T2 5 mm drill' || ui_fail "the second tool is not listed: $(head -8 "$FILE")"
[ "$(sed -n 8,10p "$FILE" | tr '\n' '|')" = "G90 G94 G17|G21|G54|" ] ||
  ui_fail "the header is not grbl's in millimeters: $(sed -n 8,10p "$FILE" | tr '\n' '|')"
for name in "Facing 1" "Contour 1" "Pocket 1" "Drill 1"; do
  grep -qxF "($name)" "$FILE" || ui_fail "no $name in the program"
done
grep -qx 'M0' "$FILE" || ui_fail "no stop for the drill's manual change"
[ "$(tail -1 "$FILE")" = "M30" ] || ui_fail "the program does not end with M30"
grep -q 'G20' "$FILE" && ui_fail "the program is in inches"
echo "ok   grbl's program: header, tools, four operations, the manual change, M30"
ui_finish "UI CAM: the CNC group"

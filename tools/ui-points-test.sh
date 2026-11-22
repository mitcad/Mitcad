#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files core/sketch/src/point_file.rs core/sketch/src/point_fit.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/points.mitcad-ui
# Insert Points (mitcad#2232) through the real UI, the shared input script
# tools/ui-scripts/points.mitcad-ui (also run by
# tools/ui-windows-points-test.ps1), on naca2412.dat written here from the
# NACA 4-digit formula (a Selig file of 121 points, 5 decimals): Insert
# Points on a new design finds the airfoil, inserts it on the XY plane as a
# fitted spline closed by a line across its blunt trailing edge, and
# Extrude makes a solid of the selected profile.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-points-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-points.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

# NACA 2412 (camber 0.02 at 0.4, thickness 0.12), 61 stations per surface
# spaced closer at both edges, from the upper trailing edge round the
# leading edge to the lower one.
LC_ALL=C awk '
function station(x, upper,   yt, yc, slope, theta) {
  yt = 5 * 0.12 * (0.2969 * sqrt(x) - 0.1260 * x - 0.3516 * x ^ 2 + 0.2843 * x ^ 3 - 0.1015 * x ^ 4)
  if (x < 0.4) {
    yc = 0.02 / 0.16 * (0.8 * x - x * x); slope = 0.04 / 0.16 * (0.4 - x)
  } else {
    yc = 0.02 / 0.36 * (0.2 + 0.8 * x - x * x); slope = 0.04 / 0.36 * (0.4 - x)
  }
  theta = atan2(slope, 1)
  if (upper) printf " %.5f  %.5f\n", x - yt * sin(theta), yc + yt * cos(theta)
  else printf " %.5f  %.5f\n", x + yt * sin(theta), yc - yt * cos(theta)
}
BEGIN {
  n = 61; pi = atan2(0, -1)
  print "NACA 2412"
  for (k = n - 1; k >= 0; k--) station(0.5 * (1 - cos(pi * k / (n - 1))), 1)
  for (k = 1; k < n; k++) station(0.5 * (1 - cos(pi * k / (n - 1))), 0)
}' > "$work/naca2412.dat"
[ "$(wc -l < "$work/naca2412.dat")" -eq 122 ] || { ui_fail "naca2412.dat: not 121 points"; exit 1; }
echo "ok   naca2412.dat written from the NACA 4-digit formula"

ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/points.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Insert Points driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Insert Points of an airfoil file and Extrude of its profile'
ui_finish 'UI Insert Points'

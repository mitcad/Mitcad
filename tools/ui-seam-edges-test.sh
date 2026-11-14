#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewEdges.cpp app/view/ViewEdges.hpp
# check-all sources: app/framework/TestDriver.cpp geometry/src/smooth_seams.cpp tools/cli
# check-all sources: tools/ui-scripts/seam-edges.mitcad-ui tools/ui-scripts/seam-edges.json
# check-all sources: tools/ui-scripts/seam-edges-reported.mitcad-ui
# Seam edges (mitcad#1773): the 3D view does not draw an edge that lies
# inside one smooth surface (a cylinder's or a ball's seam, the edge
# between two pieces of one face), and draws every other edge as before.
# Through the real UI, the input script tools/ui-scripts/seam-edges.mitcad-ui
# on a design that mitcad-cli makes (tools/ui-scripts/seam-edges.json): a
# box with a hole, a cylinder, a plate with rounded corners and a teardrop
# with a sharp tip, each body's drawn edges and left out seams counted.
# Then, when the private test files have it (MITCAD_MITCAD_REPORTED,
# seam-edges/; never committed), the reported lattice of balls and rods
# (tools/ui-scripts/seam-edges-reported.mitcad-ui).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-seam-edges-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-seam-edges.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=20

# run_script script design: the input script on the design, to its end.
run_script() {
  export MITCAD_TEST_INPUT=$1
  ui_start_app --ui=qt --no-mcp --open "$2"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'seam edges driver timed out'; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
}

"$cli" run "$here/ui-scripts/seam-edges.json" --save "$work/seam-edges.mitcad" || exit 1
ui_start_display

echo "--- A box with a hole, a cylinder, a rounded plate and a teardrop"
run_script "$here/ui-scripts/seam-edges.mitcad-ui" "$work/seam-edges.mitcad"
ui_expect_log 'body F1.b0: 14 of 15 edges drawn, smooth seams left out: 1' 'the hole'"'"'s seam left out, the box'"'"'s edges drawn'
ui_expect_log 'body F5.b0: 2 of 3 edges drawn, smooth seams left out: 1' 'the cylinder'"'"'s seam left out'
ui_expect_log 'body F6.b0: 24 of 24 edges drawn, smooth seams left out: 0' 'the rounded plate'"'"'s edges all drawn'
ui_expect_log 'body F9.b0: 3 of 3 edges drawn, smooth seams left out: 0' 'the teardrop'"'"'s crease drawn'
ui_expect_log 'TestDriver: done' 'the seams of the made design'

reported=$(find "${MITCAD_MITCAD_REPORTED:-/nonexistent}/seam-edges" -maxdepth 1 -name '*.mitcad' 2> /dev/null | sort | head -1)
if [ -n "$reported" ] && [ -f "$reported" ]; then
  echo "--- The reported lattice of balls and rods"
  run_script "$here/ui-scripts/seam-edges-reported.mitcad-ui" "$reported"
  ui_expect_log 'body F2.b0: ' 'the lattice'"'"'s seams left out'
  grep -E 'body F2.b0: ' "$UI_LOG" | tail -1
  ui_expect_log 'TestDriver: done' 'the seams of the reported design'
else
  echo "skip the reported design of the private test files (MITCAD_MITCAD_REPORTED/seam-edges)"
fi

ui_finish 'UI seam edges'

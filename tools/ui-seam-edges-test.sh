#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewEdges.cpp app/view/ViewEdges.hpp
# check-all sources: app/framework/TestDriver.cpp geometry/src/smooth_seams.cpp tools/cli
# check-all sources: tools/ui-scripts/seam-edges.mitcad-ui tools/ui-scripts/seam-edges.json
# check-all sources: tools/ui-scripts/seam-edges-reported.mitcad-ui
# check-all sources: tools/ui-scripts/seam-edges-sweep-reported.mitcad-ui
# check-all sources: tools/ui-scripts/seam-edges-mirror-reported.mitcad-ui
# Seam edges (mitcad#1773): the 3D view does not draw an edge that lies
# inside one smooth surface (a cylinder's or a ball's seam, the edge
# between two pieces of one face), and draws every other edge as before.
# Through the real UI, the input script tools/ui-scripts/seam-edges.mitcad-ui
# on a design that mitcad-cli makes (tools/ui-scripts/seam-edges.json): a
# box with a hole, a cylinder, a plate with rounded corners and a teardrop
# with a sharp tip, each body's drawn edges and left out seams counted.
# Pieces of one face on different analytic surfaces (mitcad#2148): circles
# swept along a line and a tangent arc, along a line, an arc and a line,
# along two arcs of one circle, and the first mirrored and joined; a ball
# and a torus whose faces are split into pieces.
# Then, when the private test files have them (MITCAD_MITCAD_REPORTED,
# never committed), the reported lattice of balls and rods (seam-edges/,
# tools/ui-scripts/seam-edges-reported.mitcad-ui) and the reported sweep
# and its mirror (github-515/, their archives and designs found by their
# content ids; tools/ui-scripts/seam-edges-sweep-reported.mitcad-ui and
# seam-edges-mirror-reported.mitcad-ui).
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

# content_id file: the first 8 hex digits of its SHA-256.
content_id() {
  sha256sum "$1" | cut -c1-8
}

# reported_design archive-id design-id dir: sets `design` to the design of
# that content id from the archive of that content id in
# MITCAD_MITCAD_REPORTED/github-515, unpacked into $work/dir; empty when
# the archive is not there.
reported_design() {
  local archive="" candidate
  design=""
  for candidate in "${MITCAD_MITCAD_REPORTED:-/nonexistent}"/github-515/*.zip; do
    [ -f "$candidate" ] && [ "$(content_id "$candidate")" = "$1" ] && { archive=$candidate; break; }
  done
  [ -n "$archive" ] || return 0
  mkdir -p "$work/$3"
  (cd "$work/$3" && cmake -E tar xf "$archive") || { ui_fail "the reported archive $1 was not unpacked"; exit 1; }
  while IFS= read -r candidate; do
    [ "$(content_id "$candidate")" = "$2" ] && { design=$candidate; return 0; }
  done < <(find "$work/$3" -name '*.mitcad' | sort)
  ui_fail "the reported archive $1 holds no design $2"
  exit 1
}

"$cli" run "$here/ui-scripts/seam-edges.json" --save "$work/seam-edges.mitcad" || exit 1
ui_start_display

echo "--- A box with a hole, a cylinder, a rounded plate and a teardrop"
run_script "$here/ui-scripts/seam-edges.mitcad-ui" "$work/seam-edges.mitcad"
ui_expect_log 'body F1.b0: 14 of 15 edges drawn, smooth seams left out: 1' 'the hole'"'"'s seam left out, the box'"'"'s edges drawn'
ui_expect_log 'body F5.b0: 2 of 3 edges drawn, smooth seams left out: 1' 'the cylinder'"'"'s seam left out'
ui_expect_log 'body F6.b0: 24 of 24 edges drawn, smooth seams left out: 0' 'the rounded plate'"'"'s edges all drawn'
ui_expect_log 'body F9.b0: 3 of 3 edges drawn, smooth seams left out: 0' 'the teardrop'"'"'s crease drawn'
ui_expect_log 'body F12.b0: 3 of 5 edges drawn, smooth seams left out: 2' \
  'the circle between the line-and-arc sweep'"'"'s cylinder and torus drawn, their seams left out'
ui_expect_log 'body F15.b0: 4 of 7 edges drawn, smooth seams left out: 3' \
  'both circles between the line, arc and line sweep'"'"'s pieces drawn'
ui_expect_log 'body F18.b0: 2 of 5 edges drawn, smooth seams left out: 3' \
  'no line between the pieces of the sweep along two arcs of one circle'
ui_expect_log 'body F21.b0: 4 of 7 edges drawn, smooth seams left out: 3' \
  'the circles between cylinder and torus drawn on the original and on its joined mirror image'
ui_expect_log 'body F23.b0: 2 of 4 edges drawn, smooth seams left out: 2' 'no line between the ball'"'"'s pieces'
ui_expect_log 'body F25.b0: 0 of 5 edges drawn, smooth seams left out: 5' 'no line between the torus'"'"'s pieces'
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

reported_design 840fde01 f0a91c0a reported-sweep
if [ -n "$design" ]; then
  echo "--- The reported sweep along a line and a tangent arc (GitHub issue 515)"
  run_script "$here/ui-scripts/seam-edges-sweep-reported.mitcad-ui" "$design"
  ui_expect_log 'body F3.b0: 3 of 5 edges drawn, smooth seams left out: 2' \
    'the circle between the reported sweep'"'"'s cylinder and torus drawn'
  ui_expect_log 'TestDriver: done' 'the edges of the reported sweep'
else
  echo "skip the reported sweep of the private test files (MITCAD_MITCAD_REPORTED/github-515)"
fi

reported_design 3e865721 fb1a36cb reported-mirror
if [ -n "$design" ]; then
  echo "--- The reported sweep mirrored and joined (GitHub issue 516)"
  run_script "$here/ui-scripts/seam-edges-mirror-reported.mitcad-ui" "$design"
  ui_expect_log 'body F3.b0: ' 'the reported mirror'"'"'s edges counted'
  grep -E 'body F3.b0: ' "$UI_LOG" | tail -1
  ui_expect_log 'TestDriver: done' 'the edges of the reported mirror'
else
  echo "skip the reported mirror of the private test files (MITCAD_MITCAD_REPORTED/github-515)"
fi

ui_finish 'UI seam edges'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/sketch/SketchSurface.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/extrude.rs core/model/src/features/extrude_scope.rs
# check-all sources: core/model/src/features/developed.rs core/model-api/src/sketch_surface.rs tools/cli
# check-all sources: tools/ui-scripts/extrude-cylinder-face.mitcad-ui tools/ui-scripts/extrude-cylinder-face.json
# check-all sources: tools/ui-scripts/extrude-cylinder-face-reported.mitcad-ui
# Extrude of a sketch on a cylinder's side with the automatic operation
# (mitcad#1908, GitHub mitcad/Mitcad#362) through the real UI, the shared
# input script tools/ui-scripts/extrude-cylinder-face.mitcad-ui (also run by
# tools/ui-windows-extrude-cylinder-face-test.ps1 and
# tools/ui-macos-extrude-cylinder-face-test.sh) on a design that mitcad-cli
# makes as reported (tools/ui-scripts/extrude-cylinder-face.json: a
# cylinder of radius 150 in a component, a rectangle and a circle sketched
# on its side): the picked profiles are highlighted on the face, the
# distance arrow starts there, the automatic Join builds and OK adds it.
# Then, when the private test files have it (MITCAD_MITCAD_REPORTED,
# github-362/; never committed), the same on the reported design
# (tools/ui-scripts/extrude-cylinder-face-reported.mitcad-ui), its archive
# and design found by their content ids.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-cylinder-face-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-cylinder-face.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100 MITCAD_LOG_PICKS=1

# run_script script design: the input script on the design, to its end.
run_script() {
  export MITCAD_TEST_INPUT=$1
  ui_start_app --ui=qt --no-mcp --open "$2"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'extrude on a cylinder driver timed out'; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
}

# content_id file: the first 8 hex digits of its SHA-256.
content_id() {
  sha256sum "$1" | cut -c1-8
}

"$cli" run "$here/ui-scripts/extrude-cylinder-face.json" --save "$work/cylinder.mitcad" || exit 1
ui_start_display

echo "--- A rectangle and a circle on a cylinder's side, extruded with the automatic Join"
run_script "$here/ui-scripts/extrude-cylinder-face.mitcad-ui" "$work/cylinder.mitcad"
ui_expect_log 'profile F3/r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]} in O1 at 13' \
  "the rectangle's highlight on the cylinder's side"
ui_expect_log 'Extrude: automatic Operation = Join' 'the New Body preview turned Join'
ui_expect_log 'Added extrude (Join)' 'the Join of the profiles on the face added'
ui_expect_log 'TestDriver: done' 'Extrude of a sketch on a cylinder with the automatic Join'

archive=""
for candidate in "${MITCAD_MITCAD_REPORTED:-/nonexistent}"/github-362/*.zip; do
  [ -f "$candidate" ] && [ "$(content_id "$candidate")" = 6ce359a8 ] && { archive=$candidate; break; }
done
if [ -n "$archive" ]; then
  echo "--- The reported design"
  mkdir -p "$work/reported"
  (cd "$work/reported" && cmake -E tar xf "$archive") || { ui_fail 'the reported archive was not unpacked'; exit 1; }
  reported=""
  while IFS= read -r candidate; do
    [ "$(content_id "$candidate")" = 21225c72 ] && { reported=$candidate; break; }
  done < <(find "$work/reported" -name '*.mitcad' | sort)
  [ -n "$reported" ] || { ui_fail 'the reported archive holds no design 21225c72'; exit 1; }
  run_script "$here/ui-scripts/extrude-cylinder-face-reported.mitcad-ui" "$reported"
  ui_expect_log 'Added extrude (Join)' 'the Join on the reported design added'
  ui_expect_log 'TestDriver: done' 'Extrude of the reported sketch on a cylinder'
else
  echo "skip the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-362)"
fi

ui_finish 'UI extrude of a sketch on a cylindrical face'

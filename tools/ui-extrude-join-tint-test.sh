#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/CommandSession.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-join-tint.mitcad-ui tools/ui-scripts/extrude-join-tint.json
# An Extrude Join's target body in the command's look (mitcad#2069, GitHub
# mitcad/Mitcad#474) through the real UI, the shared input script
# tools/ui-scripts/extrude-join-tint.mitcad-ui (also run by
# app.ui-windows-extrude-join-tint and app.ui-macos-extrude-join-tint) on a
# design that mitcad-cli makes (tools/ui-scripts/extrude-join-tint.json): a
# block and a square on its top face. With Selection Through Bodies on, the
# Join's target is tinted faintly, nearer its own color than the cut's red,
# with no copy through the bodies, and the added material reads blue.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-join-tint-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-join-tint.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/extrude-join-tint.mitcad-ui
"$cli" run "$here/ui-scripts/extrude-join-tint.json" --save "$work/tint.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/tint.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'extrude join tint driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
# (ui_expect_log matches fixed text: these two are patterns.)
for want in 'off [0-9]+ from .* and [0-9]+ from |the target body nearer its own color than the red' \
  'blue [0-9]+ over red |the added material blue'; do
  grep -Eq "^TestDriver: view point pixel .*, ${want%%|*}" "$UI_LOG" &&
    echo "ok   ${want#*|}" || ui_fail "${want#*|}: no view point pixel line '${want%%|*}'"
done
ui_expect_log 'TestDriver: done' 'the Join target tint'
ui_finish 'UI extrude join tint'

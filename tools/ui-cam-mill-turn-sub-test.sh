#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn-sub.mitcad-ui tools/ui-scripts/cam-mill-turn-sub.json
# CAM mill-turn: live tools on the sub-spindle, the B head's and the
# sub-spindle's codes in Machines and Tools…, the sub-spindle's travel in
# the lathe check (mitcad#2375, app/COMMANDS.md "Live tools on a lathe"),
# the shared input script tools/ui-scripts/cam-mill-turn-sub.mitcad-ui
# (also run by tools/ui-windows-cam-mill-turn-sub-test.ps1 and
# tools/ui-macos-cam-mill-turn-sub-test.sh) on a block turned on a lathe
# with every axis (tools/ui-scripts/cam-mill-turn-sub.json): Edit Machine…
# with the B head's clamp, the sub-spindle's speed, travel, chuck, axis,
# stop and C axis; a lathe setup handing over to the sub-spindle; the back
# faced, drilled from the Drill panel and milled with the Y axis on the
# sub-spindle; Generate; Simulate; Post Process… to the Fanuc turning
# dialect: the sub-spindle stopped with its own code, its own C axis on,
# clamped and off, G19 for the flat, its axis E for the hand-over; the
# lathe check's finding when it cannot reach the part. The sub-spindle's
# body, a zone moving with it and a control that mirrors only Z on it, and
# an operation on the main spindle after the sub-spindle's (mitcad#2392).
# The sub-spindle's profile and the turret's body in Edit Machine…, the
# turret running into the sub-spindle's chuck, Simulate drawing the
# sub-spindle where it waits and at the grip (mitcad#2405).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-mill-turn-sub-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-mill-turn-sub.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-mill-turn-sub.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-mill-turn-sub.json" --save "$WORK/cam-mill-turn-sub.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-mill-turn-sub.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM mill-turn sub-spindle driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the sub-spindle's codes, live tools on the sub-spindle, generate, simulate, post and the lathe check"

# The Fanuc program with live tools on the sub-spindle.
FILE=$WORK/setup1.nc
[ -s "$FILE" ] || ui_fail "no program written"
[ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark: $(head -2 "$FILE")"
for line in G55 M206 M264 'G28 H0.' M114 G19 G18 M255 G99 '(DRILL 1)' '(FACING 1)' '(TURN FACING 2)'; do
  grep -qxF "$line" "$FILE" || ui_fail "no '$line' in the program"
done
grep -qxF M154 "$FILE" && ui_fail "the main spindle's C axis in the program"
grep -qE '^G1 E-?[0-9.]+ F[0-9.]+$' "$FILE" || ui_fail "the sub-spindle does not come up to the part on its axis E"
# The sub-spindle stops before its C axis goes on for the hole.
drill=$(grep -nxF '(DRILL 1)' "$FILE" | head -1 | cut -d: -f1)
stop=$(awk -v from="$drill" 'NR > from && $0 == "M206" { print NR; exit }' "$FILE")
on=$(awk -v from="$drill" 'NR > from && $0 == "M264" { print NR; exit }' "$FILE")
[ -n "$stop" ] && [ -n "$on" ] && [ "$stop" -lt "$on" ] || ui_fail "the sub-spindle does not stop before its C axis goes on"
[ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
echo "ok   the Fanuc program with live tools on the sub-spindle: M206, M264, M114, G19, M255, E"
ui_finish "UI CAM mill-turn sub-spindle: its codes, its live tools and its travel"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Real model warnings/failures, compact history and native Browser badges;
# one level per surface (mitcad#1040); the message band above the timeline,
# its stub and its history (mitcad#1098).
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 - "$here/cli/tests/p9_warnings.json" "$MITCAD_TEST_WORK/model.json" <<'PY'
import json
import sys

# The rib and its full round (F3), built 0.1 % smaller: information, not
# a warning (mitcad#1106).
commands = []
for entry in json.load(open(sys.argv[1])):
    if "cmd" in entry:
        commands.append(entry)
    if entry.get("cmd") == "add_feature" and entry["def"]["type"] == "fillet":
        break
commands.extend([
    {"expect": {"feature": "Fillet1", "status": "ok"}},
    {"cmd": "add_feature", "name": "Missing edge", "def": {
        "type": "fillet", "body": "F2.b0", "edges": ["E{F2:side0|F2:side1}"], "radius": 100}},
    {"expect": {"feature": "Missing edge", "status": "error"}},
    # A plane at 90 radians warns, and has a Browser row (mitcad#1040).
    {"cmd": "add_feature", "name": "Tilted", "def": {"type": "construction_plane",
        "definition": {"type": "angle", "line": "z", "angle": 90, "plane": "xz"}}},
    {"expect": {"feature": "Tilted", "status": "warning"}},
    # The rib turned by 90 radians warns too: two warnings.
    {"cmd": "add_feature", "name": "Turned", "def": {"type": "move", "bodies": ["F2.b0"],
        "transform": {"type": "rotate", "axis": "z", "angle": 90}}},
    {"expect": {"feature": "Turned", "status": "warning"}},
])
for distance in range(12):
    commands.append({"cmd": "add_feature", "def": {"type": "construction_plane",
        "definition": {"type": "offset", "plane": "xy", "distance": distance + 10}}})
with open(sys.argv[2], "w") as output:
    json.dump(commands, output)
PY
"$2" run "$MITCAD_TEST_WORK/model.json" --save "$MITCAD_TEST_WORK/issues.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-issues.mitcad-ui \
  ui_run_app issues --ui=native --no-recovery --open "$MITCAD_TEST_WORK/issues.mitcad"
ui_expect_log 'TestDriver: done' 'native issue workflow reached its end'
ui_expect_no_log 'TestDriver: FAIL|Native Mac canvas:|OCCT view event failed' 'native issues retain input and rendering'
# The message band showing the error in full: a card of its own size right
# above the timeline, as wide as it (mitcad#1098).
LINE=$(grep -E 'Native message band placed: full ' "$UI_LOG_FILE" | head -n 1 || true)
read -r BX BY BW BH TX TY TW <<< "$(sed -E \
  's/.*: full ([0-9-]+),([0-9-]+) ([0-9]+)x([0-9]+), row [0-9]+, timeline ([0-9-]+),([0-9-]+) ([0-9]+)x[0-9]+$/\1 \2 \3 \4 \5 \6 \7/' \
  <<< "$LINE")"
if [ -n "${TW:-}" ] && [ "$BH" -gt 0 ] && [ "$BW" -gt 0 ] && [ "$BX" = "$TX" ] && [ "$BW" = "$TW" ] &&
  [ $((BY + BH)) -le "$TY" ]; then
  ui_pass "the message band is placed right above the timeline, as wide as it: $LINE"
else
  ui_fail "the message band's place: '$LINE'"
fi
# The navigation bar (mitcad#1099) just above the band's row, as the
# Floating layout places it: placed in the same layout pass, after the band.
NAV=$(grep -E -A3 'Native message band placed: full ' "$UI_LOG_FILE" | grep -m 1 -E 'Native navigation bar shown at' || true)
read -r NY NH <<< "$(sed -E 's/.*shown at [0-9-]+,([0-9-]+) [0-9]+x([0-9]+)$/\1 \2/' <<< "$NAV")"
if [ -n "${NH:-}" ] && [ -n "${BY:-}" ] && [ $((NY + NH)) -le "$BY" ]; then
  ui_pass "the navigation bar is above the message band's row: $NAV"
else
  ui_fail "the navigation bar's place above the band: '$NAV' (band: '$LINE')"
fi
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: compact native history, issue colours, messages, levels per surface, the message band and Browser navigation"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget app/MainWindow.cpp app/MainWindowNative.cpp app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeTestInput.mm app/framework/TestDriver tools/cli
# The AppKit timeline's hover, tooltips and groups (mitcad#1076,
# mitcad#1077), on the design of the Qt timeline inspection
# (tools/ui-timeline-inspection-test.py): hovering a cell previews its
# feature through the solids and a Fillet's picked edges, its tooltip is
# the shared feature details; the group's band folds it into one cell, a
# double-click opens it, and the shared group menu folds, renames and
# ungroups it (tools/ui-scripts/native-macos-timeline.mitcad-ui).
# Usage: tools/ui-macos-native-timeline-test.sh <app> <mitcad-cli> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# Inner (F1) and Inner detail (F2) wholly inside Outer (F3), grouped as
# Inner details; Failed round (F4) fails, Fillet1 (F5) rounds Outer's four
# vertical edges, Suppressed detail (F6) is suppressed and Later plane (F7)
# rolled back.
cat > "$MITCAD_TEST_WORK/timeline.json" << 'EOF'
[
  {"cmd":"add_feature","name":"Inner","def":{"type":"box","plane":"xy","corner":[-5,-5],
    "length":10,"width":10,"height":10,"symmetric":true,"operation":"new_body"}},
  {"cmd":"add_feature","name":"Inner detail","def":{"type":"box","plane":"xy","corner":[-3,-3],
    "length":6,"width":6,"height":6,"symmetric":true,"operation":"new_body"}},
  {"cmd":"add_feature","name":"Outer","def":{"type":"box","plane":"xy","corner":[-20,-20],
    "length":40,"width":40,"height":40,"symmetric":true,"operation":"new_body"}},
  {"cmd":"add_feature","name":"Failed round","def":{"type":"fillet","body":"F3.b0",
    "edges":["E{F3:side0|F3:side1}"],"radius":100}},
  {"cmd":"add_feature","name":"Fillet1","def":{"type":"fillet","body":"F3.b0",
    "edges":["E{F3:side0|F3:side1}","E{F3:side1|F3:side2}","E{F3:side2|F3:side3}","E{F3:side0|F3:side3}"],
    "radius":2}},
  {"cmd":"add_feature","name":"Suppressed detail","def":{"type":"box","plane":"xy","corner":[0,0],
    "length":8,"width":8,"height":8,"operation":"new_body"}},
  {"cmd":"suppress_feature","uid":"F6","suppressed":true},
  {"cmd":"add_feature","name":"Later plane","def":{"type":"construction_plane",
    "definition":{"type":"offset","plane":"xy","distance":50}}},
  {"cmd":"group_features","features":["F1","F2"],"name":"Inner details"},
  {"cmd":"set_marker","position":6}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/timeline.json" --save "$MITCAD_TEST_WORK/timeline.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-timeline.mitcad-ui \
  ui_run_app native-timeline --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/timeline.mitcad"
ui_expect_log '^TestDriver: done' 'the AppKit timeline: hover, tooltips and groups'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
cat > "$MITCAD_TEST_WORK/filtered-group.json" << 'EOF'
[
  {"cmd":"add_feature","name":"Root first","def":{"type":"box","plane":"xy","corner":[0,0],
    "length":10,"width":10,"height":10,"operation":"new_body"}},
  {"cmd":"create_component","name":"Child"},
  {"cmd":"add_feature","name":"Child middle","def":{"type":"box","plane":"xy","corner":[20,0],
    "length":10,"width":10,"height":10,"operation":"new_body"}},
  {"cmd":"activate_component","component":"C0"},
  {"cmd":"add_feature","name":"Root last","def":{"type":"box","plane":"xy","corner":[40,0],
    "length":10,"width":10,"height":10,"operation":"new_body"}},
  {"cmd":"group_features","features":["F1","F2","F3"],"name":"G"}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/filtered-group.json" --save "$MITCAD_TEST_WORK/filtered-group.mitcad" \
  > "$UI_OUT/filtered-group-setup.log" 2>&1 || { cat "$UI_OUT/filtered-group-setup.log" >&2; exit 1; }
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-filtered-group.mitcad-ui \
  ui_run_app filtered-group --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/filtered-group.mitcad"
ui_expect_log '^TestDriver: done' 'the folded group survives a filtered middle and head'
if grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG_FILE"; then
  ui_fail 'the filtered group driver reported a failed assertion'
else
  ui_pass 'the filtered group assertions passed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: the AppKit timeline hover, tooltips and groups'

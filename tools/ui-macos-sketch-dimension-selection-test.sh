#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/commands/EqualSpacingCommands.cpp
# Select first, press D, place once: real viewport picks and value entry.
# Usage: bash tools/ui-macos-sketch-dimension-selection-test.sh <app> <cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-dimension-selection}"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
cat > "$MITCAD_TEST_WORK/setup.json" <<'EOF'
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_line","sketch":"F1","start":[10,10],"end":[40,10]},
  {"cmd":"sketch.add_line","sketch":"F1","start":[10,30],"end":[40,30]},
  {"cmd":"sketch.add_line","sketch":"F1","start":[50,10],"end":[70,30]},
  {"cmd":"sketch.circle","sketch":"F1","mode":"center","center":[30,60],"radius":8},
  {"cmd":"sketch.arc","sketch":"F1","mode":"center","center":[60,60],"start":[70,60],"end":[60,70]},
  {"cmd":"sketch.add_point","sketch":"F1","at":[10,85]},
  {"cmd":"sketch.add_point","sketch":"F1","at":[30,85]},
  {"cmd":"sketch.add_point","sketch":"F1","at":[50,85]}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_WORK/selection.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-dimension-selection.mitcad-ui \
  ui_run_app selection --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/selection.mitcad"
ui_expect_log 'TestDriver: done' 'preselected sketch dimensions completed'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi

# Body selections take the same projection path as picks made after D.
cat > "$MITCAD_TEST_WORK/body-setup.json" <<'EOF'
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[0,0],"width":60,"height":40},
  {"cmd":"add_feature","def":{"type":"extrude",
    "profiles":[{"sketch":"F1","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent":{"type":"distance","distance":20},"operation":"new_body"}},
  {"cmd":"sketch.create","plane":{"face":"F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})","body":"F2.b0"}},
  {"cmd":"sketch.add_point","sketch":"F3","at":[20,10]}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/body-setup.json" --save "$MITCAD_TEST_WORK/body-selection.mitcad" \
  > "$UI_OUT/body-setup.log" 2>&1 || { cat "$UI_OUT/body-setup.log" >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-dimension-body-selection.mitcad-ui \
  ui_run_app body-selection --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/body-selection.mitcad"
ui_expect_log 'TestDriver: done' 'body preselection, projection and grouped undo completed'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'body preselection assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi

# The same body definition in another occurrence must not be mistaken for
# the local edge that plane_geometry exposes once for the sketch component.
python3 - "$MITCAD_TEST_WORK/body-setup.json" "$MITCAD_TEST_WORK/occurrence-setup.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    commands = json.load(source)
commands.insert(0, {'cmd': 'create_component', 'name': 'Repeated', 'activate': True})
commands.extend([
    {'cmd': 'activate_component', 'component': 'C0'},
    {'cmd': 'copy_occurrence', 'occurrence': 'O1', 'transform': {'translation': [100, 0, 0]}},
    {'cmd': 'activate_component', 'component': 'C1'},
])
with open(sys.argv[2], 'w', encoding='utf-8') as target:
    json.dump(commands, target)
PY
if [ $? -ne 0 ]; then exit 1; fi
"$cli" run "$MITCAD_TEST_WORK/occurrence-setup.json" --save "$MITCAD_TEST_WORK/occurrence-selection.mitcad" \
  > "$UI_OUT/occurrence-setup.log" 2>&1 || { cat "$UI_OUT/occurrence-setup.log" >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-dimension-occurrence-selection.mitcad-ui \
  ui_run_app occurrence-selection --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/occurrence-selection.mitcad"
ui_expect_log 'TestDriver: done' 'own occurrence accepted and other occurrence rejected'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'occurrence preselection assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: sketch dimension selection; evidence in $UI_OUT"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterPicker app/framework/CommandSessionPicks.cpp
# check-all sources: app/MainWindowParameterPicks.cpp app/view/ViewController.cpp app/OcctViewer
# Parameter labels follow menu-driven camera changes and pixel-scroll events
# without giving the distance field's keyboard focus to the viewport (#1179).
# Synthetic scroll coverage is separate from physical-trackpad acceptance.
# Usage: bash tools/ui-macos-dimension-names-test.sh <app> <cli> <out> qt|native
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ]; then
  echo "usage: $0 <app> <cli> <out> qt|native" >&2
  exit 2
fi
case "$4" in qt|native) frontend=$4 ;; *) echo "expected qt or native" >&2; exit 2 ;; esac
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=150
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
cat > "$MITCAD_TEST_WORK/setup.json" <<'JSON'
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[0,0],"width":40,"height":25},
  {"cmd":"sketch.set_dimension_text","sketch":"F1","dimension":"k5","text":[20,-10]},
  {"cmd":"sketch.set_dimension_text","sketch":"F1","dimension":"k6","text":[50,12]},
  {"cmd":"sketch.add_dimension","sketch":"F1","dimension":{"type":"length","line":"c4"},"driven":true,"text":[-10,12]},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F1","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],"extent":{"type":"distance","distance":10},"operation":"new_body"}},
  {"cmd":"set_feature_visible","uid":"F1","visible":true}
]
JSON
"$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_WORK/names.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
# Enter through each frontend's real history and value field. Typing the
# unchanged distance is the native presenter's normal parameter-label path.
# Qt menu actions retain the field; AppKit menu actions end its edit, so
# the native scenario verifies dismissal then begins a fresh field edit.
if [ "$frontend" = native ]; then
  # This process's INI only: cover Shift orbit/Option zoom independently of
  # the fresh Qt default (Option orbit/Shift zoom), preserving user settings.
  mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
  printf '[view]\nnavigationScheme=trackpad-shift\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
  scenario=$here/ui-scripts/macos-native-dimension-names.mitcad-ui
  cat > "$MITCAD_TEST_WORK/entry.mitcad-ui" <<'NATIVE'
expect-setting view/navigationScheme trackpad-shift
native-activate native.history.F2
expect-log 'Editing F2 with Extrude'
wait-idle 30
mark
native-pointer-click input_distance
native-type input_distance '10 mm'
expect-native-focus input_distance '^10 mm$'
NATIVE
else
  scenario=$here/ui-scripts/macos-dimension-names.mitcad-ui
  cat > "$MITCAD_TEST_WORK/entry.mitcad-ui" <<'QT'
dblclick-logged '^Timeline Extrude1 at '
expect-log 'Editing F2 with Extrude'
wait-idle 30
mark
click-logged '^Panel Extrude input distance at '
QT
fi
cat "$MITCAD_TEST_WORK/entry.mitcad-ui" "$scenario" \
  > "$MITCAD_TEST_WORK/check.mitcad-ui"
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/check.mitcad-ui \
  ui_run_app "$frontend" --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/names.mitcad"
ui_expect_log 'TestDriver: quit \([0-9]+ of [0-9]+ steps\)' "$frontend: camera/parameter-label workflow completed"
for problem in 'TestDriver: FAIL' 'OCCT view event failed'; do
  ui_expect_no_log "$problem" "$frontend: no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
# Check ordinary paint logs before each actual WindowServer capture. The
# later -reference-composite images manually paint the overlays, so they
# provide geometry context only and never stand in for those paint logs.
python3 - "$UI_LOG_FILE" "$frontend" <<'PYTHON'
import re, sys
from pathlib import Path
lines = Path(sys.argv[1]).read_text().splitlines()
computed, drawn, snapshots = {}, {}, {}
labels = {"Sketch1 k5 d1", "Sketch1 k6 d2", "Sketch1 k7 d3"}
for line in lines:
    match = re.match(r"Parameter label (Sketch1 k[567] d[123]) (drawn )?at (-?\d+,-?\d+)$", line)
    if match:
        (drawn if match[2] else computed)[match[1]] = match[3]
    capture = re.match(r"TestDriver: line \d+: screenshot-screen '(.*)/([^/]+)[.]png'$", line)
    if capture:
        stage = capture[2]
        if stage.startswith("cancelled") or stage.endswith("-blurred"):
            continue  # Original screen evidence that dismissal removes the overlay.
        if labels - computed.keys() or labels - drawn.keys():
            raise SystemExit(f"FAIL {stage}: all three parameter labels must have been computed and painted")
        for label in sorted(labels):
            if computed[label] != drawn[label]:
                raise SystemExit(f"FAIL {stage}: {label} painted at {drawn[label]}, camera puts it at {computed[label]}")
        snapshots[stage] = dict(computed)
        print(f"ok   {stage}: ordinary paints match all three current camera projections")
expected = {"initial", "right", "front", "right-again", "front-again", "orbit", "pan", "zoom"}
if snapshots.keys() != expected:
    raise SystemExit(f"FAIL: missing stages {expected - snapshots.keys()}")
width = "Sketch1 k5 d1"
orbit_start = "initial" if sys.argv[2] == "native" else "front-again"
for before, after in [("initial", "right"), ("right", "front"), (orbit_start, "orbit"), ("orbit", "pan"), ("pan", "zoom")]:
    if snapshots[before][width] == snapshots[after][width]:
        raise SystemExit(f"FAIL: width label did not move between {before} and {after}")
for first, repeated in [("right", "right-again"), ("front", "front-again")]:
    if snapshots[first] != snapshots[repeated]:
        raise SystemExit(f"FAIL: {repeated} did not return the labels to their original {first} positions")
print("PASS: menu views, synthetic orbit/pan/zoom and repeated views; physical trackpad remains a separate check")
PYTHON
if [ "$?" -ne 0 ]; then exit 1; fi
echo "PASS: $frontend parameter labels follow the camera; original screen captures in $MITCAD_TEST_WORK"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowForm.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/view/FormCageOverlay.cpp
# check-all sources: core/model/src/document/kept_cages.rs core/model/src/features/cage.rs
# check-all sources: core/model/src/features/base.rs core/model-api/src/form.rs core/subdiv tools/cli
# check-all sources: tools/ui-scripts/form-kept-cage.mitcad-ui
# Open Cage as Form (mitcad#1757, mitcad#1810) on macOS, as
# tools/ui-form-kept-cage-test.sh on Linux: the shared steps
# tools/ui-scripts/form-kept-cage.mitcad-ui in the Qt frontend on a design
# made here: mitcad-cli writes a 40 mm cube's B-rep data and imports it as
# a base feature whose body is CageBody, and the saved design's body is
# given a kept control cage, a uniform cube net of 8 vertices and 6 faces
# (written in before the body's B-rep data with awk: no JSON tool is
# assumed). The body's Browser row opens the cage as Form1 in Edit Form; a
# face of it is moved, one undo step whose body is a solid; Close keeps it
# and the imported body stays.
# Usage: tools/ui-macos-form-kept-cage-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-form-kept-cage-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
work=$MITCAD_TEST_WORK
rm -rf "$work"
mkdir -p "$work"
# The cube's B-rep data imported as a base feature (F2: the box F1 that
# made the data is deleted), its body named CageBody.
cat > "$work/model.json" << JSON
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -20],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "export", "path": "$work/cube.brep", "format": "brep"},
  {"cmd": "delete_feature", "uid": "F1"},
  {"cmd": "import_file", "path": "$work/cube.brep", "name": "Imported Form", "wrap": false},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "CageBody"}
]
JSON
"$cli" run "$work/model.json" --save "$work/imported.mitcad" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli' >&2; exit 1; }
# Its kept cage: the cube's net, uniform (degree 3, weight 1, no secondary
# points), its faces in no particular turn (the model turns them). The
# design's only body is the base feature's; its key "brep" is found once.
cage='{"source": "synthetic cube", "degree": 3, "vertices": [{"position": [-20, -20, 0]}, {"position": [20, -20, 0]}, {"position": [20, 20, 0]}, {"position": [-20, 20, 0]}, {"position": [-20, -20, 40]}, {"position": [20, -20, 40]}, {"position": [20, 20, 40]}, {"position": [-20, 20, 40]}], "faces": [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]]}'
grep -Eq '"type": *"base"' "$work/imported.mitcad" ||
  { echo 'FAIL: the design has no base feature to keep the cage' >&2; exit 1; }
awk -v cage="$cage" '!done && index($0, "\"brep\":") {
    i = index($0, "\"brep\":")
    $0 = substr($0, 1, i - 1) "\"cage\": " cage ", " substr($0, i)
    done = 1
  }
  { print }
  END { if (!done) exit 1 }' "$work/imported.mitcad" > "$work/kept-cage.mitcad" ||
  { echo 'FAIL: the kept cage not written' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/form-kept-cage.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app form-kept-cage --ui=qt --no-recovery --no-mcp --open "$work/kept-cage.mitcad"
ui_expect_log '^Opened the cage of F2[.]b0 as ' 'the kept cage opened as a Form'
ui_expect_log 'Form edit Form1: cage of 8 vertices, 12 edges, 6 faces, closed' 'Edit Form on the kept cage'
ui_expect_log 'Form edit Move in Form1: done' 'a face of the opened cage moved, one undo step'
ui_expect_log 'Closed Edit Form after 1' 'Close kept the edit'
ui_expect_log '^TestDriver: done' "an imported body's kept cage opened and edited as a Form"
ui_expect_no_log 'TestDriver: FAIL' 'no failed Open Cage as Form assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Open Cage as Form'

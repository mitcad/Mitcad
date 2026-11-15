#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowForm.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/view/FormCageOverlay.cpp
# check-all sources: core/model/src/document/kept_cages.rs core/model/src/features/cage.rs
# check-all sources: core/model/src/features/base.rs core/model-api/src/form.rs core/subdiv tools/cli
# check-all sources: tools/ui-scripts/form-kept-cage.mitcad-ui
# Open Cage as Form (mitcad#1757, mitcad#1810) through the real UI, the
# input script tools/ui-scripts/form-kept-cage.mitcad-ui, on a design made
# here: mitcad-cli writes a 40 mm cube's B-rep data and imports it as a
# base feature whose body is CageBody (as an import's body is), and the
# saved design's body is given a kept control cage, a uniform cube net of
# 8 vertices and 6 faces (commands.md, "Control cages of imported Form
# bodies"), as an imported Form body carries one. The body's Browser row
# opens the cage as Form1 in Edit Form; a face of it is moved, one undo
# step whose body is a solid; Close keeps it and the imported body stays.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-form-kept-cage-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-form-kept-cage.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/form-kept-cage.mitcad-ui

# The cube's B-rep data imported as a base feature (F2: the box F1 that
# made the data is deleted), its body named CageBody.
cat > "$work/model.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -20],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "export", "path": "$work/cube.brep", "format": "brep"},
  {"cmd": "delete_feature", "uid": "F1"},
  {"cmd": "import_file", "path": "$work/cube.brep", "name": "Imported Form", "wrap": false},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "CageBody"}
]
EOF
"$CLI" run "$work/model.json" --save "$work/imported.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli"; exit 1; }
# Its kept cage: the cube's net, uniform (degree 3, weight 1, no secondary
# points), its faces in no particular turn (the model turns them).
cage='{"source": "synthetic cube", "degree": 3,
  "vertices": [{"position": [-20, -20, 0]}, {"position": [20, -20, 0]}, {"position": [20, 20, 0]},
               {"position": [-20, 20, 0]}, {"position": [-20, -20, 40]}, {"position": [20, -20, 40]},
               {"position": [20, 20, 40]}, {"position": [-20, 20, 40]}],
  "faces": [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]]}'
jq --argjson cage "$cage" '(.features[] | select(.type == "base") | .bodies[0].cage) |= $cage' \
  "$work/imported.mitcad" > "$work/kept-cage.mitcad" || { ui_fail "the kept cage not written"; exit 1; }
[ "$(jq '[.features[] | select(.type == "base") | .bodies[0].cage.faces | length] | .[0]' "$work/kept-cage.mitcad")" = 6 ] ||
  { ui_fail "the design has no base feature to keep the cage"; exit 1; }

ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/kept-cage.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Open Cage as Form driver timed out'; exit 1; }
wait "$UI_RUNNER" || ui_fail 'the Open Cage as Form steps failed'
grep -E '^Opened the cage of |^Open Cage as Form of |^Form edit (Move in|Form1: body) ' "$UI_LOG" | sed 's/^/     /'
ui_expect_log 'Opened the cage of F2.b0 as ' 'the kept cage opened as a Form'
ui_expect_log 'Form edit Form1: cage of 8 vertices, 12 edges, 6 faces, closed' 'Edit Form on the kept cage'
ui_expect_log 'Form edit Move in Form1: done' 'a face of the opened cage moved, one undo step'
ui_expect_log 'Closed Edit Form after 1' 'Close kept the edit'
ui_expect_log 'TestDriver: done' 'an imported body'"'"'s kept cage opened and edited as a Form'
ui_finish 'UI Open Cage as Form'

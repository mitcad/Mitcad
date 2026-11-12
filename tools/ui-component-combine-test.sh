#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/browser app/framework core/model/src/features/combine.rs
# check-all sources: app/OcctViewer.cpp core/model/src/features/moves.rs
# check-all sources: core/model/src/features/point_pattern.rs
# check-all sources: core/model/src/features/extrude.rs core/model/src/features/reference.rs
# Combine with a tool of another component, through the real UI
# (mitcad#104, mitcad#105, mitcad#544): a block in the root, a block of
# Part:1 beside it and Part:2 on its other side, both overlapping it.
#   - Combine with the root's block as the target and Part's block, picked
#     where Part:1 shows it, as the tool: the tool comes in as a body in
#     its occurrence and joins the target. Consumed, it leaves Part: the
#     preview draws it where Part:2 shows it as a tool overlay, and the
#     panel says it is also gone from Part:2 (mitcad#561).
#   - Edited to a cut: the panel takes the tool back where it was picked.
#   - Edited again, Keep Tools still off, Part's block picked on its tool
#     overlay where Part:2 shows it (mitcad#561): the same body in two
#     occurrences is two tools, listed "Part:1, Part:2", and the target
#     has both cuts.
#   - The project file keeps each tool with its occurrence.
#   - Move/Copy Point to Point between components (mitcad#699), on a second
#     design: Part:1 10 mm and Part:2 50 mm along X, nothing in the root.
#     Part:2 moved from a vertex of its block to the same vertex where
#     Part:1 shows it: the preview puts Part:2 on Part:1, the file keeps
#     each point's occurrence, an edit loads them back.
#   - Extrude To Object to a face of another component (mitcad#762), on a
#     third design: crank:1 active, its rectangle extruded up to the top
#     face of frame:1's block, picked where frame:1 shows it: the
#     extrusion builds with frame inactive, the file keeps the face's link,
#     and an edit shows the face where it was picked and keeps the link.
#     With crank placed twice and the link seeing the profiles in crank:2,
#     an edit keeps that target (the session puts profiles in crank:1).
#   - Extrude To Object to a construction plane of frame, crank active
#     (mitcad#772): refused in the panel with the reason, nothing sent to
#     the model.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-component-combine-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-component-combine.XXXXXX)
FILE=$WORK/overlap.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_file expression expected description: a Python expression on the
# saved project file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# Part:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: F2.b0.
# Part:2 (O2) 30 along X. The root, active: Sketch2 (F3) 20 x 20 at
# x = 20, Extrude2 (F4) 15 up: F4.b0, overlapping Part:1's block by
# 10 x 20 x 10 mm (x 20 to 30) and Part:2's as much (x 30 to 40).
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "Part:1", "transform": {"translation": [30, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [20, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the blocks shown"
ui_expect_log "Body F2.b0 in O2 at " "Part:2's block shown"

echo "--- Combine with Part's block as the tool"
ui_mark
ui_step "combine (search)"               ui_command "Combine"
ui_step "pick the root's block"          ui_click_logged "Body F4.b0"
ui_expect_new "Combine Target Body: 1 body [body F4.b0]" "the target picked"
ui_step "pick Part's block"              ui_click_logged "Body F2.b0 in O1"
ui_expect_new "Combine Tool Bodies: 1 body [body F2.b0 in O1]" "the tool picked where Part:1 shows it"
ui_expect_new "Combine Tool Bodies listed: Part:1" "the tool listed by its occurrence"
ui_expect_new "Preview Combine: ok" "previewed"
ui_expect_new "F2.b0 in O2 at 30,0,0 tool" "Part's block drawn as a tool where Part:2 shows it"
ui_expect_new "leaves Part: it is also gone from Part:2." "the panel says the tool is also gone from Part:2"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                     ui_key Return
# 6000 + 6000 - 2000 mm3.
ui_expect_volume "Body [A-Za-z0-9]* (F4.b0): volume 6000.000 -> \([0-9.]*\) mm3" 10000 "joined with Part's block"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][4]["type"], doc["features"][4]["tools"], "tool_links" in doc["features"][4]' \
  "('combine', [{'body': 'F2.b0', 'occurrence': 'O1'}], False)" "the tool in Part:1"

echo "--- Edited to a cut"
ui_mark
ui_step "double-click Combine1"          ui_double_click_logged "Timeline Combine1"
ui_expect_new "Editing F5 with Combine" "editing the combine"
ui_expect_new "Combine Tool Bodies listed: Part:1" "the tool back in Part:1"
ui_step "Cut"                            ui_choose "Panel Combine input operation" 1
# The tool where it was picked again: the preview reads it in Part:1.
ui_expect_new "Preview Combine: ok" "previewed with the tool in Part:1"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F5" "edited"
ui_expect_volume "Body [A-Za-z0-9]* (F4.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 4000 "Part:1's block cut away"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][4]["operation"], doc["features"][4]["tools"]' \
  "('cut', [{'body': 'F2.b0', 'occurrence': 'O1'}])" "a cut, still in Part:1"

echo "--- Part:2's block added: the same body in two occurrences"
ui_mark
ui_step "double-click Combine1"          ui_double_click_logged "Timeline Combine1"
ui_expect_new "Editing F5 with Combine" "editing the combine again"
ui_expect_new "Preview Combine: ok" "previewed"
ui_expect_new "leaves Part: it is also gone from Part:2." "the edit says the tool is also gone from Part:2"
# Keep Tools stays off: a consumed tool leaves its own component
# (mitcad#104), so with Part:1's block a tool the preview has Part's block
# in no occurrence; with Tool Bodies active it draws it where Part:2 shows
# it as a tool overlay, which a click picks (mitcad#561).
ui_mark
ui_step "Tool Bodies"                    ui_click_logged "Panel Combine input tools"
ui_expect_new "F2.b0 in O2 at 30,0,0 tool" "Part's block drawn as a tool where Part:2 shows it"
ui_mark
ui_step "pick Part:2's block"            ui_click_logged "Body F2.b0 in O2"
ui_expect_new "Combine Tool Bodies: 2 bodies [body F2.b0 in O1; body F2.b0 in O2]" \
  "the same body picked where Part:2 shows it"
ui_expect_new "Combine Tool Bodies listed: Part:1, Part:2" "listed by their occurrences"
ui_expect_new "Preview Combine: ok" "previewed with both"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F5" "edited"
# 6000 - 2000 - 2000 mm3.
ui_expect_volume "Body [A-Za-z0-9]* (F4.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 2000 "both blocks cut away"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][4]["tools"]' \
  "[{'body': 'F2.b0', 'occurrence': 'O1'}, {'body': 'F2.b0', 'occurrence': 'O2'}]" \
  "a tool in each occurrence"

echo "--- Move/Copy Point to Point between copies of a component (mitcad#699)"
# Part:1 (O1, C1) 10 along X: Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up:
# F2.b0. Part:2 (O2) 50 along X. Nothing in the root.
ui_stop_app
FILE=$WORK/copies.mitcad
cat > "$WORK/copies.json" << EOF
[
  {"cmd": "create_component", "name": "Part", "transform": {"translation": [10, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "Part:1", "transform": {"translation": [50, 0, 0]}}
]
EOF
"$CLI" run "$WORK/copies.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F2.b0 in O2 at " "Part:2's block shown"
ui_mark
ui_step "select Part:2 in the browser"   ui_click_logged "Browser Root/Part:2"
ui_expect_new "Browser selected: Root/Part:2" "Part:2 selected"
ui_step "move (M)"                       ui_key m
ui_expect_new "Command Move/Copy started" "Move/Copy started"
ui_expect_new "Panel Move/Copy choices: move_type=components" "Part:2 moves as a component"
ui_step "Point to Point"                 ui_choose "Panel Move/Copy input type" 3
ui_expect_new "Move/Copy: Move Type = Point to Point" "Point to Point"
ui_step "Origin Point"                   ui_click_logged "Panel Move/Copy input from"
ui_expect_new "Pick places:" "the places of the picks logged"
ui_sync
# Where a click picks the vertices of Part's block ("<vertex> <x> <y>"),
# both copies: in the top view X points right, so of the two places of a
# vertex Part:2's is the one on the right.
start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
places=$(tail -n +"$start" "$UI_LOG" |
  sed -n 's/^Pick vertex F2\.b0\/\(V{[^ ]*}\) at \([0-9]*\),\([0-9]*\)$/\1 \2 \3/p')
vertex=$(awk '{print $1}' <<< "$places" | sort | uniq -d | head -1)
[ -n "$vertex" ] || ui_fail "no vertex of Part's block can be picked in both copies: $places"
read -r left_x left_y <<< "$(awk -v v="$vertex" '$1 == v {print $2, $3}' <<< "$places" | sort -n | head -1)"
read -r right_x right_y <<< "$(awk -v v="$vertex" '$1 == v {print $2, $3}' <<< "$places" | sort -n | tail -1)"
[ "$right_x" -gt "$left_x" ] || ui_fail "the two places of $vertex are not side by side"
echo "ok   $vertex at $left_x,$left_y (Part:1) and $right_x,$right_y (Part:2)"
ui_mark
ui_step "pick it where Part:2 shows it"  xdotool mousemove $((X + right_x * UI_SCALE)) $((Y + right_y * UI_SCALE)) click 1
ui_expect_new "Move/Copy Origin Point: 1 vertex [vertex $vertex of F2.b0 in O2]" "the origin picked in Part:2"
ui_step "Target Point"                   ui_click_logged "Panel Move/Copy input to"
ui_step "pick it where Part:1 shows it"  xdotool mousemove $((X + left_x * UI_SCALE)) $((Y + left_y * UI_SCALE)) click 1
ui_expect_new "Move/Copy Target Point: 1 vertex [vertex $vertex of F2.b0 in O1]" "the target picked in Part:1"
ui_expect_new "Preview Move/Copy: ok" "previewed"
ui_expect_new "Preview Move/Copy moves O2" "the preview moves Part:2"
# From 50 mm to 10 mm along X: onto Part:1.
ui_expect_new "F2.b0 in O2 at 10,0,0" "Part:2 previewed on Part:1"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Moved 1 components (Point to Point)" "Part:2 moved"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file '[(f["transform"].get("from_occurrence"), f["transform"].get("to_occurrence")) for f in doc["features"] if f["type"] == "move_occurrence"]' \
  "[('O2', 'O1')]" "each point kept with its occurrence"
ui_mark
ui_step "double-click Move1"             ui_double_click_logged "Timeline Move1"
ui_expect_new "Editing F3 with Move/Copy" "editing the move"
ui_expect_new "Move/Copy Origin Point: 1 vertex [vertex $vertex of F2.b0 in O2]" "the origin back in Part:2"
ui_expect_new "Move/Copy Target Point: 1 vertex [vertex $vertex of F2.b0 in O1]" "the target back in Part:1"
ui_expect_new "Preview Move/Copy: ok" "previewed again"
ui_step "cancel (Esc)"                   ui_key Escape
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Extrude To Object to a face of another component (mitcad#762)"
# frame:1 (O1, C1) 30 up: Sketch1 (F1) 40 x 20, Extrude1 (F2) 10 up:
# F2.b0, its top 40 up in the design. crank:1 (O2, C2) at the origin:
# Sketch2 (F3) 10 x 10 at x = 60, beside frame's block.
ui_stop_app
FILE=$WORK/crank.mitcad
cat > "$WORK/crank.json" << EOF
[
  {"cmd": "create_component", "name": "frame", "transform": {"translation": [0, 0, 30]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "crank"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [60, 0], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/crank.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F2.b0 in O1 at " "frame's block shown"
# Saved with crank active, it opens so (mitcad#1678).
ui_expect_log "Opened with C2 active, as saved (unmodified)" "crank is active"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F3" "crank's rectangle taken"
ui_mark
ui_step "To Object"                      ui_choose "Panel Extrude input extent" 1
ui_expect_new "Extrude: Extent = To Object" "to object chosen"
ui_expect_new "Pick places:" "the object input takes faces"
ui_mark
ui_step "pick frame's top face"          ui_click_pick "face F2.b0/F2:end("
ui_expect_new "Extrude Object: 1 face [face F2:end($R) of F2.b0 in O1]" \
  "the face picked where frame:1 shows it"
ui_expect_new "Preview Extrude: ok" "previewed with frame inactive"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (New Body), distance to object" "the extrusion added"
# From crank's sketch up to the plane of frame's top, 40 up: 10 x 10 x 40.
ui_expect_volume "New body [^(]* (F4.b0): volume \([0-9.]*\) mm3" 4000 "up to frame's top"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
grep -q "belongs to frame" "$UI_LOG" && ui_fail "the extrusion could not use frame's face"
save
expect_file 'doc["features"][3]["extent"]["object"]["body"], doc["features"][3]["object_links"]' \
  "('F2.b0', {'F2.b0': {'source': 'O1', 'target': 'O2'}})" "the face linked where frame:1 and crank:1 are"

echo "--- Edited: the face kept where it was picked"
ui_mark
ui_step "double-click Extrude2"          ui_double_click_logged "Timeline Extrude2"
ui_expect_new "Editing F4 with Extrude" "editing the extrusion"
ui_expect_new "Extrude Object: 1 face [face F2:end($R) of F2.b0 in O1]" "the face back where frame:1 shows it"
ui_expect_new "Preview Extrude: ok" "the edit previewed"
ui_mark
ui_step "offset 5"                       ui_type_in "Panel Extrude input offset" "5"
ui_expect_new "Preview Extrude: ok" "5 mm beyond the face previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F4" "edited"
ui_expect_volume "Body [^(]* (F4.b0): volume 4000.000 -> \([0-9.]*\) mm3" 4500 "5 mm beyond frame's top"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][3]["object_links"]' "{'F2.b0': {'source': 'O1', 'target': 'O2'}}" \
  "the link kept by the edit"

echo "--- An edit keeps a link that saw the profiles in crank's second copy (mitcad#762)"
# As above, crank:2 (O3) 40 along Y and 5 up. Extrude2 (F4), crank's,
# goes up to frame's top linked where crank:2 sees it: 35 high there
# (40 where crank:1 does). The session puts a profile in the first copy
# of its component; the edit must keep the link's target, crank:2.
ui_stop_app
FILE=$WORK/crank-twice.mitcad
cat > "$WORK/crank-twice.json" << EOF
[
  {"cmd": "create_component", "name": "frame", "transform": {"translation": [0, 0, 30]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "crank"},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "crank:1", "transform": {"translation": [0, 40, 5]}},
  {"cmd": "activate_component", "component": "crank"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [60, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "to_object", "object": {"type": "face", "body": "F2.b0", "face": "F2:end($R)"}},
    "object_links": {"F2.b0": {"source": "O1", "target": "O3"}}, "operation": "new_body"}},
  {"expect": {"feature": "Extrude2", "status": "ok", "volume": 3500, "tolerance": 1e-6}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/crank-twice.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F2.b0 in O1 at " "frame's block shown"
ui_mark
ui_step "double-click Extrude2"          ui_double_click_logged "Timeline Extrude2"
ui_expect_new "Editing F4 with Extrude" "editing the extrusion"
ui_expect_new "Extrude Object: 1 face [face F2:end($R) of F2.b0 in O1]" "the face where frame:1 shows it"
ui_expect_new "Preview Extrude: ok" "the edit previewed"
ui_mark
ui_step "offset 5"                       ui_type_in "Panel Extrude input offset" "5"
ui_expect_new "Preview Extrude: ok" "5 mm beyond the face previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F4" "edited"
# 35 + 5 high where crank:2 sees frame's top; 40 + 5 had the link moved to crank:1.
ui_expect_volume "Body [^(]* (F4.b0): volume 3500.000 -> \([0-9.]*\) mm3" 4000 "still as crank:2 sees frame"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][3]["object_links"]' "{'F2.b0': {'source': 'O1', 'target': 'O3'}}" \
  "the link's target still crank:2"

echo "--- Another component's construction plane as To Object is refused in the panel (mitcad#772)"
# frame:1 (O1, C1) 30 up: Plane1 (F1), 20 above frame's XY (50 up in the
# design). crank:1 (O2, C2): Sketch1 (F2) 10 x 10 at x = 60. No link
# carries a construction plane, and the model reads one only in its own
# component: Extrude in crank refuses frame's plane with the reason.
ui_stop_app
FILE=$WORK/crank-plane.mitcad
cat > "$WORK/crank-plane.json" << EOF
[
  {"cmd": "create_component", "name": "frame", "transform": {"translation": [0, 0, 30]}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "crank"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [60, 0], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/crank-plane.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
# Saved with crank active, it opens so (mitcad#1678).
ui_expect_log "Opened with C2 active, as saved (unmodified)" "crank is active"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F2" "crank's rectangle taken"
ui_mark
ui_step "To Object"                      ui_choose "Panel Extrude input extent" 1
ui_expect_new "Extrude: Extent = To Object" "to object chosen"
ui_expect_log "Datum F1 in O1 at" "frame's plane can be picked"
ui_sync
ui_mark
ui_step "pick frame's plane"             ui_click_logged "Datum F1 in O1"
ui_expect_new "Extrude Object: 1 plane [plane F1 in O1]" "frame's plane picked where frame:1 shows it"
ui_expect_new "Preview Extrude: not built: Plane1 is in frame: pick a plane of crank, or a face of frame." \
  "refused in the panel with the reason"
ui_step "cancel (Esc)"                   ui_key Escape
ui_sync
grep -q "a feature of crank cannot use it" "$UI_LOG" && ui_fail "frame's plane reached the model"
grep -q "Added extrude" "$UI_LOG" && ui_fail "an extrusion was added"

ui_finish "UI component combine test"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandSession.cpp
# The SURFACE group (mitcad#455) through the real UI, with the bodies the
# model reports (commands.md, "Surface modelling"):
#   - Surface Extrude: a 60 x 40 rectangle's profile (instead of the newest
#     one) and an open L of two lines beside it (Chain Selection), 10 mm:
#     two surface bodies, no volume; edited, its panel loads it.
#   - Surface Revolve: a line 5 mm off a sketch line, a full turn about it.
#   - Surface Sweep: a line along a line square to its sketch.
#   - Surface Loft: a line to the same line 10 mm above, two sections.
#   - Patch: a 40 x 40 square filled; Thicken: the patch 2 mm thick, a new
#     body of 3200 mm3 (the patch stays).
#   - Offset Surface: the thickened patch's top 3 mm out; Ruled Surface: a
#     strip off a wall's open edges; Extend: that wall past its open edges.
#   - Trim: the walls cut by Plane1 (10 mm up), the plane drawn as the
#     trim extends it (mitcad#712), the piece clicked kept;
#     Unstitch: one of their faces a surface body of its own; Reverse
#     Normal: a face of the revolved surface; Untrim: the offset face with
#     its outer boundary back, a new surface.
#   - A surface of one loose face (mitcad#2544), on a second design: a
#     ruled strip off one free edge of a patch. Offset Surface takes its
#     face from the view; Thicken takes it as a body clicked in the view
#     and from the browser, the same solid both ways; the patch (a shell)
#     clicked in the view is a body as before.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-test.sh
# check-all sources: tools/cli core/model

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-surface.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Sketch1 (F1): a 60 x 40 rectangle (c1-c4, its points p5-p8) and an open
# L, (80, 0)-(90, 0) (c9) and (90, 0)-(90, 10) (c12); Sketch2 (F2) on YZ:
# the revolve's line (c1) and its axis (c4) apart; Sketch3 (F3) on XZ: the
# sweep's path; Sketch4 (F4): the swept line from the path's start;
# Sketch5 (F5) and Sketch6 (F7, on Plane1 F6 10 mm up): the loft's lines;
# Sketch7 (F8): a 40 x 40 square for the patch.
cat > "$WORK/surfaces.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [80, 0], "end": [90, 0]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [90, 0], "end": [90, 10]},
  {"cmd": "sketch.create", "plane": "yz"},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [5, 0], "end": [5, 10]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [0, -5], "end": [0, 15]},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_line", "sketch": "F3", "start": [-40, 0], "end": [-40, 20]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [-40, 0], "end": [-30, 0]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F5", "start": [0, 60], "end": [10, 60]},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 10}}},
  {"cmd": "sketch.create", "plane": "F6"},
  {"cmd": "sketch.add_line", "sketch": "F7", "start": [0, 60], "end": [10, 60]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F8", "corner": [100, 0], "width": 40, "height": 40}
]
EOF
"$CLI" run "$WORK/surfaces.json" --save "$WORK/surfaces.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli surfaces"; }

ui_start_display
ui_start_app --open "$WORK/surfaces.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Surface Extrude: the rectangle and the open L"
ui_step "surface extrude (search)"         ui_command "Surface Extrude"
ui_step "the profiles input"               ui_click_logged "Panel Surface Extrude input profiles"
ui_step "take the newest profile away"     ui_click_pick "profile F8/"
ui_step "pick the rectangle"               ui_click_pick "profile F1/"
ui_step "pick a line of the L"             ui_click_logged "Sketch entity F1/c9"
ui_expect_log "Preview Surface Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added surface extrude of 1 profiles and 1 curves" "surface extrude added"
ui_expect_log "(F9.b0): volume 0.000 mm3" "the rectangle's walls, a surface"
ui_expect_log "(F9.b1): volume 0.000 mm3" "the L's walls, a surface"

echo "--- Extrude1 edited: the Surface Extrude panel"
ui_step "double-click Extrude1"            ui_double_click_logged "Timeline Extrude1"
ui_expect_log "Editing F9 with Surface Extrude" "editing it with its own panel"
ui_step "distance 15"                      ui_type_in "Panel Surface Extrude input distance" "15"
ui_expect_log "Preview Surface Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F9" "Extrude1 edited"

echo "--- Surface Revolve: a line a full turn about another"
ui_step "surface revolve (search)"         ui_command "Surface Revolve"
ui_step "the profiles input"               ui_click_logged "Panel Surface Revolve input profiles"
ui_step "pick the line"                    ui_click_logged "Sketch entity F2/c1"
ui_step "the axis input"                   ui_click_logged "Panel Surface Revolve input axis"
ui_step "pick the axis line"               ui_click_logged "Sketch entity F2/c4"
ui_expect_log "Preview Surface Revolve: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added surface revolve of 0 profiles and 1 curves" "surface revolve added"
ui_expect_log "(F10.b0): volume 0.000 mm3" "a cylinder's wall"

echo "--- Surface Sweep: a line along a line"
ui_step "surface sweep (search)"           ui_command "Surface Sweep"
ui_step "the profiles input"               ui_click_logged "Panel Surface Sweep input profiles"
ui_step "pick the swept line"              ui_click_logged "Sketch entity F4/c1"
ui_step "the path input"                   ui_click_logged "Panel Surface Sweep input path"
ui_step "pick the path"                    ui_click_logged "Sketch entity F3/c1"
ui_expect_log "Preview Surface Sweep: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added surface sweep of 0 profiles and 1 curves" "surface sweep added"
ui_expect_log "(F11.b0): volume 0.000 mm3" "the swept surface"

echo "--- Surface Loft: two lines in order"
ui_step "surface loft (search)"            ui_command "Surface Loft"
ui_step "the lower line"                   ui_click_logged "Sketch entity F5/c1"
ui_step "the upper line"                   ui_click_logged "Sketch entity F7/c1"
ui_expect_log "Preview Surface Loft: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added surface loft through 2 sections" "surface loft added"
ui_expect_log "(F12.b0): volume 0.000 mm3" "the lofted surface"

echo "--- Patch and Thicken"
ui_step "patch (search)"                   ui_command "Patch"
ui_step "pick the square"                  ui_click_pick "profile F8/"
ui_expect_log "Preview Patch: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added patch of 1 items" "patch added"
ui_expect_log "(F13.b0): volume 0.000 mm3" "the patch"
body=$(sed -n 's/^New body \(.*\) (F13\.b0): volume.*/\1/p' "$UI_LOG" | tail -1)
[ -n "$body" ] || ui_fail "the patch's body has no name in the log"
ui_step "thicken (search)"                 ui_command "Thicken"
ui_step "the patch in the browser"         ui_click_logged "Browser Root/Bodies/$body"
ui_step "thickness 2"                      ui_type_in "Panel Thicken input thickness" "2"
ui_expect_log "Preview Thicken: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added thicken (New Body) of 1 bodies" "thicken added"
ui_expect_volume "New body .* (F14.b0): volume \([0-9.]*\) mm3" 3200 "the patch 2 mm thick"

echo "--- Offset Surface, Ruled Surface and Extend"
ui_step "offset surface (search)"          ui_command "Offset Surface"
ui_step "the thickened patch's top"        ui_click_pick "face F14.b0/F14:front("
ui_step "distance 3"                       ui_type_in "Panel Offset Surface input distance" "3"
ui_expect_log "Preview Offset Surface: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added offset surface of 1 faces" "offset surface added"
ui_expect_log "(F15.b0): volume 0.000 mm3" "the offset face, a surface"
ui_step "ruled surface (search)"           ui_command "Ruled Surface"
ui_step "the open edges input"             ui_click_logged "Panel Ruled Surface input open_faces"
ui_step "a wall of the extrusion"          ui_click_pick "face F9.b0/F9:side("
ui_step "distance 3"                       ui_type_in "Panel Ruled Surface input distance" "3"
ui_expect_log "Preview Ruled Surface: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added ruled surface off 0 edges and the open edges of 1 faces" "ruled surface added"
ui_expect_log "(F16.b0): volume 0.000 mm3" "the strips, a surface"
ui_step "extend (search)"                  ui_command "Extend"
ui_step "the open edges input"             ui_click_logged "Panel Extend input open_faces"
ui_step "a wall of the extrusion"          ui_click_pick "face F9.b0/F9:side("
ui_step "distance 2"                       ui_type_in "Panel Extend input distance" "2"
ui_expect_log "Preview Extend: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added extend of 0 edges and the open edges of 1 faces" "extend added"

echo "--- Trim, Unstitch, Reverse Normal and Untrim"
ui_step "trim (search)"                    ui_command "Trim"
ui_expect_log "Command search chose surface.trim" "the surface trim"
walls=$(sed -n 's/^New body \(.*\) (F9\.b0): volume.*/\1/p' "$UI_LOG" | tail -1)
[ -n "$walls" ] || ui_fail "the extrusion's walls have no name in the log"
ui_step "the walls in the browser"         ui_click_logged "Browser Root/Bodies/$walls"
ui_step "the tool input"                   ui_click_logged "Panel Trim input tool"
ui_step "Plane1 (10 mm up) as the tool"    ui_click_logged "Browser Root/Construction/Plane1"
ui_step "the keep input"                   ui_click_logged "Panel Trim input keep"
ui_step "click the walls"                  ui_click_pick "face F9.b0/"
ui_expect_log "Preview Trim: ok" "previewed"
# Plane1 as the trim extends it, drawn across the walls at z = 10
# (mitcad#712).
ui_expect_log "Preview extended tool: " "the extended tool logged"
grep -Eq '^Preview extended tool: -?[0-9.]+,-?[0-9.]+,10\.0 to -?[0-9.]+,-?[0-9.]+,10\.0$' "$UI_LOG" ||
  ui_fail "the extended plane is not drawn at z = 10: $(grep '^Preview extended tool: ' "$UI_LOG" | tail -1)"
echo "ok   the extended plane drawn at z = 10"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added trim keeping 1 pieces" "trim added"
ui_step "unstitch (search)"                ui_command "Unstitch"
ui_step "a face of the trimmed walls"      ui_click_pick "face F9.b0/"
ui_expect_log "Preview Unstitch: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added unstitch of 1 faces" "unstitch added"
ui_expect_log "(F19.b0): volume 0.000 mm3" "the face a surface body of its own"
ui_step "reverse normal (search)"          ui_command "Reverse Normal"
ui_step "the revolved surface"             ui_click_pick "face F10.b0/"
ui_expect_log "Preview Reverse Normal: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added reverse normal of 1 faces" "reverse normal added"
ui_step "untrim (search)"                  ui_command "Untrim"
ui_step "the offset face"                  ui_click_pick "face F15.b0/"
ui_step "the outer boundary too"           ui_click_logged "Panel Untrim input outer"
ui_expect_log "Preview Untrim: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added untrim (holes closed, outer restored)" "untrim added"
ui_expect_log "(F21.b0): volume 0.000 mm3" "the untrimmed face, a surface"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- A surface of one loose face picked as a body in the view (mitcad#2544)"
# Started again on a 40 x 40 patch (F2, Body1, a shell) and one ruled
# strip 5 mm wide off its free edge along y = 0 (F3, Body2): the strip is
# sewn alone, a body whose shape is that one face.
ui_stop_app
SQUARE="r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
cat > "$WORK/strip.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "patch", "profiles": [{"sketch": "F1", "region": "$SQUARE"}]}},
  {"cmd": "add_feature", "def": {"type": "ruled_surface", "body": "F2.b0", "distance": 5,
    "openings": [{"opening": {"faces": ["F2:patch($SQUARE)"]}, "at": [[20, 0, 0]]}]}}
]
EOF
"$CLI" run "$WORK/strip.json" --save "$WORK/strip.mitcad" > "$WORK/cli-strip.log" 2>&1 ||
  { cat "$WORK/cli-strip.log"; ui_fail "mitcad-cli (patch and strip)"; }
ui_start_app --open "$WORK/strip.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the patch and the strip shown"
# A face input still takes the strip's face from the view.
ui_mark
ui_step "offset surface (search)"          ui_command "Offset Surface"
ui_expect_new "Pick face F3.b0/F3:ruled(" "the strip's face can be picked as a face"
ui_step "the strip's face"                 ui_click_pick "face F3.b0/F3:ruled("
ui_expect_log "Preview Offset Surface: ok" "the face previewed"
ui_mark
ui_step "cancel (Escape)"                  ui_key Escape
ui_expect_new "Command Offset Surface cancelled" "offset surface cancelled"
# Thicken's Surfaces input takes the strip as a body where it is clicked.
ui_mark
ui_step "thicken (search)"                 ui_command "Thicken"
ui_expect_new "Pick body F3.b0/ at" "a click on the strip picks its body"
ui_expect_new "Pick body F2.b0/ at" "a click on the patch picks its body"
ui_step "the strip in the view"            ui_click_pick "body F3.b0/ at"
ui_step "thickness 2"                      ui_type_in "Panel Thicken input thickness" "2"
ui_expect_new "Preview Thicken: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added thicken (New Body) of 1 bodies" "thicken of the strip clicked in the view added"
ui_expect_volume "New body .* (F4.b0): volume \([0-9.]*\) mm3" 400 "the strip clicked in the view 2 mm thick"
# The same strip from the browser: the same body, the same solid.
ui_mark
ui_step "thicken (search)"                 ui_command "Thicken"
ui_step "the strip in the browser"         ui_click_logged "Browser Root/Bodies/Body2"
ui_step "thickness 2"                      ui_type_in "Panel Thicken input thickness" "2"
ui_expect_new "Preview Thicken: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added thicken (New Body) of 1 bodies" "thicken of the strip chosen in the browser added"
ui_expect_volume "New body .* (F5.b0): volume \([0-9.]*\) mm3" 400 "the strip chosen in the browser 2 mm thick"
# The patch, a shell, still picks as a body in the view.
ui_mark
ui_step "thicken (search)"                 ui_command "Thicken"
ui_expect_new "Pick body F2.b0/ at" "a click on the patch picks its body"
ui_step "the patch in the view"            ui_click_pick "body F2.b0/ at"
ui_step "thickness 2"                      ui_type_in "Panel Thicken input thickness" "2"
ui_expect_new "Preview Thicken: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added thicken (New Body) of 1 bodies" "thicken of the patch clicked in the view added"
ui_expect_volume "New body .* (F6.b0): volume \([0-9.]*\) mm3" 3200 "the patch clicked in the view 2 mm thick"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app
ui_finish "surface modelling"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render app/framework/Appearances.cpp app/framework/Appearances.hpp
# Appearances of faces and textures (mitcad#53, docs/rendering.md
# "Materials"), through the real UI:
#   - The Appearance panel gives the box's top face Paint - Red (its Faces
#     input): the shaded view draws only that face red.
#   - With the renderer (MITCAD_RENDER builds): the rendered view sends the
#     face's material and renders red where the shaded view is red, and
#     nowhere else.
#   - Edit Appearances lists the face, Clear gives it the body's grey
#     again, and Undo brings the red back.
#   - An appearance whose image is missing says so in Edit Appearances.
#   - With the renderer: a checker image made by the test (black and white
#     squares of 10 mm: the image's two cells per 20 mm repeat), assigned
#     to the box in Edit Appearances and embedded there, renders from the
#     top with squares of 10 mm (the period within 15 %) after its file
#     is gone.
#
# Without the renderer, the parts that need it are skipped.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-faces-test.sh [directory for screenshots]
# check-all sources: tools/cli

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
RENDER=0
grep -qs '^MITCAD_RENDER:BOOL=ON' "$BUILD_DIR/CMakeCache.txt" && RENDER=1

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
TOOLS=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d /tmp/mitcad-ui-render-faces.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_RENDER_SAMPLES=16

# A 60 x 60 x 20 box (F1.b0) around the origin, in the default grey; an
# appearance with a checker texture of 20 mm repeats projected from the
# top, and one whose image is missing.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-30, -30],
    "length": 60, "width": 60, "height": 20, "operation": "new_body"}},
  {"cmd": "create_appearance", "id": "checker", "name": "Checker", "based_on": "plastic_white",
    "texture": {"path": "checker.png", "size": [20, 20], "projection": "planar"}},
  {"cmd": "create_appearance", "id": "lost", "name": "Lost Image",
    "texture": {"path": "nowhere/lost.png", "size": [20, 20]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/box.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# The checker: 2 x 2 cells of 32 pixels, black and white (a PNG written
# with Python's standard library).
python3 - "$WORK/checker.png" << 'EOF'
import struct, sys, zlib
size, cell = 64, 32
rows = b"".join(b"\0" + bytes(0 if (x // cell + y // cell) % 2 else 255 for x in range(size))
                for y in range(size))
def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 0, 0, 0, 0)) +
       chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
open(sys.argv[1], "wb").write(png)
EOF

# view_region: the view below the orientation cube, in screen coordinates.
view_region() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ vx vy vw vh <<< "$area"
  echo $((X + vx + 10)) $((Y + vy + 200)) $((vw - 20)) $((vh - 220))
}

shot() {
  ui_sync
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
  if [ -n "$SHOTS" ]; then
    python3 "$TOOLS/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/render-faces-$1.png"
  fi
}

# red shot [other]: JSON with the body's pixels in the shot ("object"),
# the red ones among them ("red": red clearly above green and blue), and
# with another shot the red pixels of both and of either.
red() {
  python3 - "$TOOLS" "$WORK/$1.xwd" "${2:+$WORK/$2.xwd}" $(view_region) << 'EOF'
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
region = [int(v) for v in sys.argv[4:8]]
def masks(path):
    rows = stats.region_rows(path, *region)
    body = stats.object_mask(rows)
    red = {(x, y) for x, y in body
           if rows[y][x][0] > 100 and rows[y][x][0] > 1.8 * rows[y][x][1] and rows[y][x][0] > 1.8 * rows[y][x][2]}
    return body, red
body, red = masks(sys.argv[2])
result = {"object": len(body), "red": len(red)}
if sys.argv[3]:
    _, other = masks(sys.argv[3])
    result["other_red"] = len(other)
    result["both"] = len(red & other)
    result["either"] = len(red | other)
print(json.dumps(result))
EOF
}

# expect_done description: the worker rendered all samples of the last
# view, and the view shows its last frame (no new frame for a second).
expect_done() {
  local view frames last
  for _ in $(seq 1 300); do
    view=$(grep -o "Render view [0-9]*: " "$UI_LOG" | tail -1 | tr -dc 0-9)
    [ -n "$view" ] && grep -qE "Render view $view: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" && break
    ui_crashed && ui_fail "$1: the app crashed"
    sleep 0.2
  done
  grep -qE "Render view $view: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" ||
    ui_fail "$1: view $view did not get its $MITCAD_RENDER_SAMPLES samples"
  last=-1
  for _ in $(seq 1 20); do
    frames=$(grep -cE "Render frame view $view " "$UI_LOG")
    [ "$frames" = "$last" ] && break
    last=$frames
    sleep 1
  done
  echo "ok   $1"
}

# expect_red json min max description: the share of the body's pixels
# that are red is within [min, max].
expect_red() {
  python3 -c '
import json, sys
d = json.loads(sys.argv[1])
low, high = float(sys.argv[2]), float(sys.argv[3])
share = d["red"] / d["object"] if d["object"] else 0.0
sys.exit(0 if d["object"] > 3000 and low <= share <= high else 1)' "$1" "$2" "$3" ||
    ui_fail "$4: $1"
  echo "ok   $4 ($1)"
}

ui_start_display
ui_start_app --open "$WORK/box.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/box.mitcad" "the box opened"
ui_step "hide the layout grid" ui_command "Layout Grid"
shot plain
json=$(red plain)
expect_red "$json" 0 0.01 "the box is grey"

echo "--- The top face in Paint - Red (the Appearance panel)"
ui_mark
ui_step "appearance (search)"            ui_command "Appearance"
ui_step "the Faces input"                ui_click_logged "Panel Appearance input faces"
ui_step "pick the top face"              ui_click_pick "face F1.b0/F1:top"
ui_step "Paint - Red"                    ui_choose "Panel Appearance input value" 5
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance paint_red for 0 body(ies), 1 face(s)" "the top face is painted"
ui_step "Escape (clear the selection)"   ui_key Escape
ui_wait_idle
shot redface
json=$(red redface)
expect_red "$json" 0.15 0.75 "the shaded view draws only the top face red"

if [ "$RENDER" = 1 ]; then
  echo "--- Rendered"
  ui_mark
  ui_step "View > Rendered"              ui_command "Rendered"
  ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
  ui_expect_new "Render body F1.b0: appearance default, 1 faces paint_red" "the face's material goes to the worker"
  expect_done "the view rendered"
  shot rendered
  json=$(red rendered redface)
  python3 -c '
import json, sys
d = json.loads(sys.argv[1])
iou = d["both"] / d["either"] if d["either"] else 0.0
ok = d["red"] > 2000 and iou > 0.75 and d["object"] > 1.5 * d["red"]
sys.exit(0 if ok else 1)' "$json" || ui_fail "the render is red only where the shaded view is: $json"
  echo "ok   red where the shaded view is red, grey elsewhere ($json)"
  ui_step "leave the rendered view"      ui_command "Rendered"
else
  echo "skip the rendered view (built without MITCAD_RENDER)"
fi

echo "--- Edit Appearances: the face listed, cleared, and undone"
ui_mark
ui_step "right-click Body1"              ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."           ui_menu_choose "Appearance..."
ui_expect_new "Appearances for bodies: F1.b0" "the dialog is for the box"
ui_expect_new "Appearances face F1.b0 F1:top paint_red at" "the dialog lists the red face"
ui_focus_dialog '^Appearances$'
ui_step "select the face"                ui_click_logged "Appearances face F1.b0 F1:top paint_red"
ui_step "Clear"                          ui_click_logged "Appearances clear_faces"
ui_expect_new "Cleared face appearances of F1.b0 F1:top" "the face's appearance is cleared"
ui_step "Close"                          ui_click_logged "Appearances close"
ui_focus_main
ui_step "Escape (clear the selection)"   ui_key Escape
ui_wait_idle
shot cleared
json=$(red cleared)
expect_red "$json" 0 0.01 "the top face is grey again"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Clear Appearance of Face of Body1" "the face's appearance is back"
ui_wait_idle
shot undone
json=$(red undone)
expect_red "$json" 0.15 0.75 "undo makes it red again"
ui_mark
ui_step "right-click Body1"              ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."           ui_menu_choose "Appearance..."
ui_expect_new "Appearances face F1.b0 F1:top paint_red at" "the dialog lists the red face again"
ui_focus_dialog '^Appearances$'
ui_step "select the face again"          ui_click_logged "Appearances face F1.b0 F1:top paint_red"
ui_step "Clear"                          ui_click_logged "Appearances clear_faces"
ui_expect_new "Cleared face appearances of F1.b0 F1:top" "cleared again"

echo "--- Textures"
ui_focus_dialog '^Appearances$'
ui_mark
# The design's own appearances are last in the list: checker, then lost.
ui_step "the list"                       ui_click_logged "Appearances item steel_satin"
ui_step "the appearance with a lost image (End)" ui_key End
ui_expect_new "Appearance lost texture missing: the image $WORK/nowhere/lost.png is missing" \
  "the dialog says the image is missing"
if [ "$RENDER" = 1 ]; then
  ui_mark
  ui_step "the checker (Up)"             ui_key Up
  ui_expect_new "Appearance selected: checker" "the checker is selected"
  ui_step "Assign to Body1"              ui_click_logged "Appearances assign"
  ui_expect_new "Assigned appearance checker to F1.b0" "the box gets the checker"
  ui_step "embed the image"              ui_click_logged "Appearances field texture_embed"
  ui_expect_new "Appearance checker texture embedded" "the image is embedded"
  ui_step "Close"                        ui_click_logged "Appearances close"
  ui_focus_main
  rm "$WORK/checker.png"
  ui_step "Escape (clear the selection)" ui_key Escape
  ui_step "the view from the top"        ui_command "Top"
  sleep 1
  ui_wait_idle
  shot top
  ui_mark
  ui_step "View > Rendered"              ui_command "Rendered"
  ui_expect_new "Render body F1.b0: appearance checker" "the box goes with its checker" 30
  expect_done "the view rendered"
  shot checker
  # The box's width in the shaded view is 60 mm; the squares along the
  # row through the top face's middle are 10 mm.
  python3 - "$TOOLS" "$WORK/top.xwd" "$WORK/checker.xwd" $(view_region) << 'EOF' || ui_fail "the checker's squares"
import importlib.util, statistics, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
region = [int(v) for v in sys.argv[4:8]]
body = stats.object_mask(stats.region_rows(sys.argv[2], *region))
xs = [x for x, _ in body]
ys = [y for _, y in body]
x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
per_mm = (x1 - x0 + 1) / 60.0
rows = stats.region_rows(sys.argv[3], *region)
def runs(values):
    dark = [sum(v) / 3 < 110 for v in values]
    lengths, count = [], 1
    for a, b in zip(dark, dark[1:]):
        if a == b:
            count += 1
        else:
            lengths.append(count)
            count = 1
    return lengths[1:]  # the first run starts at the face's edge
middle_row = (y0 + y1) // 2
along_x = runs([rows[middle_row][x] for x in range(x0 + 3, x1 - 2)])
expected = 10.0 * per_mm
measured = statistics.median(along_x) if along_x else 0.0
print("     squares: %.1f px measured, %.1f px expected (%s)" % (measured, expected, along_x))
sys.exit(0 if len(along_x) >= 3 and abs(measured - expected) <= 0.15 * expected else 1)
EOF
  echo "ok   the checker's squares are 10 mm"
else
  echo "skip the texture's render (built without MITCAD_RENDER)"
fi

ui_finish "appearances of faces and textures"

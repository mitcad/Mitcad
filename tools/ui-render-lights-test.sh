#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The render settings' lights and coloured reflections on the ground
# (mitcad#54, docs/rendering.md "Lights" and "Environment"), through View >
# Render Environment on a red plastic block in the dark studio:
#   - Add puts a point light at the camera: the block is brighter; Aim at
#     Face puts it out along the normal of a face clicked in the view: that
#     face is brighter than without the light; its glyph is drawn over the
#     view (orange: the chosen light).
#   - Delete takes the light away and undo brings it back (the worker gets
#     the lights each time).
#   - Reflections on the ground: below the block the ground is reddish (the
#     shadow catcher's colours, not grey), in the view and in an image of
#     File > Render Image saved as PNG.
#   - The lights are saved in the file.
#
# Skipped (exit 0) when the build has no render worker (MITCAD_RENDER off).
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-lights-test.sh [directory for screenshots]

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
if ! grep -qs '^MITCAD_RENDER:BOOL=ON' "$BUILD_DIR/CMakeCache.txt"; then
  echo "SKIP: $UI_APP is built without the render worker (MITCAD_RENDER=OFF)"
  exit 0
fi

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
TOOLS=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-render-lights.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_RENDER_SAMPLES=16

# A red plastic block of 60 x 40 x 30 mm (F1.b0) in the dark studio.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-30, -20],
    "length": 60, "width": 40, "height": 30, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "plastic_red"},
  {"cmd": "set_render_settings", "environment": {"preset": "studio_dark"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

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
    python3 "$TOOLS/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/render-lights-$1.png"
  fi
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

# stats shaded shot [x y]: JSON with the mean luminance of the block's
# rendered pixels (where the shaded view shows it, inside its outline), and
# with a point (screen) the mean luminance of the 15 x 15 pixels around it.
stats() {
  python3 - "$TOOLS" "$WORK/$1.xwd" "$WORK/$2.xwd" $(view_region) "${3:--1}" "${4:--1}" << 'EOF'
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
region = [int(v) for v in sys.argv[4:8]]
shaded = stats.object_mask(stats.region_rows(sys.argv[2], *region))
rendered = stats.region_rows(sys.argv[3], *region)
inside = [(x, y) for x, y in shaded
          if all((x + dx, y + dy) in shaded for dx in (-4, 0, 4) for dy in (-4, 0, 4))]
luma = lambda c: 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
block = [luma(rendered[y][x]) for x, y in inside]
result = {"pixels": len(block), "block": round(sum(block) / max(len(block), 1), 1)}
cx, cy = int(sys.argv[8]) - region[0], int(sys.argv[9]) - region[1]
if cx >= 0:
    patch = [luma(rendered[y][x]) for y in range(cy - 7, cy + 8) for x in range(cx - 7, cx + 8)]
    result["patch"] = round(sum(patch) / len(patch), 1)
if inside:
    # The middle of the block's outline (screen), to click a face there.
    xs = sorted(x for x, _ in inside)
    ys = sorted(y for _, y in inside)
    result["middle"] = [region[0] + xs[len(xs) // 2], region[1] + ys[len(ys) // 2]]
print(json.dumps(result))
EOF
}

# reflection rows: JSON of where the red block is in the rows ([[r, g,
# b], ...] top first) and how red the ground right below it is: the mean of
# red minus green of the 4 to 24 pixels below the block's lowest red pixel
# in each of its columns, and the same in the image's upper left corner.
REFLECTION='
def reflection(rows):
    red = lambda c: c[0] - c[1]
    height, width = len(rows), len(rows[0])
    lowest = {}
    for y in range(height):
        for x in range(width):
            c = rows[y][x]
            if red(c) > 80 and c[0] > 90:
                lowest[x] = y
    columns = sorted(lowest)
    # The middle half of the block (its edges show its sides).
    columns = columns[len(columns) // 4: 3 * len(columns) // 4]
    below = [red(rows[y][x]) for x in columns for y in range(lowest[x] + 4, min(lowest[x] + 25, height))]
    corner = [red(c) for row in rows[:30] for c in row[:40]]
    return {"columns": len(columns), "below": round(sum(below) / max(len(below), 1), 1),
            "corner": round(sum(corner) / max(len(corner), 1), 1)}
'

screen_reflection() {
  python3 - "$TOOLS" "$WORK/$1.xwd" $(view_region) << EOF
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
$REFLECTION
print(json.dumps(reflection(stats.region_rows(sys.argv[2], *[int(v) for v in sys.argv[3:7]]))))
EOF
}

png_reflection() {
  python3 - "$TOOLS" "$1" << EOF
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
$REFLECTION
width, height, step, data = stats.read_png(sys.argv[2])
rows = [[tuple(data[y][x * step:x * step + 3]) for x in range(width)] for y in range(height)]
result = reflection(rows)
result["size"] = [width, height]
print(json.dumps(result))
EOF
}

# check description python-condition json: the condition on d (the JSON).
check() {
  python3 -c '
import json, sys
d = json.loads(sys.argv[2])
sys.exit(0 if eval(sys.argv[1]) else 1)' "$2" "$3" || ui_fail "$1: $3"
  echo "ok   $1"
}
value() { echo "$1" | python3 -c "import json, sys; print(json.load(sys.stdin)$2)"; }

# set_light field value: types a value into a light's field.
set_light() {
  ui_focus_dialog '^Render Environment$'
  ui_type_in "Render light field $1" "$2"
  ui_key Return
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_step "hide the layout grid" ui_command "Layout Grid"
shot shaded

echo "--- The dark studio without lights"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
ui_expect_new "Render environment studio_dark, background view, ground shadows, 0 lights" "no lights yet"
expect_done "the dark studio rendered"
shot dark
dark=$(stats shaded dark)
echo "     no lights: $dark"
check "the block is found" 'd["pixels"] > 500' "$dark"
read -r mx my <<< "$(value "$dark" '["middle"][0]') $(value "$dark" '["middle"][1]')"
dark=$(stats shaded dark "$mx" "$my")

echo "--- Add a point light: at the camera"
ui_step "View > Render Environment..." ui_command "Render Environment"
ui_expect_log "Render environment dialog opened" "the dialog opened"
ui_focus_dialog '^Render Environment$'
ui_expect_log "Render settings page lights at" "the dialog logs its pages"
ui_mark
ui_step "the Lights page" ui_click_logged "Render settings page lights"
ui_expect_new "Render settings page lights" "the lights are shown"
ui_expect_new "Render light field add at" "the dialog logs the lights' fields"
ui_mark
ui_step "Add (Point)" ui_click_logged "Render light field add"
ui_expect_new "Render light light1 added (point)" "a light is added in the model"
ui_expect_new "Render environment studio_dark, background view, ground shadows, 1 lights" "the worker gets the light"
expect_done "the light rendered"
shot added
added=$(stats shaded added "$mx" "$my")
echo "     a light at the camera: $added"
check "the light at the camera brightens the block" "d['block'] > $(value "$dark" '["block"]') + 15" "$added"

echo "--- Aim at Face: out along a face's normal"
ui_step "distance 25" set_light distance 25
ui_step "Aim at Face" ui_click_logged "Render light field aim"
ui_expect_log "Render light light1: click a face to aim at" "the dialog waits for a click"
ui_mark
ui_step "click the block" eval "xdotool mousemove $mx $my click 1"
ui_expect_new "Render light light1 aimed at " "the light is aimed at the face"
ui_expect_new "Render light light1 aimed at a face" "the model has the new place"
ui_expect_new "Render environment studio_dark, background view, ground shadows, 1 lights" "the worker gets it"
expect_done "the aimed light rendered"
shot aimed
aimed=$(stats shaded aimed "$mx" "$my")
echo "     aimed at the face: $aimed"
check "the lit face is brighter than without the light" "d['patch'] > $(value "$dark" '["patch"]') + 25" "$aimed"
read -r gx gy <<< "$(grep -o "Render light glyph light1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //; s/,/ /')"
[ -n "$gx" ] || ui_fail "no glyph of the light was drawn"
glyph=$(python3 - "$TOOLS" "$WORK/aimed.xwd" $((X + gx * UI_SCALE)) $((Y + gy * UI_SCALE)) << 'EOF'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
rows = stats.region_rows(sys.argv[2], int(sys.argv[3]) - 2, int(sys.argv[4]) - 2, 5, 5)
orange = [c for row in rows for c in row if c[0] > 200 and 90 < c[1] < 180 and c[2] < 90]
print(len(orange))
EOF
)
[ "$glyph" -ge 5 ] || ui_fail "the glyph at $gx,$gy is not orange ($glyph pixels)"
echo "ok   the light's glyph is drawn at $gx,$gy"

echo "--- Delete, undo"
ui_mark
ui_focus_dialog '^Render Environment$'
ui_step "Delete" ui_click_logged "Render light field delete"
ui_expect_new "Render light light1 deleted" "the light is deleted"
ui_expect_new "Render environment studio_dark, background view, ground shadows, 0 lights" "the worker has no light"
ui_focus_main
ui_mark
ui_step "undo (ctrl+z)" ui_key ctrl+z
ui_expect_new "Render environment studio_dark, background view, ground shadows, 1 lights" "undo brings the light back"
expect_done "the light is back"

echo "--- Reflections on the ground: reddish below the block"
ui_step "View > Render Environment..." ui_command "Render Environment"
ui_focus_dialog '^Render Environment$'
ui_mark
ui_step "the Environment page" ui_click_logged "Render settings page environment"
ui_expect_new "Render settings page environment" "the environment is shown"
ui_expect_new "Render settings field ground.reflections at" "the dialog logs its fields"
shot shadows
plain=$(screen_reflection shadows)
echo "     without reflections: $plain"
ui_mark
ui_step "Reflections on the ground" ui_click_logged "Render settings field ground.reflections"
ui_expect_new "Render setting ground.reflections = true" "reflections are on"
ui_expect_new "Render environment studio_dark, background view, ground shadows and reflections, 1 lights" \
  "the worker renders the polished ground"
expect_done "the reflections rendered"
shot reflections
seen=$(screen_reflection reflections)
echo "     with reflections: $seen"
check "the ground below the block is reddish" \
  "d['columns'] > 20 and d['below'] > d['corner'] + 12 and d['below'] > $(value "$plain" '["below"]') + 8" "$seen"

echo "--- Render Image: the same in a PNG"
ui_step "Close" ui_click_logged "Render settings close"
ui_focus_main
ui_step "File > Render Image..." ui_command "Render Image"
ui_expect_log "Render image dialog opened" "the dialog opened"
ui_focus_dialog '^Render Image$'
ui_expect_log "Render image field output.width at" "the dialog logs its fields"
ui_mark
for field in "output.width 480" "output.samples 16"; do
  read -r name number <<< "$field"
  ui_focus_dialog '^Render Image$'
  ui_step "$name $number" ui_type_in "Render image field $name" "$number"
  ui_key Return
  ui_expect_new "Render setting $name = $number" "$name changed"
done
ui_mark
ui_step "Render" ui_click_logged "Render image render"
ui_expect_new "Render image done: 480 x " "the image is rendered" 90
ui_step "Save..." ui_click_logged "Render image save"
ui_focus_dialog '^Save Rendered Image$'
ui_key ctrl+a
xdotool type --delay 20 "$WORK/render.png"
sleep 0.5
ui_mark
ui_key Return
ui_expect_new "Render image saved $WORK/render.png" "Save wrote the image" 10
[ -n "$SHOTS" ] && cp "$WORK/render.png" "$SHOTS/render-lights-render.png"
image=$(png_reflection "$WORK/render.png")
echo "     the saved image: $image"
check "the saved image's ground below the block is reddish" \
  "d['size'][0] == 480 and d['columns'] > 20 and d['below'] > d['corner'] + 12" "$image"
ui_focus_dialog '^Render Image$'
ui_step "Close" ui_click_logged "Render image close"

echo "--- Saved in the file"
ui_focus_main
ui_step "save (ctrl+s)" ui_key ctrl+s
ui_expect_log "Saved $WORK/block.mitcad" "the design is saved"
python3 -c '
import json, sys
render = json.load(open(sys.argv[1]))["render"]
lights = render["lights"]
assert len(lights) == 1 and lights[0]["id"] == "light1" and lights[0]["type"] == "point", lights
assert render["ground"]["reflections"] is True, render
' "$WORK/block.mitcad" || ui_fail "the lights are not in the saved file"
echo "ok   the lights are in the file"

ui_finish "the render settings' lights and coloured reflections"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The rendered view's environment (mitcad#47, docs/rendering.md), through
# View > Render Environment on a white box and a clear glass sphere:
#   - The Dark Studio lights the box less than the Studio (its mean
#     brightness), over the same view background.
#   - A background colour shows around the bodies without a new render; the
#     environment as the background shows the dark studio there.
#   - An HDR image (.hdr, and .exr) made by the test lights the bodies and
#     is the background (orange, then green); one that cannot be read says
#     so over the view, and the studio lights the scene instead.
#   - Exposure brightens the image without a new render, and undo takes it
#     back; the settings are saved in the file.
#
# Skipped (exit 0) when the build has no render worker (MITCAD_RENDER off).
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-environment-test.sh [directory for screenshots]

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
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-render-environment.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_RENDER_SAMPLES=16

# A 40 mm white box (F1.b0) left of the origin and a clear glass sphere of
# 40 mm (F2.b0) right of it, both on the XY plane.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-60, -20],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": "xy", "center": [40, 0],
    "diameter": 40, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "plastic_white"},
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "glass_clear"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/bodies.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# Environment images, made here: an equirectangular orange (.hdr) and green
# (.exr) all around.
convert -size 64x32 xc:'rgb(255,110,20)' "$WORK/orange.hdr" &&
  convert -size 64x32 xc:'rgb(40,220,60)' "$WORK/green.exr" ||
  ui_fail "ImageMagick did not write the environment images"

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
    python3 "$TOOLS/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/render-environment-$1.png"
  fi
}

# stats shaded shot: JSON with the mean luminance of the box's rendered
# pixels (where the shaded view shows the left body, inside its outline),
# that of their darkest tenth ("low"), their median colour, and the median
# colour of the view's upper left corner (no body there).
stats() {
  python3 - "$TOOLS" "$WORK/$1.xwd" "$WORK/$2.xwd" $(view_region) << 'EOF'
import importlib.util, json, statistics, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
region = [int(v) for v in sys.argv[4:8]]
shaded = stats.object_mask(stats.region_rows(sys.argv[2], *region))
rendered = stats.region_rows(sys.argv[3], *region)
inside = {(x, y) for x, y in shaded
          if all((x + dx, y + dy) in shaded for dx in (-4, 0, 4) for dy in (-4, 0, 4))}
box = [rendered[y][x] for x, y in inside if x < region[2] // 2]
corner = [p for row in rendered[:40] for p in row[:60]]
luma = lambda c: 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
median = lambda pixels: [statistics.median(p[i] for p in pixels) for i in range(3)]
lumas = sorted(map(luma, box))
print(json.dumps({"pixels": len(box), "box": round(sum(lumas) / max(len(box), 1), 1),
                  "low": round(lumas[len(lumas) // 10], 1) if lumas else None,
                  "box_color": median(box) if box else None, "corner": median(corner)}))
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

# check description python-condition json: the condition on d (the JSON).
check() {
  python3 -c '
import json, sys
d = json.loads(sys.argv[2])
sys.exit(0 if eval(sys.argv[1]) else 1)' "$2" "$3" || ui_fail "$1: $3"
  echo "ok   $1"
}

# set_field field value: types a value into a field of Render Environment.
set_field() {
  ui_focus_dialog '^Render Environment$'
  ui_type_in "Render settings field $1" "$2"
  ui_key Return
}

ui_start_display
ui_start_app --open "$WORK/bodies.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/bodies.mitcad" "the bodies opened"
ui_step "hide the layout grid" ui_command "Layout Grid"
shot shaded

echo "--- The studio (the default)"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
ui_expect_new "Render environment studio, background view, ground shadows" "the default environment went first"
expect_done "the studio rendered"
shot studio
studio=$(stats shaded studio)
echo "     studio: $studio"
check "the box is found and lit" 'd["pixels"] > 500 and d["box"] > 120' "$studio"

echo "--- Render Environment: the dark studio"
ui_step "View > Render Environment..." ui_command "Render Environment"
ui_expect_log "Render environment dialog opened" "the dialog opened"
ui_focus_dialog '^Render Environment$'
ui_expect_log "Render settings field environment.preset at" "the dialog logs its fields"
ui_mark
ui_step "Light: Dark Studio" ui_choose "Render settings field environment.preset" 2
ui_expect_new "Render setting environment.preset = studio_dark" "the preset changed in the model"
ui_expect_new "Render environment studio_dark, background view" "the worker renders the dark studio"
expect_done "the dark studio rendered"
shot dark
dark=$(stats shaded dark)
echo "     dark studio: $dark"
value() { echo "$1" | python3 -c "import json, sys; print(json.load(sys.stdin)$2)"; }
check "the dark studio lights the box less, over the same background" \
  "d['box'] < 0.85 * $(value "$studio" '["box"]') and \
   max(abs(a - b) for a, b in zip(d['corner'], $(value "$studio" '["corner"]'))) < 12" \
  "$dark"

echo "--- Background: a colour, then the environment"
ui_mark
ui_step "Background: Colour" ui_choose "Render settings field background.mode" 1
ui_expect_new "Render setting background.mode = color" "the background is a colour"
ui_step "colour #204080" set_field background.color "#204080"
ui_expect_new "Render setting background.color = #204080" "the colour changed"
shot color
color=$(stats shaded color)
echo "     colour: $color"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Render environment " &&
  ui_fail "a background colour sent the worker a new environment"
check "the colour is behind the bodies (no new render)" \
  'abs(d["corner"][0] - 32) < 8 and abs(d["corner"][1] - 64) < 8 and abs(d["corner"][2] - 128) < 8' "$color"
ui_mark
ui_step "Background: Environment" ui_choose "Render settings field background.mode" 1
ui_expect_new "Render environment studio_dark, background environment" "the camera sees the environment"
expect_done "the environment is the background"
shot dark-environment
seen=$(stats shaded dark-environment)
echo "     dark studio as background: $seen"
check "the dark studio's surroundings are behind the bodies" 'max(d["corner"]) < 60' "$seen"

echo "--- An HDR image (.hdr): orange all around"
ui_mark
ui_step "Light: HDR Image" ui_choose "Render settings field environment.preset" 2
ui_expect_new "Render environment image, background environment" "the image preset goes to the worker"
ui_expect_new "View message: Rendering: no environment image is chosen; the studio lights the scene instead." \
  "without an image the view says so"
expect_done "the studio rendered instead"
ui_mark
ui_step "image path orange.hdr" set_field environment.image "$WORK/orange.hdr"
ui_expect_new "Render setting environment.image = $WORK/orange.hdr" "the image is set"
ui_expect_new "Render environment image, background environment" "the worker renders the image"
expect_done "the image rendered"
shot orange
orange=$(stats shaded orange)
echo "     orange: $orange"
check "the image is the background and lights the box orange" \
  'd["corner"][0] > d["corner"][1] + 40 and d["corner"][1] > d["corner"][2] and d["box_color"][0] > d["box_color"][2] + 30' \
  "$orange"

echo "--- Exposure: brighter without a new render, undone"
ui_mark
ui_step "exposure 1.5" set_field film.exposure 1.5
ui_expect_new "Render setting film.exposure = 1.5" "the exposure changed"
shot bright
bright=$(stats shaded bright)
echo "     exposure +1.5: $bright"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Render environment " &&
  ui_fail "the exposure sent the worker a new environment"
orange_box=$(value "$orange" '["box"]')
check "the box is brighter" "d['box'] > $orange_box + 15" "$bright"
ui_step "Close" ui_click_logged "Render settings close"
ui_focus_main
ui_mark
ui_step "undo (ctrl+z)" ui_key ctrl+z
shot undone
undone=$(stats shaded undone)
echo "     undone: $undone"
check "undo takes the exposure back" "abs(d['box'] - $orange_box) < 6" "$undone"

echo "--- An .exr image, then one that cannot be read"
ui_step "View > Render Environment..." ui_command "Render Environment"
ui_mark
ui_step "image path green.exr" set_field environment.image "$WORK/green.exr"
ui_expect_new "Render environment image, background environment" "the worker renders the .exr"
expect_done "the .exr rendered"
shot green
green=$(stats shaded green)
echo "     green: $green"
check "the .exr is the background" 'd["corner"][1] > d["corner"][0] + 40 and d["corner"][1] > d["corner"][2] + 40' "$green"
ui_mark
ui_step "image path missing.hdr" set_field environment.image "$WORK/missing.hdr"
ui_expect_new "View message: Rendering: cannot read the environment image $WORK/missing.hdr" \
  "a missing image is reported over the view"
expect_done "the studio rendered instead"

echo "--- Saved in the file"
ui_focus_main
ui_step "save (ctrl+s)" ui_key ctrl+s
ui_expect_log "Saved $WORK/bodies.mitcad" "the design is saved"
python3 -c '
import json, sys
render = json.load(open(sys.argv[1]))["render"]
assert render["environment"]["preset"] == "image", render
assert render["environment"]["image"].endswith("missing.hdr"), render
assert render["background"]["mode"] == "environment", render
assert [round(c * 255) for c in render["background"]["color"]] == [32, 64, 128], render
' "$WORK/bodies.mitcad" || ui_fail "the render settings are not in the saved file"
echo "ok   the render settings are in the file"

ui_finish "the rendered view's environment"

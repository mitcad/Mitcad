#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/TexturedBody.hpp app/render/GrainProjection.hpp app/OcctViewer.cpp app/framework/Appearances.cpp
# check-all sources: app/render/CyclesRenderer.cpp core/model-base/src/appearance.rs core/model-base/src/appearance core/model-base/textures
# The built-in woods' and panels' textures (mitcad#2186), on a private Xvfb
# display: a plywood cabinet (two sides, bottom, top, a shelf and a thin
# back, all Plywood - Birch, which a design names by its id only) and,
# right of it, a reference panel in a plain copy of the same plywood (its
# colour, no texture), made by mitcad-cli:
#   - In the light and the dark color scheme the shaded view draws each
#     cabinet body with its texture (the view logs "View textured body
#     <uid>: 1 textured looks, grain offset <u> <v>"), each body with its
#     own grain offset (mitcad#2605), not the reference panel; where the view
#     shows the cabinet its neighbouring pixels differ by the grain clearly
#     more than on the plain panel of the same colour.
#   - The rendered view sends the bodies as Plywood - Birch with its images
#     (no "texture image is not drawn"), and renders the cabinet with grain
#     too: more pixel-to-pixel variation than the plain panel, which takes
#     the same light and noise.
# The pictures (cabinet-shaded-light.png, cabinet-shaded-dark.png,
# cabinet-rendered.png) stay in build/ui-wood-textures-pictures
# (UI_PICTURES), for the ticket.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-wood-textures-test.sh
# check-all sources: tools/cli

set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
UI_APP=${UI_APP:-$(cd "$here/.." && pwd)/build/dev/app/mitcad}
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
ROOT=$(cd "$here/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-wood-textures.XXXXXX")
PICTURES=${UI_PICTURES:-$ROOT/build/ui-wood-textures-pictures}
mkdir -p "$PICTURES"
FILE=$WORK/cabinet.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_RENDER_SAMPLES=16

# A cabinet 600 wide, 400 deep and 720 high of 18 mm plywood with a 6 mm
# back (F1.b0 to F6.b0), and a reference panel 600 x 18 x 720 (F7.b0) right
# of it, facing the isometric view.
plane() { printf '{"origin": [0, 0, %s], "normal": [0, 0, 1], "x_axis": [1, 0, 0]}' "$1"; }
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 18, "width": 400, "height": 720, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [582, 0],
    "length": 18, "width": 400, "height": 720, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": $(plane 0), "corner": [18, 0],
    "length": 564, "width": 400, "height": 18, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": $(plane 702), "corner": [18, 0],
    "length": 564, "width": 400, "height": 18, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": $(plane 351), "corner": [18, 0],
    "length": 564, "width": 380, "height": 18, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": $(plane 18), "corner": [18, 394],
    "length": 564, "width": 6, "height": 684, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [1400, 0],
    "length": 600, "width": 18, "height": 720, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "plywood_birch"},
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "plywood_birch"},
  {"cmd": "set_body_appearance", "uid": "F3.b0", "appearance": "plywood_birch"},
  {"cmd": "set_body_appearance", "uid": "F4.b0", "appearance": "plywood_birch"},
  {"cmd": "set_body_appearance", "uid": "F5.b0", "appearance": "plywood_birch"},
  {"cmd": "set_body_appearance", "uid": "F6.b0", "appearance": "plywood_birch"},
  {"cmd": "create_appearance", "id": "plain_birch", "name": "Plain Birch", "based_on": "plywood_birch",
    "texture": null},
  {"cmd": "set_body_appearance", "uid": "F7.b0", "appearance": "plain_birch"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# The design names the built-in plywood by its id and embeds no image.
python3 -c '
import json, sys
doc = json.load(open(sys.argv[1]))
assert not doc.get("images"), "the design embeds images"
assert "plywood_birch" in open(sys.argv[1]).read(), "the design does not name plywood_birch"
' "$FILE" || ui_fail "the design file is not as expected"
echo "ok   the design names plywood_birch and embeds no image"

# view_region: the view below the orientation cube, in screen coordinates.
view_region() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ vx vy vw vh <<< "$area"
  echo $((X + vx + 10)) $((Y + vy + 200)) $((vw - 20)) $((vh - 220))
}

# shot name: a screen dump of the test display, and its picture.
shot() {
  ui_sync
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
  python3 "$here/xwd2png.py" "$WORK/$1.xwd" "$PICTURES/$1.png"
}

# grain mask pixels: JSON with the mean difference of horizontally
# neighbouring pixels (8-bit luminance; differences over 12, edges and
# outlines, left out) where the shaded dump `mask` shows an object, a few
# pixels inside its outline, in `pixels` (a dump of the same view): left of
# the objects' widest gap the cabinet, right of it the reference panel.
grain() {
  python3 - "$here" "$WORK/$1.xwd" "$WORK/$2.xwd" $(view_region) << 'EOF'
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
region = [int(v) for v in sys.argv[4:8]]
mask = stats.object_mask(stats.region_rows(sys.argv[2], *region))
rows = stats.region_rows(sys.argv[3], *region)
inside = {(x, y) for x, y in mask
          if all((x + dx, y + dy) in mask for dx in (-4, 0, 4) for dy in (-4, 0, 4))}
# The widest run of columns without an object between the two, of the
# pixels well inside objects (thin things, the view's version text in its
# corner, the axes, drop out), in columns with at least 20 of them.
counts = {}
for x, _ in inside:
    counts[x] = counts.get(x, 0) + 1
columns = sorted(x for x, n in counts.items() if n >= 20)
gap, split = 0, region[2] // 2
for a, b in zip(columns, columns[1:]):
    if b - a > gap:
        gap, split = b - a, (a + b) // 2
lum = lambda c: 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
result = {"gap": gap}
for side, keep in (("cabinet", lambda x: x < split), ("panel", lambda x: x >= split)):
    steps = [abs(lum(rows[y][x + 1]) - lum(rows[y][x])) for x, y in inside
             if keep(x) and (x + 1, y) in inside]
    steps = [s for s in steps if s <= 12]
    result[side] = {"pixels": len(steps), "grain": round(sum(steps) / len(steps), 3) if steps else None}
print(json.dumps(result))
EOF
}

# expect_grain json what: the cabinet's pixels vary clearly more than the
# plain panel's.
expect_grain() {
  local verdict
  verdict=$(python3 -c '
import json, sys
d = json.loads(sys.argv[1])
cabinet, panel = d["cabinet"], d["panel"]
if cabinet["pixels"] < 2000 or panel["pixels"] < 500:
    print("the bodies are not found in the view: %s" % d)
elif cabinet["grain"] < 1.5 * panel["grain"] + 0.3:
    print("no grain on the cabinet: %s" % d)
else:
    print("ok cabinet %s, plain panel %s" % (cabinet["grain"], panel["grain"]))
' "$1")
  [ "${verdict#ok }" != "$verdict" ] || ui_fail "$2: $verdict"
  echo "ok   $2: ${verdict#ok }"
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

export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME
# Each cabinet body's own part of the plywood's images (mitcad#2605): the
# model's grain_offset of its uid (core/model-base/src/appearance/grain.rs),
# mm along and across the grain; neighbouring panels lie hundreds of mm
# apart within the 1000 mm repeat, so they do not read as one board.
grain_offsets=([1]="2697.2 3432.1" [2]="9396.5 5476.1" [3]="8159.9 5121.6" [4]="1406.8 9617.5"
  [5]="708.1 716.3" [6]="9830.5 897.5")
ui_start_display
for scheme in light dark; do
  echo "--- The shaded view, $scheme"
  MITCAD_TEST_COLOR_SCHEME=$scheme ui_start_app --open "$FILE" --no-native-dialogs
  ui_expect_log "Opened $FILE" "$scheme: the cabinet opened"
  for body in 1 2 3 4 5 6; do
    ui_expect_log "View textured body F$body.b0: 1 textured looks, grain offset ${grain_offsets[$body]}" \
      "$scheme: F$body.b0 is drawn with its texture, its own part of the grain"
  done
  grep -q "View textured body F7.b0" "$UI_LOG" && ui_fail "$scheme: the plain panel is drawn with a texture"
  echo "ok   $scheme: the plain panel is drawn in its colour"
  ui_step "hide the layout grid" ui_command "Layout Grid"
  shot "cabinet-shaded-$scheme"
  json=$(grain "cabinet-shaded-$scheme" "cabinet-shaded-$scheme")
  echo "     $json"
  expect_grain "$json" "$scheme: the shaded view shows the grain"
  if [ "$scheme" = light ]; then
    echo "--- Rendered"
    ui_mark
    ui_step "View > Rendered" ui_command "Rendered"
    ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
    ui_expect_new "Render body F1.b0: appearance plywood_birch" "the side goes as Plywood - Birch"
    ui_expect_new "Render body F7.b0: appearance plain_birch" "the panel goes as its plain copy"
    expect_done "the view rendered"
    grep -q "texture image is not drawn" "$UI_LOG" && ui_fail "a texture image is not drawn"
    echo "ok   every texture image is drawn"
    shot cabinet-rendered
    json=$(grain "cabinet-shaded-$scheme" cabinet-rendered)
    echo "     $json"
    expect_grain "$json" "the rendered view shows the grain"
  fi
  ui_stop_app
done
ui_finish "wood textures (pictures in $PICTURES)"

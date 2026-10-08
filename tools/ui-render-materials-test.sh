#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Appearances in the rendered view (mitcad#46, docs/rendering.md): a box in
# red plastic and a sphere in chrome, rendered through the real UI:
#   - The scene sent to the worker names each body's appearance.
#   - Where the shaded view shows each body, the rendered pixels are those
#     of its material: the plastic's near its base colour (red, its hue
#     kept), the chrome's a neutral grey with brighter and darker
#     reflections; the two differ.
#   - A copy of the plastic made, assigned and edited in Edit Appearances
#     while rendering: a change of its look sends the worker a new scene
#     (also one of the roughness alone), and the box renders in the copy's
#     colour.
#
# Skipped (exit 0) when the build has no render worker (MITCAD_RENDER off).
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-materials-test.sh [directory for screenshots]

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
WORK=$(mktemp -d /tmp/mitcad-ui-render-materials.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_RENDER_SAMPLES=16

# A 50 x 50 x 40 box (F1.b0) left of the origin in red plastic, a sphere of
# 50 mm diameter (F2.b0) right of it in chrome, both on the XY plane.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-70, -25],
    "length": 50, "width": 50, "height": 40, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": "xy", "center": [45, 0],
    "diameter": 50, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "plastic_red"},
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "chrome"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/bodies.mitcad" > "$WORK/cli.log" 2>&1 ||
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
    python3 "$TOOLS/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/render-materials-$1.png"
  fi
}

# colors shaded rendered: JSON with the median colour of each body's
# rendered pixels (left: the box, right: the sphere), taken where the shaded
# view shows the body, a few pixels inside its outline.
colors() {
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
middle = region[2] // 2
result = {}
for side, pixels in (("left", [p for p in inside if p[0] < middle]),
                     ("right", [p for p in inside if p[0] >= middle])):
    colours = [rendered[y][x] for x, y in pixels]
    if not colours:
        result[side] = None
        continue
    median = [statistics.median(c[i] for c in colours) for i in range(3)]
    spread = statistics.pstdev(sum(c) / 3 for c in colours)
    result[side] = {"pixels": len(colours), "median": median, "spread": round(spread, 1)}
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

ui_start_display
ui_start_app --open "$WORK/bodies.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/bodies.mitcad" "the bodies opened"
ui_step "hide the layout grid" ui_command "Layout Grid"
shot shaded

echo "--- Rendered"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
ui_expect_new "Render body F1.b0: appearance plastic_red" "the box goes as red plastic"
ui_expect_new "Render body F2.b0: appearance chrome" "the sphere goes as chrome"
ui_expect_new ": 2 bodies, 2 meshes sent (F1.b0, F2.b0)" "both went to the worker"
expect_done "the view rendered"
shot rendered
json=$(colors shaded rendered)
echo "     colours: $json"
verdict=$(python3 -c '
import json, sys
d = json.loads(sys.argv[1])
box, sphere = d["left"], d["right"]
if not box or not sphere or box["pixels"] < 500 or sphere["pixels"] < 500:
    print("the bodies are not found in the view: %s" % d)
    sys.exit()
r, g, b = box["median"]
# Red plastic: sRGB (200, 32, 30). Lit by the studio, with the reflection
# of its surroundings on it, its red stays far above green and blue and
# each channel near that of the base colour.
far = max(abs(m - c) for m, c in zip(box["median"], (200, 32, 30)))
if not (r > 2.4 * g and r > 2.4 * b):
    print("the plastic is not red: %s" % box["median"])
elif far > 70:
    print("the plastic is far from its base colour (200, 32, 30): %s" % box["median"])
else:
    r2, g2, b2 = sphere["median"]
    if max(r2, g2, b2) - min(r2, g2, b2) > 30:
        print("the chrome is not neutral: %s" % sphere["median"])
    elif max(abs(a - b) for a, b in zip(sphere["median"], box["median"])) < 40:
        print("the chrome looks like the plastic: %s and %s" % (sphere["median"], box["median"]))
    else:
        print("ok plastic %s, chrome %s" % (box["median"], sphere["median"]))
' "$json")
[ "${verdict#ok }" != "$verdict" ] || ui_fail "$verdict"
echo "ok   ${verdict#ok }"

echo "--- A teal copy of the plastic, edited while rendering"
dialog_type() {
  ui_focus_dialog '^Appearances$'
  ui_click_logged "Appearances field $1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 -- "$2"
  ui_key Return
}
ui_mark
ui_step "right-click Body1"             ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."          ui_menu_choose "Appearance..."
ui_expect_new "Appearances for bodies: F1.b0 (plastic_red)" "the dialog shows the box's appearance"
ui_focus_dialog '^Appearances$'
ui_step "New"                           ui_click_logged "Appearances new"
ui_expect_new "Created appearance custom1 (Plastic - Red Copy) from plastic_red" "a copy of the plastic"
ui_step "Assign to Body1"               ui_click_logged "Appearances assign"
ui_expect_new "Assigned appearance custom1 to F1.b0" "assigned (the same look: no new scene)"
ui_mark
ui_step "base colour #1a8c99"           dialog_type base_color "#1a8c99"
ui_expect_new "Appearance custom1 base_color = #1a8c99" "the base colour changed"
ui_expect_new "Render body F1.b0: appearance custom1" "the render gets the new colour"
ui_mark
ui_step "roughness 0.6"                 dialog_type roughness 0.6
ui_expect_new "Appearance custom1 roughness = 0.6" "the roughness changed"
ui_expect_new ": 2 bodies, 0 meshes sent (-), 0 meshed" "a change of parameters alone sends a new scene, without meshes"
ui_expect_new "changed F1.b0; moved -; removed -; kept 1;" "the worker changed only the box's material"
ui_step "Close"                         ui_click_logged "Appearances close"
ui_focus_main
ui_step "leave the rendered view"       ui_command "Rendered"
ui_step "Escape (clear the selection)"  ui_key Escape
ui_mark
ui_step "View > Rendered"               ui_command "Rendered"
ui_expect_new "Render body F1.b0: appearance custom1" "the box goes in its new appearance" 30
expect_done "the view rendered"
shot teal
json=$(colors shaded teal)
echo "     colours: $json"
python3 -c '
import json, sys
r, g, b = json.loads(sys.argv[1])["left"]["median"]
sys.exit(0 if g > 1.5 * r and b > 1.5 * r else 1)' "$json" || ui_fail "the box is not teal: $json"
echo "ok   the box renders in the new base colour"

ui_finish "appearances in the rendered view"

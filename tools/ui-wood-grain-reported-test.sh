#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/TexturedBody.hpp app/render/GrainProjection.hpp app/OcctViewer.cpp
# check-all sources: app/render/CyclesRenderer.cpp app/render/RenderMode.cpp core/model-base/src/appearance/grain.rs
# check-all sources: core/model-base/src/appearance/wood_textures.rs core/model-base/textures
# Each body its own part of a wood's grain (mitcad#2605, mitcad#2624,
# GitHub mitcad/Mitcad#663) on the reported design, when the private test
# files have it (MITCAD_MITCAD_REPORTED, github-663/; never committed; its
# archive and design found by their content ids, the test skipped without
# them): two 3 mm plates of the built-in Pine side by side in a component,
# 40 and 30 mm wide and 200 mm long, joined along their long sides, whose
# grain read as one board.
#   - The shaded view draws both with the texture, each with a grain offset
#     of its own (`View textured body <uid>: 1 textured looks, grain offset
#     <u> <v>`), different for the two.
#   - Seen from below (Bottom View, orthographic), the pine's stripes run
#     along the joint and wave along it; on one board they wave together
#     across the joint. In windows 30 mm long along the joint, the stripes'
#     direction (the structure tensor of the pixels, smoothed over 5 x 5)
#     in a strip 10 mm wide on each side next to the joint differs by
#     clearly more than between two strips the same distance apart on one
#     body: at least 3 degrees on average and twice as much.
#   - After a redraw (Wireframe, then the shaded style again) the view
#     shows the same: the same directions, and no other grain offset is
#     logged.
#   - The rendered view (32 samples) of the same faces from below: the
#     stripes' directions differ across the joint by at least 1 degree
#     more than within a body (render noise taken into account).
#   - Body2 moved 15 mm with Move/Copy, the design saved and opened again:
#     each body keeps the grain offset it had.
# The pictures (bottom-shaded.png, bottom-redrawn.png, bottom-rendered.png)
# stay in build/ui-wood-grain-reported-pictures (UI_PICTURES), for the
# ticket.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-wood-grain-reported-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
UI_APP=${UI_APP:-$root/build/dev/app/mitcad}
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-wood-grain-reported.XXXXXX")
PICTURES=${UI_PICTURES:-$root/build/ui-wood-grain-reported-pictures}
mkdir -p "$PICTURES"
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_RENDER_SAMPLES=32

# content_id file: the first 8 hex digits of its SHA-256.
content_id() {
  sha256sum "$1" | cut -c1-8
}

archive=""
for candidate in "${MITCAD_MITCAD_REPORTED:-/nonexistent}"/github-663/*.zip; do
  [ -f "$candidate" ] && [ "$(content_id "$candidate")" = 46d2c230 ] && { archive=$candidate; break; }
done
if [ -z "$archive" ]; then
  echo "skip the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-663)"
  echo "PASS: wood grain per body on the reported design (skipped)"
  exit 0
fi
mkdir -p "$work/reported"
(cd "$work/reported" && cmake -E tar xf "$archive") || ui_fail "the reported archive was not unpacked"
reported=""
while IFS= read -r candidate; do
  [ "$(content_id "$candidate")" = 1037c354 ] && { reported=$candidate; break; }
done < <(find "$work/reported" -name '*.mitcad' | sort)
[ -n "$reported" ] || ui_fail "the reported archive holds no design 1037c354"
# A name of the test's own: the file is saved and opened again.
FILE=$work/plates.mitcad
cp "$reported" "$FILE" || ui_fail "the reported design was not copied"

# offsets: each pine body's grain offsets the view logged, "<uid> <u> <v>"
# per line, sorted; a body drawn with two offsets has two lines.
offsets() {
  grep -o "^View textured body F[34]\.b0: [0-9]* textured looks, grain offset [-0-9.]* [-0-9.]*" "$UI_LOG" |
    sed 's/^View textured body \([^:]*\): .* grain offset /\1 /' | sort -u
}

# expect_offsets what: both bodies drawn with one offset each, as at first.
expect_offsets() {
  local now
  now=$(offsets)
  [ "$now" = "$first" ] || ui_fail "$1: the grain offsets changed: '$(tr '\n' ';' <<< "$now")', first '$(tr '\n' ';' <<< "$first")'"
  echo "ok   $1: each body keeps its grain offset"
}

# body_place uid: the screen place of the body's middle (x y), as last
# logged ("Body F3.b0 in O1 at x,y", window coordinates).
body_place() {
  local at
  at=$(grep -o "^Body $1 in O1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //')
  [ -n "$at" ] || return 1
  echo $((X + ${at%,*} * UI_SCALE)) $((Y + ${at#*,} * UI_SCALE))
}

# shot name: a screen dump of the test display, the pointer out of the
# view (no hover highlight), and its picture.
shot() {
  xdotool mousemove $((X + WIDTH - 3)) $((Y + HEIGHT - 3))
  ui_sync
  ui_wait_idle
  ui_sync
  xwd -root -silent > "$work/$1.xwd" || ui_fail "no screenshot"
  python3 "$here/xwd2png.py" "$work/$1.xwd" "$PICTURES/$1.png"
}

# joint dump [joint column]: JSON of the stripes' directions next to the
# long joint, from the bodies' places (Body1 x 0..40 mm, Body2 x 40..70
# mm, both y 0..200 mm; their middles 35 mm apart, the joint 20 mm from
# Body1's middle): the joint's screen column (the darkest column near
# where it should be: the bodies' edges drawn along it; or the one given,
# for the rendered view, which draws no edges), the scale (px per mm),
# and in windows 30 mm long along the joint (y 10..190 mm) the direction
# of the stripes in a strip 10 mm wide on each side, 4 px from the joint
# ("near"), and in a strip as far again from it on the same body ("far"):
#   across  the mean difference (degrees) between the two near strips
#   within  the mean difference between a body's near and far strips
#           (the same distance apart), of both bodies
#   contrast the luminance's spread in the near strips (texture drawn)
joint() {
  local body1 body2
  body1=$(body_place F3.b0) || ui_fail "no place logged for Body1 (F3.b0 in O1)"
  body2=$(body_place F4.b0) || ui_fail "no place logged for Body2 (F4.b0 in O1)"
  # shellcheck disable=SC2086 # the places are two numbers each
  python3 - "$here" "$work/$1.xwd" $body1 $body2 "${2:-}" << 'EOF'
import importlib.util, json, math, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1] + "/ui-image-stats.py")
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
path = sys.argv[2]
c1 = (float(sys.argv[3]), float(sys.argv[4]))
c2 = (float(sys.argv[5]), float(sys.argv[6]))
given = sys.argv[7] if len(sys.argv) > 7 else ""
scale = abs(c2[0] - c1[0]) / 35.0
if not 1.0 <= scale <= 30.0 or abs(c2[1] - c1[1]) > 2 * scale:
    print(json.dumps({"error": "the bodies are not side by side in the view: %s %s" % (c1, c2)}))
    sys.exit(0)
side = 1 if c2[0] > c1[0] else -1  # Body1 to Body2 on the screen
middle = (c1[1] + c2[1]) / 2.0
top = int(round(middle - 90.0 * scale)) - 2
height = int(round(180.0 * scale)) + 4
predicted = int(round(c1[0] + side * 20.0 * scale))
reach = int(math.ceil(30.0 * scale)) + 12
left = predicted - reach
rows = stats.region_rows(path, left, top, 2 * reach + 1, height)
lum = [[0.2126 * r + 0.7152 * g + 0.0722 * b for r, g, b in row] for row in rows]
if given:
    joint = int(given)
else:
    # The edges along the joint: the darkest column near it.
    def column_mean(x):
        return sum(row[x - left] for row in lum) / len(lum)
    joint = min(range(predicted - 8, predicted + 9), key=column_mean)
skip = 4
width = int(round(10.0 * scale))
window = int(round(30.0 * scale))


def strip(direction, start):
    """Columns start..start+width px from the joint towards direction,
    in increasing screen x."""
    xs = [joint + direction * (start + k) for k in range(width)]
    xs.sort()
    return [[row[x - left] for x in xs] for row in lum]


def smoothed(pic):
    """The mean of each 5 x 5 square (the picture 4 px smaller)."""
    h, w = len(pic), len(pic[0])
    across = [[sum(row[x:x + 5]) / 5.0 for x in range(w - 4)] for row in pic]
    return [[sum(across[y + k][x] for k in range(5)) / 5.0 for x in range(w - 4)] for y in range(h - 4)]


def directions(pic):
    """The stripes' direction (radians) in each window along the joint."""
    pic = smoothed(pic)
    out = []
    for start in range(0, len(pic) - window + 1, window):
        xx = yy = xy = 0.0
        for y in range(max(start, 1), min(start + window, len(pic) - 1)):
            row, up, down = pic[y], pic[y - 1], pic[y + 1]
            for x in range(1, len(row) - 1):
                gx = (row[x + 1] - row[x - 1]) / 2.0
                gy = (down[x] - up[x]) / 2.0
                xx += gx * gx
                yy += gy * gy
                xy += gx * gy
        out.append(0.5 * math.atan2(2.0 * xy, xx - yy))
    return out


def spread(pic):
    values = [v for row in pic for v in row]
    mean = sum(values) / len(values)
    return math.sqrt(sum((v - mean) ** 2 for v in values) / len(values))


near1, far1 = strip(-side, skip), strip(-side, 3 * skip + width)
near2, far2 = strip(side, skip), strip(side, 3 * skip + width)
d_near1, d_far1 = directions(near1), directions(far1)
d_near2, d_far2 = directions(near2), directions(far2)


def mean_difference(a, b):
    return math.degrees(sum(abs(p - q) for p, q in zip(a, b)) / len(a))


print(json.dumps({
    "joint": joint, "scale": round(scale, 3), "windows": len(d_near1),
    "across": round(mean_difference(d_near1, d_near2), 2),
    "within": round((mean_difference(d_near1, d_far1) + mean_difference(d_near2, d_far2)) / 2.0, 2),
    "contrast": round(min(spread(near1), spread(near2)), 2),
}))
EOF
}

# expect_joint json what minimum factor margin: the stripes' directions
# differ across the joint by at least `minimum` degrees, `factor` times
# and `margin` degrees more than within a body.
expect_joint() {
  local verdict
  verdict=$(python3 -c '
import json, sys
d = json.loads(sys.argv[1])
minimum, factor, margin = (float(v) for v in sys.argv[2:5])
if "error" in d:
    print(d["error"])
elif d["windows"] < 4 or d["contrast"] < 2.0:
    print("the plates or their texture are not found in the view: %s" % d)
elif d["across"] < minimum or d["across"] < factor * d["within"] or d["across"] < d["within"] + margin:
    print("the grain continues across the joint: %s" % d)
else:
    print("ok across %s degrees, within %s" % (d["across"], d["within"]))
' "$1" "$3" "$4" "$5")
  [ "${verdict#ok }" != "$verdict" ] || ui_fail "$2: $verdict"
  echo "ok   $2: ${verdict#ok }"
}

# expect_rendered: the worker rendered all samples of the last view, and
# the view shows its last frame (no new frame for a second).
expect_rendered() {
  local view frames last
  for _ in $(seq 1 600); do
    view=$(grep -o "Render view [0-9]*: " "$UI_LOG" | tail -1 | tr -dc 0-9)
    [ -n "$view" ] && grep -qE "Render view $view: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" && break
    ui_crashed && ui_fail "the app crashed while rendering"
    sleep 0.2
  done
  grep -qE "Render view $view: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" ||
    ui_fail "view $view did not get its $MITCAD_RENDER_SAMPLES samples"
  last=-1
  for _ in $(seq 1 20); do
    frames=$(grep -cE "Render frame view $view " "$UI_LOG")
    [ "$frames" = "$last" ] && break
    last=$frames
    sleep 1
  done
  echo "ok   the view rendered"
}

ui_start_display
echo "--- The shaded view"
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "the reported design opened"
for body in F3.b0 F4.b0; do
  ui_expect_log "View textured body $body: 1 textured looks, grain offset" "$body is drawn with the pine's texture"
done
first=$(offsets)
echo "     grain offsets: $(tr '\n' ';' <<< "$first")"
[ "$(grep -c . <<< "$first")" = 2 ] || ui_fail "not one grain offset for each body: '$first'"
[ "$(cut -d' ' -f2- <<< "$first" | sort -u | grep -c .)" = 2 ] || ui_fail "the two bodies have the same grain offset"
echo "ok   each body has a grain offset of its own"
ui_step "hide the layout grid"           ui_command "Layout Grid"
ui_step "orthographic (search)"          ui_command "Orthographic"
ui_mark
ui_step "Bottom View (search)"           ui_command "Bottom View"
ui_expect_new "Camera direction 0 0 1 " "seen from below"
grep "^Camera direction 0 0 1 " "$UI_LOG" | tail -1 | grep -q "(orthographic" ||
  ui_fail "the view from below is not orthographic"
shot bottom-shaded
shaded=$(joint bottom-shaded) || ui_fail "the shaded view was not measured: $shaded"
echo "     $shaded"
expect_joint "$shaded" "the shaded view: the two plates' grain differs across the joint" 3.0 2.0 0.0
column=$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["joint"])' "$shaded")

echo "--- A redraw"
ui_mark
ui_step "Wireframe (search)"             ui_command "Wireframe"
ui_expect_new "Visual style Wireframe" "drawn as wireframe"
ui_step "the shaded style again"         ui_command "Shaded with Visible Edges Only"
ui_expect_new "Visual style Shaded with Visible Edges Only" "drawn shaded again"
shot bottom-redrawn
redrawn=$(joint bottom-redrawn) || ui_fail "the redrawn view was not measured: $redrawn"
echo "     $redrawn"
python3 -c '
import json, sys
a, b = json.loads(sys.argv[1]), json.loads(sys.argv[2])
same = a.get("joint") == b.get("joint") and all(abs(a[k] - b[k]) <= 0.3 for k in ("across", "within"))
sys.exit(0 if same else 1)
' "$shaded" "$redrawn" || ui_fail "the redrawn view shows other grain: $redrawn, first $shaded"
echo "ok   the redrawn view shows the same grain"
expect_offsets "the redraw"

echo "--- Rendered"
ui_mark
ui_step "View > Rendered"                ui_command "Rendered"
ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
ui_expect_new "Render body F3.b0@O1: appearance wood_pine" "Body1 goes as Pine"
ui_expect_new "Render body F4.b0@O1: appearance wood_pine" "Body2 goes as Pine"
expect_rendered
grep -q "texture image is not drawn" "$UI_LOG" && ui_fail "a texture image is not drawn"
echo "ok   every texture image is drawn"
shot bottom-rendered
rendered=$(joint bottom-rendered "$column") || ui_fail "the rendered view was not measured: $rendered"
echo "     $rendered"
expect_joint "$rendered" "the rendered view: the two plates' grain differs across the joint" 1.0 1.0 1.0
ui_step "leave the rendered view"        ui_command "Rendered"

echo "--- A move, a save and a reopen"
ui_expect_log "Browser Root/Component1:1/Bodies/Body2 at" "the browser lists Body2 in Component1:1"
ui_mark
ui_step "right-click Body2"              ui_click_logged "Browser Root/Component1:1/Bodies/Body2" 3
ui_step "choose Move/Copy"               ui_menu_choose "Move/Copy"
ui_expect_new "Move/Copy Objects: 1 body [body F4.b0" "Move/Copy on Body2"
ui_expect_new "Panel Move/Copy input tx at" "the panel is laid out"
ui_step "15 mm along X"                  ui_type_in "Panel Move/Copy input tx" "15"
ui_expect_new "Preview Move/Copy: ok" "previewed" 30
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Moved 1 bodies (" "Body2 moved" 30
ui_wait_idle
ui_sync
expect_offsets "the move"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "the moved design opened again"
for body in F3.b0 F4.b0; do
  ui_expect_log "View textured body $body: 1 textured looks, grain offset" "$body is drawn with the pine's texture"
done
expect_offsets "the reopen"
ui_stop_app
ui_finish "wood grain per body on the reported design (pictures in $PICTURES)"

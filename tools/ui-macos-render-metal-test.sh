#!/bin/bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render app/view app/framework/TestDriver
# Metal/OIDN acceptance through the existing in-process driver. The
# helper isolates settings and touches only the test application's window.
# Usage: ui-macos-render-metal-test.sh <app> [output directory] [qt|native]
# Missing Metal OIDN may skip; either required-GPU switch makes it fail.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -lt 1 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> [output directory] [qt|native]" >&2
  exit 2
fi
frontend=${3:-qt}
case "$frontend" in
  qt|native) ;;
  *) echo "invalid frontend: $frontend (qt or native)" >&2; exit 2 ;;
esac
ui_init "$1" "${2:-ui-macos-render-metal}"
# A failed run must not reuse an earlier run's screenshots or statistics.
for shot in rendered orbited zoomed resized; do
  rm -f "$UI_OUT/$shot.png" "$UI_OUT/$shot.stats.json"
done
worker=${MITCAD_RENDER_WORKER:-$(dirname "$UI_APP")/mitcad-render}
if ! "$worker" --list-devices > "$UI_OUT/devices.json" 2> "$UI_OUT/devices.err"; then
  cat "$UI_OUT/devices.err" >&2
  echo 'FAIL: cannot query the render worker' >&2
  exit 1
fi
python3 - "$UI_OUT/devices.json" "${MITCAD_RENDER_REQUIRE_METAL:-0}" "${MITCAD_RENDER_REQUIRE_GPU:-0}" <<'PY'
import json, sys
with open(sys.argv[1]) as stream:
    devices = json.load(stream)["devices"]
metal = [device for device in devices if device["type"] == "METAL"]
reason = None
if not metal:
    reason = "no Metal render device"
elif not metal[0].get("denoiser_available", False):
    reason = "the Metal device has no OIDN support"
elif "Paravirtual" in metal[0]["name"] and sys.argv[2] not in ("", "0"):
    reason = "required physical-GPU acceptance cannot use a paravirtual device"
if reason:
    required = any(value not in ("", "0") for value in sys.argv[2:])
    print(("FAIL: " if required else "SKIP: ") + reason)
    sys.exit(1 if required else 77)
print("Metal OIDN capability: " + metal[0]["name"] + "; actual execution is checked below")
PY
capability=$?
[ "$capability" = 0 ] || exit "$capability"

# The hardware path must use the window server and GPU, with no software
# or headless override inherited from another test.
unset QT_QPA_PLATFORM QT_OPENGL LIBGL_ALWAYS_SOFTWARE GALLIUM_DRIVER
unset MITCAD_RENDER_TEST_DENOISER MITCAD_RENDER_TEST_COMPARE_DENOISER
unset MITCAD_RENDER_TEST_DENOISER_FAILURE MITCAD_RENDER_TEST_DEVICE_FAILURE
unset MITCAD_RENDER_TEST_NATIVE_PASS_COMPARE MITCAD_RENDER_TEST_COMMAND_TRACE
unset CYCLES_TEST_SINGLE_SAMPLE_UPDATES
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[render]\ndevice=metal\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
script="$UI_OUT/render.script"
cat > "$script" <<'DRIVER'
expect-ever 'View area'
window-resize 1280 800
# Frame the body in the final viewport with room for the zoom-out check.
expect-viewport
command view.fit
capture-view-height fitted_height
wheel-burst 50 50 1 1
expect-view-zoom ${fitted_height} in
command view.rendered
expect-log 'Rendered view on'
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 150
screenshot-view '${MITCAD_METAL_SHOTS}/rendered.png'
mark
view-pointer press 40 50 middle shift
view-pointer move 45 48 middle shift
view-pointer move 50 46 middle shift
view-pointer move 55 45 middle shift
sleep 700
expect-no-log 'Render view [0-9]+: (orthographic|perspective) '
view-pointer release 55 45 middle shift
wait-log 'Render view [0-9]+: first frame' 120
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
screenshot-view '${MITCAD_METAL_SHOTS}/orbited.png'
mark
capture-view-height before_zoom
wheel-burst 50 50 -1 1
wait-log 'Render view [0-9]+: first frame' 120
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
expect-view-zoom ${before_zoom} out
screenshot-view '${MITCAD_METAL_SHOTS}/zoomed.png'
DRIVER
shots='rendered orbited zoomed resized'
cat >> "$script" <<'DRIVER'
mark
window-resize 1440 900
wait-log 'Render view [0-9]+: first frame' 120
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
screenshot-view '${MITCAD_METAL_SHOTS}/resized.png'
DRIVER
cat >> "$script" <<'DRIVER'
expect-viewport
# Selection remains OCCT-backed while the rendered frame is composited.
click-visible-face
expect-selection 1 0
screenshot-view '${MITCAD_METAL_SHOTS}/selected.png'
key Escape
DRIVER
if [ "${MITCAD_RENDER_ACCEPTANCE:-0}" = 1 ]; then
  cat >> "$script" <<'DRIVER'
# Preferences changes the running worker, rather than just a saved setting.
command tools.preferences.display
focus-dialog '^Settings$'
focus-widget renderDevice
key Home Down
key ctrl+w
focus-main
wait-log 'Render device: .*\(asked for cpu\)' 30
expect-log 'Render diagnostics: backend CPU,'
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
mark
command tools.preferences.display
focus-dialog '^Settings$'
focus-widget renderDevice
key Home Down Down
key ctrl+w
focus-main
wait-log 'Render device: .*METAL.*\(asked for METAL_' 30
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120

# A geometry change and undo update the live render scene as well.
key Escape
mark
command solid.box
expect-log 'Command Box started'
# The view shows the origin as its light bulb does (mitcad#1030).
click-logged 'Browser eye Root/Origin'
expect-log 'Visibility Root/Origin: shown'
expect-log 'Datum xy at'
click-logged 'Datum xy'
focus-widget input_at_offset1
key ctrl+a
type 80
key Tab
focus-widget input_at_offset2
key ctrl+a
type 0
key Tab
focus-widget input_length
key ctrl+a
type 10
key Tab
focus-widget input_width
key ctrl+a
type 10
key Tab
focus-widget input_height
key ctrl+a
type 10
key Tab
button '^OK$'
expect-model-bodies 2 49000
wait-log 'Render scene [0-9]+: 2 bodies' 30
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
mark
command edit.undo
expect-model-bodies 1 48000
wait-log 'Render scene [0-9]+: 1 bodies' 30
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120

# A real appearance assignment updates the material in the live scene:
# the Appearance tool on Body1 selected through the Browser, Glass - Clear
# chosen in its picker (the AppKit frontend's popover), OK (mitcad#1059,
# mitcad#1066). Its Bodies input takes bodies; face picking and its actual
# rendered selection overlay were verified above.
key Escape
click-logged '^Browser Root/Bodies/Body1 at '
mark
command solid.appearance
expect-log 'Command Appearance started'
expect-log '^Appearance Bodies: 1 body \[body F2.b0\]$'
expect-log '^Panel Appearance input value at '
click-logged '^Panel Appearance input value at '
expect-log '^Appearance picker opened: '
focus-dialog '^Appearance Picker$'
type 'Glass - Clear'
expect-log '^Appearance picker search: Glass - Clear, first Glass - Clear$'
key Return
expect-log '^Appearance picker chose: Glass - Clear$'
focus-main
mark
button '^OK$'
expect-log '^Appearance glass_clear for 1 body\(ies\)$'
wait-log 'Render material swap .* -> glass_clear' 30
focus-main
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
screenshot-view '${MITCAD_METAL_SHOTS}/glass.png'
mark
command edit.undo
wait-log 'Render material swap .*glass_clear.* -> ' 30
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 ' 120
DRIVER
fi
cat >> "$script" <<'DRIVER'
command view.rendered
expect-log 'Rendered view off'
wait-log 'Render worker ended' 15
DRIVER
if [ "${MITCAD_RENDER_ACCEPTANCE:-0}" = 1 ]; then
  cat >> "$script" <<'DRIVER'
# Render Image uses the separate batch worker with the chosen Metal device.
command file.render_image
focus-dialog '^Render Image$'
focus-widget renderImage_aspect
key End
focus-widget renderImage_width
key ctrl+a
type 320
key Tab
focus-widget renderImage_height
key ctrl+a
type 240
key Tab
focus-widget renderImage_samples
key ctrl+a
type 16
key Tab
mark
key Return
wait-log 'Render image worker ready: Cycles' 30
expect-log 'Render diagnostics: backend METAL,'
wait-log 'Render image done: 320 x 240, 16 samples' 120
expect-log 'Render denoising diagnostics: backend METAL,.*OIDN METAL.*execution confirmed'
screenshot-dialog '${MITCAD_METAL_SHOTS}/final-image.png'
focus-widget renderImage_samples
key ctrl+a
type 65536
key Tab
mark
button '^Render$'
wait-log 'Render image progress ' 30
button '^Cancel$'
wait-log 'Render image cancelled' 15
expect-model-bodies 1 48000
# Escape follows the same stop path as closing the render window.
mark
button '^Render$'
wait-log 'Render image progress ' 30
key Escape
wait-log 'Render image cancelled' 15
expect-dialog-gone '^Render Image$'
expect-model-bodies 1 48000
focus-main
DRIVER
fi
if [ "${MITCAD_RENDER_ACCEPTANCE:-0}" = 1 ]; then
  # Keep this broader interaction matrix bounded; the separate navigation
  # case retains its larger Retina workload.
  sed -i '' -e 's/window-resize 1280 800/window-resize 900 650/' \
            -e 's/window-resize 1440 900/window-resize 1000 720/' "$script"
fi
if [ "$frontend" = native ] && [ "${MITCAD_RENDER_ACCEPTANCE:-0}" = 1 ]; then
  python3 - "$script" "$UI_OUT/devices.json" <<'PY'
import json, pathlib, shlex, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text()
text = text.replace("click-logged 'Browser eye Root/Origin'",
                    "native-expand native.modelTree\nnative-click native.model.eye.origin")
text = text.replace("click-logged '^Browser Root/Bodies/Body1 at '",
                    "native-expand native.modelTree\nnative-select native.model.body:F2.b0@")
devices = json.load(open(sys.argv[2]))["devices"]
cpu = next(d for d in devices if d["type"] == "CPU")
metal = next(d for d in devices if d["type"] == "METAL")
text = text.replace("focus-widget renderDevice\nkey Home Down Down",
                    "native-choice renderDevice " + shlex.quote(metal["name"] + " (METAL)"))
text = text.replace("focus-widget renderDevice\nkey Home Down",
                    "native-choice renderDevice " + shlex.quote("CPU: " + cpu["name"]))
for field, value in (("width", 320), ("height", 240), ("samples", 16), ("samples", 65536)):
    text = text.replace(f"focus-widget renderImage_{field}\nkey ctrl+a\ntype {value}",
                        f"native-type renderImage_{field} {value}")
text = text.replace("focus-widget renderImage_aspect\nkey End", "native-choice renderImage_aspect Fixed")
for field, value in (("at_offset1", 80), ("at_offset2", 0), ("length", 10), ("width", 10), ("height", 10)):
    text = text.replace(f"focus-widget input_{field}\nkey ctrl+a\ntype {value}\nkey Tab",
                        f"native-type input_{field} {value}")
path.write_text(text)
PY
fi
MITCAD_TEST_INPUT="$script" MITCAD_METAL_SHOTS="$UI_OUT" \
  MITCAD_RENDER_SAMPLES=16 MITCAD_RENDER_PREVIEWS=4,16 UI_TIMEOUT=420 \
  ui_run_app metal "--ui=$frontend" --no-recovery --demo
ui_expect_log 'TestDriver: done' 'GPU navigation script completed'
if [ "$frontend" = native ]; then
  ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' 'AppKit frontend was used'
else
  ui_expect_no_log 'Native UI: AppKit shell with embedded OCCT viewport' 'Qt frontend was used'
fi
renderer=$(ui_log_value 'OpenGL renderer: *\(.*\)')
if [ -z "$renderer" ] || printf '%s' "$renderer" | grep -Eiq 'software|llvmpipe|swiftshader'; then
  ui_fail "no hardware OpenGL renderer: $renderer"
elif [ -n "${MITCAD_UI_RENDERER:-}" ] && ! printf '%s' "$renderer" | grep -Eq -- "$MITCAD_UI_RENDERER"; then
  ui_fail "OpenGL renderer is '$renderer', expected '$MITCAD_UI_RENDERER'"
else
  ui_pass "OpenGL renderer: $renderer"
fi
if [ -z "${MITCAD_RENDER_WORKER:-}" ] || [ "${MITCAD_RENDER_REQUIRE_IOSURFACE:-0}" = 1 ]; then
  ui_expect_log 'Render transport: IOSurface, private XPC session' 'private native surface session selected'
  ui_expect_log 'Render frame transport: Metal buffer interop' 'Cycles wrote the final native light buffer'
  ui_expect_log 'Render frame transport: native auxiliary GPU, composite [01]' 'native GPU auxiliary conversion executed'
  if [ "${MITCAD_RENDER_ACCEPTANCE:-0}" = 1 ]; then
    ui_expect_log 'Render frame transport: native auxiliary CPU, composite [01]' 'CPU device switch kept native auxiliary output'
  fi
fi
ui_expect_log 'Render device: .*METAL.*\(asked for metal\)' 'explicit Metal selected'
ui_expect_log 'Render denoising diagnostics: backend METAL,.*OIDN METAL.*\(execution confirmed\)' 'Metal OIDN executed'
for forbidden in 'Rendering on the CPU:' 'Denoising on the CPU:' 'Render worker error' 'Render failed' 'Recompute failed' 'ERROR:'; do
  ui_expect_no_log "$forbidden" 'no fallback, worker error or shutdown pipeline error'
done
if [ "${MITCAD_RENDER_ACCEPTANCE:-0}" != 1 ]; then
  ui_expect_no_log 'OIDN CPU' 'no CPU denoising on the required Metal path'
fi
ui_expect_log 'Render worker ended' 'leaving rendered mode ended the worker'
if ! python3 - "$UI_LOG_FILE" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
states = re.findall(r'backend METAL, MetalRT (on|off) \(available (yes|no), default (on|off)\); OIDN METAL[^\n]*\(execution confirmed\)', text)
assert states, "no completed Metal execution diagnostics"
for actual, available, default in states:
    assert (actual == "on") == (available == "yes" and default == "on"), "actual MetalRT violates automatic policy"
PY
then
  ui_fail 'actual MetalRT state did not match the automatic policy'
fi
for shot in $shots; do
  if [ ! -s "$UI_OUT/$shot.png" ]; then
    ui_fail "$shot: no rendered image"
  elif ! python3 "$UI_STATS" --shot "$UI_OUT/$shot.png" > "$UI_OUT/$shot.stats.json"; then
    ui_fail "$shot: image analysis failed"
  elif ! python3 - "$UI_OUT/$shot.stats.json" <<'PY'
import json, sys
with open(sys.argv[1]) as stream:
    stats = json.load(stream)
print(json.dumps({key: stats[key] for key in ("width", "height", "colors", "face_percent")}))
assert stats["width"] >= 300 and stats["height"] >= 200
assert stats["colors"] >= 8 and stats["face_percent"] >= 10
PY
  then
    ui_fail "$shot: image is too small, blank or missing the body"
  else
    ui_pass "$shot: rendered image contains the body"
  fi
done
if ! python3 - "$UI_STATS" "$UI_OUT/resized.png" "$UI_OUT/selected.png" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("image_stats", sys.argv[1])
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
before, after = (stats.read_png(path) for path in sys.argv[2:])
assert before[:3] == after[:3], "selection changed the framebuffer dimensions"
channels = before[2]
changed = sum(max(abs(x-y) for x, y in zip(a[:3], b[:3])) > 24
              for left, right in zip(before[3], after[3])
              for a, b in zip(zip(*[iter(left)] * channels), zip(*[iter(right)] * channels)))
assert changed > 32, "selection overlay did not change the actual rendered framebuffer"
print("Selection overlay changed %d rendered pixels" % changed)
PY
then
  ui_fail 'selection overlay was not visible over the rendered image'
else
  ui_pass 'actual face picking also drew its selection overlay'
fi
if ! python3 - "$UI_OUT/zoomed.stats.json" "$UI_OUT/resized.stats.json" <<'PY'
import json, sys
with open(sys.argv[1]) as stream:
    before = json.load(stream)
with open(sys.argv[2]) as stream:
    after = json.load(stream)
assert (before["width"], before["height"]) != (after["width"], after["height"]), "window resize did not change the rendered framebuffer"
PY
then
  ui_fail 'window resize did not produce a resized rendered image'
else
  ui_pass 'window resize produced a new framebuffer size'
fi
echo "Metal UI logs and screenshots: $UI_OUT"
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend Metal/OIDN preview, orbit, zoom, resize and shutdown"

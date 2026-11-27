#!/bin/bash
# SPDX-License-Identifier: MIT
# Narrow, opt-in physical-Mac measurement, not an additional CTest run.
# Usage: ui-macos-render-transport-measure.sh <app> <output directory> <cpu|metal>
# Optional MITCAD_TRANSPORT_WINDOW_WIDTH/HEIGHT choose the requested window;
# MIN_WIDTH/HEIGHT require a minimum actual pixel frame. Never changes displays.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo "usage: $0 <app> <output directory> <cpu|metal>" >&2
  exit 2
fi
device=$3
case "$device" in cpu|metal) ;; *) echo 'device must be cpu or metal' >&2; exit 2 ;; esac
ui_init "$1" "$2"
unset QT_QPA_PLATFORM QT_OPENGL LIBGL_ALWAYS_SOFTWARE GALLIUM_DRIVER
unset MITCAD_RENDER_TEST_DENOISER MITCAD_RENDER_TEST_COMPARE_DENOISER
unset MITCAD_RENDER_TEST_DENOISER_FAILURE MITCAD_RENDER_TEST_DEVICE_FAILURE
unset MITCAD_RENDER_REQUIRE_METAL MITCAD_RENDER_REQUIRE_GPU CYCLES_TEST_SINGLE_SAMPLE_UPDATES
unset MITCAD_RENDER_TEST_NATIVE_PASS_COMPARE MITCAD_RENDER_TEST_COMMAND_TRACE
window_width=${MITCAD_TRANSPORT_WINDOW_WIDTH:-1440}
window_height=${MITCAD_TRANSPORT_WINDOW_HEIGHT:-1020}
minimum_width=${MITCAD_TRANSPORT_MIN_WIDTH:-1}
minimum_height=${MITCAD_TRANSPORT_MIN_HEIGHT:-1}
for dimension in "$window_width" "$window_height" "$minimum_width" "$minimum_height"; do
  case "$dimension" in ''|*[!0-9]*) echo "frame/window dimensions must be positive integers" >&2; exit 2 ;; esac
  if [ "$dimension" -lt 1 ] || [ "$dimension" -gt 16384 ]; then echo "frame/window dimension out of range" >&2; exit 2; fi
done
if [ "$window_width" -lt 820 ] || [ "$window_height" -lt 520 ] ||
   [ "$window_width" -gt 10000 ] || [ "$window_height" -gt 10000 ]; then
  echo 'requested window must be 820..10000 by 520..10000 points' >&2; exit 2
fi
# Capture the real configuration; this harness neither selects a display mode
# nor labels an arbitrary Qt scale as physical Retina. Preserve the raw display
# report because requested window points and actual frame pixels can differ.
system_profiler SPDisplaysDataType -json > "$UI_OUT/displays.json"
sw_vers > "$UI_OUT/os-version.txt"
{
  printf "requested_window_points=%sx%s\n" "$window_width" "$window_height"
  printf "qt_scale_factor=%s\n" "${QT_SCALE_FACTOR:-unset}"
  printf "app=%s\n" "$UI_APP"
  git -C "$here/.." rev-parse HEAD
} > "$UI_OUT/measurement-context.txt"
if ! python3 - "$UI_APP" "${MITCAD_TRANSPORT_BUILD_MODE:-}" "$UI_OUT/measurement-context.txt" <<'CONTEXT'
import pathlib, re, sys
mode = sys.argv[2]
if not mode:
    for parent in pathlib.Path(sys.argv[1]).resolve().parents:
        cache = parent / "CMakeCache.txt"
        if cache.is_file():
            match = re.search(r"^CMAKE_BUILD_TYPE:STRING=(.+)$", cache.read_text(), re.M)
            if match:
                mode = match.group(1)
                break
if not mode:
    raise SystemExit("Set MITCAD_TRANSPORT_BUILD_MODE for a relocated bundle without its CMake cache")
with open(sys.argv[3], "a") as output:
    output.write("build_mode=" + mode + "\n")
CONTEXT
then exit 2; fi
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[render]\ndevice=%s\n' "$device" > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
script="$UI_OUT/transport.script"
rm -f "$UI_OUT/rendered.png"
cat > "$script" <<'DRIVER'
expect-ever 'View area'
window-resize ${MITCAD_TRANSPORT_WINDOW_WIDTH} ${MITCAD_TRANSPORT_WINDOW_HEIGHT}
command view.rendered
wait-log 'Render frame view [0-9]+ [0-9]+x[0-9]+ samples 16 denoised covers ' 240
screenshot-view '${MITCAD_TRANSPORT_SHOTS}/rendered.png'
command view.rendered
wait-log 'Render worker ended' 15
DRIVER
MITCAD_TEST_INPUT="$script" MITCAD_TRANSPORT_SHOTS="$UI_OUT" \
  MITCAD_TRANSPORT_WINDOW_WIDTH="$window_width" MITCAD_TRANSPORT_WINDOW_HEIGHT="$window_height" \
  MITCAD_RENDER_TEST_SCENE=1 MITCAD_RENDER_SAMPLES=16 MITCAD_RENDER_PREVIEWS=4,16 \
  MITCAD_LOG_TIMING=1 UI_TIMEOUT=300 \
  ui_run_app transport --ui=qt --no-recovery --demo
ui_expect_log 'TestDriver: done' 'synthetic transport driver completed'
for failure in 'Rendering on the CPU:' 'Render worker error' 'Render failed' 'ERROR:'; do
  ui_expect_no_log "$failure" "no $failure"
done
if [ "$device" = metal ]; then
  ui_expect_log 'Render device: .*METAL.*\(asked for metal\)' 'actual Metal device selected'
else
  ui_expect_log 'Render device: .*\(asked for cpu\)' 'CPU device requested'
  ui_expect_log 'Render denoising diagnostics: backend CPU,.*\(execution confirmed\)' 'actual CPU execution'
fi
ui_expect_log 'OpenGL renderer:.*Apple' 'hardware OpenGL renderer'
if ! python3 - "$UI_LOG_FILE" "$UI_OUT/rendered.png" "$UI_STATS" "$minimum_width" "$minimum_height" "$UI_OUT" <<'PY'
import json, re, subprocess, sys
text = open(sys.argv[1]).read()
stats = json.loads(subprocess.check_output([sys.executable, sys.argv[3], "--shot", sys.argv[2]]))
assert stats["width"] >= int(sys.argv[4]) and stats["height"] >= int(sys.argv[5]), "actual viewport is below requested minimum"
with open(sys.argv[6] + "/measurement-context.txt", "a") as context:
    context.write("actual_frame_pixels=%sx%s\n" % (stats["width"], stats["height"]))
    context.write("note=Compare only equal dimensions, display configuration and build mode; no baseline speedup inferred.\n")
assert stats["colors"] >= 8 and stats["face_percent"] >= 10, "rendered body is missing"
surface = "Render transport: IOSurface, private XPC session" in text
if surface:
    assert "Render frame transport: native auxiliary " in text, "auxiliary native path was not used"
if surface and "(asked for metal)" in text:
    assert "Render frame transport: Metal buffer interop" in text, "Metal used a CPU display-copy fallback"
stages = (("surface claim", "surface display", "surface draw", "diagnostics", "delivery") if surface else
          ("allocate", "copy", "convert", "texture prepare", "upload submit", "diagnostics", "delivery"))
for stage in stages:
    assert re.search(r"Timing render " + stage + r": [\d.]+ ms, view \d+ samples 16 "
                     + str(stats["width"]) + "x" + str(stats["height"]) + r" denoised 1", text), stage
if surface:
    for removed in ("copy", "convert", "texture prepare", "upload submit"):
        assert not re.search(r"Timing render " + removed + r":", text), "CPU frame stage remained: " + removed
print("Transport: " + ("private IOSurface/XPC" if surface else "POSIX shared-memory baseline"))
print("Synthetic scene, actual framebuffer: %s x %s pixels" % (stats["width"], stats["height"]))
PY
then
  ui_fail 'missing rendered image or final-frame timing stages'
fi
echo "Transport timing log: $UI_LOG_FILE"
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $device synthetic frame measurement"

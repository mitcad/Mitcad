#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer app/platform app/framework/TestDriver
# Physical-GPU window lifecycle and synthetic navigation acceptance.
# Usage: ui-macos-physical-test.sh <app> [output directory] [qt|native]
# MITCAD_UI_REQUIRE_HARDWARE=1 turns missing physical GPU into a failure.
# MITCAD_PHYSICAL_MEASURE_FRAMES=1 adds one opt-in, unthrottled orbit sample.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> [output directory] [qt|native]" >&2
  exit 2
fi
frontend=${3:-qt}
case "$frontend" in qt|native) ;; *) echo 'frontend must be qt or native' >&2; exit 2 ;; esac
ui_init "$1" "${2:-ui-macos-physical}"
worker=${MITCAD_RENDER_WORKER:-$(dirname "$UI_APP")/mitcad-render}
if ! "$worker" --list-devices > "$UI_OUT/devices.json" 2> "$UI_OUT/devices.err"; then
  cat "$UI_OUT/devices.err" >&2
  echo 'FAIL: cannot inspect hardware capability' >&2
  exit 1
fi
python3 - "$UI_OUT/devices.json" "${MITCAD_UI_REQUIRE_HARDWARE:-0}" <<'PY'
import json, sys
devices = json.load(open(sys.argv[1]))["devices"]
physical = [d for d in devices if d["type"] == "METAL" and
            not any(s in d["name"].lower() for s in ("paravirtual", "software"))]
if not physical:
    required = sys.argv[2] not in ("", "0")
    print(("FAIL: " if required else "SKIP: ") + "no physical Metal GPU")
    sys.exit(1 if required else 77)
print("Physical GPU capability: " + physical[0]["name"])
PY
capability=$?
[ "$capability" = 0 ] || exit "$capability"
for scale_var in QT_SCALE_FACTOR QT_SCREEN_SCALE_FACTORS QT_DEVICE_PIXEL_RATIO QT_ENABLE_HIGHDPI_SCALING; do
  if printenv "$scale_var" > /dev/null; then
    echo "FAIL: unset $scale_var; this check requires the display's actual scale" >&2
    exit 2
  fi
done
unset QT_QPA_PLATFORM QT_OPENGL LIBGL_ALWAYS_SOFTWARE GALLIUM_DRIVER
export MITCAD_TEST_WORK=$UI_OUT/files MITCAD_PHYSICAL_SHOTS=$UI_OUT
mkdir -p "$MITCAD_TEST_WORK"
UI_TIMEOUT=${UI_PHYSICAL_TIMEOUT:-180}

# AppKit frame restoration is separate from the isolated Qt settings root.
# Preserve this app's one autosaved frame, including its original absence.
if [ "$frontend" = native ]; then
  frame_domain=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$(dirname "$(dirname "$UI_APP")")/Info.plist") || exit 1
  frame_key='NSWindow Frame MitcadNativeDocumentWindow'
  frame_present=0
  frame_value=""
  if frame_value=$(defaults read "$frame_domain" "$frame_key" 2> /dev/null); then frame_present=1; fi
  physical_cleanup() {
    ui_cleanup
    if [ "$frame_present" = 1 ]; then defaults write "$frame_domain" "$frame_key" -string "$frame_value"
    else defaults delete "$frame_domain" "$frame_key" > /dev/null 2>&1 || true; fi
  }
  trap physical_cleanup EXIT
fi

# Every checkpoint reads the real backing framebuffer and makes a fresh
# OCCT face pick. Screenshots retain evidence for visual inspection.
physical_checkpoint() {
  cat >> "$script" <<'DRIVER'
key Escape
key F6
sleep 600
expect-viewport
click-visible-face
expect-selection 1 0
DRIVER
  if [ "$frontend" = native ]; then echo 'expect-native-layout' >> "$script"; fi
  printf 'screenshot-view '\''${MITCAD_PHYSICAL_SHOTS}/%s-%s.png'\''\n' "$1" "$scheme" >> "$script"
  if [ "$1" = windowed ]; then
    printf 'screenshot '\''${MITCAD_PHYSICAL_SHOTS}/windowed-ui-%s.png'\''\n' "$scheme" >> "$script"
    # QWidget grabs omit the platform glass windows; capture only this
    # process's actual on-screen windows as separate visual evidence.
    printf 'screenshot-screen '\''${MITCAD_PHYSICAL_SHOTS}/windowed-screen-%s.png'\''\n' "$scheme" >> "$script"
  fi
  cat >> "$script" <<'DRIVER'
key Escape
expect-selection 0 0
DRIVER
}

for scheme in light dark; do
  script=$UI_OUT/$scheme.mitcad-ui
  rm -f "$UI_OUT/initial-$scheme.png" "$UI_OUT/resized-$scheme.png" \
    "$UI_OUT/moved-$scheme.png" "$UI_OUT/restored-$scheme.png" \
    "$UI_OUT/fullscreen-$scheme.png" "$UI_OUT/windowed-$scheme.png" \
    "$UI_OUT/windowed-ui-$scheme.png" "$UI_OUT/windowed-screen-$scheme.png"
  cat > "$script" <<'DRIVER'
expect-ever 'View area'
window-state normal
expect-window-state normal
window-resize 1280 820
sleep 800
DRIVER
  if [ "$frontend" = native ]; then printf 'native-appearance %s\n' "$scheme" >> "$script"; fi
  physical_checkpoint initial
  echo 'window-resize 1000 650' >> "$script"
  physical_checkpoint resized
  echo 'window-move 35 45' >> "$script"
  physical_checkpoint moved
  # Repetition catches a second restoration leaving stale native children.
  for repeat in 1 2; do
    cat >> "$script" <<'DRIVER'
window-state minimized
expect-window-state minimized
sleep 400
window-state normal
expect-window-state normal
sleep 800
DRIVER
  done
  physical_checkpoint restored
  cat >> "$script" <<'DRIVER'
window-state fullscreen
expect-window-state fullscreen
sleep 1200
DRIVER
  physical_checkpoint fullscreen
  cat >> "$script" <<'DRIVER'
window-state normal
expect-window-state normal
sleep 1200
DRIVER
  physical_checkpoint windowed
  cat >> "$script" <<'DRIVER'
# Synthetic pixel scroll exercises the shared viewport handler; it does
# not establish physical trackpad latency, natural scrolling or momentum.
capture-view-target before_pan
capture-view-orientation pan_orientation
capture-view-height pan_height
trackpad-scroll 55 45 45 30 ''
expect-view-pan ${before_pan} changed
expect-view-orientation ${pan_orientation} unchanged
expect-view-zoom ${pan_height} unchanged
capture-view-orientation before_orbit
capture-view-height orbit_height
trackpad-scroll 55 45 45 30 alt
expect-view-orientation ${before_orbit} changed
expect-view-zoom ${orbit_height} unchanged
sleep 300
expect-view-buttons none
capture-view-height before_zoom
trackpad-scroll 55 45 0 30 shift
expect-view-zoom ${before_zoom} in
capture-view-height before_pinch
DRIVER
  if [ "$frontend" = native ]; then echo 'native-pinch 55 45 0.25' >> "$script"
  else echo 'qt-pinch 55 45 0.25' >> "$script"; fi
  cat >> "$script" <<'DRIVER'
expect-view-zoom ${before_pinch} in
expect-view-buttons none
expect-never 'OCCT view event failed|Native Mac canvas:|The document was read while a job computes it'
DRIVER
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$script \
    ui_run_app "$scheme" "--ui=$frontend" --no-recovery --no-mcp --demo
  ui_expect_log 'TestDriver: done' "$scheme: lifecycle, picking and synthetic navigation completed"
  ui_expect_log 'TestDriver: viewport logical .*; OpenGL ' "$scheme: actual backing size, refresh and GL version recorded"
  renderer=$(ui_log_value 'OpenGL renderer: *\(.*\)')
  if [ -z "$renderer" ] || printf '%s' "$renderer" | grep -Eiq 'software|llvmpipe|swiftshader|paravirtual'; then
    ui_fail "$scheme: physical GPU did not create the viewport context ($renderer)"
  elif [ -n "${MITCAD_UI_RENDERER:-}" ] && ! printf '%s' "$renderer" | grep -Eq -- "$MITCAD_UI_RENDERER"; then
    ui_fail "$scheme: unexpected OpenGL renderer: $renderer"
  fi
  if [ "$frontend" = native ]; then
    ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$scheme: AppKit frontend selected"
  else
    ui_expect_no_log 'Native UI: AppKit shell with embedded OCCT viewport' "$scheme: Qt frontend selected"
  fi
  for shot in initial resized moved restored fullscreen windowed; do
    if ! python3 "$UI_STATS" --shot "$UI_OUT/$shot-$scheme.png" > "$UI_OUT/$shot-$scheme.stats.json"; then
      ui_fail "$scheme/$shot: framebuffer image analysis failed"
    elif ! python3 - "$UI_OUT/$shot-$scheme.stats.json" "$UI_OUT/$shot-$scheme.png" "$UI_STATS" <<'PY'
import importlib.util, json, sys
s = json.load(open(sys.argv[1]))
assert s["width"] >= 300 and s["height"] >= 200, "framebuffer too small"
assert s["colors"] >= 8, "blank framebuffer"
spec = importlib.util.spec_from_file_location("image_stats", sys.argv[3])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
w, h, bpp, rows = module.read_png(sys.argv[2])
# Unlike face_percent's light-background metric, contrast works in both
# appearances. Exclude the orientation cube and sample the real body.
objects = samples = 0
cube = min(w, h) // 3
for y in range(8, h - 8, 8):
    row = rows[y]
    background = row[(w - 1) * bpp:(w - 1) * bpp + 3]
    for x in range(8, w - 8, 8):
        if x > w - cube and y < cube:
            continue
        samples += 1
        objects += max(abs(row[x * bpp + c] - background[c]) for c in range(3)) > 24
assert samples and objects / samples >= .05, "body missing from framebuffer"
PY
    then ui_fail "$scheme/$shot: blank framebuffer or missing body"; fi
  done
  [ "$UI_FAILURES" = 0 ] || exit 1
done

if [ "${MITCAD_PHYSICAL_MEASURE_FRAMES:-0}" = 1 ]; then
  # One pixel scroll every two 1 ms driver ticks avoids the old 20 ms input
  # cap. No mouse button is held, so this sample cannot open a context menu.
  # This is a synthetic submission/swap observation, never proof of panel
  # presentation rate or physical input-to-photon latency.
  script=$UI_OUT/orbit.mitcad-ui
  awk 'BEGIN {
    print "expect-ever '\''View area'\''"
    print "window-resize 1280 820"
    print "key F6"
    print "sleep 800"
    print "expect-viewport"
    print "capture-view-orientation before"
    for (i=1; i<=210; ++i) {
      print "trackpad-scroll 55 45 3 1 alt"
      if (i==30) print "set PHYSICAL_PHASE measured"
    }
    print "set PHYSICAL_PHASE finished"
    print "sleep 300"
    print "expect-view-orientation ${before} changed"
    print "expect-view-buttons none"
  }' > "$script"
  MITCAD_TEST_STEP_MS=1 MITCAD_TEST_SETTLE_MS=0 MITCAD_LOG_TIMING=1 \
    MITCAD_TEST_INPUT=$script ui_run_app orbit "--ui=$frontend" --no-recovery --no-mcp --demo
  ui_expect_log 'TestDriver: done' 'opt-in orbit completed'
  awk '/TestDriver:.*set PHYSICAL_PHASE measured/ { measured=1; next }
       /TestDriver:.*set PHYSICAL_PHASE finished/ { measured=0 }
       measured && /^Timing frame swap: / { print $4 }' "$UI_LOG_FILE" | sort -n > "$UI_OUT/orbit-swap-ms.txt"
  if ! awk '{ samples[NR]=$1; sum+=$1 } END {
    if (NR<10 || sum<=0) exit 1
    median=NR%2 ? samples[(NR+1)/2] : (samples[NR/2]+samples[NR/2+1])/2
    printf "swap_samples\tmedian_ms\tmean_ms\tobserved_swaps_per_second\n%d\t%.2f\t%.2f\t%.2f\n", NR, median, sum/NR, 1000*NR/sum
  }' "$UI_OUT/orbit-swap-ms.txt" > "$UI_OUT/orbit-summary.tsv"; then
    ui_fail 'opt-in orbit did not produce enough frame-swap evidence'
  fi
  cat "$UI_OUT/orbit-summary.tsv"
fi
echo "Physical Mac logs, framebuffer captures and metadata: $UI_OUT"
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend physical GPU window lifecycle and synthetic navigation"

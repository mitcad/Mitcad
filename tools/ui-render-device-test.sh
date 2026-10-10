#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render
# The render device (mitcad#50, docs/rendering.md "Devices") through the
# real UI, on a red block:
#   - View > Rendered renders on the automatic choice: a GPU when the
#     worker lists one (a build with build-cycles.sh --cuda on a machine
#     with an NVIDIA GPU), else the CPU; the log names the device and the
#     devices the worker found.
#   - Preferences > Display lists the devices; choosing the CPU starts the
#     worker again on it, which renders; the choice is saved in the user's
#     settings (render/device).
#   - A device that is not there (a choice of an earlier machine): the CPU
#     renders and the view says why; Preferences keeps the choice as "Not
#     found".
#   - A device that fails while it renders (the worker's test hook
#     MITCAD_RENDER_TEST_DEVICE_FAILURE): the CPU takes over, the view says
#     so, and the render finishes.
#
# Skipped (exit 0) when the build has no render worker (MITCAD_RENDER off).
# The app starts three times.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-device-test.sh
# check-all sources: tools/cli

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
if ! grep -qs '^MITCAD_RENDER:BOOL=ON' "$BUILD_DIR/CMakeCache.txt"; then
  echo "SKIP: $UI_APP is built without the render worker (MITCAD_RENDER=OFF)"
  exit 0
fi

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
WORKER=$(dirname "$UI_APP")/mitcad-render
WORK=$(mktemp -d /tmp/mitcad-ui-render-device.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_RENDER_SAMPLES=8

cat > "$WORK/block.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 60, "width": 40, "height": 20, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "paint_red"}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The devices as the worker lists them; the automatic choice is the first
# GPU (CUDA before HIP and the others, as RenderDevices.cpp orders them).
"$WORKER" --list-devices > "$WORK/devices.json" || ui_fail "mitcad-render --list-devices"
GPU=$(python3 -c '
import json, sys
devices = json.load(open(sys.argv[1]))["devices"]
for kind in ("OPTIX", "CUDA", "HIP", "METAL", "ONEAPI"):
    for d in devices:
        if d["type"] == kind:
            print("%s (%s)" % (d["name"], d["type"]))
            sys.exit(0)' "$WORK/devices.json")
CPU=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["devices"][0]["name"])' "$WORK/devices.json")
echo "     devices: ${GPU:-no GPU}; CPU $CPU"

# expect_rendered description: the last view sent rendered all its samples.
expect_rendered() {
  local view=""
  for _ in $(seq 1 300); do
    view=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Render view [0-9]*: first frame" | tail -1 | tr -dc 0-9)
    [ -n "$view" ] && grep -qE "Render view $view: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" && break
    ui_crashed && ui_fail "$1: the app crashed"
    sleep 0.2
  done
  [ -n "$view" ] && grep -qE "Render view $view: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" ||
    ui_fail "$1: no view rendered its $MITCAD_RENDER_SAMPLES samples"
  echo "ok   $1: $(grep -E "Render view $view: [0-9]+ samples in" "$UI_LOG" | tail -1 | sed 's/.*: //')"
}

echo "--- The automatic choice"
ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block is open"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render device: ${GPU:-$CPU} (asked for auto)" "the automatic choice renders on ${GPU:-the CPU}" 30
ui_expect_new "; devices: $CPU" "the worker lists its devices, the CPU first" 5
expect_rendered "it renders"

echo "--- Preferences: the CPU"
ui_mark
ui_step "preferences, Display (search)" ui_command "Preferences: Display"
ui_expect_new "Preferences: render devices auto, cpu" "Preferences lists the automatic choice, the CPU and the GPUs"
ui_expect_new "Preferences render device at " "the device's list is shown"
ui_focus_dialog '^Preferences$'
# The list: Automatic, the CPU, the GPUs.
ui_step "open the device list" ui_click_logged "Preferences render device"
ui_sync
ui_key Home Down Return
ui_sync
ui_step "OK" ui_key Return
ui_focus_main
ui_expect_new "Preferences: render device cpu" "the CPU is chosen"
ui_expect_new "Render device changed to cpu: the worker starts again" "the worker starts again"
ui_expect_new "Render device: $CPU (asked for cpu)" "it renders on the CPU" 30
expect_rendered "the CPU renders"
grep -A3 '^\[render\]' "$UI_CONFIG/Mitcad/Mitcad.conf" | grep -q '^device=cpu$' ||
  ui_fail "the choice is not saved in the settings"
echo "ok   the choice is saved (render/device=cpu)"
ui_stop_app

echo "--- A device that is not there"
python3 - "$UI_CONFIG/Mitcad/Mitcad.conf" << 'EOF'
import re, sys
path = sys.argv[1]
text = open(path).read()
text = re.sub(r"(?m)^device=cpu$", "device=CUDA_gone_0000:00:00", text)
open(path, "w").write(text)
EOF
ui_start_app --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block is open again"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new 'Render device fallback: There is no render device "CUDA_gone_0000:00:00"; rendering on the CPU' \
  "the worker says it renders on the CPU" 30
ui_expect_new 'View message: There is no render device "CUDA_gone_0000:00:00"' "the view says so"
ui_expect_new "Render device: $CPU (asked for CUDA_gone_0000:00:00)" "the CPU renders" 30
expect_rendered "the CPU renders instead"
ui_mark
ui_step "preferences, Display (search)" ui_command "Preferences: Display"
ui_expect_new "Preferences: render devices auto, cpu" "Preferences opens"
grep -F "Preferences: render devices auto, cpu" "$UI_LOG" | tail -1 | grep -qF ", CUDA_gone_0000:00:00" ||
  ui_fail "Preferences does not keep the missing device's choice"
echo "ok   Preferences keeps the missing device's choice"
ui_focus_dialog '^Preferences$'
ui_key Escape
ui_focus_main
ui_stop_app

echo "--- A device that fails while it renders"
python3 - "$UI_CONFIG/Mitcad/Mitcad.conf" << 'EOF'
import re, sys
path = sys.argv[1]
text = open(path).read()
text = re.sub(r"(?m)^device=.*$", "device=auto", text)
open(path, "w").write(text)
EOF
export MITCAD_RENDER_TEST_DEVICE_FAILURE="a test's device failure"
ui_start_app --open "$WORK/block.mitcad"
unset MITCAD_RENDER_TEST_DEVICE_FAILURE
ui_expect_log "Opened $WORK/block.mitcad" "the block is open a third time"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render device fallback: Rendering on the CPU: ${GPU:-$CPU} failed (a test's device failure)" \
  "the CPU takes over from the failed device" 30
ui_expect_new "View message: Rendering on the CPU: ${GPU:-$CPU} failed" "the view says so"
expect_rendered "the CPU renders the same scene"
ui_stop_app

ui_finish "the render device"

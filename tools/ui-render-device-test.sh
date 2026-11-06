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
#   - A render worker of one's own (mitcad#1332), picked in Preferences
#     (render/worker; here the shipped one under another name, as a user's
#     own OptiX worker is): it starts and renders, and Preferences shows it.
#     A picked worker of another protocol version gives way to the shipped
#     one, with a line in the log and a message in the status bar, and the
#     shipped one renders.
#
# The app starts five times.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-device-test.sh
# check-all sources: tools/cli

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
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

# pick_worker path: the render worker picked in Preferences (render/worker).
pick_worker() {
  python3 - "$UI_CONFIG/Mitcad/Mitcad.conf" "$1" << 'EOF'
import re, sys
path, worker = sys.argv[1:3]
text = open(path).read()
text = re.sub(r"(?m)^worker=.*\n", "", text)
text = re.sub(r"(?m)^\[render\]\n", "[render]\nworker=%s\n" % worker, text, count=1)
open(path, "w").write(text)
EOF
}

echo "--- A render worker of one's own"
# The shipped worker under another name, as a user's own OptiX worker is
# (mitcad-render-optix, docs/rendering.md "Building an OptiX worker
# yourself").
OWN=$WORK/mitcad-render-optix
printf '%s\n' '#!/bin/sh' "exec \"$WORKER\" \"\$@\"" > "$OWN"
chmod +x "$OWN"
pick_worker "$OWN"
ui_start_app --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block is open a fourth time"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "): $OWN" "the picked worker starts" 30
expect_rendered "the picked worker renders"
grep -q "Render worker fallback:" "$UI_LOG" && ui_fail "the picked worker gave way: $(grep "Render worker fallback:" "$UI_LOG")"
ui_mark
ui_step "preferences, Display (search)" ui_command "Preferences: Display"
ui_expect_new "Preferences: render worker shown $OWN" "Preferences shows the picked worker"
ui_expect_new "Preferences: render devices auto, cpu" "Preferences lists the picked worker's devices"
ui_focus_dialog '^Preferences$'
ui_key Escape
ui_focus_main
ui_stop_app

echo "--- A picked worker of another protocol version"
printf '%s\n' '#!/bin/sh' "echo '{\"event\":\"hello\",\"protocol\":1,\"version\":\"0.0.1\"}'" \
  'exec cat > /dev/null' > "$WORK/old-render"
chmod +x "$WORK/old-render"
pick_worker "$WORK/old-render"
ui_start_app --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block is open a fifth time"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render worker fallback: The render worker $WORK/old-render cannot be used (mitcad-render does not match" \
  "the log says the picked worker gives way to the shipped one" 30
ui_expect_new "Status warning: The render worker $WORK/old-render cannot be used" "the status bar says so"
expect_rendered "the shipped worker renders instead"
started=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Render worker started (pid" | tail -1)
case $started in
  */mitcad-render) echo "ok   the shipped worker started: ${started##*: }" ;;
  *) ui_fail "the last worker started is not the shipped one: $started" ;;
esac
grep -A3 '^\[render\]' "$UI_CONFIG/Mitcad/Mitcad.conf" | grep -qF "worker=$WORK/old-render" ||
  ui_fail "the picked worker is not kept in the settings"
echo "ok   the choice is kept (render/worker)"
ui_crashed && ui_fail "the app ended"
ui_stop_app

ui_finish "the render device"

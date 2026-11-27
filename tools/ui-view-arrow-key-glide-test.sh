#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp
# The arrow keys within one camera glide (mitcad#2527 item 12, mitcad#2555),
# the keyboard half of tools/ui-cube-arrow-glide-test.sh: with the turn
# step at 90 degrees (Preferences > Navigation), the orientation cube's
# front clicked and the Right arrow key pressed twice while the 0.4 s glide
# to Front still runs. Each key turns on from where the glide goes
# (OcctViewer::turnView settles the camera first), so the view ends on
# Back, not on a slanted view turned from the half-way camera.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-arrow-key-glide-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-view-arrow-key-glide.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# Where the orientation cube's faces can be clicked.
export MITCAD_LOG_ORIENTATION_CUBE=1
# The arrow keys' step (Preferences > Navigation > Arrow key step): a
# quarter turn, so that two of them from Front end on Back.
mkdir -p "$UI_CONFIG/Mitcad"
printf '\n[view]\nturnStep=90\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

cat > "$WORK/block.json" << 'EOF'
[{"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
  "corner": [0, 0], "length": 60, "width": 40, "height": 20, "operation": "new_body"}}]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_expect_log "Orientation cube front at" "the cube's places logged"

echo "--- The cube's front, then the Right arrow key twice within its glide"
ui_mark
read -r fx fy <<< "$(ui_logged_at "Orientation cube front")"
# The click gives the 3D view the keyboard and starts the glide from the
# opening view to Front; the keys follow 0.05 s and 0.13 s after it, well
# inside the 0.4 s glide.
ui_step "the cube's front, then Right twice" \
  xdotool mousemove "$fx" "$fy" click 1 sleep 0.05 key --delay 80 Right Right
turns=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF "View turned right by 90 degrees")
[ "$turns" -ge 2 ] || ui_fail "the Right key did not turn the view twice by 90 degrees ($turns turn(s) logged)"
echo "ok   both keys turned the view by the step"
# Back; from the half-way camera the turns ended on a slanted view.
back='Camera direction -?0 -1 -?0 up -?0 -?0 1 [(]'
last=""
for _ in $(seq 1 100); do
  last=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Camera direction " | tail -1)
  grep -qE "$back" <<< "$last" && break
  sleep 0.1
done
grep -qE "$back" <<< "$last" ||
  ui_fail "two Right keys within the glide to Front did not end on Back (last camera: '$last')"
echo "ok   two Right keys within the glide to Front end on Back"
ui_crashed && ui_fail "the application crashed"

ui_finish "UI arrow keys within a glide"

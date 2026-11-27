#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# The orientation cube's arrow pressed twice within one camera glide
# (mitcad#2527): from Front, the right arrow twice in quick succession turns
# on from where the first glide goes (Right), so the view ends on Back, not
# on Right again from the half-way camera (OcctViewer::turnCube settles the
# camera first, as the other relative moves do).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cube-arrow-glide-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cube-arrow-glide.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# Where the orientation cube's faces can be clicked.
export MITCAD_LOG_ORIENTATION_CUBE=1

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

ui_mark
ui_step "click the cube's front"           ui_click_logged "Orientation cube front"
ui_expect_new "Camera direction 0 1 0 up 0 0 1" "looking at the front" 8
ui_expect_new "Orientation cube arrow right at" "the arrows show face on"

echo "--- The right arrow twice within one glide"
ui_mark
read -r ax ay <<< "$(ui_logged_at "Orientation cube arrow right")"
# The second press 0.12 s after the first, well inside the 0.4 s glide; the
# arrows stay where they are until the camera rests.
ui_step "the right arrow, twice"           xdotool mousemove "$ax" "$ay" click 1 sleep 0.12 click 1
presses=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cx "Orientation cube arrow right")
[ "$presses" -ge 2 ] || ui_fail "the second press did not reach the arrow ($presses press(es) logged)"
echo "ok   both presses turned the view"
# Back; from the half-way camera the second turn ended on Right again.
ui_expect_new "Camera direction 0 -1 0 up 0 0 1" "two right arrows from Front end on Back" 8
ui_crashed && ui_fail "the application crashed"

ui_finish "UI orientation cube arrows within a glide"

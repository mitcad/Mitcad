#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Anaglyph 3D's picture (mitcad#1449, mitcad#1461): a one-off visual check,
# not a recurring test (its name is not ui-*-test.sh, so no check picks it
# up). The block of tools/ui-scripts/view-anaglyph.json in the home view,
# shaded, without and with View › Anaglyph 3D; it saves:
#   <dir>/anaglyph-off-view.png    the view's framebuffer (screenshot-view)
#   <dir>/anaglyph-on-view.png     the same with Anaglyph 3D on
#   <dir>/anaglyph-off-window.png  the window as the X server shows it (xwd)
#   <dir>/anaglyph-on-window.png   the same with Anaglyph 3D on
# and the app's logs as <dir>/look-off.log and <dir>/look-on.log.
#
# What to look for: with Anaglyph 3D on, two images of the block offset
# sideways, the left eye's in red and the right eye's in cyan (green and
# blue), so the block's outline and the grid lines get a red fringe on one
# side and a cyan one on the other; a grey surface stays grey where the two
# images overlap. Off, the normal shaded picture. Never a plain or black
# view.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-anaglyph-look.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-view-anaglyph-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT

"$cli" run "$here/ui-scripts/view-anaglyph.json" --save "$DIR/block.mitcad" || exit 1
ui_start_display

# shoot off|on: the app on the block, Anaglyph 3D off or on; the driver
# takes the view's framebuffer and stays, so that the window is then
# dumped as the X server shows it.
shoot() {
  local state=$1 script=$DIR/look-$1.mitcad-ui
  {
    echo '# SPDX-License-Identifier: MIT'
    echo 'wait-idle'
    echo 'command view.home'
    echo 'command view.fit'
    echo 'wait-idle'
    if [ "$state" = on ]; then
      echo 'mark'
      echo 'command view.anaglyph'
      echo "expect-log '^Anaglyph 3D on: stereo camera, red/cyan'"
      echo "wait-log '[(]stereo[)]\$' 30"
      # The stereo camera is a perspective one: fitted again, the block is
      # framed as off.
      echo 'command view.fit'
      echo 'wait-idle'
    fi
    echo 'sleep 1000'
    echo "screenshot-view $DIR/anaglyph-$state-view.png"
    echo 'stay'
  } > "$script"
  export MITCAD_TEST_WORK=$DIR MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$script
  ui_start_app --ui=qt --no-mcp --open "$DIR/block.mitcad"
  for _ in $(seq 1 600); do
    grep -q -e 'TestDriver: done' -e 'TestDriver: FAIL' "$UI_LOG" && break
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  ui_expect_log 'TestDriver: done' "the view's framebuffer, Anaglyph 3D $state"
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  xwd -id "$UI_WINDOW" -silent > "$DIR/window.xwd"
  python3 "$here/xwd2png.py" "$DIR/window.xwd" "$DIR/anaglyph-$state-window.png"
  echo "ok   $DIR/anaglyph-$state-view.png"
  echo "ok   $DIR/anaglyph-$state-window.png"
  cp -f "$UI_LOG" "$DIR/look-$state.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
}

shoot off
shoot on
ui_finish "Anaglyph 3D look pictures in $DIR"

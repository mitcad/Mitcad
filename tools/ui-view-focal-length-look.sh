#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The focal length's picture (mitcad#1717): a one-off visual check, not a
# recurring test (its name is not ui-*-test.sh, so no check picks it up).
# The block of tools/ui-scripts/view-anaglyph.json in the home view with a
# perspective camera, fitted at the default 29 mm, then Preferences ›
# Display › Focal length set to 20 and to 70 mm; it saves:
#   <dir>/focal-length-29-view.png    the view's framebuffer at 29 mm
#   <dir>/focal-length-20-view.png    the same at 20 mm
#   <dir>/focal-length-70-view.png    the same at 70 mm
#   <dir>/preferences-display.png     Preferences' Display page with the
#                                      Focal length row (at 29 and 50 mm)
# and the app's log as <dir>/look.log.
#
# What to look for: the block is as large in the middle of the view in all
# three (the camera moves back as the lens gets longer). At 20 mm its near
# corner and the grid run off strongly towards the edges, as with a wide
# lens; at 70 mm its sides are almost parallel, nearly as in an
# orthographic view; 29 mm is the picture of earlier versions. In the
# Preferences picture the Focal length row (two sliders with their mm
# values, the second labelled Anaglyph 3D) sits under the Camera row
# whole, within the page.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-focal-length-look.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-view-focal-length-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT

"$cli" run "$here/ui-scripts/view-anaglyph.json" --save "$DIR/block.mitcad" || exit 1
ui_start_display

script=$DIR/look.mitcad-ui
{
  echo '# SPDX-License-Identifier: MIT'
  echo 'wait-idle'
  echo 'mark'
  echo 'command view.perspective'
  echo "wait-log '[(]perspective[)]\$' 30"
  echo 'command view.home'
  echo 'command view.fit'
  echo 'wait-idle'
  echo 'sleep 1000'
  echo "screenshot-view $DIR/focal-length-29-view.png"
  # Preferences' Display page as it shows the row.
  echo 'command tools.preferences.display'
  echo "focus-dialog '^Preferences\$'"
  echo 'sleep 500'
  echo "screenshot-dialog $DIR/preferences-display.png"
  echo 'key Escape'
  echo "expect-dialog-gone '^Preferences\$'"
  echo 'focus-main'
  for length in 20 70; do
    echo 'mark'
    echo 'command tools.preferences.display'
    echo "focus-dialog '^Preferences\$'"
    echo 'focus-widget preferencesFocalLength'
    echo 'key ctrl+a'
    echo "type $length"
    echo "expect-log '^View: focal length $length mm, '"
    echo 'key Return'
    echo "expect-dialog-gone '^Preferences\$'"
    echo 'focus-main'
    echo "wait-log '^Camera field of view [0-9.]+ degrees, focal length $length[.]0 mm [(]perspective[)]' 30"
    echo 'wait-idle'
    echo 'sleep 1000'
    echo "screenshot-view $DIR/focal-length-$length-view.png"
  done
  echo "expect-never 'TestDriver: FAIL'"
} > "$script"

export MITCAD_TEST_WORK=$DIR MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$script
ui_start_app --ui=qt --no-mcp --open "$DIR/block.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'focal length look driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
cp -f "$UI_LOG" "$DIR/look.log"
ui_expect_log 'TestDriver: done' 'the focal length pictures'
grep -h '^Camera field of view\|^View: focal length\|^Preferences: focal length' "$DIR/look.log" || true
for name in focal-length-29-view focal-length-20-view focal-length-70-view preferences-display; do
  [ -s "$DIR/$name.png" ] && echo "ok   $DIR/$name.png" || ui_fail "$DIR/$name.png not saved"
done
ui_finish "Focal length look pictures in $DIR"

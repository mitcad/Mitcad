#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# macOS UI test: the counterpart of tools/ui-windows-test.ps1. Runs the
# application and checks its screenshots:
#   1. --demo: OpenGL renders (the log names the renderer, the Apple GPU
#      or Apple's software renderer), and the demo block is drawn with the
#      body colours on the light background; the image is not blank.
#   2. --open of a project file with a fillet: recomputed and drawn.
#   3. the same file with --set d3=35: the block is drawn taller.
#   4. the result store (P7d): the first open stores, the second restores.
# The app renders into an offscreen framebuffer and --screenshot reads it
# back, so nothing is clicked and no Accessibility or Screen Recording
# permission is needed; the window still opens, so a window server is
# needed (a logged-in user's session; over SSH into a Mac with a user
# logged in at the console it normally works too).
#
# Exit status: 0 passed, 1 failed, 2 wrong usage or environment, and 77
# (ctest's SKIP, SKIP_RETURN_CODE) when the session has no window server or
# the app could not create an OpenGL context: `launchctl managername` says
# "System" (a daemon), or the log of a failed run says so (ui_gui_unavailable
# in ui-macos-lib.sh). MITCAD_UI_REQUIRE_GUI=1 makes that a failure.
#
# Usage: tools/ui-macos-test.sh <app> [out directory]
#   <app>  .../mitcad.app/Contents/MacOS/mitcad
#   <out>  screenshots and logs (default: ui-macos in the current directory)
# Environment: UI_TIMEOUT seconds per run (120), MITCAD_UI_RENDERER (a
# regular expression the renderer must match, default any),
# MITCAD_UI_LLDB=1 (run under lldb --batch to see the stack of a crash).
# The app's settings, recovery folder and projects are in a private temporary
# directory, so the user's ~/Library is not touched; see ui-macos-lib.sh.

set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos}"

# A daemon's session (launchd's System domain) has no window server at all.
# An SSH login says Background and can still work: the run decides.
if command -v launchctl > /dev/null; then
  session=$(launchctl managername 2> /dev/null || true)
  echo "     session: ${session:-unknown}"
  if [ "$session" = System ] && [ "${MITCAD_UI_REQUIRE_GUI:-0}" != 1 ]; then
    echo "SKIP: no window server in the System session"
    exit 77
  fi
fi

echo '--- Demo block'
ui_run_app demo --demo --screenshot "$UI_OUT/demo.png"
renderer=$(ui_log_value 'OpenGL renderer: *\(.*\)')
if [ -z "$renderer" ]; then
  ui_fail 'the log has no "OpenGL renderer:" line: OpenGL did not initialise'
elif [ -n "${MITCAD_UI_RENDERER:-}" ] && ! printf '%s' "$renderer" | grep -Eq -- "$MITCAD_UI_RENDERER"; then
  ui_fail "OpenGL renderer is '$renderer', expected '$MITCAD_UI_RENDERER'"
else
  ui_pass "OpenGL renderer: $renderer"
fi
ui_expect_log 'Style: macos Chrome:' "Qt's native macOS style"
ui_expect_no_log 'OCCT could not' 'OCCT attached to the OpenGL context'
ui_check_shot demo

echo '--- Open a project file, then change a parameter'
file=$UI_OUT/block.mitcad
cat > "$file" << 'EOF'
{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0 }, { "name": "d2", "value": 40.0 },
    { "name": "d3", "value": 20.0 }, { "name": "d4", "value": 3.0 }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" } ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 0 },
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude1", "role": "side", "index": 2 },
      { "extrude": "Extrude1", "role": "side", "index": 3 } ], "radius": "d4" }
  ]
}
EOF
ui_run_app block20 --open "$file" --screenshot "$UI_OUT/block20.png"
ui_expect_log 'Opened .*block\.mitcad' 'opened with --open'
ui_expect_log 'Recomputed 3 feature\(s\)' 'sketch, extrude and fillet recomputed'
ui_expect_no_log 'Recompute failed' 'no recompute failed'
ui_check_shot block20
low_ok=$UI_SHOT_OK low_shades=$UI_SHOT_SHADES low_ratio=$UI_SHOT_RATIO

ui_run_app block35 --open "$file" --set d3=35 --screenshot "$UI_OUT/block35.png"
ui_expect_log 'Recomputed 2 feature\(s\)' 'd3=35 recomputed extrude and fillet'
ui_check_shot block35
if [ "$low_ok" = 1 ] && [ "$UI_SHOT_OK" = 1 ]; then
  message="side faces / top face $low_ratio -> $UI_SHOT_RATIO"
  if ui_gt "$UI_SHOT_RATIO" "$(awk -v r="$low_ratio" 'BEGIN { printf "%.4f", 1.3 * r }')"; then
    ui_pass "block drawn taller with d3=35: $message"
  else
    ui_fail "block not drawn taller with d3=35: $message"
  fi
fi

# The result store (P7d): the first open stores the extrusion and the fillet
# (however quick), the second takes them from the store.
results=$UI_OUT/results
rm -rf "$results"
MITCAD_RESULT_STORE=$results MITCAD_RESULT_STORE_MIN_MS=0 \
  ui_run_app store1 --open "$file" --screenshot "$UI_OUT/store1.png"
ui_expect_log 'Result store: restored 0, evaluated 3; stored 2 ' 'the first open stored two results'
MITCAD_RESULT_STORE=$results MITCAD_RESULT_STORE_MIN_MS=0 \
  ui_run_app store2 --open "$file" --screenshot "$UI_OUT/store2.png"
ui_expect_log 'Result store: restored 2, evaluated 1; stored 0 ' 'the second open took them from the store'
ui_expect_log 'Recomputed 1 feature\(s\)' 'only the sketch computed'
ui_check_shot store2
if [ "$low_ok" = 1 ] && [ "$UI_SHOT_OK" = 1 ] && [ "$UI_SHOT_SHADES" = "$low_shades" ]; then
  ui_pass 'the stored block is drawn as the computed one'
fi

echo "Screenshots and logs: $UI_OUT"
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: UI macOS test ($UI_FAILURES failures)"
  exit 1
fi
echo 'PASS: UI macOS test'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/GraphicsStart.cpp app/framework/GraphicsStart.hpp
# check-all sources: app/framework/DisplayPlatform.cpp app/framework/DisplayPlatform.hpp app/main.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/Snapshot.cpp tools/ui-image-stats.py
# check-all sources: tools/ui-scripts/gl-failure.json
# The window when OpenGL cannot draw it at start (mitcad#2292, GitHub
# mitcad/Mitcad#586 and #599): 0.70.0 showed users' Wayland desktops
# (NVIDIA's driver, XWayland) a black window, or none, as Qt composes a
# window holding the 3D view with OpenGL and its X11 window got no EGL
# surface. MITCAD_TEST_GL_FAILURE=start makes the start-up check's EGL
# contexts fail, as Qt's context or the window's EGL surface did there
# (framework/GraphicsStart.hpp). Two runs:
#   start       the first start logs each format it tried and that it
#               starts again on X11 with GLX, in the same process; the
#               second start logs the first one's reason, its window draws
#               with GLX as up to 0.69, and the 3D view draws in its own EGL
#               context, read back into the window: a box opened is drawn
#               on the screen and the view says nothing failed;
#   start-view  the same, with the view's own EGL context failing too: the
#               window still draws, nothing else is left to start on, and
#               the 3D view says once that it cannot start.
# Each run: the window's top band (menu bar and toolbar) is drawn on the
# screen, not black, and the app quits cleanly without a crash report.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-gl-start-failure-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
build=$(cd "$(dirname "$UI_APP")/.." && pwd)
cli=${UI_CLI:-$build/tools/cli/mitcad-cli}
stats_tool=$here/ui-image-stats.py
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-gl-start-failure.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_CRASH_DIR=$work/crashes
mkdir -p "$MITCAD_CRASH_DIR"
unset MITCAD_GRAPHICS_FALLBACK MITCAD_GRAPHICS_RESTARTS MITCAD_GRAPHICS_TRIED QT_XCB_GL_INTEGRATION QT_QPA_PLATFORM

# A 20 x 10 x 5 box for the 3D view to draw.
design=$work/box.mitcad
"$cli" run "$here/ui-scripts/gl-failure.json" --save "$design" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli could not make the design'; exit 1; }

glx='X11 with GLX, the 3D view through its own EGL context'

# stats x y width height: ui-image-stats.py's counts of a screen region as
# "pixels object dark".
stats() {
  local json
  xwd -root -silent > "$work/screen.xwd" || ui_fail 'no screen dump'
  json=$(python3 "$stats_tool" "$work/screen.xwd" "$1" "$2" "$3" "$4") || ui_fail 'no image statistics'
  python3 -c 'import json, sys; s = json.load(sys.stdin); print(s["pixels"], s["object"], s["dark"])' <<< "$json"
}

# first_start mode: the first start's lines, both formats failed and one
# start again on GLX in the same process.
first_start() {
  local forced="forced by MITCAD_TEST_GL_FAILURE=$1"
  grep -qxF "Graphics check: OpenGL 4.5 compatibility on xcb failed: $forced" "$UI_LOG" ||
    ui_fail "$1: the first start did not log the viewport format failing"
  grep -qxF "Graphics check: OpenGL 2.0 without a profile on xcb failed: $forced" "$UI_LOG" ||
    ui_fail "$1: the first start did not log the fallback format failing"
  [ "$(grep -c "^Graphics check: starting again on $glx\$" "$UI_LOG")" = 1 ] ||
    ui_fail "$1: the first start did not start again on X11 with GLX exactly once"
  kill -0 "$UI_RUNNER" 2> /dev/null || ui_fail "$1: the started process is gone"
  ui_expect_log "Graphics check: started again after: no OpenGL for the window on xcb (OpenGL 4.5 compatibility: $forced; OpenGL 2.0 without a profile: $forced); started again on $glx" \
    "$1: the second start logs why it runs on GLX"
  echo "ok   $1: the first start tried both formats and started again on X11 with GLX"
}

# top_band_drawn mode: the window's top band (the menu bar and the toolbar)
# has drawn content, where a window Qt cannot compose stays one colour.
top_band_drawn() {
  local pixels object dark
  ui_sync
  read -r pixels object dark <<< "$(stats "$X" "$Y" "$WIDTH" $((HEIGHT / 10)))"
  [ -n "$pixels" ] && [ "$pixels" -gt 0 ] || ui_fail "$1: no pixels in the window's top band"
  if [ $((object * 200)) -lt "$pixels" ] || [ $((dark * 100)) -ge $((pixels * 98)) ]; then
    ui_fail "$1: the window's top band is not drawn ($object of $pixels pixels drawn content, $dark dark)"
  fi
  echo "ok   $1: the window's top band is drawn ($object of $pixels pixels drawn content, $dark dark)"
}

# quit_cleanly mode
quit_cleanly() {
  local status
  ui_key ctrl+q
  for _ in $(seq 1 150); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: MitCad did not quit"
  wait "$UI_RUNNER"
  status=$?
  UI_RUNNER=""
  [ "$status" = 0 ] || ui_fail "$1: MitCad did not quit cleanly (status $status)"
  if compgen -G "$MITCAD_CRASH_DIR/*.crash" > /dev/null; then
    ls -l "$MITCAD_CRASH_DIR"
    ui_fail "$1: a crash report was written"
  fi
  echo "ok   $1: quit cleanly, no crash report"
}

ui_start_display

echo '--- start: X11 with GLX, the 3D view in its own EGL context'
export MITCAD_TEST_GL_FAILURE=start
ui_start_app --ui=qt --no-mcp --open "$design"
unset MITCAD_TEST_GL_FAILURE
first_start start
grep -Eq "^Graphics check: the window draws with OpenGL .+: GL .+ through GLX; the 3D view through its own EGL context: GL .+ through EGL .+\$" "$UI_LOG" ||
  ui_fail 'start: the second start did not check its window with GLX and the view'"'"'s own EGL context'
ui_expect_log '3D view: drawn in its own EGL context and read back into the window, as Qt' \
  'start: the view draws in its own EGL context'
ui_expect_log "Opened $design" 'start: the box opened'
ui_expect_log ': first frame ' 'start: the view drew its first frame'
ui_expect_log "drawing into the view's own EGL context" 'start: the composition line names the view'"'"'s own context'
grep -q '^3D view: no graphics: ' "$UI_LOG" && ui_fail 'start: the view logged a graphics failure'
grep -q '^Status error: The 3D view' "$UI_LOG" && ui_fail 'start: the view showed a graphics message'
top_band_drawn start
# The box on the screen: the view's middle has drawn content apart from its
# background, read back from the view's own context and composed with GLX.
read -r x0 y0 <<< "$(ui_view_at 30 30)"
read -r x1 y1 <<< "$(ui_view_at 70 70)"
read -r pixels object dark <<< "$(stats "$x0" "$y0" $((x1 - x0)) $((y1 - y0)))"
[ -n "$pixels" ] && [ "$pixels" -gt 0 ] || ui_fail 'start: no pixels in the view'"'"'s middle'
if [ $((object * 20)) -lt "$pixels" ] || [ $((dark * 100)) -ge $((pixels * 98)) ]; then
  ui_fail "start: the box is not drawn in the view ($object of $pixels pixels drawn content, $dark dark)"
fi
echo "ok   start: the box is drawn in the view ($object of $pixels pixels drawn content, $dark dark)"
quit_cleanly start

echo '--- start-view: the view'"'"'s own EGL context fails too'
export MITCAD_TEST_GL_FAILURE=start-view
ui_start_app --ui=qt --no-mcp
unset MITCAD_TEST_GL_FAILURE
first_start start-view
grep -Eq "^Graphics check: OpenGL 4.5 compatibility on xcb failed: the window draws \(GL .+ through GLX\), but the 3D view's own EGL context was not made: forced by MITCAD_TEST_GL_FAILURE=start-view\$" "$UI_LOG" ||
  ui_fail 'start-view: the second start did not log the view'"'"'s own context failing'
ui_expect_log 'Graphics check: no other platform to start on' 'start-view: nothing else to start on'
ui_expect_log "3D view: no graphics: Qt's OpenGL context is not an EGL context" 'start-view: the view logs why it cannot draw'
ui_expect_log 'Status error: The 3D view cannot start with this graphics driver.' 'start-view: the view says it cannot start'
[ "$(grep -c '^3D view: no graphics: ' "$UI_LOG")" = 1 ] || ui_fail 'start-view: the view logged its failure more than once'
[ "$(grep -c '^Status error: The 3D view cannot start' "$UI_LOG")" = 1 ] ||
  ui_fail 'start-view: the message was not shown exactly once'
grep -q '^Status error: The 3D view lost its connection' "$UI_LOG" &&
  ui_fail 'start-view: the view said it lost a connection it never had'
top_band_drawn start-view
quit_cleanly start-view

ui_finish 'UI window and 3D view drawn when OpenGL fails at start'

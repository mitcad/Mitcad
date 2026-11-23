#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/GraphicsStart.cpp app/framework/GraphicsStart.hpp
# check-all sources: app/framework/DisplayPlatform.cpp app/framework/DisplayPlatform.hpp app/main.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp tools/ui-image-stats.py
# The window when OpenGL cannot draw it at start (mitcad#2292, GitHub
# mitcad/Mitcad#586): 0.70.0 showed a user's X11 desktop a whole black
# window, panels and all, as Qt composes a window holding the 3D view with
# OpenGL. MITCAD_TEST_GL_FAILURE=start makes the start-up check's EGL
# contexts fail, as Qt's context or the window's EGL surface did there
# (framework/GraphicsStart.hpp):
#   - the first start logs each format it tried and that it starts again
#     on X11 with GLX, in the same process;
#   - the second start logs the first one's reason, its window draws with
#     GLX, and the 3D view (EGL) says once that it cannot start;
#   - the window's top band (menu bar and toolbar) is drawn on the screen,
#     not black;
#   - the app quits cleanly without a crash report.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-gl-start-failure-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
stats_tool=$here/ui-image-stats.py
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-gl-start-failure.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_CRASH_DIR=$work/crashes
mkdir -p "$MITCAD_CRASH_DIR"
unset MITCAD_GRAPHICS_FALLBACK MITCAD_GRAPHICS_RESTARTS QT_XCB_GL_INTEGRATION QT_QPA_PLATFORM

ui_start_display
export MITCAD_TEST_GL_FAILURE=start
ui_start_app --ui=qt --no-mcp
unset MITCAD_TEST_GL_FAILURE

# The first start: each format failed, then the start again.
forced='forced by MITCAD_TEST_GL_FAILURE=start'
grep -qxF "Graphics check: OpenGL 4.5 compatibility on xcb failed: $forced" "$UI_LOG" ||
  ui_fail 'the first start did not log the viewport format failing'
grep -qxF "Graphics check: OpenGL 2.0 without a profile on xcb failed: $forced" "$UI_LOG" ||
  ui_fail 'the first start did not log the fallback format failing'
[ "$(grep -c '^Graphics check: starting again on X11 with GLX' "$UI_LOG")" = 1 ] ||
  ui_fail 'the first start did not start again on X11 with GLX exactly once'
echo 'ok   the first start tried both formats and started again on X11 with GLX'
# The same process: the one the test started still runs the window.
kill -0 "$UI_RUNNER" 2> /dev/null || ui_fail 'the started process is gone'

# The second start: the reason, the window drawn with GLX, the view's message.
ui_expect_log "Graphics check: started again after: no OpenGL for the window on xcb (OpenGL 4.5 compatibility: $forced; OpenGL 2.0 without a profile: $forced); started again on X11 with GLX" \
  'the second start logs why it runs on GLX'
grep -Eq '^Graphics check: the window draws with OpenGL .+: GL .+ through GLX$' "$UI_LOG" ||
  ui_fail 'the second start did not check its window with GLX'
ui_expect_log "3D view: no graphics: Qt's OpenGL context is not an EGL context" 'the view logs why it cannot draw'
ui_expect_log 'Status error: The 3D view cannot start with this graphics driver.' 'the view says it cannot start'
[ "$(grep -c '^3D view: no graphics: ' "$UI_LOG")" = 1 ] || ui_fail 'the view logged its failure more than once'
[ "$(grep -c '^Status error: The 3D view cannot start' "$UI_LOG")" = 1 ] ||
  ui_fail 'the message was not shown exactly once'
grep -q '^Status error: The 3D view lost its connection' "$UI_LOG" &&
  ui_fail 'the view said it lost a connection it never had'

# The window drawn: its top band (the menu bar and the toolbar) has drawn
# content, where a window Qt cannot compose stays one color (black).
ui_sync
xwd -root -silent > "$work/window.xwd" || ui_fail 'no screen dump'
band=$((HEIGHT / 10))
stats=$(python3 "$stats_tool" "$work/window.xwd" "$X" "$Y" "$WIDTH" "$band") || ui_fail 'no image statistics'
pixels=$(sed -n 's/.*"pixels": \([0-9]*\).*/\1/p' <<< "$stats")
object=$(sed -n 's/.*"object": \([0-9]*\).*/\1/p' <<< "$stats")
dark=$(sed -n 's/.*"dark": \([0-9]*\).*/\1/p' <<< "$stats")
[ -n "$pixels" ] && [ "$pixels" -gt 0 ] || ui_fail "no pixels in the window's top band: $stats"
if [ $((object * 200)) -lt "$pixels" ] || [ $((dark * 100)) -ge $((pixels * 98)) ]; then
  ui_fail "the window's top band is not drawn: $stats"
fi
echo "ok   the window's top band is drawn ($object of $pixels pixels drawn content, $dark dark)"

ui_key ctrl+q
for _ in $(seq 1 150); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail 'MitCad did not quit'
wait "$UI_RUNNER"
status=$?
UI_RUNNER=""
[ "$status" = 0 ] || ui_fail "MitCad did not quit cleanly (status $status)"
if compgen -G "$MITCAD_CRASH_DIR/*.crash" > /dev/null; then
  ls -l "$MITCAD_CRASH_DIR"
  ui_fail 'a crash report was written'
fi
echo 'ok   quit cleanly, no crash report'
ui_finish 'UI window drawn when OpenGL fails at start'

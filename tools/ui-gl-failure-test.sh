#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/DocumentFrame.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverDocuments.cpp app/report tools/cli
# check-all sources: app/webbrowser/cef/CefView.cpp app/webbrowser/cef/CefView.hpp
# check-all sources: tools/ui-scripts/gl-failure.json tools/ui-scripts/gl-failure-init.mitcad-ui
# check-all sources: tools/ui-scripts/gl-failure-reconnect.mitcad-ui tools/ui-scripts/gl-failure-screen.mitcad-ui
# check-all sources: tools/ui-scripts/gl-failure-screen-web.mitcad-ui
# The 3D view without graphics (mitcad#2125): an attach to the graphics
# card that fails never ends MitCad. MITCAD_TEST_GL_FAILURE makes it fail
# in four runs of the shared input scripts (also run by
# tools/ui-windows-gl-failure-test.ps1):
#   init       the first attach sees a driver that answers no GL version
#              (tools/ui-scripts/gl-failure-init.mitcad-ui);
#   reconnect  the attach after the view moved to a new window does
#              (tools/ui-scripts/gl-failure-reconnect.mitcad-ui);
#   throw      that attach throws Standard_NullObject, as OCCT's
#              OpenGl_Context::Init did in the reported crash (the same
#              script);
#   screen     MITCAD_TEST_GL_FAILURE=reconnect, the attach after a screen
#              change (the test driver's screen-change) with Component from
#              Web's window open, its browser told of the change too and
#              then made to throw there
#              (tools/ui-scripts/gl-failure-screen-web.mitcad-ui), where the
#              build made the web browser component (<build>/browser-component,
#              or UI_WEB_BROWSER); without it the same screen changes
#              without the browser (tools/ui-scripts/gl-failure-screen.mitcad-ui).
# Each run: the message once, the failure logged once, the view drawn
# again once the script clears the switch (the test driver's unset-env),
# the design saved, the app quit with status 0 and no crash report in its
# crash folder.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-gl-failure-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
build=$(cd "$(dirname "$UI_APP")/.." && pwd)
cli=${UI_CLI:-$build/tools/cli/mitcad-cli}
web_browser=${UI_WEB_BROWSER:-$build/browser-component}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-gl-failure.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100

"$cli" run "$here/ui-scripts/gl-failure.json" --save "$work/design.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli could not make the design'; exit 1; }

# run name mode script reason: one run until the app exits.
run() {
  local name=$1 mode=$2 script=$3 status
  export MITCAD_TEST_WORK=$work/$name
  mkdir -p "$MITCAD_TEST_WORK/crashes"
  cp "$work/design.mitcad" "$MITCAD_TEST_WORK/first.mitcad"
  cp "$work/design.mitcad" "$MITCAD_TEST_WORK/second.mitcad"
  export MITCAD_CRASH_DIR=$MITCAD_TEST_WORK/crashes
  export MITCAD_TEST_INPUT=$here/ui-scripts/$script.mitcad-ui
  export UI_GL_REASON=$4
  export MITCAD_TEST_GL_FAILURE=$mode
  ui_start_app --ui=qt --no-mcp --chrome=floating --open "$MITCAD_TEST_WORK/first.mitcad"
  unset MITCAD_TEST_GL_FAILURE
  for _ in $(seq 1 1200); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$name: the driver timed out"; exit 1; }
  wait "$UI_RUNNER"
  status=$?
  UI_RUNNER=""
  if grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG"; then
    grep -E '^TestDriver: FAIL' "$UI_LOG"
    ui_fail "$name: a step failed"
    exit 1
  fi
  grep -Eq '^TestDriver: quit' "$UI_LOG" || { ui_fail "$name: the script did not reach its quit"; exit 1; }
  [ "$status" = 0 ] || { ui_fail "$name: the app did not quit cleanly (status $status)"; exit 1; }
  if compgen -G "$MITCAD_CRASH_DIR/*.crash" > /dev/null; then
    ls -l "$MITCAD_CRASH_DIR"
    ui_fail "$name: a crash report was written"
    exit 1
  fi
  [ "$(grep -c '^Status error: The 3D view lost its connection' "$UI_LOG")" = 1 ] ||
    { ui_fail "$name: the message was not shown exactly once"; exit 1; }
  [ "$(grep -c '^3D view: no graphics: ' "$UI_LOG")" = 1 ] ||
    { ui_fail "$name: the failure was not logged exactly once"; exit 1; }
  echo "ok   $name: told once, drawn again, saved, quit cleanly, no crash report"
}

ui_start_display
no_answer='the driver answers no GL version or vendor \(GL_VERSION null, GL_VENDOR null'
run init init gl-failure-init "$no_answer"
run reconnect reconnect gl-failure-reconnect "$no_answer"
run throw throw gl-failure-reconnect 'OCCT failed: Standard_NullObject: TCollection_ExtendedString : null parameter'
if [ -f "$web_browser/component.json" ] && compgen -G "$web_browser/mitcad-webbrowser.*" > /dev/null; then
  # Component from Web's window on the component the build made: no
  # download, nothing of the user's.
  export MITCAD_BROWSER_COMPONENT_DIR=$web_browser
  export MITCAD_BROWSER_COMPONENTS_DIR=$work/screen/components
  export MITCAD_WEB_PROFILE_DIR=$work/screen/profile
  export MITCAD_WEB_DOWNLOADS_DIR=$work/screen/downloads
  export LIBGL_ALWAYS_SOFTWARE=1
  unset MITCAD_WEB_GPU MITCAD_TEST_WEB_GPU_FAILURE
  run screen reconnect gl-failure-screen-web "$no_answer"
  [ "$(grep -c '^Component browser: a screen change failed: ' "$UI_LOG")" = 1 ] ||
    { ui_fail 'screen: the browser'"'"'s failure was not logged exactly once'; exit 1; }
  echo 'ok   screen: Component from Web'"'"'s window took the screen changes'
else
  echo "note screen: no web browser component in $web_browser; the screen changes without Component from Web's window"
  run screen reconnect gl-failure-screen "$no_answer"
fi
ui_finish 'UI 3D view without graphics'

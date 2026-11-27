#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser app/report
# WebGL in Insert > Component from Web's browser (mitcad#1861) on macOS: the
# page, the steps and the test's own HTTP server of
# tools/ui-component-web-webgl-test.py, the application run through
# ui-macos-lib.sh in the Aqua session. Two sessions with one browsing
# profile (mitcad#1906): a failure of the GPU process given to the first
# (MITCAD_TEST_WEB_GPU_FAILURE) leaves the marker the second follows (WebGL
# in software only), and both check the feedback diagnostics.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-component-web-webgl-test.sh <app> <output> <the built web browser component>' >&2
  exit 2
fi
ui_init "$1" "$2"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 "$here/ui-component-web-webgl-test.py" prepare "$MITCAD_TEST_WORK" || exit 1
rm -f "$MITCAD_TEST_WORK"/webgl-*.mitcad-ui "$MITCAD_TEST_WORK/server.log"
python3 "$here/ui-component-web-webgl-test.py" serve "$MITCAD_TEST_WORK" &
server=$!
# The second session's steps are written last.
for _ in $(seq 1 100); do
  [ -s "$MITCAD_TEST_WORK/webgl-2.mitcad-ui" ] && break
  sleep 0.1
done
[ -s "$MITCAD_TEST_WORK/webgl-2.mitcad-ui" ] || { kill "$server"; echo 'the test server did not start' >&2; exit 1; }
export MITCAD_WEB_DOWNLOADS_DIR=$MITCAD_TEST_WORK/downloads
export MITCAD_WEB_PROFILE_DIR=$MITCAD_TEST_WORK/profile
export MITCAD_BROWSER_COMPONENTS_DIR=$MITCAD_TEST_WORK/components
# The web browser component the build made (no download).
export MITCAD_BROWSER_COMPONENT_DIR=$3
# The GPU's choice is the profile marker's.
unset MITCAD_WEB_GPU MITCAD_TEST_WEB_GPU_FAILURE
UI_TIMEOUT=210
# 1. The GPU's failure given: the marker written, the window says so.
MITCAD_TEST_WEB_GPU_FAILURE='GPU process exited unexpectedly: exit_code=139' \
  MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/webgl-1.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app webgl-1 --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '^TestDriver: quit' 'session 1: the WebGL page went on to its result, the diagnostics checked'
ui_expect_no_log 'TestDriver: FAIL' 'session 1: the WebGL triangle drawn, the GPU failure noted'
# 2. The next start follows the marker: WebGL in software only.
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/webgl-2.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app webgl-2 --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '^TestDriver: quit' 'session 2: the WebGL page went on to its result in software only'
ui_expect_no_log 'TestDriver: FAIL' 'session 2: the WebGL triangle drawn in software'
touch "$MITCAD_TEST_WORK/stop"
wait "$server"
python3 "$here/ui-component-web-webgl-test.py" verify "$MITCAD_TEST_WORK" || exit 1
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Component from Web: WebGL drawn, the next start in software only after the GPU'"'"'s failure, the diagnostics carry the browser'

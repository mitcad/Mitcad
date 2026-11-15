#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser
# WebGL in Insert > Component from Web's browser (mitcad#1861) on macOS: the
# page, the steps and the test's own HTTP server of
# tools/ui-component-web-webgl-test.py, the application run through
# ui-macos-lib.sh in the Aqua session.
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
rm -f "$MITCAD_TEST_WORK/webgl.mitcad-ui" "$MITCAD_TEST_WORK/server.log"
python3 "$here/ui-component-web-webgl-test.py" serve "$MITCAD_TEST_WORK" &
server=$!
for _ in $(seq 1 100); do
  [ -s "$MITCAD_TEST_WORK/webgl.mitcad-ui" ] && break
  sleep 0.1
done
[ -s "$MITCAD_TEST_WORK/webgl.mitcad-ui" ] || { kill "$server"; echo 'the test server did not start' >&2; exit 1; }
export MITCAD_WEB_DOWNLOADS_DIR=$MITCAD_TEST_WORK/downloads
export MITCAD_WEB_PROFILE_DIR=$MITCAD_TEST_WORK/profile
export MITCAD_BROWSER_COMPONENTS_DIR=$MITCAD_TEST_WORK/components
# The web browser component the build made (no download).
export MITCAD_BROWSER_COMPONENT_DIR=$3
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/webgl.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app webgl --ui=qt --no-native-dialogs --no-recovery --no-mcp
touch "$MITCAD_TEST_WORK/stop"
wait "$server"
ui_expect_log '^TestDriver: quit' 'the WebGL page went on to its result'
ui_expect_no_log 'TestDriver: FAIL' 'the WebGL triangle was drawn and read back'
python3 "$here/ui-component-web-webgl-test.py" verify "$MITCAD_TEST_WORK" || exit 1
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Component from Web: a page'"'"'s WebGL context made, a triangle drawn and read back'

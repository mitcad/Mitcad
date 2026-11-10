#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser app/files/FileFormats.cpp core/ffi/src/web_downloads.rs core/model/src/document/library_parts.rs tools/cli
# Insert > Component from Web (mitcad#1579) on macOS: the steps and the
# test's own HTTP server of tools/ui-component-web-test.py, the application
# run through ui-macos-lib.sh in the Aqua session.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 4 ]; then
  echo 'usage: ui-macos-component-web-test.sh <app> <cli> <output> <the built web browser component>' >&2
  exit 2
fi
ui_init "$1" "$3"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 "$here/ui-component-web-test.py" prepare "$2" "$MITCAD_TEST_WORK" || exit 1
rm -f "$MITCAD_TEST_WORK/web.mitcad-ui" "$MITCAD_TEST_WORK/server.log"
python3 "$here/ui-component-web-test.py" serve "$MITCAD_TEST_WORK" &
server=$!
for _ in $(seq 1 100); do
  [ -s "$MITCAD_TEST_WORK/web.mitcad-ui" ] && break
  sleep 0.1
done
[ -s "$MITCAD_TEST_WORK/web.mitcad-ui" ] || { kill "$server"; echo 'the test server did not start' >&2; exit 1; }
export MITCAD_WEB_DOWNLOADS_DIR=$MITCAD_TEST_WORK/downloads
export MITCAD_WEB_PROFILE_DIR=$MITCAD_TEST_WORK/profile
export MITCAD_BROWSER_COMPONENTS_DIR=$MITCAD_TEST_WORK/components
# The web browser component the build made (no download).
export MITCAD_BROWSER_COMPONENT_DIR=$4
UI_TIMEOUT=300
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/web.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app web --ui=qt --no-native-dialogs --no-recovery --no-mcp
touch "$MITCAD_TEST_WORK/stop"
wait "$server"
ui_expect_log '^TestDriver: quit' 'the STEP, the zip and the PDF were handled'
ui_expect_no_log 'TestDriver: FAIL' 'all Component from Web assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
python3 "$here/ui-component-web-test.py" verify "$MITCAD_TEST_WORK" || exit 1
echo 'PASS: Component from Web: a STEP as a numbered component in Move, a zip'"'"'s picked file, a PDF refused'

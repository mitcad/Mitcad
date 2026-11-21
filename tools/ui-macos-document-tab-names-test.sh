#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/DocumentFrame.cpp app/DocumentFrame.hpp app/framework/TitleBar.cpp app/framework/TestDriverDocuments.cpp
# check-all sources: tools/cli tools/ui-scripts/document-tab-names.mitcad-ui tools/ui-scripts/document-tab-names.json
# The document tabs' names (mitcad#2060) in the Qt frontend on macOS, as
# tools/ui-document-tab-names-test.sh on Linux: the shared steps
# tools/ui-scripts/document-tab-names.mitcad-ui with designs named
# Bracket, Untitled and a 60-character name in one frame's tabs: the
# first two shown whole, shown or not, the long one elided.
# Usage: tools/ui-macos-document-tab-names-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-document-tab-names-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/document-tab-names.json" --save "$MITCAD_TEST_WORK/Bracket.mitcad" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run document-tab-names.json' >&2; exit 1; }
cp "$MITCAD_TEST_WORK/Bracket.mitcad" "$MITCAD_TEST_WORK/A design with a name far too long for its tab in the top bar.mitcad"
# The designs in one frame's tabs, as app.ui-macos-document-frames.
export MITCAD_TEST_DOCUMENT_FRAMES=1
MITCAD_TEST_INPUT=$here/ui-scripts/document-tab-names.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app document-tab-names --ui=qt --no-recovery --no-mcp --no-native-dialogs --chrome=floating \
  --open "$MITCAD_TEST_WORK/Bracket.mitcad"
ui_expect_log "^Document tabs shown: 'Bracket' 'A design[^']*' 'Untitled'$" 'Bracket, the elided long name and Untitled whole'
ui_expect_log '^TestDriver: done' 'the document tabs show short names whole and elide the long one'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt document tab names'

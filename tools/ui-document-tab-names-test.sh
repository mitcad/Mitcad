#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/DocumentFrame.cpp app/DocumentFrame.hpp app/framework/TitleBar.cpp app/framework/TestDriverDocuments.cpp
# check-all sources: tools/cli tools/ui-scripts/document-tab-names.mitcad-ui tools/ui-scripts/document-tab-names.json
# The document tabs' names in the Floating top bar (mitcad#2060) through
# the real UI: the shared input script
# tools/ui-scripts/document-tab-names.mitcad-ui (also run by
# tools/ui-windows-document-tab-names-test.ps1 and
# tools/ui-macos-document-tab-names-test.sh) with designs named Bracket,
# Untitled and a 60-character name: the first two shown whole, shown or
# not, the long one elided. The picture (document-tab-names.png) is kept
# in the test's folder (MITCAD_UI_LOG_DIR when check-all gives one).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-document-tab-names-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-document-tab-names.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
long='A design with a name far too long for its tab in the top bar'
"$cli" run "$here/ui-scripts/document-tab-names.json" --save "$work/Bracket.mitcad" > "$work/setup.log" 2>&1 ||
  { cat "$work/setup.log" >&2; exit 1; }
cp "$work/Bracket.mitcad" "$work/$long.mitcad"
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/document-tab-names.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-recovery --chrome=floating --open "$work/Bracket.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'document tab names driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log "Document tabs shown: 'Bracket' 'A design" 'Bracket shown whole beside the long name'
ui_expect_log 'TestDriver: done' 'the document tabs show short names whole and elide the long one'
pictures=${MITCAD_UI_LOG_DIR:-$root/build}
[ -s "$work/document-tab-names.png" ] && cp "$work/document-tab-names.png" "$pictures/document-tab-names.png"
ui_finish 'UI document tab names'

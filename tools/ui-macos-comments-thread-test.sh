#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentsWindow.cpp
# check-all sources: app/browser/BrowserController.cpp core/model/src/document core/model-api/src tools/cli
# check-all sources: tools/ui-scripts/comments-thread.mitcad-ui tools/ui-scripts/comments-empty.json
# A comment thread with an agent (mitcad#2010) in the Qt frontend on
# macOS, as the second part of tools/ui-comments-test.sh on Linux: the
# shared steps tools/ui-scripts/comments-thread.mitcad-ui on an empty
# design (tools/ui-scripts/comments-empty.json): Add Comment with Whole
# Design and the browser root row's Add Comment…, an agent's long reply
# shown in full in the Comments panel's details pane, the comment's row
# showing the whole thread, "Open, waiting for your answer" until the
# user answers, and Reply… showing the reply it answers.
# Usage: tools/ui-macos-comments-thread-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-comments-thread-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/empty.mitcad
"$cli" run "$here/ui-scripts/comments-empty.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run comments-empty.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/comments-thread.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app comments-thread --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Comment C1: open, user, Design: Plan a wall bracket' 'Add Comment with Whole Design in an empty design'
ui_expect_log '^Comments status C1: Open, waiting for your answer$' 'waiting for your answer after the agent'"'"'s question'
ui_expect_log '^Comments details: C1 reply 1: .*Tell me if any of that should change[.]$' 'the agent'"'"'s long reply in full'
ui_expect_log '^Comment dialog answers: Reply 1 by claude-code, ' 'Reply... shows the reply it answers'
ui_expect_log '^TestDriver: done' 'the comment thread'"'"'s steps'
ui_expect_no_log 'TestDriver: FAIL' 'no failed comment thread assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt comment thread'

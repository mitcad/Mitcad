#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentsWindow.cpp
# check-all sources: app/view/CommentImages.cpp app/view/CommentMarkup.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document core/model-api/src tools/cli
# check-all sources: tools/ui-scripts/comments-attachments.mitcad-ui tools/ui-scripts/comments-attachments-reopen.mitcad-ui
# Images on comments and replies (mitcad#2011) on macOS, the same shared
# scripts as the Linux and Windows tests'
# (tools/ui-scripts/comments-attachments.mitcad-ui on the block of
# comments-attachments.json, then comments-attachments-reopen.mitcad-ui on
# the saved design): Capture and Annotate… from Add Comment's panel (a
# rectangle over the view, a line, an arc and a rectangle, one undone,
# attached), Reply… with Attach Image…, the thumbnails and the viewer;
# opened again, the images are still there.
# Usage: tools/ui-macos-comments-attachments-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-comments-attachments}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/comments-attachments.mitcad
# The image file the reply attaches: a 128 x 128 PNG.
cp "$here/../app/branding/png/mitcad-128.png" "$UI_OUT/files/photo.png"
"$2" run "$here/ui-scripts/comments-attachments.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
for phase in comments-attachments comments-attachments-reopen; do
  MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/$phase.mitcad-ui \
    ui_run_app "$phase" --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
  ui_expect_log 'TestDriver: done' "$phase: the shared steps"
  ui_expect_no_log '^TestDriver: FAIL' "$phase: no failed driver assertion"
  if [ "$phase" = comments-attachments ]; then
    ui_expect_log '^Markup attached: capture[.]png ' 'the capture marked up and attached'
    ui_expect_log '^Comment C1 reply 1 image A2: photo[.]png, image/png, 128 x 128$' 'the reply with its image'
    ui_expect_log '^Comment image viewer opened: Comment 1: capture[.]png' 'the viewer opened from a thumbnail'
    if grep -q '"comment_attachments"' "$design" && grep -q '"name": "photo.png"' "$design"; then
      ui_pass 'saved: the images in comment_attachments'
    else
      ui_fail 'the saved design does not keep the images'
    fi
  else
    ui_expect_log '^Comments details images: C1: A1 capture[.]png, A2 photo[.]png$' 'opened again: both images'
  fi
  [ "$UI_FAILURES" = 0 ] || exit 1
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: images on comments and replies'

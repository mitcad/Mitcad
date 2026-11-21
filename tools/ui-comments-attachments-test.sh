#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentsWindow.cpp
# check-all sources: app/view/CommentImages.cpp app/view/CommentMarkup.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document core/model/src/file core/model-api/src tools/cli
# check-all sources: tools/ui-scripts/comments-attachments.mitcad-ui tools/ui-scripts/comments-attachments-reopen.mitcad-ui
# check-all sources: tools/ui-scripts/comments-attachments.json
# Images on comments and replies (mitcad#2011, GitHub mitcad/Mitcad#402)
# through the real UI, the shared input scripts
# tools/ui-scripts/comments-attachments.mitcad-ui and
# comments-attachments-reopen.mitcad-ui (also run by
# tools/ui-windows-comments-attachments-test.ps1 and
# tools/ui-macos-comments-attachments-test.sh) on a block mitcad-cli makes
# (tools/ui-scripts/comments-attachments.json):
#   - Add Comment with Whole Design and Capture and Annotate…: a rectangle
#     dragged over the view, a line, an arc and a rectangle drawn, one
#     undone, attached: C1 with its capture.
#   - Reply… with Attach Image… (a PNG file): the reply with its image.
#   - The thumbnails in the Comments panel; a click opens the viewer.
#   - Saved: the file keeps both images in comment_attachments; opened
#     again, they are still there.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-comments-attachments-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
command -v jq > /dev/null || { echo "jq is required"; exit 2; }
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-comments-attachments.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/comments-attachments.mitcad
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
# The image file the reply attaches: a 128 x 128 PNG.
cp "$root/app/branding/png/mitcad-128.png" "$work/photo.png"
"$cli" run "$here/ui-scripts/comments-attachments.json" --save "$design" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli'
}
ui_start_display

# run_phase <script>: one of the shared scripts on the design.
run_phase() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp --open "$design"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { tail -n 80 "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$1: the shared steps"
}

run_phase comments-attachments
ui_expect_log 'Markup attached: capture.png' 'the capture marked up and attached'
ui_expect_log 'Comment C1 reply 1 image A2: photo.png, image/png, 128 x 128' 'the reply with its image'
ui_expect_log 'Comment image viewer opened: Comment 1: capture.png' 'the viewer opened from a thumbnail'
# The length in parentheses: a pipe binds looser than the comma, and
# without them the paths after it would index the attachments' array.
actual=$(jq -c '[(.comment_attachments | length), .comment_attachments[0].comment, .comment_attachments[0].image.format,
  .comment_attachments[1].reply, .comment_attachments[1].name, (.comments[0] | has("attachments"))]' "$design") ||
  ui_fail "cannot read $design"
[ "$actual" = '[2,"C1","png",1,"photo.png",false]' ] || ui_fail "the saved images are $actual"
echo "ok   saved: the capture on C1 and the photo on its reply, apart from the comments"
run_phase comments-attachments-reopen
ui_expect_log 'Comments details images: C1: A1 capture.png, A2 photo.png' 'opened again: both images'
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish 'UI Comments attachments'

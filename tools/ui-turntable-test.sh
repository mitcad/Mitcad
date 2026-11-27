#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/Turntable.cpp app/view/TurntableDialog.cpp app/view/TurntableExport.cpp app/view/Snapshot.cpp app/render/ShownScene.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/turntable.mitcad-ui tools/ui-scripts/turntable.json
# Export Turntable (mitcad#2523) through the real UI: the shared input
# script tools/ui-scripts/turntable.mitcad-ui (also run by
# tools/ui-windows-turntable-test.ps1 and tools/ui-macos-turntable-test.sh)
# on a block that mitcad-cli makes (tools/ui-scripts/turntable.json): 36
# frames of a full turn about the design's Z, each camera direction 10
# degrees on from the one before, written as PNG files; with an FFmpeg
# program also an MP4 video, whose container the app reads back (36
# frames of 320 x 240) and which ffprobe decodes here (the video plays:
# 36 frames of 320 x 240). Without FFmpeg the frames only.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-turntable-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-turntable.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/turntable.mitcad-ui
ffmpeg=$(command -v ffmpeg || true)
if [ -n "$ffmpeg" ]; then
  export MITCAD_FFMPEG=$ffmpeg TURNTABLE_OUTPUT='MP4 video and PNG frames' TURNTABLE_PATH=$work/turn.mp4
else
  echo 'note: no ffmpeg on the PATH: the PNG frames only'
  export TURNTABLE_OUTPUT='PNG frames' TURNTABLE_PATH=$work/turn-frames
fi
design=$work/turntable.mitcad
"$cli" run "$here/ui-scripts/turntable.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'turntable driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Turntable frame 36 of 36: camera direction' 'the 36th frame, 10 degrees on from the 35th'
ui_expect_log 'turn-frames: 36 frames, 320 x 240' 'the 36 frames written'
if [ -n "$ffmpeg" ]; then
  ui_expect_log 'turn.mp4: 36 frames, 320 x 240' 'the video container: 36 frames of 320 x 240'
  probe=$(command -v ffprobe || true)
  if [ -n "$probe" ]; then
    info=$("$probe" -v error -select_streams v:0 -count_frames \
      -show_entries stream=nb_read_frames,width,height -of default=nw=1 "$work/turn.mp4")
    for want in width=320 height=240 nb_read_frames=36; do
      grep -qx -- "$want" <<< "$info" || ui_fail "the video plays: $want not in ffprobe's answer: $info"
    done
    echo 'ok   the video plays: ffprobe decodes 36 frames of 320 x 240'
  else
    echo 'note: no ffprobe: the video read by its container only'
  fi
fi
ui_expect_log 'TestDriver: done' 'Export Turntable (mitcad#2523): every step of the script'
ui_finish 'UI turntable'

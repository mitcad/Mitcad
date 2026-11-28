#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/Turntable.cpp app/view/TurntableDialog.cpp app/view/TurntableExport.cpp app/view/Snapshot.cpp app/render/ShownScene.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/turntable.mitcad-ui tools/ui-scripts/turntable.json
# Export Turntable (mitcad#2523): the shared input script
# tools/ui-scripts/turntable.mitcad-ui of tools/ui-turntable-test.sh in the
# Qt frontend. 36 frames of a full turn about the design's Z, each camera
# direction 10 degrees on from the one before, written as PNG files; with
# an FFmpeg program (on the PATH, or Homebrew's or MacPorts') also an MP4
# video, its container read back (36 frames of 320 x 240) and, with
# ffprobe, decoded here.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-turntable-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
# An empty folder each run (mitcad#2611): an earlier run's frames make Export
# ask to replace them, and the script waits for a line that never comes.
rm -rf -- "${MITCAD_TEST_WORK:?}"
mkdir -p "$MITCAD_TEST_WORK"
ffmpeg=$(command -v ffmpeg || true)
for folder in /opt/homebrew/bin /usr/local/bin /opt/local/bin; do
  if [ -z "$ffmpeg" ] && [ -x "$folder/ffmpeg" ]; then
    ffmpeg=$folder/ffmpeg
  fi
done
if [ -n "$ffmpeg" ]; then
  export MITCAD_FFMPEG=$ffmpeg TURNTABLE_OUTPUT='MP4 video and PNG frames'
  export TURNTABLE_PATH=$MITCAD_TEST_WORK/turn.mp4
else
  echo 'note: no ffmpeg: the PNG frames only'
  export TURNTABLE_OUTPUT='PNG frames' TURNTABLE_PATH=$MITCAD_TEST_WORK/turn-frames
fi
"$cli" run "$here/ui-scripts/turntable.json" --save "$MITCAD_TEST_WORK/turntable.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/turntable.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app turntable --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/turntable.mitcad"
ui_expect_log 'Turntable frame 36 of 36: camera direction .*, step 10[.]00 deg' 'the 36th frame, 10 degrees on'
ui_expect_log 'turn-frames: 36 frames, 320 x 240' 'the 36 frames written'
if [ -n "$ffmpeg" ]; then
  ui_expect_log 'turn[.]mp4: 36 frames, 320 x 240' 'the video container: 36 frames of 320 x 240'
  probe=$(dirname "$ffmpeg")/ffprobe
  if [ -x "$probe" ]; then
    info=$("$probe" -v error -select_streams v:0 -count_frames \
      -show_entries stream=nb_read_frames,width,height -of default=nw=1 "$MITCAD_TEST_WORK/turn.mp4")
    for want in width=320 height=240 nb_read_frames=36; do
      printf '%s\n' "$info" | grep -qx -- "$want" || ui_fail "the video plays: $want not in ffprobe's answer: $info"
    done
  fi
fi
ui_expect_log '^TestDriver: done' 'Export Turntable (mitcad#2523): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Export Turntable (mitcad#2523)'

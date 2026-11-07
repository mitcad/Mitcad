#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowImageReferences.cpp app/view app/platform/macos/HeifImageIO.mm app/platform/PlatformHeif.hpp tools/heif-test-photo.sh tools/cli
# HEIF image references through macOS' own decoder, ImageIO (mitcad#1304),
# in the default (Qt) frontend, on a 60 x 60 x 3 plate:
#   1. tools/heif-test-photo.sh's HEIF file (64 x 32 pixels, turned a
#      quarter by its irot) inserted 32 mm wide on the plate's top: decoded
#      upright (32 x 64 pixels) when it is chosen, no message, drawn
#      32 x 64 mm; saved (tools/ui-scripts/image-references-heif.mitcad-ui).
#   2. The saved design embeds it as a PNG file of 32 x 64 pixels.
#   3. Opened again with the HEIF file deleted and no HEIF decoder
#      (MITCAD_TEST_HEIF_DECODER=none): drawn from the design's copy, and
#      nothing in the log speaks of HEIF
#      (tools/ui-scripts/image-references-heif-reopen.mitcad-ui).
# Usage: tools/ui-macos-image-references-heif-test.sh <app> <cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
. "$here/heif-test-photo.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-image-references-heif-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"

# A 60 x 60 x 3 plate (Box1 F1, Body1).
cat > "$MITCAD_TEST_WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 60, "height": 3, "operation": "new_body"}}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/model.json" --save "$MITCAD_TEST_WORK/plate.mitcad" > "$UI_OUT/cli.log" 2>&1 || {
  cat "$UI_OUT/cli.log"
  exit 1
}
heif_test_photo "$MITCAD_TEST_WORK/photo.heic" || {
  echo 'FAIL: the HEIF file is not 467 bytes'
  exit 1
}

MITCAD_TEST_INPUT=$here/ui-scripts/image-references-heif.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app heif-insert --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plate.mitcad"
ui_expect_log '^TestDriver: done' 'a HEIF file inserted through ImageIO, upright, and saved'
ui_expect_no_log 'TestDriver: FAIL' 'no failed HEIF insert assertion'
[ "$UI_FAILURES" = 0 ] || exit 1

# The design's copy (its image_references' `image`, the file pretty-printed):
# a PNG file of the upright image.
design=$(tr -d ' \r\n' < "$MITCAD_TEST_WORK/plate.mitcad")
if grep -q '"format":"png","width":32,"height":64' <<< "$design"; then
  ui_pass 'embedded as a PNG file of 32 x 64 pixels'
else
  ui_fail "the design does not embed a PNG file of 32 x 64 pixels: $(grep -o '"format":"[a-z]*","width":[0-9]*,"height":[0-9]*' <<< "$design")"
fi
[ "$UI_FAILURES" = 0 ] || exit 1

rm "$MITCAD_TEST_WORK/photo.heic"
MITCAD_TEST_HEIF_DECODER=none MITCAD_TEST_INPUT=$here/ui-scripts/image-references-heif-reopen.mitcad-ui \
  MITCAD_TEST_STEP_MS=100 ui_run_app heif-reopen --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plate.mitcad"
ui_expect_log '^TestDriver: done' 'opened again with no HEIF decoder: drawn from the PNG copy'
ui_expect_no_log 'TestDriver: FAIL' 'no failed reopen assertion'
# The app's own words (the test driver's echo of its steps and the files'
# paths left out).
heif_lines=$(grep -E 'HEIF|HEIC' "$UI_LOG_FILE" | grep -v 'TestDriver' || true)
if [ -n "$heif_lines" ]; then
  ui_fail "the log speaks of HEIF with no HEIF file used: $(head -n 3 <<< "$heif_lines")"
else
  ui_pass 'nothing about HEIF in the log with no HEIF file used'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: HEIF image references through ImageIO'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowImageReferences.cpp app/OcctViewer.cpp app/view app/browser app/framework tools/cli app/platform/unix app/platform/PlatformHeif.hpp tools/heif-test-photo.sh
# check-all sources: core/model/src/document core/model/src/file core/model-api/src core/model-base/src
# Image references (mitcad#782) through the real UI, in the Docked layout,
# on a 60 x 60 x 3 plate and a 200 x 100 pixel grey BMP file that states
# 4000 pixels per metre (0.25 mm each):
#   - Insert > Image Reference (command search): the plate's top face
#     picked, the file's path typed, Opacity 40: Image1 drawn 50 x 25 mm,
#     40 % opaque, behind the bodies, listed in the browser's Images
#     folder.
#   - Calibrate Image Reference: two clicks on the image's middle row, a
#     tenth in from its left and right edges (160 pixels apart), and 80
#     typed as their distance: the image twice as large (100 x 50 mm).
#   - Its row double-clicked: the panel edits it; Opacity 70 and Through
#     Bodies: drawn over the bodies, in the view's image layer before the
#     sketch layers (a sketch traced on it stays on top).
#   - Its light bulb off: not drawn, and Calibrate Image Reference refused
#     ("Show an image reference first."); saved, opened again, its light bulb on:
#     drawn as it was (the image embedded in the design).
#   - Nothing in the log speaks of HEIF while no HEIF file is used
#     (mitcad#1304: no decoder looked for at start, no notice).
#   - A HEIF file (mitcad#850, tools/heif-test-photo.sh: 64 x 32 pixels
#     turned a quarter by its irot property), Width 32, with a decoder (a
#     build with MITCAD_HEIF, or the distribution's libheif with its HEVC
#     plugin): Image2 decoded upright (32 x 64 pixels) and drawn
#     32 x 64 mm; saved, the file deleted, opened again with no HEIF
#     decoder at all (MITCAD_TEST_HEIF_DECODER=none): drawn from the
#     design's copy, a PNG file. A libheif without the plugin: the message.
#   - With no libheif (MITCAD_TEST_HEIF_LIBRARY, a path that is not there)
#     the panel gives the one message, naming libheif and its HEVC plugin,
#     and nothing is inserted.
# tools/ui-windows-image-references-test.ps1 does the same in the Windows
# VM (with WIC's decoder missing), tools/ui-macos-image-references-heif-test.sh
# the HEIF part through ImageIO on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-image-references-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-image-references.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# A 60 x 60 x 3 plate (Box1 F1, Body1).
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 60, "height": 3, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/plate.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli model"; }

# The BMP file: a 54-byte header (200 x 100, 24 bits, 4000 pixels per
# metre), then 100 rows of 600 grey bytes.
{
  printf 'BM\x96\xea\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00'
  printf '\x28\x00\x00\x00\xc8\x00\x00\x00\x64\x00\x00\x00\x01\x00\x18\x00'
  printf '\x00\x00\x00\x00\x60\xea\x00\x00\xa0\x0f\x00\x00\xa0\x0f\x00\x00'
  printf '\x00\x00\x00\x00\x00\x00\x00\x00'
  head -c 60000 /dev/zero | tr '\0' '\200'
} > "$WORK/plan.bmp"
[ "$(wc -c < "$WORK/plan.bmp")" = 60054 ] || ui_fail "the BMP file is not 60054 bytes"

# The HEIF file of tools/heif-test-fixture.py (quadrants-8-rot1.heic): an
# HEVC image of 64 x 32 pixels (flat red, green, blue and white quarters)
# with an irot of a quarter turn anticlockwise.
source "$(dirname "$0")/heif-test-photo.sh"
heif_test_photo "$WORK/photo.heic" || ui_fail "the HEIF file is not 467 bytes"

# expect_no_heif "when": nothing in the run's log speaks of HEIF (the app's
# words, in upper case; paths are not): no decoder is looked for, no notice
# given, while no HEIF file is used (mitcad#1304).
expect_no_heif() {
  if grep -Eq 'HEIF|HEIC' "$UI_LOG"; then
    ui_fail "the log speaks of HEIF $1: $(grep -E 'HEIF|HEIC' "$UI_LOG" | head -n 3)"
  fi
  echo "ok   nothing about HEIF in the log $1"
}

# expect_size "name" width: the image's width in the last "Image references
# drawn" line is within 2 % of the width expected (clicks land on whole
# pixels of the screen).
expect_size() {
  local width=""
  for _ in $(seq 1 30); do
    width=$(grep "^Image references drawn: .*$1 " "$UI_LOG" | tail -1 | sed -n "s/.*$1 \([0-9.]*\) x .*/\1/p")
    [ -n "$width" ] && awk -v w="$width" -v e="$2" 'BEGIN { exit !((w - e) ^ 2 < (0.02 * e) ^ 2) }' && break
    sleep 0.2
  done
  [ -n "$width" ] || ui_fail "$1 is not drawn"
  awk -v w="$width" -v e="$2" 'BEGIN { exit !((w - e) ^ 2 < (0.02 * e) ^ 2) }' ||
    ui_fail "$1 is drawn $width mm wide, not $2 mm"
  echo "ok   $1 drawn $width mm wide"
}

# expect_drawn "pattern" description: a line "Image references drawn: …"
# since the mark matches the extended regular expression.
expect_drawn() {
  for _ in $(seq 1 30); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Image references drawn: $1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no 'Image references drawn: $1' since the mark"
}

ui_start_display
ui_start_app --open "$WORK/plate.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Insert > Image Reference on the plate's top"
ui_mark
ui_step "Image Reference"                  ui_command "Image Reference"
ui_expect_new "Command Image Reference started" "the panel opened"
ui_step "click the plate's top"            ui_click_pick "face F1.b0/F1:top"
ui_expect_log "Image Reference Plane: 1 face" "the face picked"
ui_step "the image's file"                 ui_type_in "Panel Image Reference input file" "$WORK/plan.bmp"
ui_step "opacity 40"                       ui_type_in "Panel Image Reference input opacity" "40"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Inserted image reference from plan.bmp" "inserted"
ui_expect_new "Image references drawn: Image1 50.0 x 25.0 mm 40% behind" "drawn 50 x 25 mm, 40 % opaque, behind"
ui_expect_log "Browser Root/Images/Image1 at" "listed in the Images folder"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Calibrate: 160 pixels are 80 mm"
ui_mark
ui_step "Calibrate Image Reference"        ui_command "Calibrate Image Reference"
ui_expect_new "Calibrate Image1 started" "calibration started"
ui_expect_new "Calibrate Image1 right at" "the image's places logged"
ui_step "the first point"                  ui_click_logged "Calibrate Image1 left"
ui_expect_new "Calibrate Image1 first point" "the first point taken"
ui_step "the second point"                 ui_click_logged "Calibrate Image1 right"
ui_expect_new "Calibrate Image1 second point" "the second point taken"
ui_focus_dialog "$(ui_title_regex "Calibrate Image1")"
ui_step "80 mm"                            ui_type_text "80"
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Calibrated Image1: " "calibrated"
expect_size Image1 100

echo "--- Edited in its panel: 70 % opaque, through the bodies"
ui_mark
ui_double_click_expect "Browser Root/Images/Image1" "Editing image reference Image1 with Image Reference" \
  "the edit panel opened"
ui_step "opacity 70"                       ui_type_in "Panel Image Reference input opacity" "70"
ui_step "Through Bodies"                   ui_click_logged "Panel Image Reference input through"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited image reference Image1" "edited"
expect_drawn "Image1 .* 70% through" "drawn 70 % opaque, over the bodies"
ui_expect_new "Image references through the bodies: in the image layer, before the sketch halos" \
  "drawn under the sketches"
expect_size Image1 100

echo "--- Hidden, saved, opened again and shown"
ui_mark
ui_step "Image1's light bulb off"          ui_click_logged "Browser eye Root/Images/Image1"
ui_expect_new "Visibility Root/Images/Image1: hidden" "hidden"
ui_expect_new "Image references drawn: none" "not drawn"
ui_step "Calibrate Image Reference"        ui_command "Calibrate Image Reference"
ui_expect_new "Show an image reference first." "a hidden image is not calibrated"
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $WORK/plate.mitcad" "saved"
expect_no_heif "with no HEIF file used"
ui_stop_app
ui_start_app --open "$WORK/plate.mitcad"
ui_expect_log "Browser eye Root/Images/Image1 at" "Image1 read back"
ui_mark
ui_step "Image1's light bulb on"           ui_click_logged "Browser eye Root/Images/Image1"
ui_expect_new "Visibility Root/Images/Image1: shown" "shown"
expect_drawn "Image1 .* 70% through" "drawn as it was saved"
expect_size Image1 100

expect_no_heif "with no HEIF file used"

# HEIF (mitcad#1304): a build with MITCAD_HEIF decodes with the libheif it
# links (the build's CMake cache says); others with the distribution's
# libheif, loaded when the file is chosen, which decodes only with its HEVC
# plugin: with a libheif.so.1 installed the file is either inserted or
# refused with the message, and without one there is nothing to decode with.
HEIF_BUILT=0
grep -q '^MITCAD_HEIF:BOOL=ON$' "$(dirname "$UI_APP")/../CMakeCache.txt" 2> /dev/null && HEIF_BUILT=1
HEIF_SYSTEM=0
LIBRARIES=$( (ldconfig -p || /sbin/ldconfig -p) 2> /dev/null || true)
grep -q 'libheif\.so\.1 ' <<< "$LIBRARIES" && HEIF_SYSTEM=1
HEIF_MESSAGE="Reading HEIF and HEIC images needs your distribution's libheif with its HEVC plugin"

# insert_heif: Image Reference on the plate's top with photo.heic and
# Width 32; sets HEIF_INSERTED to 1 when it was inserted, 0 when the panel
# gave the message (Esc then cancels the panel).
insert_heif() {
  ui_mark
  ui_step "Image Reference"                  ui_command "Image Reference"
  ui_expect_new "Command Image Reference started" "the panel opened"
  ui_step "click the plate's top"            ui_click_pick "face F1.b0/F1:top"
  ui_expect_new "Image Reference Plane: 1 face" "the face picked"
  ui_step "the HEIF file"                    ui_type_in "Panel Image Reference input file" "$WORK/photo.heic"
  ui_step "width 32"                         ui_type_in "Panel Image Reference input width" "32"
  HEIF_INSERTED=""
  for _ in $(seq 1 50); do
    if grep -qF "Panel Image Reference error: $HEIF_MESSAGE" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG"); then
      HEIF_INSERTED=0
      ui_step "cancel (Esc)"                 ui_key Escape
      ui_expect_new "Command Image Reference cancelled" "cancelled"
      return
    fi
    if grep -qF "Image Reference HEIF file photo.heic decoded" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG"); then
      ui_step "OK (Enter)"                   ui_key Return
      ui_expect_new "Inserted image reference from photo.heic" "inserted"
      HEIF_INSERTED=1
      return
    fi
    sleep 0.2
  done
  ui_fail "photo.heic was neither decoded nor refused with the message"
}

if [ "$HEIF_BUILT" = 1 ] || [ "$HEIF_SYSTEM" = 1 ]; then
  echo "--- A HEIF file: decoded upright, embedded as PNG, read back with no decoder and without the file"
  insert_heif
  if [ "$HEIF_INSERTED" = 1 ]; then
    ui_expect_new "Image reference Image2 decoded: 32 x 64 pixels" "decoded upright: the quarter turn applied"
    expect_drawn "Image1 .* 70% through, Image2 32.0 x 64.0 mm 50% behind" "drawn 32 x 64 mm"
    grep -q "Image reference Image2 cannot be drawn" "$UI_LOG" && ui_fail "the HEIF image was not decoded"
    ui_step "save (Ctrl+S)"                  ui_key ctrl+s
    ui_expect_new "Saved $WORK/plate.mitcad" "saved"
    ui_stop_app
    rm "$WORK/photo.heic"
    # The design's copy is a PNG file: no HEIF decoder needed to open it.
    export MITCAD_TEST_HEIF_DECODER=none
    ui_start_app --open "$WORK/plate.mitcad"
    ui_expect_log "Image reference Image2 decoded: 32 x 64 pixels" "decoded from the design's copy, with no decoder"
    # The new run's log from its start.
    UI_MARK=0
    expect_drawn "Image1 .* 70% through, Image2 32.0 x 64.0 mm 50% behind" "drawn as it was saved"
    expect_no_heif "opened with no HEIF decoder"
    unset MITCAD_TEST_HEIF_DECODER
    heif_test_photo "$WORK/photo.heic" || ui_fail "the HEIF file is not 467 bytes"
  else
    echo "--- the distribution's libheif has no HEVC plugin: the message shown, nothing inserted"
  fi
else
  echo "--- A HEIF file decoded: skipped, no MITCAD_HEIF and no libheif.so.1 here"
fi

# With no libheif (a library path that is not there, the run-time load's
# MITCAD_TEST_HEIF_LIBRARY): one message, naming what to install, and
# nothing inserted. A build with MITCAD_HEIF decodes with its own.
if [ "$HEIF_BUILT" = 0 ]; then
  echo "--- A HEIF file with no libheif: the message only"
  ui_stop_app
  export MITCAD_TEST_HEIF_LIBRARY="$WORK/no-libheif/libheif.so.1"
  ui_start_app --open "$WORK/plate.mitcad"
  insert_heif
  [ "$HEIF_INSERTED" = 0 ] || ui_fail "a HEIF file was inserted with no libheif"
  [ "$(grep -cF "Panel Image Reference error: $HEIF_MESSAGE" "$UI_LOG")" -ge 1 ] &&
    echo "ok   the message names libheif with its HEVC plugin"
  grep -q "Inserted image reference from photo.heic" "$UI_LOG" && ui_fail "inserted with no decoder"
  unset MITCAD_TEST_HEIF_LIBRARY
fi

ui_finish "UI image references test"

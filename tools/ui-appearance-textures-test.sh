#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/AppearanceDialog.cpp app/browser/AppearanceDialog.hpp app/materials
# Texture sources in Edit Appearances (mitcad#315) through the real UI, on a
# 40 x 30 x 20 mm box, with a synthetic texture folder (generated images):
#   - The folder is a texture source (the setting textures/folders); the
#     dialog's source selector shows its materials as thumbnails, which go
#     into the texture cache (MITCAD_TEXTURES_DIR).
#   - The search finds the material by its name; Use makes it an appearance
#     of the design with its base colour, normal and roughness maps
#     embedded and its source recorded; a second Use of the same material
#     shares the images the design has already.
#   - The Appearance tool gives the body the appearance (mitcad#1059); the
#     saved design keeps each image once.
#   - With the texture folder and the cache moved away, the design opens
#     again with its texture and maps embedded.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-appearance-textures-test.sh [screenshot of the dialog.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-appearance-textures.XXXXXX)
FILE=$WORK/box.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_file expression expected description: a Python expression on the
# saved project file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# The texture folder: a material by name stem in two resolutions (the base
# colour 64 x 32 pixels), and one more.
python3 - "$WORK/textures" << 'EOF'
import os, struct, sys, zlib

def png(path, width, height, rgb):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + bytes(rgb) * width for _ in range(height))
    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "wb").write(data)

root = sys.argv[1]
png(f"{root}/walls/red_bricks_diff_1k.png", 32, 16, (200, 40, 30))
png(f"{root}/walls/red_bricks_diff_2k.png", 64, 32, (200, 40, 30))
png(f"{root}/walls/red_bricks_nor_gl_2k.png", 16, 16, (128, 128, 255))
png(f"{root}/walls/red_bricks_rough_2k.png", 16, 16, (180, 180, 180))
png(f"{root}/wood/oak_planks_diff_2k.png", 16, 16, (150, 110, 60))
EOF
mkdir -p "$UI_CONFIG/Mitcad"
printf '[textures]\nfolders=%s\n' "$WORK/textures" >> "$UI_CONFIG/Mitcad/Mitcad.conf"
export MITCAD_TEXTURES_DIR=$WORK/cache

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -15],
    "length": 40, "width": 30, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
# Qt's own file dialogs: Preferences' Add... is answered by typing the folder.
ui_start_app --no-native-dialogs --open "$FILE"
ui_expect_log "Opened $FILE" "the box opened"

echo "--- The texture folder as a source"
ui_mark
ui_step "select Body1 in the browser"   ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Edit Appearances (search)"     ui_command "Edit Appearances"
ui_expect_new "Appearances dialog opened" "the dialog opened"
ui_expect_new "Appearances texture sources: folder, polyhaven (1 folders)" "the folder source is there"
ui_expect_new "Appearances source at" "the source selector is shown"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "choose Local Folders"          ui_choose "Appearances source" 1
ui_expect_new "Appearances source: folder" "the folder source chosen"
ui_expect_new 'Texture search folder "" (all categories): 2 found' "its two materials are listed"
ui_expect_new "Appearances texture Red bricks at" "as a grid"
ui_expect_new "Texture thumbnail Red bricks shown" "with a thumbnail"
ls "$WORK"/cache/folder/*/thumbnail-96.png > /dev/null 2>&1 || ui_fail "no thumbnail in the texture cache"
echo "ok   the thumbnails are in the texture cache"

echo "--- Search and Use"
ui_mark
ui_step "click the search field"        ui_click_logged "Appearances search"
ui_step "type brick"                    xdotool type --delay 60 "brick"
ui_expect_new 'Texture search folder "brick" (all categories): 1 found' "the search finds the material"
ui_mark
ui_step "click Red bricks"              ui_click_logged "Appearances texture Red bricks"
ui_expect_new "Texture selected: Red bricks (2k)" "selected, in 2k"
ui_mark
ui_step "Use"                           ui_click_logged "Appearances use"
ui_expect_new "Texture used: folder red_bricks 2k as custom1 (3 maps; 3 images embedded, 0 shared)" \
  "Use made an appearance with the maps embedded"
ui_expect_new "Appearance custom1 texture: embedded, maps normal roughness, source folder red_bricks 2k" \
  "the dialog shows its maps and source"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- A second Use, the Appearance tool, save"
ui_mark
ui_step "Use again"                     ui_click_logged "Appearances use"
ui_expect_new "Texture used: folder red_bricks 2k as custom2 (3 maps; 0 images embedded, 3 shared)" \
  "a second Use shares the design's images"
ui_step "Close"                         ui_click_logged "Appearances close"
ui_focus_main
ui_mark
ui_step "right-click Body1"             ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."          ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool for F1.b0" "the Appearance tool for the box"
ui_step "the picker: Red bricks"        ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red bricks"
ui_mark
ui_step "OK (Enter)"                    ui_key Return
ui_expect_new "Appearance custom1 for 1 body(ies)" "assigned"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[(a["id"], a["name"]) for a in doc["appearances"]]' \
  "[('custom1', 'Red bricks'), ('custom2', 'Red bricks 2')]" "the file keeps both appearances"
expect_file 'doc["bodies"][0]["appearance"]' "custom1" "and the body's appearance"
expect_file 'len(doc["images"])' "3" "each image once"
expect_file 'sorted(k for k in doc["appearances"][0]["texture"] if k in ("image", "normal", "roughness", "metalness", "occlusion"))' \
  "['image', 'normal', 'roughness']" "the texture refers to its base colour and two maps"
expect_file 'doc["appearances"][0]["texture"]["image"] == doc["appearances"][1]["texture"]["image"]' "True" \
  "both appearances refer to the same base colour image"
expect_file 'doc["appearances"][0]["texture"]["source"]' \
  "{'id': 'folder', 'asset': 'red_bricks', 'resolution': '2k'}" "the source is recorded"
expect_file 'doc["appearances"][0]["texture"]["size"]' "[100.0, 50.0]" "100 mm wide, as high as the image's proportions"

echo "--- A second folder from Preferences"
python3 - "$WORK/more/blue_bricks_diff.png" << 'EOF'
import os, struct, sys, zlib
def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
rows = b"".join(b"\0" + bytes((40, 60, 200)) * 8 for _ in range(8))
os.makedirs(os.path.dirname(sys.argv[1]), exist_ok=True)
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 8, 8, 8, 2, 0, 0, 0)) +
                              chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
EOF
ui_mark
ui_step "Preferences: Textures"         ui_command "Preferences: Textures"
ui_expect_new "Preferences texture folder add at" "the Textures page"
ui_focus_dialog '^Preferences$'
ui_step "Add..."                        ui_click_logged "Preferences texture folder add"
ui_step "choose the second folder"      ui_type_path "Texture Folder" "$WORK/more"
ui_focus_dialog '^Preferences$'
ui_step "OK (Return)"                   ui_key Return
ui_expect_new "Preferences: texture folders $WORK/textures, $WORK/more" "the folder is added"
grep -q "^folders=.*$WORK/more" "$UI_CONFIG/Mitcad/Mitcad.conf" || ui_fail "the settings do not keep the folder"
echo "ok   the settings keep it"
ui_focus_main
ui_mark
ui_step "Edit Appearances (search)"     ui_command "Edit Appearances"
ui_expect_new "Appearances texture sources: folder, polyhaven (2 folders)" "the dialog takes the new folder"
ui_expect_new 'Texture search folder "brick" (all categories): 2 found' "and finds its material too"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "back to the appearances"       ui_choose "Appearances source" -1
ui_expect_new "Appearances source: appearances" "the appearances chosen"
ui_expect_new 'Appearances filtered "brick" (All Categories): 2 shown' "the search filters them too"
ui_expect_new "Appearances item custom2 at" "the design's are listed"
ui_step "Close"                         ui_click_logged "Appearances close"
ui_focus_main

echo "--- Opened again without the texture folder and the cache"
ui_stop_app
mv "$WORK/textures" "$WORK/textures-moved"
mv "$WORK/cache" "$WORK/cache-moved"
ui_start_app --open "$FILE"
UI_MARK=0 # a new log
ui_expect_new "Opened $FILE" "the design opened again"
ui_step "Edit Appearances (search)"     ui_command "Edit Appearances"
ui_expect_new "Appearances item steel_satin at" "the library is listed"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "the list"                      ui_click_logged "Appearances item steel_satin"
ui_step "the design's last (End)"       ui_key End
ui_step "Red bricks (Up)"               ui_key Up
ui_expect_new "Appearance selected: custom1" "the body's appearance selected"
ui_expect_new "Appearance custom1 texture: embedded, maps normal roughness, source folder red_bricks 2k" \
  "with its maps embedded"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Appearance custom1 texture missing"; then
  ui_fail "the embedded texture is missing after reopening"
fi
echo "ok   the embedded images are there"

ui_finish "Texture sources in Edit Appearances"

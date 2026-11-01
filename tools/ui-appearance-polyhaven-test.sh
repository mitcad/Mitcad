#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/AppearanceDialog.cpp app/browser/AppearanceDialog.hpp app/materials
# Poly Haven as a texture source in Edit Appearances (mitcad#313) through
# the real UI, on a 40 x 30 x 20 mm box, without the network: the
# source's requests are answered by files of a folder
# (MITCAD_TEXTURE_FIXTURES, app/materials/TextureFetcher.hpp) holding a
# small hand-written list of textures, a list of files and generated
# images (nothing copied from the service).
#   - Texture sources on the internet are off at first: choosing Poly Haven
#     asks; No asks for nothing and goes back; Yes allows them.
#   - The source shows its attribution line, lists the textures with
#     thumbnails, and the search finds one; choosing it lists its
#     resolutions with their download sizes.
#   - Use downloads the 2k maps into the texture cache and makes them an
#     appearance with the maps embedded, its size the texture's physical
#     size, its source recorded with the licence and the page; the
#     Appearance tool gives it to the body (mitcad#1059).
#   - With the fixtures and the cache moved away, the saved design opens
#     again with the maps embedded.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-appearance-polyhaven-test.sh [screenshot of the dialog.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-appearance-polyhaven.XXXXXX)
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

# The answers: the list of textures (two), the files of one (2k: base
# colour, normal and roughness as PNG; 1k: base colour), its thumbnail.
python3 - "$WORK/fixtures" << 'EOF'
import hashlib, json, os, struct, sys, zlib

def png(width, height, rgb):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + bytes(rgb) * width for _ in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))

def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "wb").write(data)

root = sys.argv[1]
api, cdn, dl = "api.polyhaven.com", "cdn.polyhaven.com", "dl.polyhaven.org"
write(f"{root}/{api}/assets", json.dumps({
    "red_tiles_01": {"type": 1, "name": "Red Tiles 01", "tags": ["tiles", "glazed"],
                     "categories": ["floor", "man made"], "dimensions": [2000, 1000],
                     "max_resolution": [2048, 1024], "files_hash": "0123abcd", "download_count": 50,
                     "thumbnail_url": f"https://{cdn}/thumbs/red_tiles_01.png?width=256&format=webp&v=1"},
    "grey_stone_02": {"type": 1, "name": "Grey Stone 02", "tags": ["rock"], "categories": ["natural"],
                      "dimensions": [3000, 3000], "max_resolution": [4096, 4096], "download_count": 90,
                      "thumbnail_url": f"https://{cdn}/thumbs/grey_stone_02.png?v=1"},
}).encode())
write(f"{root}/{cdn}/thumbs/red_tiles_01.png", png(96, 96, (200, 40, 30)))
write(f"{root}/{cdn}/thumbs/grey_stone_02.png", png(96, 96, (120, 120, 120)))
files = {}
for key, name, res, size, rgb in [("Diffuse", "diff", "1k", (32, 16), (200, 40, 30)),
                                  ("Diffuse", "diff", "2k", (64, 32), (200, 40, 30)),
                                  ("nor_gl", "nor_gl", "2k", (16, 16), (128, 128, 255)),
                                  ("Rough", "rough", "2k", (16, 16), (180, 180, 180))]:
    data = png(size[0], size[1], rgb)
    path = f"file/Textures/png/{res}/red_tiles_01/red_tiles_01_{name}_{res}.png"
    write(f"{root}/{dl}/{path}", data)
    files.setdefault(key, {})[res] = {"png": {"url": f"https://{dl}/{path}", "size": len(data),
                                             "md5": hashlib.md5(data).hexdigest()}}
write(f"{root}/{api}/files/red_tiles_01", json.dumps(files).encode())
EOF
export MITCAD_TEXTURE_FIXTURES=$WORK/fixtures
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
ui_start_app --open "$FILE"
ui_expect_log "Opened $FILE" "the box opened"

echo "--- The network is asked for first"
ui_mark
ui_step "select Body1 in the browser"   ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Edit Appearances (search)"     ui_command "Edit Appearances"
ui_expect_new "Appearances texture sources: folder, polyhaven (0 folders)" "Poly Haven is a source"
ui_expect_new "Appearances source at" "the source selector is shown"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "choose Poly Haven"             ui_choose "Appearances source" 2
ui_focus_dialog '^Texture Sources on the Internet$'
ui_step "No (Return)"                   ui_key Return
ui_focus_dialog '^Appearances$'
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Texture request\|Appearances source: polyhaven"; then
  ui_fail "Poly Haven was asked without the user's permission"
fi
echo "ok   No: nothing asked, the source not chosen"
ui_mark
ui_step "choose Poly Haven again"       ui_choose "Appearances source" 2
ui_focus_dialog '^Texture Sources on the Internet$'
ui_step "Yes"                           ui_key alt+y
ui_expect_new "Appearances: texture sources on the internet allowed" "Yes allows the network"
grep -q "^allowNetwork=true" "$UI_CONFIG/Mitcad/Mitcad.conf" || ui_fail "the settings do not keep the permission"
echo "ok   the settings keep it"
ui_expect_new "Appearances source: polyhaven" "Poly Haven chosen"
ui_expect_new "Texture source polyhaven attribution: Powered by" "the attribution line is shown"
ui_expect_new "Texture request https://api.polyhaven.com/assets?t=textures" "the list of textures asked for"
ui_expect_new 'Texture search polyhaven "" (all categories): 2 found' "two textures listed"
ui_expect_new "Appearances texture Red Tiles 01 at" "as a grid"
ui_expect_new "Texture thumbnail Red Tiles 01 shown" "with a thumbnail"
[ -f "$WORK/cache/polyhaven/red_tiles_01/thumbnail-96.png" ] || ui_fail "no thumbnail in the texture cache"
[ -f "$WORK/cache/polyhaven/textures.json" ] || ui_fail "the list of textures is not cached"
echo "ok   the list and the thumbnails are in the texture cache"
ui_focus_dialog '^Appearances$'

echo "--- Search, choose, Use"
ui_mark
ui_step "click the search field"        ui_click_logged "Appearances search"
ui_step "type tiles"                    xdotool type --delay 60 "tiles"
ui_expect_new 'Texture search polyhaven "tiles" (all categories): 1 found' "the search finds the texture"
ui_mark
ui_step "click Red Tiles 01"            ui_click_logged "Appearances texture Red Tiles 01"
ui_expect_new "Texture selected: Red Tiles 01 (2k)" "selected, in 2k"
ui_expect_new "Texture request https://api.polyhaven.com/files/red_tiles_01" "its files asked for"
ui_expect_new "Texture details: Red Tiles 01: 1k " "its resolutions with their sizes"
ui_mark
ui_step "Use"                           ui_click_logged "Appearances use"
ui_expect_new "Texture fetching polyhaven red_tiles_01 2k" "Use downloads the 2k maps"
ui_expect_new "Poly Haven: red_tiles_01 2k in the cache (3 maps)" "into the texture cache"
ui_expect_new "Texture used: polyhaven red_tiles_01 2k as custom1 (3 maps; 3 images embedded, 0 shared)" \
  "Use made an appearance with the maps embedded"
ui_expect_new "Appearance custom1 texture: embedded, maps normal roughness, source polyhaven red_tiles_01 2k" \
  "the dialog shows its maps and source"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "Close"                         ui_click_logged "Appearances close"
ui_focus_main
ui_mark
ui_step "right-click Body1"             ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."          ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool for F1.b0" "the Appearance tool for the box"
ui_step "the picker: Red Tiles 01"      ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red Tiles 01"
ui_mark
ui_step "OK (Enter)"                    ui_key Return
ui_expect_new "Appearance custom1 for 1 body(ies)" "assigned"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[(a["id"], a["name"]) for a in doc["appearances"]]' "[('custom1', 'Red Tiles 01')]" \
  "the file keeps the appearance"
expect_file 'doc["bodies"][0]["appearance"]' "custom1" "and the body's appearance"
expect_file 'doc["appearances"][0]["texture"]["source"]' \
  "{'id': 'polyhaven', 'asset': 'red_tiles_01', 'resolution': '2k', 'license': 'CC0-1.0', 'url': 'https://polyhaven.com/a/red_tiles_01'}" \
  "the source is recorded with the licence and the page"
expect_file 'doc["appearances"][0]["texture"]["size"]' "[2000.0, 1000.0]" "the texture's physical size"
expect_file 'sorted(k for k in doc["appearances"][0]["texture"] if k in ("normal", "roughness", "metalness", "occlusion"))' \
  "['normal', 'roughness']" "with its normal and roughness maps"

echo "--- Opened again without the fixtures and the cache"
ui_stop_app
mv "$WORK/fixtures" "$WORK/fixtures-moved"
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
ui_expect_new "Appearance selected: custom1" "the design's appearance selected"
ui_expect_new "Appearance custom1 texture: embedded, maps normal roughness, source polyhaven red_tiles_01 2k" \
  "with its maps embedded"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Appearance custom1 texture missing\|Texture request"; then
  ui_fail "the texture is missing or asked for again after reopening"
fi
echo "ok   the embedded images are there, nothing asked"

ui_finish "Poly Haven in Edit Appearances"

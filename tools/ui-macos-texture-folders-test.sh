#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/materials/TexturePreferences app/platform/macos/NativeDialogs app/platform/macos/NativeTableCells app/platform/macos/NativeTestInput tools/ui-scripts/texture-folders-script.py
# One populated folder list in each appearance: available row width,
# middle elision preserving its suffix, selection/remove and saved settings.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
[ "$#" = 4 ] || { echo "usage: $0 <app> <cli> <output> qt|native" >&2; exit 2; }
frontend=$4
case "$frontend" in qt|native) ;; *) exit 2 ;; esac
ui_init "$1" "$3"
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_LIBRARIES_DIR"
# Several valid short directory components, making the whole path too long
# for the viewport without exceeding the filesystem's component limit.
export MITCAD_TEXTURE_SHORT=$UI_ROOT/texture-maps
export MITCAD_TEXTURE_LONG=$UI_ROOT
for index in 1 2 3 4 5 6; do
  MITCAD_TEXTURE_LONG=$MITCAD_TEXTURE_LONG/a-deliberately-long-synthetic-folder-for-texture-path-elision-$index
 done
export MITCAD_TEXTURE_LONG=$MITCAD_TEXTURE_LONG/recognizable-texture-maps
mkdir -p "$MITCAD_TEXTURE_SHORT" "$MITCAD_TEXTURE_LONG"
for scheme in light dark; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scheme
  mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
  cat > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini" <<INI
[textures]
folders=$MITCAD_TEXTURE_SHORT, $MITCAD_TEXTURE_LONG
allowNetwork=false
INI
  script=$UI_OUT/$frontend-$scheme.mitcad-ui
  python3 "$here/ui-scripts/texture-folders-script.py" "$frontend" > "$script" || exit 1
  args=(--demo --no-recovery --no-mcp --ui="$frontend")
  [ "$frontend" != qt ] || args+=(--no-native-dialogs)
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$script ui_run_app "$scheme" "${args[@]}"
  ui_expect_log '^TestDriver: done' "$frontend/$scheme workflow completed"
  ui_expect_no_log '^TestDriver: FAIL' "$frontend/$scheme assertions passed"
  cp -R "$MITCAD_SETTINGS_DIR" "$UI_OUT/settings-$scheme"
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  python3 - "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini" <<'PY'
import configparser
import os
import sys

settings = configparser.ConfigParser()
settings.read(sys.argv[1])
expected = os.path.normpath(os.environ['MITCAD_TEXTURE_LONG'])
assert settings['textures']['folders'] == expected, dict(settings['textures'])
assert settings['textures']['allowNetwork'] == 'false'
assert os.path.isdir(os.environ['MITCAD_TEXTURE_SHORT'])
assert os.path.isdir(os.environ['MITCAD_TEXTURE_LONG'])
PY
  [ "$?" = 0 ] || exit 1
done
echo "PASS: $frontend texture folder row width, suffix, removal and persistence; $UI_OUT"

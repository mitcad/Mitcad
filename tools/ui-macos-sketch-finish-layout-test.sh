#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/TitleBar app/framework/Ribbon app/framework/TestDriver app/sketch/SketchPalette app/MainWindow.cpp tools/cli
# Only ribbon/palette finish controls, with real actions in wide/narrow windows.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <cli> [output directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-finish-layout}"
mkdir -p "$UI_OUT/files"
cat > "$UI_OUT/files/sketch.json" <<'JSON'
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[0,0],"width":40,"height":25}
]
JSON
"$cli" run "$UI_OUT/files/sketch.json" --save "$UI_OUT/files/sketch.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
for chrome in floating docked; do
  for scheme in light dark; do
    name=$chrome-$scheme
    export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$name
    export MITCAD_TEST_SHOTS=$UI_OUT/$name
    mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_TEST_SHOTS"
    MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/sketch-finish-layout.mitcad-ui \
      ui_run_app "$name" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --open "$UI_OUT/files/sketch.mitcad"
    ui_expect_log 'Sketch finish layout: title none, ribbon and palette enabled, no reserved title slot' "$name: no duplicate or unused title slot"
    ui_expect_log 'TestDriver: done' "$name: ribbon, palette, collapsed ribbon group and shortcut finish the sketch"
    ui_expect_no_log 'TestDriver: FAIL' "$name: all finish layout assertions passed"
    [ "$UI_FAILURES" = 0 ] || exit 1
  done
done
echo "PASS: Finish Sketch in ribbon and palette, Floating/Docked, light/dark, wide/narrow; evidence in $UI_OUT"

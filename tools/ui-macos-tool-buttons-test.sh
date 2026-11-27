#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MacToolButtonStyle app/framework/GlassCard app/framework/TestDriver app/MainWindow.cpp app/framework/CommandPanel app/browser app/projects tools/cli
# Shared macOS controls: resting pixels and automatic static/dynamic enrollment.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <cli> [output directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-tool-buttons}"
mkdir -p "$UI_OUT/files"
cat > "$UI_OUT/files/body.json" <<'JSON'
[{"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":40,"width":30,"height":20,"operation":"new_body"}}]
JSON
"$cli" run "$UI_OUT/files/body.json" --save "$UI_OUT/files/body.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
# Discover the physical backing ratio, then request absolute DPR 1 and 2.
# These cover Qt's dense artwork/layout; they do not change display modes.
printf 'expect-viewport\n' > "$UI_OUT/scale.mitcad-ui"
QT_SCALE_FACTOR=1 MITCAD_TEST_INPUT=$UI_OUT/scale.mitcad-ui \
  ui_run_app scale --ui=qt --no-recovery --no-mcp
ui_expect_log 'TestDriver: done' 'viewport ratio discovered'
base=$(sed -n 's/.*framebuffer [0-9]*x[0-9]*, scale \([0-9.]*\);.*/\1/p' "$UI_LOG_FILE" | tail -1)
[ -n "$base" ] || { echo 'FAIL: no viewport scale' >&2; exit 1; }
for chrome in floating docked; do
  for scale in 1 2; do
    factor=$(awk -v wanted="$scale" -v base="$base" 'BEGIN { print wanted/base }')
    for scheme in light dark; do
      name=$chrome-${scale}x-$scheme
      export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$name
      export MITCAD_TEST_SHOTS=$UI_OUT/$name
      mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_TEST_SHOTS"
      QT_SCALE_FACTOR=$factor MITCAD_TEST_COLOR_SCHEME=$scheme \
        MITCAD_TEST_INPUT=$here/ui-scripts/macos-tool-buttons.mitcad-ui \
        ui_run_app "$name" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --open "$UI_OUT/files/body.mitcad"
      ui_expect_log "framebuffer .*scale $scale;" "$name: actual device pixel ratio $scale"
      ui_expect_log 'macOS tool button audit: .*dynamic idle bezel absent' "$name: shared static/dynamic rendering"
      ui_expect_log 'TestDriver: done' "$name: initial, selection, pattern and box controls audited"
      ui_expect_no_log 'TestDriver: FAIL' "$name: no failed control assertions"
      [ "$UI_FAILURES" = 0 ] || exit 1
    done
  done
done
echo "PASS: macOS tool buttons, Floating/Docked, light/dark, DPR 1/2; evidence in $UI_OUT"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/projects app/framework/TreeItemDelegate app/framework/WindowStyle app/framework/TestDriver
# Real Browser and Projects rename delegates, both chromes and appearances.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <cli> [output directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-tree-rename}"
mkdir -p "$UI_OUT/files"
"$cli" project create "$MITCAD_PROJECTS_DIR/Rename" --author 'UI Tester <ui@example.invalid>' --design base.mitcad \
  > "$UI_OUT/setup.log" 2>&1 || exit 1
cat > "$UI_OUT/files/setup.json" <<'JSON'
[
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":40,"width":25,"height":10,"operation":"new_body"}},
  {"cmd":"create_component","name":"Bracket"},
  {"cmd":"activate_component","component":"C0"},
  {"cmd":"add_measurement","a":{"datum":"x"},"b":{"datum":"y"},"kind":"angle","name":"Angle1"}
]
JSON
"$cli" run "$UI_OUT/files/setup.json" --save "$MITCAD_PROJECTS_DIR/Rename/base.mitcad" \
  >> "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
# Measure the physical backing ratio before requesting absolute DPR 1/2.
printf 'expect-viewport\n' > "$UI_OUT/scale.mitcad-ui"
QT_SCALE_FACTOR=1 MITCAD_TEST_INPUT=$UI_OUT/scale.mitcad-ui \
  ui_run_app scale --ui=qt --no-recovery --no-mcp
ui_expect_log 'TestDriver: done' 'backing ratio discovery completed'
base=$(sed -n 's/.*framebuffer [0-9]*x[0-9]*, scale \([0-9.]*\);.*/\1/p' "$UI_LOG_FILE" | tail -1)
[ -n "$base" ] || { echo 'FAIL: no viewport scale' >&2; exit 1; }
for chrome in floating docked; do
  for scale in 1 2; do
    factor=$(awk -v wanted="$scale" -v base="$base" 'BEGIN { print wanted/base }')
    for scheme in light dark; do
      export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$chrome-${scale}x-$scheme
      export MITCAD_TEST_SHOTS=$UI_OUT/$chrome-${scale}x-$scheme
      mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_TEST_SHOTS"
      MITCAD_TEST_INPUT=$here/ui-scripts/macos-tree-rename.mitcad-ui \
        MITCAD_TEST_COLOR_SCHEME=$scheme QT_SCALE_FACTOR=$factor \
        ui_run_app "$chrome-${scale}x-$scheme" --ui=qt --chrome="$chrome" --no-recovery --no-mcp \
          --open "$MITCAD_PROJECTS_DIR/Rename/base.mitcad"
      ui_expect_log 'TestDriver: done' "$chrome ${scale}x $scheme: all rename paths completed"
      ui_expect_log "framebuffer .*scale $scale;" "$chrome ${scale}x $scheme: actual device pixel ratio $scale"
      ui_expect_no_log 'TestDriver: FAIL' "$chrome ${scale}x $scheme: opaque editor and hidden old name"
    done
  done
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Browser body/component and Projects renaming, Floating/Docked, light/dark, 1x/2x Qt layouts'

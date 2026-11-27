#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/PickContext.cpp app/PickContext.hpp app/OcctViewer.cpp app/framework app/commands/FilletCommands.cpp
# Selected face/edge boundaries and removal with completed previews at 1x/2x.
# Usage: bash tools/ui-macos-pick-test.sh <app> <cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-pick}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LOG_ORIENTATION_CUBE=1
mkdir -p "$MITCAD_TEST_WORK"
cat > "$MITCAD_TEST_WORK/bracket.json" <<'JSON'
[
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":60,"width":40,"height":5,"operation":"new_body"}},
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,35],"length":60,"width":5,"height":40,"operation":"join","participants":["F1.b0"]}}
]
JSON
"$2" run "$MITCAD_TEST_WORK/bracket.json" --save "$MITCAD_TEST_WORK/bracket.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
cat > "$MITCAD_TEST_WORK/narrow.json" <<'JSON'
[{"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":60,"width":0.5,"height":5,"operation":"new_body"}}]
JSON
"$2" run "$MITCAD_TEST_WORK/narrow.json" --save "$MITCAD_TEST_WORK/narrow.mitcad" \
  > "$UI_OUT/narrow-setup.log" 2>&1 || { cat "$UI_OUT/narrow-setup.log" >&2; exit 1; }
# Query the real viewport ratio first, then request absolute 1x/2x ratios
# through Qt's scale multiplier. No physical display setting is changed.
printf 'expect-viewport\n' > "$UI_OUT/scale.mitcad-ui"
QT_SCALE_FACTOR=1 MITCAD_TEST_INPUT=$UI_OUT/scale.mitcad-ui \
  ui_run_app scale --ui=qt --no-recovery --no-mcp
ui_expect_log 'TestDriver: done' 'viewport ratio discovered'
base=$(sed -n 's/.*framebuffer [0-9]*x[0-9]*, scale \([0-9.]*\);.*/\1/p' "$UI_LOG_FILE" | tail -1)
[ -n "$base" ] || { echo 'FAIL: no viewport scale'; exit 1; }
for scale in 1 2; do
  factor=$(awk -v wanted="$scale" -v base="$base" 'BEGIN { print wanted/base }')
  mkdir -p "$UI_OUT/scale-$scale"
  export MITCAD_TEST_WORK=$UI_OUT/scale-$scale
  QT_SCALE_FACTOR=$factor MITCAD_TEST_INPUT=$here/ui-scripts/pick-highlights.mitcad-ui \
    ui_run_app "pick-$scale" --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/bracket.mitcad"
  ui_expect_log "framebuffer .*scale $scale;" "actual device pixel ratio $scale"
  ui_expect_log 'TestDriver: done' "selection highlights at $scale x"
  if grep -Eq '^TestDriver: FAIL|^OCCT view event failed' "$UI_LOG_FILE"; then
    ui_fail 'pick assertion or viewport failure'
  else
    ui_pass 'all pick assertions passed'
  fi
  [ "$UI_FAILURES" -eq 0 ] || exit 1
  QT_SCALE_FACTOR=$factor MITCAD_TEST_INPUT=$here/ui-scripts/pick-narrow-face.mitcad-ui \
    ui_run_app "narrow-$scale" --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/narrow.mitcad"
  ui_expect_log 'TestDriver: done' "narrow face remains selectable at $scale x"
  if grep -Eq '^TestDriver: FAIL|^OCCT view event failed' "$UI_LOG_FILE"; then
    ui_fail 'narrow-face assertion or viewport failure'
  else
    ui_pass 'narrow-face assertions passed'
  fi
  [ "$UI_FAILURES" -eq 0 ] || exit 1
done
echo "PASS: selection highlight picking; evidence in $UI_OUT"

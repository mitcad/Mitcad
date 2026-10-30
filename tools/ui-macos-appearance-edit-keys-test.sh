#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/AppearanceDialog app/platform/MacDialogs.hpp app/platform/macos/NativeDialogs tools/ui-scripts/appearance-edit-keys-script.py
# Return/Enter commit appearance fields without closing the editor (#977),
# in Qt and AppKit, light/dark. Actual keys and pointer/button actions,
# validation, exact undo steps and independently saved model values.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory> qt|native" >&2
  exit 2
fi
cli=$2
frontend=$4
case "$frontend" in qt|native) ;; *) exit 2 ;; esac
ui_init "$1" "$3"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_LIBRARIES_DIR"
cat > "$UI_OUT/setup.json" <<'JSON'
[{"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":40,"width":60,"height":20,"operation":"new_body"}}]
JSON
for scheme in light dark; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scheme
  mkdir -p "$MITCAD_SETTINGS_DIR"
  model=$MITCAD_TEST_WORK/appearance-$frontend-$scheme.mitcad
  "$cli" run "$UI_OUT/setup.json" --save "$model" > "$UI_OUT/setup-$scheme.log" 2>&1 || exit 1
  script=$UI_OUT/appearance-$frontend-$scheme.mitcad-ui
  python3 "$here/ui-scripts/appearance-edit-keys-script.py" "$frontend" > "$script" || exit 1
  args=(--ui="$frontend" --no-recovery --no-mcp --open "$model")
  [ "$frontend" != qt ] || args+=(--no-native-dialogs)
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$script ui_run_app "$scheme" "${args[@]}"
  ui_expect_log '^TestDriver: done' "$frontend/$scheme: appearance edit keys completed"
  ui_expect_no_log 'TestDriver: FAIL' "$frontend/$scheme: appearance assertions passed"
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  python3 - "$model" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as source:
    document = json.load(source)
appearance = next(a for a in document["appearances"] if a["id"] == "custom1")
assert appearance["name"] == "Key test appearance", appearance
assert abs(appearance["roughness"] - 1.0) < 1e-9, appearance
assert [round(v * 255) for v in appearance["base_color"]] == [208, 32, 32], appearance
assert document["bodies"][0]["appearance"] == "custom1", document["bodies"]
PY
  [ "$?" = 0 ] || exit 1
done
echo "PASS: $frontend appearance commits, retention, validation, Close/Escape and saved values; $UI_OUT"

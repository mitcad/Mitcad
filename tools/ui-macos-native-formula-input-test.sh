#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/FormulaEditor app/platform/NativeDialogs.hpp app/platform/macos/NativeDialogs app/platform/macos/NativeTestInput app/browser/ParametersDialog tools/cli
# Real native selection, function/template insertion, completion and
# acceptance/refusal with model and undo assertions, in both appearances.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  model=$MITCAD_TEST_WORK/formula-input.mitcad
  "$cli" run "$here/ui-scripts/native-formula-input.json" --save "$model" \
    > "$UI_OUT/setup-$scheme.log" 2>&1 || { cat "$UI_OUT/setup-$scheme.log" >&2; exit 1; }
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/native-formula-input.mitcad-ui \
    ui_run_app "formula-$scheme" --ui=native --no-recovery --no-mcp --open "$model"
  ui_expect_log '^TestDriver: done' "$scheme: native formula input completed"
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' \
                 'Native Mac canvas:' 'OCCT view event failed:'; do
    ui_expect_no_log "$problem" "$scheme: no $problem"
  done
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  python3 - "$model" "$MITCAD_TEST_WORK" "$scheme" <<'PY'
import json
from pathlib import Path
import sys

model, work = map(Path, sys.argv[1:3])
scheme = sys.argv[3]
design = json.loads(model.read_text())
parameters = {p["name"]: p for p in design["parameters"]}
assert parameters["result"]["expression"] == "pitch", parameters["result"]
assert parameters["length"]["expression"] == "100 mm", parameters["length"]
assert parameters["pitch"]["expression"] == "8 mm", parameters["pitch"]
assert len(design["bodies"]) == 1, design["bodies"]
for state in ["function", "template", "refused", "completion"]:
    image = work / f"{state}-{scheme}.png"
    assert image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), image
print("Verified: saved shared completion result, unchanged inputs/body and real-input captures")
PY
  [ "$?" = 0 ] || exit 1
  cp "$model" "$MITCAD_TEST_WORK/formula-input-$scheme.mitcad"
done
echo "PASS: native formula caret, function/template, completion, acceptance/refusal and undo (light/dark)"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/Configuration app/browser/TablesPage app/configurations app/platform/macos/NativeRuleCanvas app/framework/Dialogs
# check-all sources: core/model-base/src/configurations core/model/src/document/configuration_tables core/model-api/src/tables_csv tools/cli
# Recent table/configuration UI (mitcad#849), through real Qt and AppKit
# controls in both appearances. The fixture is synthetic and local. It
# exercises text/number column domains, Generate Rows/cancel, an in_table
# block, native table-name completion, column-map cancel/edit, driven
# geometry, actual CSV preview/import/move notices and one-step undo.
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
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# Replacing M5 repairs Size and Length in the same import. The nearest
# numeric fallback (16) must also match the M3 row chosen by the text
# fallback, so the retained in_table rule accepts the repaired pair. Undo must
# restore both choices, the source table and the body's 2000 mm3 volume.
cat > "$MITCAD_TEST_WORK/Screws.csv" <<'CSV'
Size [Text],dk [mm],Length [mm]
M3,5.5,16
M4,7,26
M6,10,30
CSV
for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  model=$MITCAD_TEST_WORK/configuration-tables-$frontend-$scheme.mitcad
  if ! "$cli" run "$here/ui-scripts/configuration-tables-parity.json" --save "$model" \
      > "$UI_OUT/setup-$scheme.log" 2>&1; then
    cat "$UI_OUT/setup-$scheme.log" >&2
    exit 1
  fi
  script=$UI_OUT/configuration-tables-$frontend-$scheme.mitcad-ui
  python3 "$here/ui-scripts/configuration-tables-parity-script.py" "$frontend" > "$script"
  args=(--ui="$frontend" --no-recovery --no-mcp --open "$model")
  [ "$frontend" != qt ] || args+=(--no-native-dialogs)
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=${MITCAD_TEST_STEP_MS:-90} MITCAD_TEST_INPUT=$script \
    ui_run_app "$frontend-$scheme" "${args[@]}"
  ui_expect_log '^TestDriver: done' "$frontend/$scheme: complete table/configuration workflow"
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:'; do
    ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
  done
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  python3 - "$model" "$MITCAD_TEST_WORK" "$frontend" "$scheme" <<'PY'
import json
from pathlib import Path
import sys
model, work = map(Path, sys.argv[1:3])
frontend, scheme = sys.argv[3:]
design = json.loads(model.read_text())
parameters = {item["name"]: item for item in design["parameters"]}
for name in ["Size", "Length"]:
    assert parameters[name]["values"] == [f"Screws.{name}"], parameters[name]
assert parameters["Size"]["expression"] == "'M5'", parameters["Size"]
assert abs(parameters["Length"]["value"] - 20) < 1e-6, parameters["Length"]
assert abs(parameters["HeadDia"]["value"] - 8.5) < 1e-6, parameters["HeadDia"]
configuration = design["configurations"]
assert [row["name"] for row in configuration["rows"]] == ["M3", "M4", "M5"], configuration
assert [row["values"] for row in configuration["rows"]] == [
    {"Size": "'M3'"}, {"Size": "'M4'"}, {"Size": "'M5'"}], configuration
rule = configuration["rules"][0]
assert rule["name"] == "Made sizes", rule
assert rule["then"] == [{"in_table": "Screws", "columns": {"Size": "Size", "Length": "Length"}}], rule
screws = next(table for table in design["tables"] if table["name"] == "Screws")
assert screws["rows"] == [["M3", 5.5, 10], ["M4", 7, 16], ["M5", 8.5, 20]], screws
for surface in ["rule", "csv"]:
    image = work / f"table-{surface}-{frontend}-{scheme}.png"
    assert image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), image
print("Verified: table domains, three generated rows, rule mapping, CSV undo, geometry assertions and captures")
PY
  [ "$?" -eq 0 ] || exit 1
done
echo "PASS: table/configuration UI parity ($frontend, light and dark)"

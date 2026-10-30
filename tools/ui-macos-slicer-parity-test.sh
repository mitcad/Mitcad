#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/platform/macos/NativeDialogs app/platform/macos/NativeTestInput app/framework/Dialogs app/framework/CommandSession app/framework/Command.hpp app/framework/TestDriver tools/cli
# Recent print UI (#828-#830): identical local synthetic profiles and presets
# in Qt/AppKit, both appearances. No installed/user preset folder is read.
# Usage: ui-macos-slicer-parity-test.sh <app> <cli> <output> [qt|native|both]
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "usage: $0 <app> <cli> <output> [qt|native|both]" >&2
  exit 2
fi
cli=$2
frontends=${4:-both}
case "$frontends" in qt|native) ;; both) frontends='qt native' ;; *) exit 2 ;; esac
ui_init "$1" "$3"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 - "$MITCAD_TEST_WORK" <<'PY'
import json
from pathlib import Path
import sys
root = Path(sys.argv[1])
box = {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
        "length": 20, "width": 20, "height": 10, "operation": "new_body"}}
(root / "model.json").write_text(json.dumps([{"comment": "SPDX-License-Identifier: MIT"}, box]))
commands = [{"comment": "SPDX-License-Identifier: MIT"}, box,
    {"cmd": "create_print_profile", "kind": "printer", "name": "Bench X", "values": {
        "bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]], "max_print_height": 250,
        "nozzle_diameter": 0.4, "printer_settings_id": {"orcaslicer": "Bench X"}}},
    {"cmd": "create_print_profile", "kind": "process", "name": "Standard", "values": {
        "layer_height": 0.2, "perimeters": 3, "fill_density": 15}},
    {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile", "value": "bench-x"},
    {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "standard"},
    {"cmd": "export", "path": str(root / "plate.3mf"), "dialect": "orcaslicer"}]
(root / "plate.json").write_text(json.dumps(commands, indent=2) + "\n")
presets = root / "presets/user/default"
(presets / "process").mkdir(parents=True, exist_ok=True)
(presets / "machine").mkdir(parents=True, exist_ok=True)
(presets / "filament").mkdir(parents=True, exist_ok=True)
(presets / "process/Fine.json").write_text(json.dumps({"name": "Fine", "from": "User", "layer_height": "0.12"}))
(presets / "machine/Bench X.json").write_text(json.dumps({"name": "Bench X", "from": "User", "printable_height": "180"}))
(presets / "filament/Amber.json").write_text(json.dumps({"name": "Amber", "from": "User",
    "filament_type": ["PLA"], "filament_density": ["1.24"], "nozzle_temperature": ["215"]}))
(root / "folders.json").write_text(json.dumps([{"dialect": "bambu_studio", "path": str(root / "presets")}]))
PY
"$cli" run "$MITCAD_TEST_WORK/plate.json" --save "$MITCAD_TEST_WORK/plate.mitcad" > "$UI_OUT/setup-plate.log" 2>&1 || {
  cat "$UI_OUT/setup-plate.log" >&2
  exit 1
}
[ -s "$MITCAD_TEST_WORK/plate.3mf" ] || { echo 'FAIL: missing synthetic slicer project' >&2; exit 1; }
export MITCAD_TEST_SLICER_FOLDERS=$(cat "$MITCAD_TEST_WORK/folders.json")
for frontend in $frontends; do
  for scheme in light dark; do
    rm -rf "$MITCAD_SETTINGS_DIR"
    mkdir -p "$MITCAD_SETTINGS_DIR"
    export MITCAD_PRINT_LIBRARY_DIR=$UI_OUT/library-$frontend-$scheme
    # This directory contains only this case's synthetic imports. A rerun
    # must not reclassify yesterday's new presets as same-name conflicts.
    rm -rf "$MITCAD_PRINT_LIBRARY_DIR"
    mkdir -p "$MITCAD_PRINT_LIBRARY_DIR"
    model=$MITCAD_TEST_WORK/model-$frontend-$scheme.mitcad
    "$cli" run "$MITCAD_TEST_WORK/model.json" --save "$model" > "$UI_OUT/setup-$frontend-$scheme.log" 2>&1 || {
      cat "$UI_OUT/setup-$frontend-$scheme.log" >&2
      exit 1
    }
    script=$UI_OUT/slicer-$frontend-$scheme.mitcad-ui
    python3 "$here/ui-scripts/slicer-parity-script.py" "$frontend" > "$script"
    args=(--ui="$frontend" --no-recovery --no-mcp --open "$model")
    # Qt's regular widget chooser provides the Qt reference path; explicit
    # native mode always uses its actual AppKit NSOpenPanel.
    [ "$frontend" != qt ] || args+=(--no-native-dialogs)
    MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=90 MITCAD_TEST_INPUT=$script \
      ui_run_app "slicer-$frontend-$scheme" "${args[@]}"
    ui_expect_log 'TestDriver: done' "$frontend/$scheme: print actions, file/preset imports and undo completed"
    for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'Unable to simultaneously satisfy constraints'; do
      ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
    done
    [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
    python3 - "$model" "$MITCAD_PRINT_LIBRARY_DIR" "$MITCAD_TEST_WORK" "$frontend" "$scheme" <<'PY'
import json
from pathlib import Path
import sys
model, library, work = map(Path, sys.argv[1:4])
frontend, scheme = sys.argv[4:]
design = json.loads(model.read_text())
assert not design.get("print_profiles"), "undo left imported/created profiles in the design"
fine = json.loads((library / "print/processes/fine.json").read_text())
assert abs(fine["profile"]["values"]["layer_height"] - 0.12) < 1e-9
copy = json.loads((library / "print/processes/fine-2.json").read_text())
assert copy["profile"]["name"] == "Fine 2"
assert abs(copy["profile"]["values"]["layer_height"] - 0.12) < 1e-9
amber = json.loads((library / "materials/amber.json").read_text())
assert amber["material"]["name"] == "Amber"
assert abs(amber["material"]["density"] - 1.24) < 1e-9
assert abs(amber["material"]["filament"]["temperature"] - 215) < 1e-9
assert not (library / "print/printers/bench-x.json").exists(), "the unticked printer was imported"
for surface in ["file", "review", "preset", "library", "copy-review", "material"]:
    image = work / f"slicer-{surface}-{frontend}-{scheme}.png"
    assert image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), f"missing {surface} screenshot"
print("Verified: review cancellation, design undo, unchanged geometry, Fine and its copy, material classification, excluded printer and captures")
PY
  done
done
echo "PASS: recent print UI and slicer import parity ($frontends, light and dark)"

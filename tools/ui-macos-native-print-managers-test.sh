#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/platform/macos/NativeDialogs app/platform/macos/NativeTableCells
# Native managers by place, using a synthetic project and an entirely local
# base library. No network, installed libraries or personal profiles are used.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK
MITCAD_TEST_WORK=$(mktemp -d "$UI_OUT/files.XXXXXX")
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
# The destination controls are tested; scanning installed slicers is unrelated.
export MITCAD_TEST_SLICER_FOLDERS='[]'
export MITCAD_TEST_PROJECT=$MITCAD_TEST_WORK/project
base=$MITCAD_TEST_WORK/base-library
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_LIBRARIES_DIR" "$MITCAD_PRINT_LIBRARY_DIR"
"$cli" project init "$MITCAD_TEST_PROJECT" --no-history > "$UI_OUT/setup.log" 2>&1
"$cli" library init "$base" --id mitcad-parts --name 'Manager fixtures' --license MIT >> "$UI_OUT/setup.log" 2>&1
python3 - "$base" "$MITCAD_TEST_WORK" "$MITCAD_TEST_PROJECT" <<'PY'
import json
from pathlib import Path
import sys

base, work, project = map(Path, sys.argv[1:])
manifest = json.loads((base / 'mitcad-library.json').read_text())
manifest['library_version'] = '1.0.0'
for kind, key, value in [('printer', 'nozzle_diameter', 0.4), ('process', 'layer_height', 0.2)]:
    identifier = 'builtin-' + kind
    collection = {'printer': 'printers', 'process': 'processes'}[kind]
    relative = f'print/{collection}/{identifier}.json'
    item = {'format': 'mitcad-print-item', 'version': 1, 'profile': {
        'id': identifier, 'name': 'Builtin ' + kind, 'kind': kind,
        'values': {key: value}, 'version': 1}}
    path = base / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(item))
    manifest[collection] = [{'id': identifier, 'path': relative}]
(base / 'mitcad-library.json').write_text(json.dumps(manifest))
commands = [{'cmd': 'add_feature', 'def': {'type': 'box', 'plane': 'xy', 'corner': [0, 0],
             'length': 10, 'width': 10, 'height': 10, 'operation': 'new_body'}}]
for place, prefix in [('design', 'Design'), ('project', 'Project'), ('user', 'Library')]:
    location = {} if place == 'design' else {'library': place, 'project': str(project)}
    for kind, key, value in [('printer', 'nozzle_diameter', 0.4), ('process', 'layer_height', 0.2)]:
        values = {key: value}
        if place == 'user' and kind == 'printer':
            # Choosing this real profile through the manager changes the
            # design dialect and rebuilds the settings form after a drag.
            values['printer_settings_id'] = {'orcaslicer': 'Fixture printer'}
        commands.append({'cmd': 'create_print_profile', 'kind': kind,
                         'name': prefix + ' ' + kind, 'values': values, **location})
    commands.append({'cmd': 'create_material', 'id': place + '-material',
                     'name': prefix + ' material', 'class': 'plastic', 'density': 1.2,
                     'appearance': 'paint_red', 'appearances': ['paint_red', 'paint_blue'], **location})
(work / 'setup.json').write_text(json.dumps(commands))
PY
# library init creates the repository; fetch reads the committed local snapshot.
git -C "$base" -c user.name='Mitcad developers' -c user.email=devs@mitcad.org add -A
git -C "$base" -c user.name='Mitcad developers' -c user.email=devs@mitcad.org commit -qm 'Synthetic manager profiles'
git -C "$base" tag v1.0.0
"$cli" library fetch "$base" >> "$UI_OUT/setup.log" 2>&1
"$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_PROJECT/model.mitcad" >> "$UI_OUT/setup.log" 2>&1
python3 "$here/ui-scripts/native-print-managers-script.py" project > "$UI_OUT/project.mitcad-ui"
MITCAD_TEST_INPUT=$UI_OUT/project.mitcad-ui ui_run_app project --ui=native --no-recovery --no-mcp \
  --open "$MITCAD_TEST_PROJECT/model.mitcad"
ui_expect_log 'TestDriver: done' 'native project managers completed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed project manager assertions'
[ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
# Read the actual library files after the native editor committed its fields.
python3 - "$MITCAD_TEST_PROJECT/print-library" "$MITCAD_PRINT_LIBRARY_DIR" <<'PY'
import json
from pathlib import Path
import sys

project, user = map(Path, sys.argv[1:])
for kind, key, value in [('printer', 'nozzle_diameter', 0.6), ('process', 'layer_height', 0.16)]:
    collection = {'printer': 'printers', 'process': 'processes'}[kind]
    item = json.loads((project / f'print/{collection}/project-{kind}.json').read_text())['profile']
    assert item['name'] == 'Edited ' + kind, item
    assert abs(item['values'][key] - value) <= 1e-6, item
    copies = [json.loads(path.read_text())['profile']
              for path in (user / f'print/{collection}').glob('*.json')]
    copies = [profile for profile in copies if profile['name'] == item['name']]
    assert len(copies) == 1, copies
    copied = copies[0]
    assert abs(copied['values'][key] - value) <= 1e-6, copied
print('PASS: native project edits and Copy to wrote the intended library files')
PY
export MITCAD_SETTINGS_DIR=$UI_ROOT/no-design-settings
mkdir -p "$MITCAD_SETTINGS_DIR"
python3 "$here/ui-scripts/native-print-managers-script.py" no-design > "$UI_OUT/no-design.mitcad-ui"
MITCAD_TEST_INPUT=$UI_OUT/no-design.mitcad-ui ui_run_app no-design --ui=native --no-recovery --no-mcp
ui_expect_log 'TestDriver: done' 'native managers without a design completed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed no-design assertions'
[ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
echo 'PASS: native Materials, Printers and Processes by place'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSessionAuto app/commands/ExtrudeCommands.cpp core/model/src/document/auto_operation.rs
# check-all sources: app/platform/macos/NativeForms app/platform/macos/NativeMacWindow app/platform/macos/NativeTestInput
# check-all sources: app/framework/ExtrudeStartPreference.cpp
# Edits keep their stored operation (mitcad#868) through the chosen panel:
# a Join going into its bodies, deliberate choices, target scope, an
# input while the first preview computes. In the Qt frontend also
# Preferences > General > Extrude starts with: New Body (mitcad#2164,
# ui-scripts/extrude-starts-new-body.mitcad-ui): a square on a block's face
# extruded inward and outward stays New Body.
# Usage: bash tools/ui-macos-edit-auto-operation-test.sh <app> [out directory] [qt|native]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> [out directory] [qt|native]" >&2
  exit 2
fi
frontend=${3:-qt}
case "$frontend" in
  qt) schemes=default; extra_phases=prefer-new-body ;;
  native) schemes='light dark'; extra_phases= ;;
  *) echo "unknown frontend: $frontend" >&2; exit 2 ;;
esac
ui_init "$1" "${2:-ui-macos-edit-auto-operation}"
UI_TIMEOUT=150
cli=${MITCAD_UI_CLI:-$(dirname "$UI_APP")/../../../../tools/cli/mitcad-cli}
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"

for scheme in $schemes; do
 for phase in automatic new-body participants overrides scope early $extra_phases; do
  run=$phase
  if [ "$frontend" = native ]; then run=$phase-$scheme; fi
  # Variants keep identical geometry; only the stored Operation/Objects
  # differ, and each edit keeps them.
  python3 - "$here/ui-scripts/edit-auto-operation.json" "$MITCAD_TEST_WORK/setup.json" "$phase" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    script = json.load(source)
feature = next(c['def'] for c in script if c.get('cmd') == 'add_feature'
               and c['def']['type'] == 'extrude')
phase = sys.argv[3]
if phase in ('new-body', 'early'):
    feature['operation'] = 'new_body'
    del feature['participants']
elif phase == 'participants':
    feature['participants'] = ['F1.b0']
elif phase == 'scope':
    feature['operation'] = 'cut'
    feature['extent']['distance'] = -15
    del feature['participants']
    feature['cut_scope'].pop('initialize_on_cut')
elif phase == 'prefer-new-body':
    # A 40 x 30 x 20 block under XY (its top face in XY) and a 10 x 10
    # square on that face, nothing else (mitcad#2164).
    script = [script[0],
              {'cmd': 'add_feature', 'def': {'type': 'box', 'plane': {'origin': [0, 0, -20], 'normal': [0, 0, 1]},
                                             'corner': [0, 0], 'length': 40, 'width': 30, 'height': 20,
                                             'operation': 'new_body'}},
              {'cmd': 'sketch.create'},
              {'cmd': 'sketch.add_rectangle', 'sketch': 'F2', 'corner': [10, 10], 'width': 10, 'height': 10},
              {'expect': {}}]
expected = script[-1]['expect']
expected['instances'] = {'automatic': 2, 'new-body': 4, 'participants': 3,
                         'overrides': 2, 'scope': 3, 'early': 4, 'prefer-new-body': 1}[phase]
expected['world_volume'] = {'automatic': 27000, 'overrides': 27000,
                            'scope': 34000, 'prefer-new-body': 24000}.get(phase, 39000)
with open(sys.argv[2], 'w', encoding='utf-8') as target:
    json.dump(script, target)
PY
  [ "$?" = 0 ] || exit 1
  "$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_WORK/edit-auto.mitcad" \
    > "$UI_OUT/setup-$run.log" 2>&1 || { cat "$UI_OUT/setup-$run.log" >&2; exit 1; }
  case "$phase" in
    automatic) driver=edit-auto-operation; delay=0 ;;
    new-body|participants) driver=edit-auto-operation-retained; delay=0 ;;
    early) driver=edit-auto-operation-early; delay=0,preview=1500 ;;
    prefer-new-body) driver=extrude-starts-new-body; delay=0 ;;
    *) driver=edit-auto-operation-$phase; delay=0 ;;
  esac
  input=$here/ui-scripts/$driver.mitcad-ui
  if [ "$frontend" = native ]; then
    input=$UI_OUT/$run.mitcad-ui
    python3 "$here/ui-scripts/edit-auto-operation-native-script.py" "$phase" > "$input" || exit 1
    export MITCAD_TEST_COLOR_SCHEME=$scheme
    export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$run
    mkdir -p "$MITCAD_SETTINGS_DIR"
  fi
  if [ "$phase" = prefer-new-body ]; then
    # Settings of the run's own with the preference set (mitcad#2164; with
    # MITCAD_SETTINGS_DIR the app's INI file is under its organization's
    # domain on macOS).
    export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$run
    mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
    printf '[extrude]\nstartOperation=new_body\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
  fi
  MITCAD_TEST_STEP_MS=20 MITCAD_TEST_RECOMPUTE_DELAY_MS=$delay \
    MITCAD_TEST_INPUT=$input \
    ui_run_app "$run" --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/edit-auto.mitcad"
  ui_expect_log '^TestDriver: done' "$run: completed every driver assertion"
  ui_expect_no_log 'TestDriver: FAIL' "$run: no driver failure"
  if [ "$frontend" = native ]; then
    ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$run: actual AppKit frontend"
    ui_expect_log 'Native pointer double click: native.history.F4 click-count=2' "$run: actual history edit input"
  else
    ui_expect_no_log 'Native UI: AppKit shell with embedded OCCT viewport' "$run: primary Qt frontend"
  fi
  # Ignore echoed driver expressions; expect-never checks runtime messages
  # in a separate recording that excludes TestDriver lines.
  for message in 'The document was read while a job computes it' 'OCCT view event failed' 'Recompute failed'; do
    if awk -v text="$message" 'index($0,"TestDriver:") != 1 && index($0,text) { found=1 }
        END { exit !found }' "$UI_LOG_FILE"; then
      ui_fail "$run: $message"
    fi
  done
  [ "$UI_FAILURES" = 0 ] || exit 1
  if [ "$phase" = automatic ] || [ "$phase" = new-body ] || [ "$phase" = participants ] || [ "$phase" = scope ]; then
    python3 - "$MITCAD_TEST_WORK/edit-auto.mitcad" "$phase" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    feature = next(f for f in json.load(source)['features'] if f['uid'] == 'F4')
phase = sys.argv[2]
assert feature['operation'] == {'automatic': 'join', 'participants': 'join', 'scope': 'cut'}.get(phase, 'new_body'), feature
assert feature.get('participants', []) == {'automatic': ['F1.b0', 'F2.b0'], 'participants': ['F1.b0']}.get(phase, []), feature
assert not set(feature.get('participants', [])).intersection(('F4.b0', 'F5.b0')), feature
if phase == 'scope':
    targets = feature['cut_scope']['targets']
    assert targets['type'] == 'automatic' and not targets['excluded'], targets
    assert targets['excluded_bodies'] == [{'component': 'C0', 'occurrence': '', 'body': 'F2.b0'}], targets
PY
    [ "$?" = 0 ] || { ui_fail "$run: persisted policy changed"; exit 1; }
    if [ "$frontend" = native ]; then
      cp "$MITCAD_TEST_WORK/edit-auto.mitcad" "$MITCAD_TEST_WORK/edit-auto-$run.mitcad" || exit 1
    fi
  fi
 done
done
echo "PASS: $frontend edit automatic operation; logs in $UI_OUT"

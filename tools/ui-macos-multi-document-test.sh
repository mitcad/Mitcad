#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/WindowManager app/DocumentFrame app/framework/DocumentSession app/framework/TestDriver
# check-all sources: app/files/Autosave app/files/Recovery app/mcp app/render
# Actual macOS Qt/AppKit document workflows. The normal UI harness owns the
# shared Aqua lock; its client process uses only the real stdio MCP bridge.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 5 ]; then
  echo 'usage: ui-macos-multi-document-test.sh <app> <cli> <output> [qt|native] [all|frames|mcp]' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
mode=${5:-all}
case "$frontend/$mode" in qt/all|native/all|qt/mcp|native/mcp|qt/frames) ;; *) echo 'expected qt|native all|mcp, or qt frames' >&2; exit 2 ;; esac
UI_TIMEOUT=240
client_pid=""
multi_cleanup() {
  if [ -d "$UI_ROOT/crashes" ]; then
    mkdir -p "$UI_OUT/crashes"
    cp -R "$UI_ROOT/crashes/." "$UI_OUT/crashes/"
  fi
  if [ -n "$client_pid" ]; then
    kill -TERM "$client_pid" 2>/dev/null || true
    wait "$client_pid" 2>/dev/null || true
  fi
  ui_cleanup
}
trap multi_cleanup EXIT
export MITCAD_TEST_AUTOSAVE_SECONDS=1
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
mkdir -p "$MITCAD_LIBRARIES_DIR"
unset MITCAD_TEST_DOCUMENT_FRAMES

new_case() {
  local name=$1
  export MITCAD_TEST_WORK=$UI_OUT/files/$name
  export MITCAD_SETTINGS_DIR=$UI_ROOT/$name/settings
  export MITCAD_AUTOSAVE_DIR=$UI_ROOT/$name/autosave
  export MITCAD_MCP_DISCOVERY_DIR=$UI_ROOT/$name/mcp
  mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_SETTINGS_DIR" "$MITCAD_AUTOSAVE_DIR"
  python3 - "$MITCAD_TEST_WORK" <<'PY'
import json
from pathlib import Path
import sys
work = Path(sys.argv[1])
for name, dimensions in [('first', (10, 20, 30)), ('second', (4, 5, 6)), ('third', (3, 3, 3))]:
    length, width, height = dimensions
    commands = [{'cmd': 'add_feature', 'def': {'type': 'box', 'plane': {'type': 'origin', 'plane': 'xy'}, 'corner': [0, 0],
                 'length': length, 'width': width, 'height': height, 'operation': 'new_body'}}]
    (work / (name + '.json')).write_text(json.dumps(commands))
PY
  local namepart
  for namepart in first second third; do
    "$cli" run "$MITCAD_TEST_WORK/$namepart.json" --save "$MITCAD_TEST_WORK/$namepart.mitcad" \
      >> "$UI_OUT/setup.log" 2>&1 || { ui_fail 'synthetic CLI fixture generation'; exit 1; }
  done
}
check_run() {
  if grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG_FILE"; then
    ui_fail 'a document assertion failed'
  fi
  ui_expect_no_log 'OCCT view event failed' 'no failed view operation'
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
}
workflow() {
  local name=$1 scenario=$2
  shift 2
  python3 "$here/ui-scripts/multi-document-script.py" "$MITCAD_TEST_WORK" "$scenario" "$frontend" > "$UI_OUT/$name.mitcad-ui"
  MITCAD_TEST_INPUT=$UI_OUT/$name.mitcad-ui ui_run_app "$name" --ui="$frontend" --no-native-dialogs --no-mcp "$@"
  check_run
}

if [ "$mode" = frames ]; then
  export MITCAD_TEST_DOCUMENT_FRAMES=1
  for chrome in floating docked; do
    new_case "frames-$chrome"
    python3 "$here/ui-scripts/multi-document-frames-script.py" "$MITCAD_TEST_WORK" full > "$UI_OUT/frames-$chrome.mitcad-ui"
    MITCAD_TEST_INPUT=$UI_OUT/frames-$chrome.mitcad-ui ui_run_app "frames-$chrome" --ui=qt --chrome="$chrome" \
      --no-native-dialogs --no-mcp --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
    check_run
    ui_expect_log '^TestDriver: (done|quit)' 'portable frame workflow completed'
    ui_expect_log '^TestDriver: hidden document quiet: .*0 frameSwapped events' 'hidden document stopped presenting GL frames'
  done
  new_case frames-close-all
  python3 "$here/ui-scripts/multi-document-frames-script.py" "$MITCAD_TEST_WORK" close-all > "$UI_OUT/frames-close-all.mitcad-ui"
  MITCAD_TEST_INPUT=$UI_OUT/frames-close-all.mitcad-ui ui_run_app frames-close-all --ui=qt --no-native-dialogs --no-mcp \
    --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
  check_run
  ui_expect_log '^TestDriver: observed Close All: 3 accepted document closes, empty process registry$' 'actual Close All removed all three owners'
  if grep -Eq '^TestDriver: done \([0-9]+ steps\)$' "$UI_LOG_FILE"; then
    ui_fail 'Close All reached the driver exit instead of staying for natural shutdown'
  fi
elif [ "$mode" = all ]; then
  new_case current-window
  workflow current-window current-window --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
  ui_expect_log '^TestDriver: quit' 'actual current-window preference kept the same owner for Open and New'
  new_case basic
  workflow basic basic --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
  ui_expect_log '^TestDriver: quit' 'basic independent document workflow completed'
  new_case quit
  workflow quit quit --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
  ui_expect_log '^Document windows closed: empty process registry$' 'Quit naturally closed every document'
  if grep -Eq '^TestDriver: done \([0-9]+ steps\)$' "$UI_LOG_FILE"; then
    ui_fail 'Quit reached the driver exit instead of staying for natural shutdown'
  fi
  new_case recovery
  workflow crash crash --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
  ui_expect_log '^TestDriver: kill' 'owned process stopped without normal document cleanup'
  cp -R "$MITCAD_AUTOSAVE_DIR" "$MITCAD_TEST_WORK/crash-autosave"
  python3 - "$MITCAD_AUTOSAVE_DIR" "$MITCAD_TEST_WORK" <<'PY'
import json
from pathlib import Path
import sys
folder, work = map(Path, sys.argv[1:])
records = list(folder.glob('*.json'))
assert len(records) == 2, records
sessions = {}
for record in records:
    about = json.loads(record.read_text())
    name = Path(about['path']).stem
    assert name in ('first', 'second') and Path(about['path']) == work / (name + '.mitcad'), about
    assert about['format'] == 'mitcad-autosave' and about['version'] == 2, about
    assert (folder / (about['session'] + '.lock')).is_file(), about
    snapshot = folder / about['project']
    assert snapshot.is_file() and snapshot.stat().st_size == about['size'], about
    document = json.loads(snapshot.read_text())
    own, other, value = ('firstCrash', 'secondCrash', 11) if name == 'first' else ('secondCrash', 'firstCrash', 22)
    parameters = {p['name']: p['value'] for p in document['parameters']}
    assert abs(parameters[own] - value) <= 1e-6 and other not in parameters, parameters
    sessions[name] = about['session']
assert set(sessions) == {'first', 'second'} and len(set(sessions.values())) == 2, sessions
(work / 'crashed-sessions.json').write_text(json.dumps(sessions))
PY
  [ "$?" = 0 ] || { ui_fail 'two exact document snapshots and locks'; exit 1; }
  workflow recovery recovery
  ui_expect_log '^TestDriver: quit' 'both recovered designs validated and saved'
  python3 - "$MITCAD_TEST_WORK" <<'PY'
import json
from pathlib import Path
import sys
work = Path(sys.argv[1])
for name, own, other, value in [('first', 'firstCrash', 'secondCrash', 11), ('second', 'secondCrash', 'firstCrash', 22)]:
    doc = json.loads((work / (name + '.mitcad')).read_text())
    parameters = {p['name']: p['value'] for p in doc['parameters']}
    assert abs(parameters[own] - value) <= 1e-6 and other not in parameters, parameters
PY
  [ "$?" = 0 ] || { ui_fail 'recovered documents saved independently to their original paths'; exit 1; }

fi

if [ "$mode" != frames ]; then
  new_case mcp
  # File gates belong to this launch. Earlier evidence in the output folder
  # must not let the driver close a document before the new client reads it.
  rm -f "$MITCAD_TEST_WORK"/mcp-{first-ready,two-ready,first-active,second-active,finished}
  python3 - "$here" "$MITCAD_TEST_WORK" > "$UI_OUT/mcp.mitcad-ui" <<'PY'
import importlib.util
from pathlib import Path
import sys
spec = importlib.util.spec_from_file_location('mcp_documents', Path(sys.argv[1]) / 'ui-mcp-documents-test.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
print(module.scenario(sys.argv[2]), end='')
PY
  rm -f "$UI_OUT/mcp.log"
  # Start the client's timeout only once ui_run_app acquired the shared GUI
  # lock and launched its app; another checkout may have held that lock.
  (while [ ! -s "$UI_OUT/mcp.log" ]; do sleep 0.1; done
   exec python3 "$here/ui-mcp-documents-test.py" "$cli" "$MITCAD_TEST_WORK") > "$UI_OUT/mcp-client.log" 2>&1 &
  client_pid=$!
  MITCAD_TEST_INPUT=$UI_OUT/mcp.mitcad-ui ui_run_app mcp --ui="$frontend" --no-native-dialogs --no-recovery --mcp-folder "$MITCAD_TEST_WORK"
  client_status=0
  wait "$client_pid" || client_status=$?
  client_pid=""
  [ "$client_status" = 0 ] || { ui_fail "real multi-document MCP client ($client_status)"; cat "$UI_OUT/mcp-client.log"; }
  check_run
  ui_expect_log '^TestDriver: quit' 'MCP client and actual window workflow completed'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend multi-document $mode"

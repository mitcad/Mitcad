#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/WindowManager app/files/FileOpenFilter app/MainWindowNative app/platform/macos/NativeMacWindow
# Ordinary LaunchServices opens target the already-running built bundle.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo 'usage: ui-macos-document-open-test.sh <app> <cli> <output> [qt|native]' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
frontend=${4:-native}
case "$frontend" in qt|native) ;; *) echo 'expected qt or native' >&2; exit 2 ;; esac
UI_TIMEOUT=210
sender_pid=""
document_open_cleanup() {
  if [ -n "$sender_pid" ]; then
    kill -TERM "$sender_pid" 2>/dev/null || true
    wait "$sender_pid" 2>/dev/null || true
  fi
  ui_cleanup
}
trap document_open_cleanup EXIT
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
unset MITCAD_TEST_DOCUMENT_FRAMES
# These gates belong to this exact launch, including repeated scoped runs.
rm -f "$MITCAD_TEST_WORK"/{first-ready.png,second-ready.png,reopened-first.png,second-sent,first-sent,process-verified,system-open-processes.json}
python3 - "$MITCAD_TEST_WORK" <<'PY'
import json
from pathlib import Path
import sys
work = Path(sys.argv[1])
for name, size in [('first', (10, 20, 30)), ('second', (4, 5, 6))]:
    length, width, height = size
    command = {'cmd': 'add_feature', 'def': {'type': 'box', 'plane': {'type': 'origin', 'plane': 'xy'},
               'corner': [0, 0], 'length': length, 'width': width, 'height': height, 'operation': 'new_body'}}
    (work / (name + '.json')).write_text(json.dumps([command]))
PY
[ "$?" = 0 ] || { ui_fail 'synthetic box command generation'; exit 1; }
for name in first second; do
  "$cli" run "$MITCAD_TEST_WORK/$name.json" --save "$MITCAD_TEST_WORK/$name.mitcad" \
    >> "$UI_OUT/setup.log" 2>&1 || { ui_fail 'synthetic box fixture generation'; exit 1; }
done
python3 "$here/ui-document-open-test.py" script "$MITCAD_TEST_WORK" > "$UI_OUT/system-open.mitcad-ui" || exit 1
rm -f "$UI_OUT/system-open.log"
python3 "$here/ui-document-open-test.py" deliver "$UI_APP" "$MITCAD_TEST_WORK" "$UI_OUT/system-open.log" \
  > "$UI_OUT/system-open-client.log" 2>&1 &
sender_pid=$!
MITCAD_TEST_INPUT=$UI_OUT/system-open.mitcad-ui ui_run_app system-open --ui="$frontend" --no-native-dialogs \
  --no-mcp --no-recovery --open "$MITCAD_TEST_WORK/first.mitcad"
sender_status=0
wait "$sender_pid" || sender_status=$?
sender_pid=""
if [ "$sender_status" != 0 ]; then
  ui_fail "system file-open companion ($sender_status)"
  cat "$UI_OUT/system-open-client.log"
fi
if grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG_FILE"; then ui_fail 'a system file-open assertion failed'; fi
ui_expect_log '^TestDriver: quit' 'system file-open workflow completed'
ui_expect_no_log 'Could not open' 'no false launch argument or genuine file-open failure'
ui_expect_no_log 'OCCT view event failed' 'no failed view operation'
for name in first second; do
  if grep -Fxq "File open event: $MITCAD_TEST_WORK/$name.mitcad" "$UI_LOG_FILE"; then
    ui_pass "actual OS file-open callback for $name"
  else
    ui_fail "missing actual OS file-open callback for $name"
  fi
done
[ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
echo "PASS: $frontend OS file opens, two retained owners and one built application process"

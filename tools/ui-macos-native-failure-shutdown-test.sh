#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeDialogs app/framework/TestDriver app/view/ViewController tools/ui-scripts/native-settings
# A failed assertion with a live Settings field editor exits1, without a
# signal or a harness kill. Ordinary close still commits the live editor.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
[ "$#" = 2 ] || { echo "usage: $0 <app> <output>" >&2; exit 2; }
ui_init "$1" "$2"
UI_TIMEOUT=30
# Keep unexpected crash stacks after the normal temporary-directory cleanup.
export MITCAD_CRASH_DIR=$UI_OUT/crashes
mkdir -p "$MITCAD_CRASH_DIR"
# ui_run_app reports every nonzero exit. The checks below qualify exactly
# one intentional assertion failure before accepting its exit code.
ui_fail() { UI_FAILURES=$((UI_FAILURES + 1)); echo "Observed failure candidate: $*"; }
for scheme in light dark; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scheme
  mkdir -p "$MITCAD_SETTINGS_DIR"
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/native-settings-failure.mitcad-ui \
    ui_run_app "failure-$scheme" --ui=native --demo --no-recovery --no-mcp
  [ "$UI_EXIT" = 1 ] && [ "$UI_TIMED_OUT" = 0 ] && [ "$UI_ASSERTION_FAILED" = 0 ] && \
    [ -z "$UI_SIGNAL" ] && [ "$UI_FAILURES" = 1 ] || { ui_log_tail; exit 1; }
  python3 - "$UI_LOG_FILE" "$MITCAD_CRASH_DIR" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text()
failures = re.findall(r'^TestDriver: FAIL.*$', text, re.M)
assert failures == ['TestDriver: FAIL line 5: expected 2 bodies with total volume 48000 mm3 (1 bodies, total volume 48000 mm3)'], failures
assert not re.search(r'^TestDriver: done|^Mitcad crashed:', text, re.M)
assert not any(Path(sys.argv[2]).iterdir()), 'unexpected retained crash report'
PY
  [ "$?" = 0 ] || exit 1
  UI_FAILURES=0
  echo "Verified $scheme: intended assertion preserved; natural exit1, no signal, timeout or harness kill"
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/native-settings-close-edit.mitcad-ui \
    ui_run_app "close-$scheme" --ui=native --demo --no-recovery --no-mcp
  ui_expect_log '^TestDriver: done' "$scheme ordinary close commits the field and reopens"
  ui_expect_no_log 'TestDriver: FAIL' "$scheme successful workflow"
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  cp -R "$MITCAD_SETTINGS_DIR" "$UI_OUT/settings-$scheme"
done
echo "PASS: native Settings assertion-failure shutdown and ordinary close editing"

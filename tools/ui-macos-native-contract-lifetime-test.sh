#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/TestDriver app/platform/NativeTestInput.hpp app/platform/macos/NativeTestInput.mm app/platform/macos/NativeDialogs app/platform/macos/NativeTableCells app/files/RemotePreferences tools/ui-scripts/native-contract tools/ui-scripts/native-cloud-header-resize
# Nested contracts finish before later steps or process exit. Actual Cloud
# header divider drags retain their width through an ordinary row refresh.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
[ "$#" = 2 ] || { echo "usage: $0 <app> <output>" >&2; exit 2; }
ui_init "$1" "$2"
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_LIBRARIES_DIR"
for scheme in light dark; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scheme
  mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
  cat > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini" <<'INI'
[remote]
allowLive=false
autoPush=false
[live]
brokers\1\address=mqtt://a.invalid:1883
brokers\1\trusted=true
brokers\2\address=mqtt://b.invalid:1883
brokers\2\trusted=false
brokers\size=2
INI
  for phase in lifetime widget-refresh cloud; do
    case "$phase" in
      lifetime) input=$here/ui-scripts/native-contract-lifetime.mitcad-ui ;;
      widget-refresh) input=$here/ui-scripts/native-contract-widget-refresh.mitcad-ui ;;
      cloud) input=$here/ui-scripts/native-cloud-header-resize.mitcad-ui ;;
    esac
    MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$input \
      ui_run_app "$phase-$scheme" --ui=native --demo --no-recovery --no-mcp
    ui_expect_log '^TestDriver: done' "$phase/$scheme natural completion"
    ui_expect_no_log 'TestDriver: FAIL' "$phase/$scheme assertions passed"
    [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  done
  cp -R "$MITCAD_SETTINGS_DIR" "$UI_OUT/settings-$scheme"
  python3 - "$UI_OUT/lifetime-$scheme.log" "$UI_OUT/widget-refresh-$scheme.log" <<'PY'
import re
import sys

for path, success in [(sys.argv[1], 'Native button order contract:'),
                      (sys.argv[2], 'Native widget refresh:')]:
    text = open(path, encoding='utf-8').read()
    complete = re.search(r'^' + re.escape(success) + r'.*verified.*$', text, re.M)
    done = re.search(r'^TestDriver: done', text, re.M)
    assert complete and done and complete.start() < done.start(), path
    # Completion of the fixture precedes even the next model/log assertion.
    next_step = re.search(r'^TestDriver: line 4:', text, re.M)
    assert next_step and complete.start() < next_step.start(), path
PY
  [ "$?" = 0 ] || exit 1
done
echo "PASS: native nested contract lifetime and actual Cloud header resize retention; $UI_OUT"

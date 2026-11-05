#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands app/framework/TestDriver app/MainWindow app/view/ViewController
# Shared canvas operations through actual Qt/AppKit viewport input.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
UI_TIMEOUT=240
for scheme in light dark; do
  export MITCAD_TEST_WORK=$UI_OUT/$scheme
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scheme
  mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_SETTINGS_DIR"
  "$cli" run "$here/ui-scripts/sketch-parity.json" --save "$MITCAD_TEST_WORK/sketch-parity.mitcad" \
    > "$MITCAD_TEST_WORK/setup.log" 2>&1 || { cat "$MITCAD_TEST_WORK/setup.log" >&2; exit 1; }
  if [ "$frontend" = native ]; then
    { printf "native-appearance '%s'\n" "$scheme"
      sed 's/^ENTER_SKETCH$/native-activate native.history.F2/' "$here/ui-scripts/sketch-parity.mitcad-ui"
    } > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
  else
    sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
      "$here/ui-scripts/sketch-parity.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
  fi
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
    ui_run_app "$scheme" --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/sketch-parity.mitcad"
  ui_expect_log 'TestDriver: done' "$frontend/$scheme: pointer, undo and save steps completed"
  ui_expect_no_log 'TestDriver: FAIL' "$frontend/$scheme: no failed driver assertions"
  if ! python3 "$here/ui-scripts/sketch-parity-verify.py" "$MITCAD_TEST_WORK"; then
    ui_fail "$frontend/$scheme: persisted sketch and query verification failed"
  fi
  [ "$UI_FAILURES" = 0 ] || exit 1
done
echo "PASS: $frontend sketch group drag, spline point insertion and origin-plane creation"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/Configuration app/configurations app/platform/macos app/framework/TestDriver
# Configuration controls and the rule canvas in both macOS frontends, with
# synthetic geometry, actual field edits and both appearances.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output> qt|native" >&2
  exit 2
fi
cli=$2
frontend=$4
case "$frontend" in qt|native) ;; *) exit 2 ;; esac
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# A new design's empty Options and Rows hints (NativeEmptyTextProperty),
# first: they need no WindowServer input, which the slider drag below does.
if [ "$frontend" = native ]; then
  for scheme in light dark; do
    rm -rf "$MITCAD_SETTINGS_DIR"
    mkdir -p "$MITCAD_SETTINGS_DIR"
    MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=70 \
      MITCAD_TEST_INPUT=$here/ui-scripts/macos-native-configurations-empty.mitcad-ui \
      ui_run_app "native-empty-$scheme" --ui=native --no-recovery --no-mcp
    ui_expect_log '^TestDriver: done' "native/$scheme: the empty Options and Rows show their hints"
    for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:'; do
      ui_expect_no_log "$problem" "native/$scheme empty: no $problem"
    done
    [ "$UI_FAILURES" = 0 ] || exit 1
  done
fi
for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  if ! "$cli" run "$here/ui-scripts/configurations.json" \
      --save "$MITCAD_TEST_WORK/configurations-$scheme.mitcad" > "$UI_OUT/setup-$scheme.log" 2>&1; then
    cat "$UI_OUT/setup-$scheme.log"
    exit 1
  fi
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=70 \
    MITCAD_TEST_INPUT=$here/ui-scripts/macos-$frontend-configurations.mitcad-ui \
    ui_run_app "$frontend-$scheme" --ui="$frontend" --no-recovery --no-mcp \
      --open "$MITCAD_TEST_WORK/configurations-$scheme.mitcad"
  ui_expect_log '^TestDriver: done' "$frontend/$scheme: configuration and rule interactions completed"
  ui_expect_log '^Configuration chosen: Length = 1200' "$frontend/$scheme: the field changes the geometry"
  ui_expect_log '^Rules saved: Set then\.0\.0 to 900 mm' "$frontend/$scheme: the rule block edit was saved"
  ui_expect_log 'Configuration bar: Color = Black .*Length = 900 mm' "$frontend/$scheme: the edited rule constrains the chosen option"
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:'; do
    ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
  done
  [ "$UI_FAILURES" = 0 ] || exit 1
done
echo "PASS: macOS configuration and rule parity ($frontend, light and dark)"

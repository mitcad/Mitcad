#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view app/OcctViewer app/framework/TestDriver tools/cli
# Real navigation settings, a process restart, and camera gestures.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> [output directory] [qt|native]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-navigation}"
UI_TIMEOUT=180
frontend=${3:-qt}
case "$frontend" in
  qt) ;;
  native)
    MITCAD_TEST_INPUT=$here/ui-scripts/navigation-release.mitcad-ui \
      ui_run_app native-release --ui=native --no-recovery --no-mcp --demo
    ui_expect_log 'Navigation release: both modifier/button orders' 'native release ordering and camera state'
    ui_expect_log 'TestDriver: done' 'native edge preselection, Fillet and Chamfer completed'
    ui_expect_no_log '^TestDriver: FAIL' 'native release and multi-selection assertions passed'
    [ "$UI_FAILURES" = 0 ] || exit 1
    echo "PASS: native navigation release ordering; evidence in $UI_OUT"
    exit 0 ;;
  *) echo 'frontend must be qt or native' >&2; exit 2 ;;
esac
for chrome in floating docked; do
  for appearance in light dark; do
    name=$chrome-$appearance
    export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$name
    export MITCAD_TEST_SHOTS=$UI_OUT/$name
    mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_TEST_SHOTS"
    script=$UI_OUT/$name-configure.mitcad-ui
    cat "$here/ui-scripts/navigation-configure.mitcad-ui" > "$script"
    if [ "$name" = floating-light ]; then
      echo 'expect-navigation-runtime' >> "$script"
      cat "$here/ui-scripts/navigation-release.mitcad-ui" >> "$script"
    fi
    # Keys, scrolls and the activation during a press (mitcad#1571-#1573),
    # in both layouts: the shared steps of the Linux and Windows tests.
    if [ "$appearance" = light ]; then
      cat "$here/ui-scripts/navigation-gesture-state.mitcad-ui" >> "$script"
    fi
    MITCAD_TEST_COLOR_SCHEME=$appearance MITCAD_TEST_INPUT=$script \
      ui_run_app "$name-configure" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --demo
    ui_expect_log 'Navigation controls: configure' "$name: custom bindings and speeds apply immediately"
    ui_expect_log 'TestDriver: done' "$name: configure completed"
    ui_expect_no_log '^TestDriver: FAIL' "$name: configure assertions passed"
    if [ "$name" = floating-light ]; then
      ui_expect_log 'Navigation runtime: actual pan, orbit, wheel zoom and trackpad camera motion follows speed' \
        'speed controls change the actual camera through real input events'
    fi
    if [ "$appearance" = light ]; then
      ui_expect_log 'Navigation gesture state: a deactivated window ended F4, Option and scroll drags' \
        "$name: keys, scrolls and deactivation during a press keep or end it as they should"
    fi
    [ "$UI_FAILURES" = 0 ] || exit 1
    MITCAD_TEST_COLOR_SCHEME=$appearance MITCAD_TEST_INPUT=$here/ui-scripts/navigation-restored.mitcad-ui \
      ui_run_app "$name-restored" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --demo
    ui_expect_log 'Navigation controls: restored' "$name: custom bindings and speeds survive a new process"
    ui_expect_log 'Navigation controls: reset' "$name: all four resets restore default speeds"
    ui_expect_log 'TestDriver: done' "$name: restore completed"
    ui_expect_no_log '^TestDriver: FAIL' "$name: restored assertions passed"
    [ "$UI_FAILURES" = 0 ] || exit 1
  done
done
echo "PASS: navigation controls, restart, release ordering and actual camera input; evidence in $UI_OUT"

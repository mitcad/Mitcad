#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/GlassCard.cpp app/framework/TestDriver.cpp
# check-all sources: app/MainWindow.cpp app/view/ViewController.cpp app/commands/ExtrudeCommands.cpp tools/cli
# check-all sources: tools/ui-scripts/panel-first-field-keys.mitcad-ui tools/ui-scripts/panel-first-field-keys.json
# check-all sources: tools/ui-scripts/panel-first-field-keys-floated.mitcad-ui
# The keys typed as a command panel opens go to its selected value field
# (mitcad#2391, GitHub mitcad/Mitcad#617) through the real UI: on a design
# that mitcad-cli makes (tools/ui-scripts/panel-first-field-keys.json, a
# sketch with one circle), E in the edited sketch opens Extrude with Distance
# selected; 25 typed without a click and Enter add a 25 mm extrusion and the
# view does not turn; then 1 turns the view to the front, the 3D view having
# the keys again. In the Floating and the Docked layout
# (tools/ui-scripts/panel-first-field-keys.mitcad-ui) and with the Docked
# layout's Command panel floated, a window of its own
# (tools/ui-scripts/panel-first-field-keys-floated.mitcad-ui), the case
# reported.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panel-first-field-keys-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-panel-first-field-keys.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/panel-first-field-keys.json" --save "$work/panel-first-field-keys.mitcad" || exit 1
ui_start_display

# first_field_keys name script [app arguments]: the script's steps in one
# layout, on a fresh copy of the design.
first_field_keys() {
  echo "=== First value field's keys, $1"
  export MITCAD_TEST_INPUT=$here/ui-scripts/$2
  shift 2
  cp "$work/panel-first-field-keys.mitcad" "$work/layout.mitcad" || { ui_fail 'the design could not be copied'; return; }
  ui_start_app --ui=qt --no-mcp "$@" --open "$work/layout.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'first value field keys driver timed out'; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'Panel Extrude: first value field (distance) focused; ' 'Extrude gives Distance the keyboard as it opens'
  ui_expect_log 'Added extrude (New Body), distance 25' '25 typed without a click is the distance'
  ui_expect_log 'TestDriver: done' 'the typed digits went to the field, not to the view, and the view keys work after OK'
}

first_field_keys "Floating" panel-first-field-keys.mitcad-ui --chrome=floating
first_field_keys "Docked" panel-first-field-keys.mitcad-ui --chrome=docked
first_field_keys "Docked, the Command panel floated" panel-first-field-keys-floated.mitcad-ui --chrome=docked
ui_finish 'UI first value field keys'

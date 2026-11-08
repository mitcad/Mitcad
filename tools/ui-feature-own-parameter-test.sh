#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowNative.cpp tools/cli
# check-all sources: core/model/src/document.rs core/model-base/src/values.rs core/model-api/src/lib.rs
# check-all sources: tools/ui-scripts/feature-own-parameter.mitcad-ui tools/ui-scripts/feature-own-parameter.json
# A command panel value that uses the field's own parameter keeps that
# parameter as a user parameter (mitcad#1536), through the real UI on a
# design that mitcad-cli makes (tools/ui-scripts/feature-own-parameter.json:
# Extrude1's distance is its own parameter depth = 10 mm): `dep` in Edit
# Extrude's Distance lists depth as this field's value, kept as a user
# parameter; depth * 2 gives the distance a new parameter of 20 mm and
# leaves depth a user parameter at 10 mm, as the preview and the status bar
# say. The steps are tools/ui-scripts/feature-own-parameter.mitcad-ui,
# which tools/ui-windows-feature-own-parameter-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-feature-own-parameter-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-feature-own-parameter.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/feature-own-parameter.mitcad-ui
"$cli" run "$here/ui-scripts/feature-own-parameter.json" --save "$work/own.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/own.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'own parameter driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "depth (this field's value, " "the field's own parameter is listed as kept"
ui_expect_log 'Preview Extrude: ok, info: depth is now a user parameter (10 mm)' 'the panel says what OK will move'
ui_expect_log 'Slot extent.distance: depth kept as a user parameter, now d' 'the distance stores depth * 2 in a new parameter'
ui_expect_log 'Feature notice: depth is now a user parameter (10 mm)' 'the status bar says what moved'
ui_expect_log 'TestDriver: done' 'the own parameter kept as a user parameter by a panel value'
ui_finish 'UI feature value own parameter kept as a user parameter'

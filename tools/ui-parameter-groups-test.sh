#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParametersDialog.hpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/parameter_groups.rs core/model/src/document/parameter_groups.rs core/model-api/src/parameter_groups.rs
# check-all sources: tools/ui-scripts/parameter-groups.mitcad-ui tools/ui-scripts/parameter-groups.json
# Groups of user parameters in Edit Parameters (mitcad#2180) through the
# real UI, the shared input script tools/ui-scripts/parameter-groups.mitcad-ui
# (also run by the Windows and macOS wrappers) on a design that mitcad-cli
# makes (tools/ui-scripts/parameter-groups.json): three ungrouped user
# parameters.
#   - New Group… makes a group, whose row is under User Parameters.
#   - Two parameters selected move into it by the context menu's Move to
#     Group.
#   - The group's row is renamed in place (F2), its parameters go along.
#   - Collapsed, it stays collapsed when Edit Parameters opens again.
#   - Deleted (asked, as it holds parameters), its parameters are back
#     directly under User Parameters.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-groups-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-parameter-groups.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameter-groups.mitcad-ui
"$cli" run "$here/ui-scripts/parameter-groups.json" --save "$work/parameter-groups.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/parameter-groups.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the parameter groups driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail 'the parameter groups driver failed'; exit 1; }
ui_expect_log 'Parameter group added: Cabinet' 'New Group makes a group'
ui_expect_log 'Parameters moved to group Cabinet: width, depth' 'the context menu moves two parameters into it'
ui_expect_log 'Parameter group Cabinet renamed to Table' 'the group is renamed in place'
ui_expect_log 'Parameters group Table collapsed' 'the group collapses'
ui_expect_log 'Parameter group Table deleted: width, depth moved to User Parameters' 'a deleted group leaves its parameters'
ui_expect_log 'TestDriver: done' 'the parameters are grouped, moved, renamed, collapsed and ungrouped'
ui_finish 'UI Edit Parameters groups'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FillCavityCommands.cpp app/commands/ModifyCommands.cpp
# check-all sources: core/model/src/features/fill_cavity.rs geometry/src/fill_cavity.cpp tools/cli
# check-all sources: tools/ui-scripts/fill-cavity.mitcad-ui tools/ui-scripts/fill-cavity.json
# Fill Cavity (mitcad#1818) through the real UI, the shared steps
# tools/ui-scripts/fill-cavity.mitcad-ui (also run by
# tools/ui-windows-fill-cavity-test.ps1 and tools/ui-macos-fill-cavity-test.sh)
# on the design mitcad-cli makes (tools/ui-scripts/fill-cavity.json): a
# blind slit filled up to a plane, its volume added, and an open groove as
# the seed refused in words.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fill-cavity-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fill-cavity.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fill-cavity.mitcad-ui
"$cli" run "$here/ui-scripts/fill-cavity.json" --save "$work/fill-cavity.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/fill-cavity.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'fill cavity driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the slit filled up to the plane, the open groove refused'
ui_finish 'UI fill cavity'

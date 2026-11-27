#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionHover.cpp app/commands/FilletCommands.cpp
# check-all sources: geometry/src/guard.cpp geometry/src/util.hpp geometry/src/dressup.cpp geometry/src/tangent_chain.cpp
# check-all sources: tools/ui-scripts/fillet-slot-boss-1429.mitcad-ui tools/ui-scripts/fillet-slot-boss-1429.json tools/cli
# Fillet on the slot-and-boss part (mitcad#1429, GitHub mitcad/Mitcad#173)
# through the real UI, the shared steps
# tools/ui-scripts/fillet-slot-boss-1429.mitcad-ui (also run by
# tools/ui-windows-fillet-boss-tangent-crash-test.ps1 and
# tools/ui-macos-fillet-boss-tangent-crash-test.sh) on the part mitcad-cli
# makes (tools/ui-scripts/fillet-slot-boss-1429.json): Fillet on the
# plate's top edge that runs tangentially into the boss, its previews at
# several radii, Tangent Chain off and on, hovers, Enter and Escape. The
# application stays up whatever the fillet gives.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-boss-tangent-crash-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-boss-tangent-crash.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-slot-boss-1429.mitcad-ui
"$cli" run "$here/ui-scripts/fillet-slot-boss-1429.json" --save "$work/fillet-boss-tangent-crash.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/fillet-boss-tangent-crash.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'fillet boss tangent driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the fillet on the slot-and-boss part, its previews, hovers, Enter and Escape'
ui_finish 'UI fillet on the slot-and-boss part'

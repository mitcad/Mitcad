#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/features/fillet.rs core/ffi/src/kernel/dressup.rs core/cpp/bridge/dressup.cpp
# check-all sources: geometry/src/full_round.cpp geometry/src/dressup.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/fillet-full-round.mitcad-ui tools/ui-scripts/fillet-full-round.json tools/cli
# Fillet's Full Round (mitcad#2279) through the real UI, in the Floating and
# the Docked layout: the shared input script
# tools/ui-scripts/fillet-full-round.mitcad-ui (also run by
# tools/ui-windows-fillet-full-round-test.ps1 and
# tools/ui-macos-fillet-full-round-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/fillet-full-round.json): a block whose top lies between
# a vertical side and a bevel. Choosing Full Round in Radius Type hides
# Radius; the top face clicked into Edges is refused, the status bar saying
# to pick the face's two edges; its two edges picked, the preview builds
# and OK adds the full round.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-full-round-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-full-round.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-full-round.mitcad-ui
"$cli" run "$here/ui-scripts/fillet-full-round.json" --save "$work/fillet-full-round.mitcad" || exit 1
ui_start_display

# full_round layout [app arguments]: the script's steps in one layout, on a
# fresh copy of the design.
full_round() {
  echo "=== Full Round, $1"
  shift
  cp "$work/fillet-full-round.mitcad" "$work/layout.mitcad" || { ui_fail 'the design could not be copied'; return; }
  ui_start_app --ui=qt --no-mcp "$@" --open "$work/layout.mitcad"
  for _ in $(seq 1 1200); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Fillet full round driver timed out'; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'Panel Fillet input sets.0.radius at' 'the panel lays out Radius'
  ui_expect_log 'not taken: Full Round takes the two edges of the face it rounds away' \
    'the face is refused, the status bar saying to pick its two edges'
  ui_expect_log 'Fillet Edges: 2 edges' 'the two edges are taken'
  ui_expect_log 'Added Fillet' 'OK adds the full round'
  ui_expect_log 'TestDriver: done' 'Full Round hides Radius and builds'
}

full_round "Floating" --chrome=floating
full_round "Docked" --chrome=docked
ui_finish 'UI fillet full round'

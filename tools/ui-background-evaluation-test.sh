#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/DocumentSession.cpp app/framework/TestDriver.cpp app/browser core/ffi/src/background.rs
# check-all sources: core/model/src/document/rolled_back.rs core/model/src/recompute.rs tools/cli tools/ui-scripts/background-evaluation.mitcad-ui
# The rolled-back features evaluated in the background (mitcad#1267)
# through the real UI, the shared input script
# tools/ui-scripts/background-evaluation.mitcad-ui (also run by
# tools/ui-windows-background-evaluation-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/background-evaluation.json): a join
# added before the marker, its rolled-back fillets evaluated in the
# background at the idle point and adopted, so that the marker dragged to
# the end evaluates nothing; a background evaluation cancelled at once by
# an edit that opens while it runs, before the edit's preview.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-background-evaluation-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-background-evaluation.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/background-evaluation.mitcad-ui
# Each background evaluation 4 s longer, so that a command starts while
# one runs (Diagnostics.hpp).
export MITCAD_TEST_RECOMPUTE_DELAY_MS='background evaluation=4000'
"$cli" run "$here/ui-scripts/background-evaluation.json" --save "$work/background.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/background.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'background evaluation driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the marker move after the background evaluation evaluates nothing; a command cancels it'
ui_finish 'UI background evaluation of the rolled-back features'

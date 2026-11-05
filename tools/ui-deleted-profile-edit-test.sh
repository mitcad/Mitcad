#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model core/model-api tools/cli
# Editing an extrusion whose profile was deleted from its sketch: the
# missing profile is left out and a new one picked (mitcad#1264).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-deleted-profile.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/deleted-profile-edit.mitcad-ui
# The script's last step leaves Extrude1 failed on purpose; the design is
# what counts.
"$cli" run "$here/ui-scripts/deleted-profile-edit.json" --save "$work/deleted-profile.mitcad"
[ -f "$work/deleted-profile.mitcad" ] || { ui_fail 'deleted profile fixture creation failed'; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/deleted-profile.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'deleted profile edit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the edit leaves the deleted profile out and takes the new one'
ui_finish 'UI edit of an extrusion whose profile was deleted'

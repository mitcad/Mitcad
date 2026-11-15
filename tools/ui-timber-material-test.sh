#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/build app/commands/BuildCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src/analysis.rs core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/timber-material.mitcad-ui tools/ui-scripts/timber-material-member.json
# check-all sources: tools/ui-scripts/timber-material-library.json
# A member placed with Timber is made of its library's wood (mitcad#1817)
# through the real UI, the shared input script
# tools/ui-scripts/timber-material.mitcad-ui (also run by
# tools/ui-windows-timber-material-test.ps1 and
# tools/ui-macos-timber-material-test.sh), with a library of one synthetic
# planed 28 x 45 member made here (mitcad-cli,
# tools/ui-scripts/timber-material-member.json; spruce at 450 kg/m3 in its
# build block; no supplier's files), fetched from a local git repository:
# Timber places it 1800 mm long, and Physical Properties shows it as
# Spruce with the mass its density gives (1.0206 kg), never as Steel.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timber-material-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-timber-material.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/timber-material.mitcad-ui
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-timber-material
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

"$cli" library init "$lib" --id test-timber-material --name "Test timber material" --license MIT > /dev/null ||
  { ui_fail "library init"; exit 1; }
mkdir -p "$lib/planed-timber"
"$cli" run "$here/ui-scripts/timber-material-member.json" --save "$lib/planed-timber/planed-28x45-test.mitcad" \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli: the member"; exit 1; }
cp "$here/ui-scripts/timber-material-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m "Test member"
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || { ui_fail "library fetch"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'timber material driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Placed Planed 28x45 test' 'a member placed with Timber'
ui_expect_log 'material Spruce' 'Physical Properties shows the library'"'"'s wood'
ui_expect_log 'TestDriver: done' 'the member is Spruce with the mass its density gives, never Steel'
ui_finish 'UI timber material'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/build app/commands/BuildCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src/analysis.rs core/vcs/src/library tools/cli
# A member placed with Timber is made of its library's wood (mitcad#1817)
# on macOS, as tools/ui-timber-material-test.sh on Linux: the shared steps
# tools/ui-scripts/timber-material.mitcad-ui, with a library of one
# synthetic planed 28 x 45 member made here (mitcad-cli,
# tools/ui-scripts/timber-material-member.json; spruce at 450 kg/m3 in its
# build block; no supplier's files) fetched from a local git repository:
# Timber places it 1800 mm long, and Physical Properties shows it as
# Spruce with the mass its density gives (1.0206 kg), never as Steel.
# Usage: tools/ui-macos-timber-material-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-timber-material-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_OUT/library-cache
lib=$UI_OUT/test-timber-material
rm -rf "$lib" "$MITCAD_LIBRARIES_DIR" "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
setup=$UI_OUT/setup.log
: > "$setup"
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" >> "$setup" 2>&1; }

"$cli" library init "$lib" --id test-timber-material --name "Test timber material" --license MIT >> "$setup" 2>&1 ||
  { cat "$setup"; echo 'FAIL: library init' >&2; exit 1; }
mkdir -p "$lib/planed-timber"
"$cli" run "$here/ui-scripts/timber-material-member.json" --save "$lib/planed-timber/planed-28x45-test.mitcad" \
  >> "$setup" 2>&1 || { cat "$setup"; echo 'FAIL: mitcad-cli: the member' >&2; exit 1; }
cp "$here/ui-scripts/timber-material-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m "Test member"
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" >> "$setup" 2>&1 || { cat "$setup"; echo 'FAIL: library fetch' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/timber-material.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app timber-material --ui=qt --no-recovery --no-mcp --no-native-dialogs
ui_expect_log '^Placed Planed 28x45 test' 'a member placed with Timber'
ui_expect_log 'material Spruce$' "Physical Properties shows the library's wood"
ui_expect_log '^TestDriver: done' 'the member is Spruce with the mass its density gives, never Steel'
ui_expect_no_log 'TestDriver: FAIL' 'no failed timber material assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt timber material'

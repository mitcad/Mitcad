#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandPanel.cpp app/browser
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp core/model/src/features/edge_frame.rs
# check-all sources: core/model/src/build core/model-build core/model-api/src/build.rs core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/build-joints.mitcad-ui tools/ui-scripts/build-joints.json
# check-all sources: tools/ui-scripts/build-end-meets-profile.json tools/ui-scripts/build-end-meets-library.json
# Joint conditions of profiles along sketch lines (mitcad#2200) through the
# real UI, the shared input script tools/ui-scripts/build-joints.mitcad-ui
# (also run by tools/ui-windows-build-joints-test.ps1 and
# tools/ui-macos-build-joints-test.sh), with a library of one synthetic
# BUILD profile made here (mitcad-cli; no supplier's files), fetched from a
# local git repository, and a design of a sketch of a rectangle and a line
# across it: Profile along sketch lines with Corners Butt, an L swapped in
# Ends, a T set to Far Side with Shift + click.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-joints-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-joints.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the profile's design and the manifest, tagged and fetched.
"$cli" library init "$lib" --id test-profiles --name "Test profiles" --license MIT > /dev/null || ui_fail 'library init'
mkdir -p "$lib/profiles"
"$cli" run "$here/ui-scripts/build-end-meets-profile.json" --save "$lib/profiles/p-slot-test.mitcad" \
  > "$work/profile.log" 2>&1 || { cat "$work/profile.log"; ui_fail 'mitcad-cli: the profile'; }
cp "$here/ui-scripts/build-end-meets-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m 'Test part'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design that places it.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-joints.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/build-joints.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-joints.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-joints.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'joint conditions driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'BUILD profile along sketch lines: ' 'the sketch picked from the Browser'
ui_expect_log 'profile(s) along sketch lines' 'Place made the profiles along the lines'
ui_expect_log 'joint butt with ' 'Ends swapped the L that runs through'
ui_expect_log 'joint far_side with ' 'Shift + click set the T to Far Side'
ui_expect_log 'TestDriver: done' 'joints placed butted, swapped and set to Far Side'
ui_finish 'UI joint conditions along sketch lines'

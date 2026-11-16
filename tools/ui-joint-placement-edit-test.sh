#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/commands/BuildCommands.cpp app/framework/Selection.cpp
# check-all sources: app/framework/CommandItemText.cpp app/framework/CommandRegistry.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp app/build/BuildModel.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src/build.rs core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/joint-placement-edit.mitcad-ui tools/ui-scripts/joint-placement-edit.json
# check-all sources: tools/ui-scripts/joint-placement-edit-profile.json tools/ui-scripts/joint-placement-edit-library.json
# A BUILD part's joint and placement edited (mitcad#1885, GitHub
# mitcad/Mitcad#354) through the real UI, the shared input script
# tools/ui-scripts/joint-placement-edit.mitcad-ui (also run by
# tools/ui-windows-joint-placement-edit-test.ps1 and
# tools/ui-macos-joint-placement-edit-test.sh), with a library of one
# synthetic BUILD profile with a slot on one side made here (mitcad-cli; no
# supplier's files), fetched from a local git repository, and a design that
# places it: a joint onto a profile's mount frame with a frame override
# opens in Joint, its Offset changed keeps the override; Placement turns a
# side placement's From from Middle to End A keeping the part where it is,
# then places it by a new Offset, each in one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-placement-edit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-joint-placement-edit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the profile's design and the manifest, tagged and fetched.
"$cli" library init "$lib" --id test-profiles --name "Test profiles" --license MIT > /dev/null || ui_fail 'library init'
mkdir -p "$lib/profiles"
"$cli" run "$here/ui-scripts/joint-placement-edit-profile.json" --save "$lib/profiles/p-slot-test.mitcad" \
  > "$work/profile.log" 2>&1 || { cat "$work/profile.log"; ui_fail 'mitcad-cli: the profile'; }
cp "$here/ui-scripts/joint-placement-edit-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m 'Test part'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design that places it.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/joint-placement-edit.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/joint-placement-edit.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/joint-placement-edit.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/joint-placement-edit.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'joint placement edit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Joint origin b keeps its frame override' 'the joint on the mount frame opened in Joint with its override'
ui_expect_log 'Edited F3' 'the new Offset of the joint committed'
ui_expect_log 'Placement of C3: from a, kept in place' 'a new From kept the part where it is'
ui_expect_log 'Placement of C3: from a, offset 50 mm' 'a new Offset placed the part from End A'
ui_expect_log 'TestDriver: done' 'the joint and the placement of a BUILD part edited'
ui_finish 'UI joint and placement edit'

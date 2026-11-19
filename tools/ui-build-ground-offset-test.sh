#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandPanel.cpp app/icons
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/joints.rs core/model/src/build core/model-build core/model-api/src/build.rs core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/build-ground-offset.mitcad-ui tools/ui-scripts/build-face-normal.json
# check-all sources: tools/ui-scripts/build-end-meets-profile.json tools/ui-scripts/build-end-meets-library.json
# A BUILD profile placed off a face by Offset and Rotate, both edited later
# with Placement (mitcad#2046, GitHub mitcad/Mitcad#446), through the real
# UI, the shared input script tools/ui-scripts/build-ground-offset.mitcad-ui
# (also run by tools/ui-windows-build-ground-offset-test.ps1 and
# tools/ui-macos-build-ground-offset-test.sh), with the library of one
# synthetic BUILD profile of the mitering test made here (mitcad-cli; no
# supplier's files), fetched from a local git repository, and the design of
# the face-normal test (a box standing for an imported machine's leg): on
# its bottom face Offset 20 mm and Rotate 30 deg are previewed and placed,
# then Placement moves the profile to Offset 50 mm and Rotate 45 deg in one
# undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-ground-offset-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-ground-offset.XXXXXX")
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

# The design: the leg.
"$cli" run "$here/ui-scripts/build-face-normal.json" --save "$work/build-face-normal.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-ground-offset.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-face-normal.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'ground-offset driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'BUILD face picked: Direction Face normal, at the ' 'a face picked chose Face normal and its centre'
ui_expect_log '; end A at 120.0, 230.0, -20.0; offset 20 mm, rotate 30 deg' 'the preview stands 20 off the face, turned 30 deg'
ui_expect_log ': offset 50 mm, rotate 45 deg' 'Placement moved it to offset 50 mm, rotate 45 deg'
ui_expect_log ': 45 deg to 135 deg' 'Rotate 90 deg turned it a quarter further'
ui_expect_log 'TestDriver: done' 'a profile placed off a face and edited with Placement'
ui_finish 'UI profile off a face by Offset and Rotate'

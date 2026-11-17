#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandPanel.cpp app/framework/ManipulatorOverlay.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src/build.rs core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/build-end-meets.mitcad-ui tools/ui-scripts/build-end-meets.json
# check-all sources: tools/ui-scripts/build-end-meets-profile.json tools/ui-scripts/build-end-meets-library.json
# Mitering two BUILD profiles placed apart (mitcad#1940) through the real
# UI, the shared input script tools/ui-scripts/build-end-meets.mitcad-ui
# (also run by tools/ui-windows-build-end-meets-test.ps1 and
# tools/ui-macos-build-end-meets-test.sh), with a library of one synthetic
# BUILD profile with a slot on one side made here (mitcad-cli; no
# supplier's files), fetched from a local git repository, and a design of
# two of them meeting in an L: a length bound to the part placed later is
# refused with the way to miter the corner; Ends labels the ends A and B,
# a click near the corner chooses the end, Also the Component It Meets is
# offered and makes the shared miter in one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-end-meets-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-end-meets.XXXXXX")
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
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-end-meets.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/build-end-meets.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-end-meets.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-end-meets.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'mitering driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Miter Auto or Shared' 'the bound length refused with the way to miter'
ui_expect_log 'BUILD ends of C2: end a picked, meets end b of C1' 'a click near the corner chose the end that meets'
ui_expect_log 'Ends of Arm: end a shared miter with end b of C1' 'Also the Component It Meets made the shared miter'
ui_expect_log 'TestDriver: done' 'two profiles placed apart mitered'
ui_finish 'UI mitering two profiles'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildEndCapCommand.cpp app/commands/BuildCommands.cpp app/build/BuildModel.cpp
# check-all sources: app/build/BuildLists.cpp app/framework/Selection.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp
# check-all sources: core/model/src/features/end_cap.rs core/model/src/features/derived_part.rs
# check-all sources: core/model/src/build core/model-build core/model-api/src/build.rs tools/cli
# check-all sources: tools/ui-scripts/build-end-cap.mitcad-ui tools/ui-scripts/build-end-cap.json
# check-all sources: tools/ui-scripts/build-end-cap-profile.json tools/ui-scripts/build-end-cap-library.json
# End Cap (mitcad#2243) through the real UI, the shared input script
# tools/ui-scripts/build-end-cap.mitcad-ui (also run by
# tools/ui-windows-build-end-cap-test.ps1 and
# tools/ui-macos-build-end-cap-test.sh), with a library of one synthetic
# steel tube made here (mitcad-cli; no supplier's files), fetched from a
# local git repository, and a design of one tube placed by BUILD: a click
# near each end chooses it, both ends are capped outward in one undo step,
# one is switched to Inward (the tube cut back in the cut list), and the
# lists' window shows the plates.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-end-cap-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-end-cap.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-tubes
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the tube's design and the manifest, tagged and fetched.
"$cli" library init "$lib" --id test-tubes --name "Test tubes" --license MIT > /dev/null || ui_fail 'library init'
mkdir -p "$lib/tubes"
"$cli" run "$here/ui-scripts/build-end-cap-profile.json" --save "$lib/tubes/tube-test.mitcad" \
  > "$work/profile.log" 2>&1 || { cat "$work/profile.log"; ui_fail 'mitcad-cli: the tube'; }
cp "$here/ui-scripts/build-end-cap-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m 'Test part'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design that places it.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-end-cap.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/build-end-cap.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-end-cap.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-end-cap.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'end cap driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'End Cap: 2 cap(s) outward' 'both ends capped outward in one step'
ui_expect_log 'End Cap lists: Tube 40x40x3 L=600 595 mm x1; plates: 2 x Plate 40 x 40 x 5' \
  'end B switched to inward: the tube cut back, two plates'
ui_expect_log 'Cut list plates: 2 x Plate 40 x 40 x 5' 'the lists show the plates'
ui_expect_log 'TestDriver: done' 'a tube capped at both ends'
ui_finish 'UI end caps of a tube'

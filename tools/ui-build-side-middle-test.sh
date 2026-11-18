#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandPanel.cpp app/framework/ManipulatorOverlay.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp
# check-all sources: core/model/src/build core/model/src/features/derived_part.rs core/model/src/features/geom_ref.rs
# check-all sources: core/model-build core/model-api/src/build.rs core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/build-side-middle.mitcad-ui tools/ui-scripts/build-side-middle.json
# check-all sources: tools/ui-scripts/build-side-middle-profile.json tools/ui-scripts/build-side-middle-library.json
# A profile centred across a side with two slots (mitcad#2002) through the
# real UI, the shared input script tools/ui-scripts/build-side-middle.mitcad-ui
# (also run by tools/ui-windows-build-side-middle-test.ps1 and
# tools/ui-macos-build-side-middle-test.sh), with a library of one
# synthetic BUILD profile, 20x40 with two slots on each long side, made
# here (mitcad-cli; no supplier's files), fetched from a local git
# repository, and a design of one of them standing: a click near the
# side's centre line places a profile centred across the side (the
# preview says so and marks it), a click near a slot places it on that
# slot.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-side-middle-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-side-middle.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the profile's design and the manifest, tagged and fetched.
"$cli" library init "$lib" --id test-profiles --name "Test profiles" --license MIT > /dev/null || ui_fail 'library init'
mkdir -p "$lib/profiles"
"$cli" run "$here/ui-scripts/build-side-middle-profile.json" --save "$lib/profiles/p-two-slot-test.mitcad" \
  > "$work/profile.log" 2>&1 || { cat "$work/profile.log"; ui_fail 'mitcad-cli: the profile'; }
cp "$here/ui-scripts/build-side-middle-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m 'Test part'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design that places it.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-side-middle.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/build-side-middle.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-side-middle.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-side-middle.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'side-middle driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '; across middle' 'a click near the centre line previews the profile centred across the side'
# ui_expect_log matches a fixed string: the taken marker's label may lead
# with the snap along the side ("> middle, centered on the side").
grep -qE '^Preview marker Profile > (.*, )?centered on the side world ' "$UI_LOG" &&
  echo 'ok   the preview marks it centred' ||
  ui_fail 'the preview marks it centred: no "Preview marker Profile > ... centered on the side" in the log'
ui_expect_log '; across slot' 'a click near a slot previews it on that slot'
ui_expect_log 'TestDriver: done' 'profiles placed centred across a side and on a slot'
ui_finish 'UI profile centred across a side with two slots'

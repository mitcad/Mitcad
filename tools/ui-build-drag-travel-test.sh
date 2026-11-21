#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowJoints.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/joints.rs core/model/src/document/joints.rs core/model/src/build
# check-all sources: core/model-build core/model-api/src/build.rs core/model-api/src/joints.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/build-drag-travel.mitcad-ui tools/ui-scripts/build-drag-travel.json
# check-all sources: tools/ui-scripts/build-drag-travel-profile.json tools/ui-scripts/build-drag-travel-rail.json
# check-all sources: tools/ui-scripts/build-drag-travel-carriage.json tools/ui-scripts/build-drag-travel-library.json
# Drags leave BUILD's placed parts at their Offset and a carriage slides
# within its travel limits (mitcad#2088, GitHub mitcad/Mitcad#411), through
# the real UI, the shared input script
# tools/ui-scripts/build-drag-travel.mitcad-ui (also run by
# tools/ui-windows-build-drag-travel-test.ps1 and
# tools/ui-macos-build-drag-travel-test.sh), with a library of a synthetic
# profile, guide rail and carriage made here (mitcad-cli; no supplier's
# files), fetched from a local git repository, and a frame placed from it
# by build-drag-travel.json: a drag of a placed profile is refused in words
# that say to use Placement, Placement moves it and the part on it follows,
# the carriage dragged along its rail stops at its limit with the rail
# staying, and Placement reads and sets its Min and Max.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-drag-travel-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-drag-travel.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the parts' designs and the manifest, tagged and fetched.
"$cli" library init "$lib" --id test-profiles --name "Test profiles" --license MIT > /dev/null || ui_fail 'library init'
mkdir -p "$lib/profiles" "$lib/linear"
for part in profile:profiles/p-drag-test rail:linear/rail-drag-test carriage:linear/carriage-drag-test; do
  "$cli" run "$here/ui-scripts/build-drag-travel-${part%%:*}.json" --save "$lib/${part#*:}.mitcad" \
    > "$work/${part%%:*}.log" 2>&1 || { cat "$work/${part%%:*}.log"; ui_fail "mitcad-cli: the ${part%%:*}"; }
done
cp "$here/ui-scripts/build-drag-travel-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m 'Test parts'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design: the frame, the rail and the carriage.
sed "s|LIBRARY_URL|$lib|" "$here/ui-scripts/build-drag-travel.json" > "$work/build-drag-travel.json"
"$cli" run "$work/build-drag-travel.json" --save "$work/build-drag-travel.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-drag-travel.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-drag-travel.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'drag-travel driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drag refused: Arm:1 stays where it was placed: move it with Placement' 'a drag of the arm is refused, naming Placement'
ui_expect_log ': from a, offset 300 mm' 'Placement moved the arm, the post on it following'
ui_expect_log 'Carriage on Rail tx 380 mm' 'the carriage stopped flush with the rail end'
ui_expect_log 'BUILD travel of C' 'Placement read the carriage travel limits'
ui_expect_log 'Carriage on Rail tx 300 mm' 'the carriage stopped at its own Max'
ui_expect_log 'TestDriver: done' 'drags and travel limits of BUILD parts'
ui_finish 'UI drags and travel limits of BUILD parts'

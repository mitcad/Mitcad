#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp app/framework/RibbonLabels.cpp app/icons tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_welds.rs core/model-api/src/drawings.rs
# check-all sources: core/model-drawing/src/annotate.rs core/model-drawing/src/weld.rs core/model/src/drawing/annotate_def.rs
# check-all sources: core/model-build tools/ui-scripts/drawing-weld.mitcad-ui tools/ui-scripts/drawing-weld.json
# check-all sources: tools/ui-scripts/build-end-meets-profile.json tools/ui-scripts/build-end-meets-library.json
# Weld symbols on a drawing (mitcad#2350) through the real UI, the shared
# input script tools/ui-scripts/drawing-weld.mitcad-ui (also run by
# tools/ui-windows-drawing-weld-test.ps1 and
# tools/ui-macos-drawing-weld-test.sh), with the mitering test's library of
# one synthetic BUILD profile made here (mitcad-cli; no supplier's files),
# fetched from a local git repository, and a design of two of them in a T
# (tools/ui-scripts/drawing-weld.json): Weld Symbol puts a 5 mm fillet weld
# on an edge, its symbol chosen from icons and its size typed; Propose
# Welds puts a fillet weld all around on the T joint of the member clicked.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-weld-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-weld.XXXXXX")
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

# The frame and its drawing.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/drawing-weld.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/welded.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-weld.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/welded.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'weld symbol driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '"type":"weld"' 'the weld symbol asked for'
ui_expect_log '"cmd":"propose_drawing_welds"' 'the welds proposed'
ui_expect_log 'TestDriver: done' 'a fillet weld placed on an edge and one proposed all around the T joint'
ui_finish 'UI drawing weld symbols'

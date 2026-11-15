#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConvertToFeatures.cpp app/browser/ConvertToFeatures.hpp app/browser/BrowserController.cpp
# check-all sources: app/MainWindowContextMenu.cpp app/browser/TimelineWidget.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/convert_derived.rs core/model/src/document/copy.rs core/model-api/src/convert_derived.rs
# check-all sources: core/model/src/features/derived_part.rs core/model/src/features/bound_length.rs core/model/src/build core/model-build core/vcs/src/library
# check-all sources: tools/ui-scripts/convert-features.mitcad-ui tools/ui-scripts/convert-features.json
# check-all sources: tools/ui-scripts/convert-features-member.json tools/ui-scripts/convert-features-library.json
# Convert to Features (mitcad#1815) through the real UI, the shared input
# script tools/ui-scripts/convert-features.mitcad-ui (also run by
# tools/ui-windows-convert-features-test.ps1 and
# tools/ui-macos-convert-features-test.sh): a library of one synthetic
# timber member made here with mitcad-cli and fetched from a local git
# repository (no supplier's files), the member placed from it and joined to
# a grounded plate (tools/ui-scripts/convert-features.json); Convert to
# Features from the member's row menu, its sketch and extrusion in the
# timeline, the section's width changed, the joint holding.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-convert-features-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-convert-features.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_LIBRARIES_DIR=$work/cache
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the member's design and the manifest, committed and fetched.
lib=$work/test-timber
"$cli" library init "$lib" --id test-timber --name "Test timber" --license MIT > /dev/null ||
  { ui_fail 'library init'; exit 1; }
mkdir -p "$lib/dimensioned-timber"
"$cli" run "$here/ui-scripts/convert-features-member.json" \
  --save "$lib/dimensioned-timber/timber-convert-test.mitcad" > /dev/null ||
  { ui_fail 'mitcad-cli could not make the member'; exit 1; }
cp "$here/ui-scripts/convert-features-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m "Test part"
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || { ui_fail 'library fetch'; exit 1; }

# The design: the plate and the member joined to it.
sed "s|LIBRARY_URL|$lib|" "$here/ui-scripts/convert-features.json" > "$work/convert-features.json"
"$cli" run "$work/convert-features.json" --save "$work/convert-features.mitcad" ||
  { ui_fail 'mitcad-cli could not make the design'; exit 1; }
[ -f "$work/convert-features.mitcad" ] || { ui_fail 'the design was not made'; exit 1; }

export MITCAD_TEST_INPUT=$here/ui-scripts/convert-features.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/convert-features.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'convert features driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the member converted from its row menu, its section edited, the joint holding'
ui_finish 'UI Convert to Features keeps the assembly and makes the section editable'

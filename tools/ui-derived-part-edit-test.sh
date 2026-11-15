#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DerivedPartCommands.cpp app/MainWindow.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/framework/TestDriver.cpp core/model/src/document/component_options.rs
# check-all sources: app/files/MainWindowLinks.cpp app/files/MainWindowProjects.cpp app/browser/FeaturePresentation.hpp core/model-api/src/libraries.rs
# check-all sources: core/model/src/build core/model-build core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/derived-part-edit.mitcad-ui tools/ui-scripts/derived-part-edit.json
# check-all sources: tools/ui-scripts/derived-part-edit-profile.json tools/ui-scripts/derived-part-edit-library.json
# The edit panel of a derived part (mitcad#1812) through the real UI, the
# shared input script tools/ui-scripts/derived-part-edit.mitcad-ui (also
# run by tools/ui-windows-derived-part-edit-test.ps1 and
# tools/ui-macos-derived-part-edit-test.sh), with a library of one
# synthetic BUILD profile made here (mitcad-cli: a square bar with the
# rows Small and Large; no supplier's files), fetched from a local git
# repository, and a design that places it: a double-click on the profile's
# timeline cell opens Derived Component, its length and row are changed in
# one undo step and Undo restores them; a double-click on a feature type
# without an edit panel shows the plain message, not a failure.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-derived-part-edit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-derived-part-edit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the profile's design and the manifest, tagged and fetched.
"$cli" library init "$lib" --id test-profiles --name "Test profiles" --license MIT > /dev/null || ui_fail 'library init'
mkdir -p "$lib/profiles"
"$cli" run "$here/ui-scripts/derived-part-edit-profile.json" --save "$lib/profiles/p-rows-test.mitcad" \
  > "$work/profile.log" 2>&1 || { cat "$work/profile.log"; ui_fail 'mitcad-cli: the profile'; }
cp "$here/ui-scripts/derived-part-edit-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m 'Test part'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design that places it.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/derived-part-edit.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/derived-part-edit.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/derived-part-edit.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/derived-part-edit.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'derived part edit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Derived component C' 'the panel changed the profile'"'"'s length and row with one command'
ui_expect_log 'Message band: info This feature has no edit panel yet.' 'a feature without a panel says so in plain words'
ui_expect_log 'Open Read-Only: ' 'Open Source Design opened the library source read-only'
ui_expect_log 'TestDriver: done' 'the panel opened in place, its edit was one undo step and Undo restored it'
ui_finish 'UI derived part edit'

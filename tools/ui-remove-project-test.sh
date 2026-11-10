#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/TestDriver.cpp app/framework/AppSettings.cpp
# check-all sources: core/vcs core/model/src/file core/ffi/src/vcs.rs core/ffi/src/projects.rs tools/cli
# Remove Project and New Project's Create in a New Subfolder (mitcad#1562)
# through the real UI, by the shared driver script
# tools/ui-scripts/remove-project.mitcad-ui (the Windows counterpart is
# tools/ui-windows-remove-project-test.ps1). A folder of the projects folder
# with two designs, one in a subfolder, and another file (no project):
#   1. New Project on it says that the folder and everything below it become
#      the project, with its 2 designs, and offers Create in a New Subfolder.
#   2. Create in a New Subfolder makes the project in "New Project" inside
#      it.
#   3. Remove Project from that project's row in the Projects panel, after
#      its confirmation, removes the project's files and history; the
#      folders, the designs and the other file stay as they were.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-remove-project-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

# Outside the checkout: a folder inside a git repository is reported as a
# project "inside" it, which this test does not expect (mitcad#1574).
work=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-remove-project.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
# The author New Project offers.
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
export MITCAD_PROJECTS_DIR=$work/projects
workshop=$MITCAD_PROJECTS_DIR/Workshop

cli() {
  "$CLI" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; }
}

echo "--- A folder with two designs, one in a subfolder, and notes (no project)"
mkdir -p "$workshop/parts"
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
  '  "length": 10, "width": 20, "height": 5, "operation": "new_body"}}]' > "$work/block.json"
cli run "$work/block.json" --save "$workshop/plate.mitcad"
cli run "$work/block.json" --save "$workshop/parts/bracket.mitcad"
printf 'notes\n' > "$workshop/notes.txt"
plate=$(sha256sum < "$workshop/plate.mitcad")
bracket=$(sha256sum < "$workshop/parts/bracket.mitcad")
echo "ok   the folder is made"

# The panel shown at the start, as for a user who turned it on (mitcad#1020).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

export MITCAD_TEST_WORKSHOP=$workshop MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/remove-project.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the driver timed out"
wait "$UI_RUNNER" || { tail -n 60 "$UI_LOG" >&2; ui_fail "the driver failed"; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the steps"

echo "--- The project removed, the folders and designs kept"
[ -d "$workshop/New Project" ] || ui_fail "the subfolder New Project is gone"
for gone in .git .mitcad .gitattributes .gitignore; do
  [ ! -e "$workshop/New Project/$gone" ] || ui_fail "New Project/$gone is still there"
  [ ! -e "$workshop/$gone" ] || ui_fail "the folder got $gone"
done
[ "$(sha256sum < "$workshop/plate.mitcad")" = "$plate" ] || ui_fail "plate.mitcad changed"
[ "$(sha256sum < "$workshop/parts/bracket.mitcad")" = "$bracket" ] || ui_fail "parts/bracket.mitcad changed"
[ "$(cat "$workshop/notes.txt")" = notes ] || ui_fail "notes.txt changed"
echo "ok   only the project's files are gone"

ui_finish "UI remove project test"

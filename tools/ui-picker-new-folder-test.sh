#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/AppSettings.cpp core/vcs core/ffi/src/vcs.rs core/ffi/src/projects.rs tools/cli
# The project picker's New Folder (mitcad#1649) through the real UI, by the
# shared driver script tools/ui-scripts/picker-new-folder.mitcad-ui (the
# Windows counterpart is tools/ui-windows-picker-new-folder-test.ps1). A
# Local project of the projects folder with a recorded design, and a loose
# design in no project, opened:
#   1. Save to Project: the project selected, New Folder, a name typed and
#      Enter make the folder as a version, selected in the picker.
#   2. The same name again: the refusal in words below the name, the editor
#      open again; Esc takes the row away.
#   3. Save: the design is in the new folder, and the Projects panel shows
#      the folder and the design.
#   4. Save As's picker: a folder made with the name proposed, then the
#      picker cancelled: the folder stays and the panel shows it.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-picker-new-folder-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-picker-new-folder.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
# The projects folder: the picker and the panel list its projects.
export MITCAD_PROJECTS_DIR=$work/projects
shop=$MITCAD_PROJECTS_DIR/Shop
loose=$work/loose

cli() {
  "$CLI" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; }
}

echo "--- A Local project with plate.mitcad recorded, a loose design"
mkdir -p "$MITCAD_PROJECTS_DIR" "$loose"
cli project init "$shop" --author "$author"
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
  '  "length": 10, "width": 20, "height": 5, "operation": "new_body"}}]' > "$work/block.json"
cli run "$work/block.json" --save "$shop/plate.mitcad"
cli version save "$shop/plate.mitcad" -m "The plate" --author "$author"
cli run "$work/block.json" --save "$loose/gear.mitcad"
echo "ok   the designs are made"

# The panel shown at the start, as for a user who turned it on (mitcad#1020:
# hidden until then).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

export MITCAD_TEST_WORK=$work MITCAD_TEST_SHOP=$shop MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/picker-new-folder.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$loose/gear.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the driver timed out"
wait "$UI_RUNNER" || { tail -n 60 "$UI_LOG" >&2; ui_fail "the driver failed"; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the steps"

[ -d "$shop/Brackets" ] || ui_fail "the folder Brackets is not in the project"
[ -f "$shop/Brackets/gear.mitcad" ] || ui_fail "gear.mitcad is not in the new folder"
[ -f "$loose/gear.mitcad" ] || ui_fail "the file the design came from is gone"
[ -d "$shop/New Folder" ] || ui_fail "the folder made in the cancelled picker is gone"
echo "ok   the folders and the design are in the project"
# Each folder a version of its own, then the design's.
log=$(git -C "$shop" log --format=%s)
for subject in "Add folder Brackets" "Add folder New Folder"; do
  printf '%s\n' "$log" | grep -qxF "$subject" || ui_fail "no version \"$subject\": $log"
done
printf '%s\n' "$log" | grep -qE '^Save .*gear\.mitcad' || ui_fail "no version of gear.mitcad: $log"
echo "ok   the folders and the design are versions"

ui_finish "UI project picker New Folder test"

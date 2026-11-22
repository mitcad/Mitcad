#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: app/framework/AppSettings.cpp app/framework/TestDriver.cpp core/vcs core/ffi/src/projects.rs
# check-all sources: tools/cli tools/ui-scripts/new-design-save.mitcad-ui
# Save of a new design (mitcad#2183) through the real UI, by the shared
# driver script tools/ui-scripts/new-design-save.mitcad-ui (the Windows
# counterpart is tools/ui-windows-new-design-save-test.ps1, the macOS one
# tools/ui-macos-new-design-save-test.sh). The remote project is a Cloud
# project of the projects folder: a local git repository following a local
# bare repository (no network). MitCad starts on a new, untitled design:
#   0. Without a remote project (Team hidden from the list): Save As as
#      before, no question.
#   1. Ctrl+S asks On this computer or To a project; Esc saves nothing.
#   2. Save on This Computer…: the file dialog, the design saved there.
#   3. A new design, Remember my choice, Save to Project…: the project
#      picker, the design saved in the project; the preference remembered.
#   4. The next new design: the picker at once, no question.
#   5. Preferences › General › Save of a new design back to Ask: the
#      question again.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-new-design-save-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-new-design-save.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset GIT_SSH GIT_ASKPASS
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
# The projects folder: the question, the picker and the panel read its
# projects.
export MITCAD_PROJECTS_DIR=$work/projects
team=$MITCAD_PROJECTS_DIR/Team

cli() {
  "$CLI" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; }
}

echo "--- A Cloud project: a repository following a local bare repository"
mkdir -p "$MITCAD_PROJECTS_DIR" "$work/remotes"
git init -q --bare "$work/remotes/team.git" || ui_fail "git init --bare"
cli project create "$team" --author "$author" --url "$work/remotes/team.git"
git -C "$team" config --get-regexp '^remote\.' > /dev/null || ui_fail "the project follows no remote"
echo "ok   the remote project is made"

export MITCAD_TEST_WORK=$work MITCAD_TEST_TEAM=$team MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/new-design-save.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the driver timed out"
wait "$UI_RUNNER" || { tail -n 60 "$UI_LOG" >&2; ui_fail "the driver failed"; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the steps"

[ -f "$work/local.mitcad" ] || ui_fail "the design saved on this computer is not there"
[ -f "$team/Untitled.mitcad" ] || ui_fail "the design saved to the project is not there"
log=$(git -C "$team" log --format=%s)
printf '%s\n' "$log" | grep -qE 'Untitled\.mitcad' || ui_fail "no version of Untitled.mitcad: $log"
echo "ok   one design on this computer, one in the project as a version"

ui_finish "UI new design save test"

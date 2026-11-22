#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: app/framework/AppSettings.cpp app/framework/TestDriver.cpp core/vcs tools/cli
# check-all sources: tools/ui-scripts/new-design-save.mitcad-ui
# Save of a new design (mitcad#2183) on macOS, in the Qt frontend, through
# the test driver (tools/ui-scripts/new-design-save.mitcad-ui, the same
# script as the Linux and Windows tests'). The remote project is a Cloud
# project of the projects folder: a local git repository following a local
# bare repository (no network). MitCad starts on a new, untitled design:
# without a remote project Save As as before; with it the question On this
# computer or To a project (Esc saves nothing), Save on This Computer… (the
# file dialog), Remember my choice with Save to Project… (the picker, the
# design saved in the project), the next new design straight to the picker,
# and Settings › General › Save of a new design back to Ask.
# Usage: tools/ui-macos-new-design-save-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-new-design-save}"
UI_TIMEOUT=300
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files/remotes"
# git's configuration of the test's own: the versions' author.
export GIT_CONFIG_GLOBAL=$files/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset GIT_SSH GIT_ASKPASS
printf '[user]\n\tname = Mac Tester\n\temail = mac@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Mac Tester <mac@example.invalid>"
export MITCAD_PROJECTS_DIR=$files/projects
team=$MITCAD_PROJECTS_DIR/Team
setup() {
  mkdir -p "$MITCAD_PROJECTS_DIR" &&
    git init -q --bare "$files/remotes/team.git" &&
    "$2" project create "$team" --author "$author" --url "$files/remotes/team.git" &&
    git -C "$team" config --get-regexp '^remote\.'
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

MITCAD_TEST_WORK=$files MITCAD_TEST_TEAM=$team MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/new-design-save.mitcad-ui \
  ui_run_app new-design-save --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log 'TestDriver: done' 'the shared steps ran to their end'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
ui_expect_log '^Save of a new design: Save As \(no remote project known\)$' 'no question without a remote project'
ui_expect_log '^Save of a new design: to a project \(the preference\)$' 'the remembered choice without the question'

if [ -f "$files/local.mitcad" ] && [ -f "$team/Untitled.mitcad" ]; then
  ui_pass 'one design on this computer, one in the project'
else
  ui_fail 'a saved design is missing'
fi
if git -C "$team" log --format=%s 2> /dev/null | grep -q 'Untitled\.mitcad'; then
  ui_pass 'the design in the project is a version'
else
  ui_fail 'no version of Untitled.mitcad in the project'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt new design save'

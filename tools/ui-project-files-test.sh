#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files/MainWindowProjects.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverFiles.cpp core/vcs core/ffi/src/vcs.rs core/ffi/src/projects.rs
# check-all sources: tools/cli tools/ui-scripts/project-files.mitcad-ui
# A project's other files (mitcad#2204) in the Projects panel through the
# real UI, the shared input script tools/ui-scripts/project-files.mitcad-ui
# (also run by tools/ui-windows-project-files-test.ps1), once in the
# Floating layout (project Shop) and once in the Docked one (project Lab).
# mitcad-cli makes each Local project with the design plate.mitcad, a PNG
# and a PDF recorded beside it, and then changes the PDF as its own app
# would; the script checks that the files are listed after the design with
# their marks and tooltip, that a double-click hands the PNG to the system's
# app (a stub: MITCAD_TEST_LOG_URLS, nothing opens), that Record Changes
# records the changed PDF, that Add Files… and a drop from the file manager
# each copy a file in as one version, and that a file is renamed in place
# and deleted as a version. The versions are then checked with mitcad-cli.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-project-files-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-project-files.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
AUTHOR="Ada Tester <ada@example.invalid>"
# The projects folder: the panel lists its projects.
export MITCAD_PROJECTS_DIR=$work/projects
export MITCAD_TEST_STEP_MS=100

cli() {
  "$cli" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; exit 1; }
}

# wait_driver what: waits for the driver's run of the application to end.
wait_driver() {
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed"; exit 1; }
  UI_RUNNER=""
}

# make_project name: a Local project with plate.mitcad, a PNG and a PDF
# recorded, the PDF changed since; the files to add in its run's outside
# folder.
make_project() {
  local dir=$MITCAD_PROJECTS_DIR/$1 run=$work/$1
  cli project create "$dir" --author "$AUTHOR" --design plate.mitcad
  printf 'not really a picture' > "$dir/photo.png"
  printf '%%PDF-1.4 not really a datasheet\n' > "$dir/datasheet.pdf"
  cli project record "$dir" --author "$AUTHOR"
  printf '%% a page more\n' >> "$dir/datasheet.pdf"
  mkdir -p "$run/outside"
  printf 'notes\n' > "$run/outside/notes.txt"
  printf 'a,b\n1,2\n' > "$run/outside/drop.csv"
  echo "ok   the project $1 is made"
}

# check_versions name: the versions the script recorded, as mitcad-cli
# lists them: the files there, the renamed and deleted one gone.
# git's output is read whole first and searched afterwards, never piped
# into `grep -q` (mitcad#2601): under pipefail, grep -q leaves at its match,
# git's next line written to the closed pipe ends it with SIGPIPE, and the
# pipeline fails although the version is there. git writes each commit's
# line as it goes, so this lost about every second run with git 2.43.
check_versions() {
  local dir=$MITCAD_PROJECTS_DIR/$1 tree status subjects subject
  tree=$("$cli" project tree "$dir")
  for line in "  datasheet.pdf  " "  drop.csv  " "  notes.txt  "; do
    grep -qF -- "$line" <<< "$tree" || ui_fail "$1: '$line' is not in the tree: $tree"
  done
  grep -qE 'photo\.png|front\.png|changed|not recorded' <<< "$tree" &&
    ui_fail "$1: the tree after the run: $tree"
  status=$(git -C "$dir" status --porcelain) || ui_fail "$1: git status failed"
  [ -z "$status" ] || ui_fail "$1: git status is not clean: $status"
  subjects=$(git -C "$dir" log --format=%s) || ui_fail "$1: git log failed"
  for subject in "Add notes.txt" "Add drop.csv" "Record datasheet.pdf"; do
    grep -qxF -- "$subject" <<< "$subjects" || ui_fail "$1: no version '$subject'"
  done
  grep -qxF "Move photo.png to front.png" <<< "$subjects" || ui_fail "$1: no version of the rename: $subjects"
  grep -qxF "Delete front.png" <<< "$subjects" || ui_fail "$1: no version of the delete: $subjects"
  echo "ok   $1: each change is one version"
}

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
mkdir -p "$MITCAD_PROJECTS_DIR"
make_project Shop
make_project Lab

# The panel shown at the start, as for a user who turned it on (mitcad#1020:
# hidden until then).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\npanelTopBar=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

ui_start_display

echo "--- 1. Floating: the project Shop"
export MITCAD_TEST_INPUT=$here/ui-scripts/project-files.mitcad-ui
MITCAD_TEST_WORK=$work/Shop MITCAD_TEST_PROJECT=Shop MITCAD_TEST_PROJECT_DIR=$MITCAD_PROJECTS_DIR/Shop \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --chrome=floating
wait_driver 'Floating'
ui_expect_log 'TestDriver: done' 'Floating: listed, opened, recorded, added, dropped, renamed and deleted'
check_versions Shop

echo "--- 2. Docked: the project Lab"
UI_MARK=0 # a new log
MITCAD_TEST_WORK=$work/Lab MITCAD_TEST_PROJECT=Lab MITCAD_TEST_PROJECT_DIR=$MITCAD_PROJECTS_DIR/Lab \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --chrome=docked
wait_driver 'Docked'
ui_expect_log 'TestDriver: done' 'Docked: listed, opened, recorded, added, dropped, renamed and deleted'
check_versions Lab

ui_finish 'UI project files'

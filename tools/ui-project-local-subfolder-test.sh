#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/TestDriver.cpp app/framework/AppSettings.cpp
# check-all sources: core/vcs core/model/src/file core/ffi/src/vcs.rs core/ffi/src/projects.rs core/ffi/src/remote.rs
# New Project's Create as Local for Now in a folder that holds a file
# (mitcad#1601) through the real UI, by the shared driver script
# tools/ui-scripts/project-local-subfolder.mitcad-ui (the Windows counterpart
# is tools/ui-windows-project-local-subfolder-test.ps1):
#   1. New Project on the folder offers Create in a New Subfolder, named
#      New Project.
#   2. Cloud with an address that is not reached offers Create as Local for
#      Now.
#   3. Create as Local for Now makes the project in the new subfolder; the
#      folder itself gets none of the project's files and keeps its file.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-project-local-subfolder-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"

# Outside the checkout: a folder inside a git repository is reported as a
# project "inside" it, which this test does not expect (mitcad#1574).
work=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-project-local-subfolder.XXXXXX")
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
export MITCAD_TEST_REMOTE_RETRY_SECONDS=2
workshop=$MITCAD_PROJECTS_DIR/Holds

echo "--- A folder with a file (no project)"
mkdir -p "$workshop"
printf 'notes\n' > "$workshop/notes.txt"
echo "ok   the folder is made"

export MITCAD_TEST_WORKSHOP=$workshop MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_UNREACHABLE=https://127.0.0.1:9/nobody/nothing.git
export MITCAD_TEST_INPUT=$here/ui-scripts/project-local-subfolder.mitcad-ui
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

echo "--- The project in the new subfolder, the folder itself no project"
[ -f "$workshop/New Project/.mitcad/project.json" ] || ui_fail "no project in the new subfolder"
for made in .mitcad .git .gitattributes .gitignore; do
  [ ! -e "$workshop/$made" ] || ui_fail "the folder with the file got $made"
done
[ "$(cat "$workshop/notes.txt")" = notes ] || ui_fail "notes.txt changed"
echo "ok   the folder with the file is no project"

ui_finish "UI project local subfolder test"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs tools/cli
# Open from Cloud and the settings everyone in a Cloud project shares
# (mitcad#89) through the real UI, with bare repositories in folders
# standing in for the cloud (no network), checked with the system's git:
#   1. A Cloud project made with New Project (project A).
#   2. Open from Cloud: the folder follows the repository's name; A's own
#      folder is a clone of the same repository (Open It); a folder that is
#      not empty is refused with the reason; a new folder gets a clone of
#      the project (project B).
#   3. Open from Cloud of an empty repository makes a Cloud project there
#      (Create Project Here), and of one with files but no project makes
#      them a project (Make It a Project), the files' history kept.
#   4. A change for everyone (edit locks off in Project Settings) is a
#      version of its own, sent at once; Mitcad in project B follows it
#      after Sync.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh); Qt's own file dialogs (--no-native-dialogs).
# Usage: tools/ui-cloud-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }

WORK=$(mktemp -d /tmp/mitcad-ui-cloud.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
DOCUMENTS=$HOME/Documents/Mitcad
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
export MITCAD_TEST_REMOTE_RETRY_SECONDS=2

in_git() {
  local folder=$1
  shift
  git -C "$folder" "$@"
}

expect_git() {
  local actual
  # shellcheck disable=SC2086
  actual=$(in_git "$1" $2 2>&1) || ui_fail "$4: git $2: $actual"
  [ "$actual" = "$3" ] || ui_fail "$4: git $2 gives '$actual', expected '$3'"
  echo "ok   $4"
}

expect_sent() {
  local mine theirs
  mine=$(in_git "$1" rev-parse HEAD)
  theirs=$(git --git-dir="$2" rev-parse main 2> /dev/null)
  [ "$mine" = "$theirs" ] || ui_fail "$3: the remote's main is $theirs, $1 has $mine"
  echo "ok   $3"
}

type_text() { ui_type_text "$1"; }

opened() {
  ui_expect_new "Opened $1" "$2" "${3:-20}"
  ui_focus_main
}

expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  UI_RUNNER=""
  echo "ok   $1"
}

# open_from_cloud url: the Open from Cloud dialog with the address typed.
open_from_cloud() {
  ui_mark
  ui_step "open from cloud (search)"     ui_command "Open from Cloud"
  ui_expect_new "Open from Cloud dialog: $DOCUMENTS" "the dialog, in Documents/Mitcad"
  ui_focus_dialog '^Open from Cloud$'
  ui_step "the address (Alt+E)"          ui_key alt+e
  ui_step "type the address"             type_text "$1"
  ui_expect_new "Cloud check $1: " "the address checked" 20
}

# the_folder path: types the folder Open from Cloud opens into.
the_folder() {
  ui_step "the folder (Alt+F)"           ui_key alt+f
  ui_step "type the folder"              type_text "$1"
}

ui_start_display

echo "--- A Cloud project: New Project"
TEAM=$WORK/team.git
git init -q --bare "$TEAM"
A=$DOCUMENTS/team
ui_start_app --no-native-dialogs
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                type_text "$A"
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "type the repository's folder"   type_text "$TEAM"
ui_expect_new "Reachable and empty: ready." "an empty repository" 20
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "New Project: created $A: version " "made and sent" 30
opened "$A/team.mitcad" "its design opened"
ui_expect_new "Project indicator: team, cloud, synced" "Cloud, synced" 20
expect_sent "$A" "$TEAM" "the repository has the project"

echo "--- Open from Cloud: Open It, a folder that is not empty, a new folder"
open_from_cloud "$TEAM"
ui_expect_new "Project \"team\": 1 version(s), latest " "the summary: the project, its versions"
ui_expect_new "Open from Cloud dialog: open, enabled" "the folder named after the repository is its clone: Open It"
NOT_EMPTY=$WORK/not-empty
mkdir -p "$NOT_EMPTY"
echo "something" > "$NOT_EMPTY/notes.txt"
the_folder "$NOT_EMPTY"
ui_expect_new "Open from Cloud dialog: clone, disabled: $NOT_EMPTY is not empty: choose a new or empty folder." \
  "a folder that is not empty is refused" 20
B=$WORK/b/team
the_folder "$B"
ui_expect_new "Open from Cloud dialog: clone, enabled" "a new folder: Open" 20
ui_step "open (Enter)"                   ui_key Return
ui_expect_new "Open from Cloud: clone $TEAM into $B" "cloned"
opened "$B/team.mitcad" "its design opened" 30
ui_expect_new "Project indicator: team, cloud" "Cloud"
[ "$(in_git "$B" rev-parse HEAD)" = "$(in_git "$A" rev-parse HEAD)" ] || ui_fail "B has not A's versions"
echo "ok   B has A's versions"

echo "--- Open from Cloud: an empty repository (Create Project Here)"
FRESH=$WORK/fresh.git
git init -q --bare "$FRESH"
open_from_cloud "$FRESH"
ui_expect_new "The repository is empty: Create Project Here makes a Cloud project in it." "empty"
ui_expect_new "Open from Cloud dialog: create, enabled" "Create Project Here, the author git's" 20
ui_step "create project here (Enter)"    ui_key Return
ui_expect_new "Open from Cloud: create $FRESH into $DOCUMENTS/fresh" "made"
opened "$DOCUMENTS/fresh/fresh.mitcad" "its first design opened" 30
expect_sent "$DOCUMENTS/fresh" "$FRESH" "sent to the repository"
expect_git "$DOCUMENTS/fresh" "log --format=%s|%an" "Create project fresh|Ada Tester" "its first version"

echo "--- Open from Cloud: files without a project (Make It a Project)"
FILES=$WORK/files.git
git init -q --bare "$FILES"
SEED=$WORK/seed
git init -q "$SEED"
echo "# Parts" > "$SEED/README.md"
in_git "$SEED" add README.md
in_git "$SEED" -c user.name="Readme Author" -c user.email=readme@example.invalid commit -qm "Add a README"
in_git "$SEED" push -q "$FILES" HEAD:main 2> /dev/null
open_from_cloud "$FILES"
ui_expect_new "The repository has files but no Mitcad project (README.md): Make It a Project adds one beside them." \
  "files, no project"
ui_expect_new "Open from Cloud dialog: adopt, enabled" "Make It a Project" 20
ui_step "make it a project (Enter)"      ui_key Return
ui_expect_new "Open from Cloud: adopt $FILES into $DOCUMENTS/files" "adopted"
ui_expect_new "Open from Cloud: adopt done into $DOCUMENTS/files" "made a project" 30
ui_expect_new "Project: cloud files at $DOCUMENTS/files" "a Cloud project"
# No design in it: a new one named after it, saved as a version and sent.
ui_expect_new "Version recorded: files.mitcad " "its first design"
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent" 20
ui_focus_main
expect_git "$DOCUMENTS/files" "log --reverse --format=%s" "Add a README
Make this repository a Mitcad project
Save files.mitcad" "the project after the README's history"
expect_sent "$DOCUMENTS/files" "$FILES" "sent to the repository"
ui_key ctrl+q
expect_exit "exited"

echo "--- A change for everyone: recorded, sent, followed by B after Sync"
ui_start_app --open "$A/team.mitcad" --no-native-dialogs
ui_expect_log "Opened $A/team.mitcad" "A opened"
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "Project Settings dialog: team, cloud $TEAM; edit locks on, idle 10 min, poll 10 s" "edit locks on"
ui_focus_dialog '^Project Settings - team$'
ui_step "edit locks off (Alt+E)"         ui_key alt+e
ui_expect_new "Project Settings: shared settings recorded: " "a version of its own"
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent at once" 20
expect_git "$A" "log -1 --format=%s" "Change project settings: edit locks off" "the version's message"
expect_sent "$A" "$TEAM" "the repository has it"
ui_key ctrl+q
expect_exit "exited"
ui_start_app --open "$B/team.mitcad" --no-native-dialogs
ui_expect_log "Opened $B/team.mitcad" "B opened"
ui_expect_log "Remote check: ahead 0, behind 1" "B finds the newer version" 20
# B's settings still have edit locks on: the lock is taken, and the design
# is not brought up to date, as only the project's settings are newer.
ui_expect_log "Edit lock taken: team.mitcad" "B takes the edit lock" 20
ui_sync
grep -qF "Edit lock: bringing team.mitcad up to date" "$UI_LOG" &&
  ui_fail "B synced on opening, though only the project's settings are newer"
echo "ok   not synced on opening: the design itself is not newer"
ui_mark
ui_step "sync (Ctrl+Alt+Y)"              ui_key ctrl+alt+y
ui_expect_new "Sync: fast_forward, 0 replayed, nothing sent" "B takes it" 20
ui_focus_main
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "Project Settings dialog: team, cloud $TEAM; edit locks off" "B follows: edit locks off"
ui_focus_dialog '^Project Settings - team$'
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_key ctrl+q
expect_exit "exited"

ui_finish "UI cloud test"

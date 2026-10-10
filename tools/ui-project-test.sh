#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs tools/cli
# Local and Cloud projects (mitcad#89) through the real UI, with bare
# repositories in folders standing in for the cloud (no network), checked
# with the system's git:
#   1. New Project, Local: Documents/Mitcad/Project1 offered, the folder
#      typed, the author git's; the project's first version holds its first
#      design, named after the folder, which opens; the indicator says
#      "Robot arm, local, v1" and its menu has Version History and Project
#      Settings.
#   2. The regression of mitcad#89: right after New Project, Project
#      Settings > Cloud > Share sends the whole history to an empty
#      repository, with no other step; the indicator is Cloud and synced,
#      and its menu has Sync Now. Change... is refused for a repository of
#      another history and takes a copy of the same repository. Cloud ->
#      Local: Stop Syncing removes the remote, nothing else. Two remotes
#      and none followed: the indicator says no remote is chosen, and
#      Project Settings asks which to follow.
#   3. New Project, Cloud: to an empty repository (the address typed: a
#      shared folder), and to one with a README (the project beside it, the
#      history after the README's). Open It for a folder that is a project,
#      Open It Instead for a remote that holds one, Create as Local for Now
#      for a remote that cannot be reached.
#   4. Open Project of each kind of folder: a Cloud project, a Local one, an
#      older one without versions (its history starts), a folder inside a
#      project, a git repository and a folder of designs and an empty
#      folder (New Project with the folder fixed), a project inside another
#      git repository (no versions); the design chooser of a project with
#      several designs; Open... and Open Recent of a design in a project.
#   5. Move to a Project for a loose file: its own folder made a project,
#      and another loose file moved into an existing project.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh); Qt's own file dialogs (--no-native-dialogs), so
# paths can be typed into them.
# Usage: tools/ui-project-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

WORK=$(mktemp -d /tmp/mitcad-ui-project.XXXXXX)
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

# expect_git folder "git arguments" expected description
expect_git() {
  local actual
  # shellcheck disable=SC2086
  actual=$(in_git "$1" $2 2>&1) || ui_fail "$4: git $2: $actual"
  [ "$actual" = "$3" ] || ui_fail "$4: git $2 gives '$actual', expected '$3'"
  echo "ok   $4"
}

expect_clean() {
  local status
  status=$(in_git "$1" status --porcelain 2>&1)
  [ -z "$status" ] || ui_fail "$2: git status in $1: $status"
  echo "ok   $2"
}

# expect_sent project remote description: the remote's main is the project's.
expect_sent() {
  local mine theirs
  mine=$(in_git "$1" rev-parse HEAD)
  theirs=$(git --git-dir="$2" rev-parse main 2> /dev/null)
  [ "$mine" = "$theirs" ] || ui_fail "$3: the remote's main is $theirs, $1 has $mine"
  echo "ok   $3"
}

# A bare repository with a README (and its own history).
readme_remote() {
  local bare=$1 seed=$WORK/seed-$RANDOM
  git init -q --bare "$bare"
  git init -q "$seed"
  echo "# Shared parts" > "$seed/README.md"
  in_git "$seed" add README.md
  in_git "$seed" -c user.name="Readme Author" -c user.email=readme@example.invalid commit -qm "Add a README"
  in_git "$seed" push -q "$bare" HEAD:main 2> /dev/null
  rm -rf "$seed"
}

type_text() { ui_type_text "$1"; }

# type_path title path: the path typed into Qt's file dialog (ui_type_path), accepted.
type_path() {
  ui_type_path "$1" "$2"
}

# opened path description [seconds]: waits until a design opened (the
# dialog that made or opened it closed), then gives the main window the
# keyboard again (Xvfb has no window manager to do it).
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

# indicator_menu "entries" description: opens the project indicator's menu
# (a click where it was logged), checks its entries, and closes it.
indicator_menu() {
  ui_mark
  ui_step "the indicator's menu"         ui_click_logged "Project indicator"
  ui_expect_new "Project indicator menu: $1" "$2"
  ui_step "close the menu (Esc)"         ui_key Escape
}

# indicator_choose "entry": opens the indicator's menu and picks an entry
# with the arrow keys, from the logged "Project indicator menu: a | b".
indicator_choose() {
  local entries index=0 found=""
  ui_mark
  ui_step "the indicator's menu"         ui_click_logged "Project indicator"
  ui_expect_new "Project indicator menu: " "the menu" > /dev/null
  entries=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Project indicator menu: " | tail -1 |
    sed 's/.*Project indicator menu: //')
  IFS='|' read -ra items <<< "$entries"
  for item in "${items[@]}"; do
    index=$((index + 1))
    item=${item# }
    item=${item% }
    if [ "$item" = "$1" ]; then
      found=$index
      break
    fi
  done
  [ -n "$found" ] || ui_fail "'$1' is not in the indicator's menu: $entries"
  xdotool key --delay 80 --repeat "$found" Down
  ui_key Return
  ui_sync
}

ui_start_display

echo "--- New Project, Local"
ROBOT="$DOCUMENTS/Robot arm"
ui_start_app --no-native-dialogs
ui_expect_log "Project indicator: not in a project" "an untitled design is in no project"
indicator_menu "Move to a Project..." "a loose design's menu: Move to a Project"
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_expect_new "New Project dialog: $DOCUMENTS/Project1" "Documents/Mitcad/Project1 offered"
ui_expect_new "New Project dialog: folder $DOCUMENTS/Project1: missing" "the folder looked at"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                type_text "$ROBOT"
ui_expect_new "New Project dialog: folder $ROBOT: missing" "a new folder"
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "New Project: creating $ROBOT (local)" "created as Local"
ui_expect_new "New Project: created $ROBOT: version " "made" 20
ui_expect_new "Project: local Robot arm at $ROBOT" "the current project"
ui_expect_new "New project $ROBOT" "the window has the new project"
opened "$ROBOT/Robot arm.mitcad" "its first design opened" 20
ui_expect_new "Project indicator: Robot arm, local, v1" "the indicator: Local, v1"
expect_git "$ROBOT" "log --format=%s|%an|%ae" "Create project Robot arm|Ada Tester|ada@example.invalid" \
  "one version, by git's author"
expect_git "$ROBOT" "ls-files" ".gitattributes
.gitignore
.mitcad/project.json
Robot arm.mitcad" "the project's files and its first design"
expect_git "$ROBOT" "config user.email" "ada@example.invalid" "the author in the repository's configuration"
expect_git "$ROBOT" "remote" "" "no remote: Local"
expect_clean "$ROBOT" "git status is clean"
indicator_menu "Version History... | Project Settings..." "a Local project's menu"

echo "--- The regression: New Project, then Project Settings > Cloud > Share"
SHARE=$WORK/share.git
git init -q --bare "$SHARE"
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "Project Settings dialog: Robot arm, local; edit locks on, idle 10 min, poll 10 s; live updates none; author Ada Tester <ada@example.invalid>" \
  "Project Settings of the Local project"
ui_focus_dialog '^Project Settings - Robot arm$'
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_expect_new "Project Settings: storage cloud (share)" "the Cloud section to share"
ui_expect_new "Project Settings: authors to publish: Ada Tester <ada@example.invalid>" "the authors of its versions"
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "type the repository's folder"   type_text "$SHARE"
ui_expect_new "Cloud check $SHARE: " "the address checked" 20
ui_expect_new "Reachable and empty: the project's versions go there." "an empty repository"
ui_step "share (Alt+S)"                  ui_key alt+s
ui_expect_new "Project Settings: share to $SHARE" "shared"
ui_expect_new "Project Settings: shared Robot arm to $SHARE: versions sent" "its versions sent" 30
ui_expect_new "Project: cloud Robot arm at $ROBOT (origin $SHARE)" "the project is Cloud now"
ui_expect_new "Project indicator: Robot arm, cloud, synced" "the indicator: Cloud, synced" 20
expect_sent "$ROBOT" "$SHARE" "the remote has the whole history"
[ "$(in_git "$ROBOT" rev-parse --abbrev-ref '@{upstream}')" = "origin/main" ] || ui_fail "main does not follow origin/main"
echo "ok   main follows origin/main"

echo "--- Change...: another history refused, the same repository taken"
OTHER=$WORK/other.git
readme_remote "$OTHER"
MOVED=$WORK/moved.git
git clone -q --bare "$SHARE" "$MOVED"
ui_mark
ui_step "change the address (Alt+H)"     ui_key alt+h
ui_expect_new "Change Address dialog: $SHARE" "the dialog"
ui_focus_dialog '^Change Address$'
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "another repository"             type_text "$OTHER"
ui_expect_new "Change Address refused: This repository has another history" "another history is refused" 20
ui_step "the same repository, moved"     type_text "$MOVED"
ui_expect_new "Cloud check $MOVED: " "checked" 20
ui_expect_new "The same repository: it holds the project's history." "the same history"
ui_step "change (Enter)"                 ui_key Return
ui_expect_new "Project Settings: address changed to $MOVED" "the address changed"
expect_git "$ROBOT" "remote get-url origin" "$MOVED" "origin is the new address"
ui_focus_dialog '^Project Settings - Robot arm$'
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Project Settings closed" "closed"
indicator_menu "Sync Now | Check for Newer Versions | Version History... | Project Settings... | Open in Browser" \
  "a Cloud project's menu"

echo "--- Cloud -> Local: Stop Syncing"
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "Project Settings dialog: Robot arm, cloud $MOVED" "Project Settings of the Cloud project"
ui_focus_dialog '^Project Settings - Robot arm$'
ui_step "local (Alt+L)"                  ui_key alt+l
ui_expect_new "Project Settings: stop syncing asked" "asked first"
ui_focus_dialog '^Stop Syncing$'
ui_step "stop syncing (Alt+S)"           ui_key alt+s
ui_expect_new "Project Settings: stopped syncing $ROBOT (was $MOVED)" "stopped"
ui_focus_dialog '^Project Settings - Robot arm$'
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Project indicator: Robot arm, local, v1" "Local again"
expect_git "$ROBOT" "remote" "" "the remote removed"
[ "$(git --git-dir="$MOVED" rev-parse main)" = "$(in_git "$ROBOT" rev-parse HEAD)" ] || ui_fail "the repository lost versions"
echo "ok   the versions stay in the repository"
ui_key ctrl+q
expect_exit "exited"

echo "--- Several remotes, none followed: Project Settings asks which"
in_git "$ROBOT" remote add first "$SHARE"
in_git "$ROBOT" remote add second "$MOVED"
# The log starts afresh: what follows counts from its start.
UI_MARK=0
ui_start_app --no-native-dialogs --open "$ROBOT/Robot arm.mitcad"
opened "$ROBOT/Robot arm.mitcad" "the design opened" 20
ui_expect_log "Project: local Robot arm at $ROBOT (remotes first, second, none followed)" "no remote followed"
ui_expect_log "Project indicator: Robot arm, cloud, no remote chosen" "the indicator: no remote chosen"
indicator_menu "Version History... | Project Settings..." "nothing to sync yet"
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "Project Settings: several remotes, none followed: first, second" "Project Settings asks which"
ui_focus_dialog '^Project Settings - Robot arm$'
ui_step "the remote (Alt+M)"             ui_key alt+m
ui_step "the second one (Down)"          ui_key Down
ui_step "follow (Alt+W)"                 ui_key alt+w
ui_expect_new "Project Settings: following second ($MOVED), second/main" "second followed"
ui_expect_new "Project: cloud Robot arm at $ROBOT (second $MOVED)" "the project is Cloud, through second"
expect_git "$ROBOT" "config branch.main.remote" "second" "main follows second"
ui_focus_dialog '^Project Settings - Robot arm$'
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Project indicator: Robot arm, cloud, synced" "the indicator: Cloud, synced" 20
ui_key ctrl+q
expect_exit "exited"
in_git "$ROBOT" remote remove first
in_git "$ROBOT" remote remove second

echo "--- New Project, Cloud: an empty repository"
EMPTY=$WORK/empty.git
git init -q --bare "$EMPTY"
GEAR=$DOCUMENTS/gear
ui_start_app --no-native-dialogs
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                type_text "$GEAR"
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_expect_new "New Project dialog: storage cloud" "Cloud chosen"
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "type the repository's folder"   type_text "$EMPTY"
ui_expect_new "Cloud check $EMPTY: " "checked" 20
ui_expect_new "Reachable and empty: ready." "empty: ready"
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "New Project: creating $GEAR (cloud $EMPTY)" "created as Cloud"
ui_expect_new "New Project: created $GEAR: version " "made and sent" 30
opened "$GEAR/gear.mitcad" "its first design opened" 20
ui_expect_new "Project indicator: gear, cloud, synced" "the indicator: Cloud, synced" 20
expect_sent "$GEAR" "$EMPTY" "the remote has the project"
expect_clean "$GEAR" "git status is clean"

echo "--- New Project, Cloud: a repository with a README"
README_REMOTE=$WORK/readme.git
readme_remote "$README_REMOTE"
PLATE=$DOCUMENTS/plate
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                type_text "$PLATE"
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "type the repository's folder"   type_text "$README_REMOTE"
ui_expect_new "Mitcad adds the project beside the repository's files (README.md)." "the README stays" 20
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "New Project: created $PLATE: version " "made beside the README" 30
opened "$PLATE/plate.mitcad" "its first design opened" 20
expect_git "$PLATE" "log --reverse --format=%s" "Add a README
Create project plate" "the project's version after the README's"
[ -f "$PLATE/README.md" ] || ui_fail "no README.md in the project"
echo "ok   the README is in the project's folder"
expect_sent "$PLATE" "$README_REMOTE" "the remote has the project"

echo "--- New Project: Open It for a project's folder"
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "a project's folder"             type_text "$ROBOT"
ui_expect_new "New Project dialog: folder $ROBOT: project" "a project already"
ui_step "open it (Enter)"                ui_key Return
ui_expect_new "New Project: Open It $ROBOT" "Open It"
ui_expect_new "Open Project: $ROBOT: project" "opened as a project"
opened "$ROBOT/Robot arm.mitcad" "its design opened" 20
ui_expect_new "Project indicator: Robot arm, local, v1" "the indicator: Robot arm"

echo "--- New Project: Open It Instead for a remote with a project"
OPENED=$DOCUMENTS/shared-gear
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                type_text "$OPENED"
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "a repository with a project"    type_text "$EMPTY"
ui_expect_new "This repository holds a Mitcad project: Open It Instead opens it into the folder." "it holds a project" 20
ui_step "open it instead (Alt+I)"        ui_key alt+i
ui_expect_new "New Project: Open It Instead $EMPTY into $OPENED" "Open It Instead"
ui_expect_new "Open from Cloud dialog: " "Open from Cloud with the same address and folder"
ui_focus_dialog '^Open from Cloud$'
ui_expect_new "Open from Cloud dialog: clone, enabled" "ready to open it" 20
ui_step "open (Enter)"                   ui_key Return
ui_expect_new "Open from Cloud: clone $EMPTY into $OPENED" "cloned"
opened "$OPENED/gear.mitcad" "its design opened" 30
ui_expect_new "Project indicator: shared-gear, cloud" "Cloud"

echo "--- New Project: Create as Local for Now"
UNREACHABLE=https://127.0.0.1:9/nobody/nothing.git
LATER=$DOCUMENTS/later
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                type_text "$LATER"
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "a server that is not there"     type_text "$UNREACHABLE"
ui_expect_new "Cloud check $UNREACHABLE: network" "it cannot be reached" 30
ui_step "create as local for now (Alt+N)" ui_key alt+n
ui_expect_new "New Project: Create as Local for Now" "Create as Local for Now"
ui_expect_new "New Project: creating $LATER (local)" "made Local"
ui_expect_new "Project indicator: later, local, v1" "the indicator: Local" 20
ui_focus_main
expect_git "$LATER" "remote" "" "no remote"
ui_key ctrl+q
expect_exit "exited"

echo "--- Open Project: each kind of folder"
OLD=$WORK/old
"$CLI" project init "$OLD" --no-history > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project init"; }
mkdir -p "$OLD/parts"
OUTER=$WORK/outer
git init -q "$OUTER"
"$CLI" project init "$OUTER/inner" --no-history > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project init"; }
REPO=$WORK/repo
readme_remote "$WORK/parts.git"
git clone -q "$WORK/parts.git" "$REPO"
DESIGNS=$WORK/designs
mkdir -p "$DESIGNS"
cp "$ROBOT/Robot arm.mitcad" "$DESIGNS/a.mitcad"
cp "$ROBOT/Robot arm.mitcad" "$DESIGNS/b.mitcad"
EMPTY_FOLDER=$WORK/empty-folder
mkdir -p "$EMPTY_FOLDER"
ui_start_app --no-native-dialogs
# open_project folder: Open Project of a folder typed into the folder dialog.
open_project() {
  ui_mark
  ui_step "open project (search)"        ui_command "Open Project"
  ui_step "type the folder"              type_path "Open Project" "$1"
  ui_expect_new "Open Project: $1: $2" "$3"
}
open_project "$GEAR" project "a Cloud project"
opened "$GEAR/gear.mitcad" "its design opened" 20
ui_expect_new "Remote check: ahead 0, behind 0" "its remote checked" 20
ui_expect_new "Project indicator: gear, cloud, synced" "Cloud"
open_project "$ROBOT" project "a Local project"
opened "$ROBOT/Robot arm.mitcad" "its design opened" 20
ui_expect_new "Project indicator: Robot arm, local, v1" "Local"
open_project "$OLD" project "an older project without versions"
ui_expect_new "Version history started in $OLD: version " "its history starts"
ui_expect_new "Open Project: no designs in $OLD: a new one, $OLD/old.mitcad" "a design named after it"
ui_expect_new "Version recorded: old.mitcad " "saved as a version" 20
ui_expect_new "Project indicator: old, local, v1" "Local"
ui_focus_main
open_project "$OLD/parts" inside_project "a folder inside a project"
opened "$OLD/old.mitcad" "the project's design opened" 20
open_project "$OUTER/inner" project "a project inside another repository"
ui_expect_new "Project: no versions inner at $OUTER/inner (inside $OUTER)" "a project without versions"
ui_expect_new "Project indicator: inner, no versions (inside $OUTER)" "the indicator says so"
ui_focus_main
indicator_menu "Move to a Project..." "its menu: Move to a Project"
open_project "$EMPTY_FOLDER" empty "an empty folder"
ui_expect_new "New Project dialog: $EMPTY_FOLDER" "New Project with the folder"
ui_focus_dialog '^New Project$'
ui_expect_new "New Project dialog: folder $EMPTY_FOLDER: empty" "an empty folder"
ui_step "create (Enter)"                 ui_key Return
opened "$EMPTY_FOLDER/empty-folder.mitcad" "made a project with a design" 20
open_project "$REPO" repository "a git repository"
ui_expect_new "New Project dialog: storage cloud" "Storage preset to Cloud: the repository has a remote"
ui_focus_dialog '^New Project$'
ui_expect_new "New Project dialog: folder $REPO: repository" "a repository"
ui_step "local (Alt+L)"                  ui_key alt+l
ui_step "create (Enter)"                 ui_key Return
opened "$REPO/repo.mitcad" "made a project in the repository" 20
open_project "$DESIGNS" designs "a folder of designs"
ui_focus_dialog '^New Project$'
ui_expect_new "New Project dialog: folder $DESIGNS: designs" "designs"
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "Design chooser: designs: " "several designs: the chooser" 20
ui_focus_dialog '^Open Project$'
ui_step "the second (Down, Enter)"       ui_key Down Return
ui_focus_main
ui_expect_new "Design chooser: chose " "chosen"
chosen=$(grep "Design chooser: chose " "$UI_LOG" | tail -1 | sed 's/.*Design chooser: chose //')
opened "$DESIGNS/$chosen" "the design chosen opened"
expect_git "$DESIGNS" "ls-files" ".gitattributes
.gitignore
.mitcad/project.json
a.mitcad
b.mitcad" "the designs are in the first version"
ui_key ctrl+q
expect_exit "exited"

echo "--- Open Project: the design chooser selects the design opened last"
ui_start_app --no-native-dialogs
open_project "$DESIGNS" project "the folder of designs is a project now"
ui_expect_new "Design chooser: designs: " "the chooser" 20
ui_expect_new "(selected $chosen)" "the design opened last selected"
ui_focus_dialog '^Open Project$'
ui_step "open the one selected (Enter)"  ui_key Return
opened "$DESIGNS/$chosen" "opened"

echo "--- Open... and Open Recent of a design in a project"
ui_mark
ui_step "open (Ctrl+O)"                  ui_key ctrl+o
ui_step "a design of a project"          type_path "Open" "$ROBOT/Robot arm.mitcad"
ui_focus_main
opened "$ROBOT/Robot arm.mitcad" "opened" 20
ui_expect_new "Project: local Robot arm at $ROBOT" "its project is the current one"
ui_mark
ui_step "file menu (Alt+F)"              ui_key alt+f
ui_step "open recent (R)"                ui_key r
ui_expect_new "Recent files: Robot arm.mitcad - Robot arm | " "designs listed with their projects"
ui_step "the second (2)"                 ui_key 2
opened "$DESIGNS/" "Open Recent opened the other project's design" 20
ui_expect_new "Project: local designs at $DESIGNS" "its project is the current one"

echo "--- Move to a Project: a loose file's folder made a project"
LOOSE=$WORK/loose
mkdir -p "$LOOSE"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "outside projects"               type_path "Save As" "$LOOSE/part.mitcad"
ui_focus_main
ui_expect_new "Saved $LOOSE/part.mitcad" "saved"
ui_expect_new "Project indicator: not in a project" "a loose file"
indicator_choose "Move to a Project..."
ui_expect_new "Move to a Project dialog: part.mitcad" "asked where"
ui_focus_dialog '^Move to a Project$'
ui_step "a new project (Enter)"          ui_key Return
ui_expect_new "New Project dialog: $LOOSE" "its own folder offered"
ui_focus_dialog '^Move to a Project$'
ui_expect_new "New Project dialog: folder $LOOSE: designs" "a folder of designs"
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "Recorded $LOOSE/part.mitcad in the project $LOOSE" "recorded in its folder's project" 20
ui_focus_main
ui_expect_new "Project indicator: loose, local" "the indicator: loose, Local"
first=$(in_git "$LOOSE" log --reverse --format=%s | head -1)
[ "$first" = "Create project loose" ] || ui_fail "the first version is '$first'"
expect_git "$LOOSE" "ls-tree --name-only $(in_git "$LOOSE" rev-list --max-parents=0 HEAD) part.mitcad" "part.mitcad" \
  "the design in the first version"
expect_clean "$LOOSE" "git status is clean"

echo "--- Move to a Project: into an existing project"
STRAY=$WORK/stray
mkdir -p "$STRAY"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "outside projects"               type_path "Save As" "$STRAY/bolt.mitcad"
ui_focus_main
ui_expect_new "Project indicator: not in a project" "a loose file"
indicator_choose "Move to a Project..."
ui_focus_dialog '^Move to a Project$'
ui_step "an existing project (Alt+E)"    ui_key alt+e
ui_step "the project's folder"           type_path "Choose a Project" "$ROBOT"
ui_expect_new "Moved $STRAY/bolt.mitcad to $ROBOT/bolt.mitcad" "moved" 20
ui_focus_main
[ ! -e "$STRAY/bolt.mitcad" ] || ui_fail "the file it came from is still there"
echo "ok   the file it came from is gone"
expect_git "$ROBOT" "log -1 --format=%s" "Save bolt.mitcad" "recorded in the project"
ui_expect_new "Project indicator: Robot arm, local, v1" "the indicator: Robot arm"
ui_key ctrl+q
expect_exit "exited"

ui_finish "UI project test"

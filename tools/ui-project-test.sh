#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs tools/cli
# Local and Cloud projects (mitcad#89) through the real UI, with bare
# repositories in folders standing in for the cloud (no network), checked
# with the system's git:
#   1. New Project, Local: Documents/Mitcad/Project1 offered, the folder
#      typed, the author git's; the project starts empty (mitcad#145), its
#      first design is made with File > New Design, named in the Projects
#      panel, and opens; the indicator says "Robot arm, local, v1" and its
#      menu has Version History and Project Settings.
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
#   4. Open Project (mitcad#176: the project picker lists the projects, also
#      one without versions; Another Folder... the folder dialog) of each
#      kind of folder: a Cloud project, a Local one, an older one without
#      versions (its history starts), a folder inside a project, a git
#      repository and a folder of designs and an empty folder (New Project
#      with the folder fixed), a project inside another git repository (no
#      versions); a project with several designs opens
#      the newest, then the one opened last (no design chooser: the
#      Projects panel lists them); Open... and Open Recent of a design in a
#      project; New Design (Ctrl+N) there names a design in the Projects
#      panel (mitcad#175: New and New Design are one command).
#   5. Save to Project (mitcad#503, File menu after Save As; Move to a Project
#      before) for a loose file (an untitled design after Close, saved
#      outside projects): the picker's New Project... makes its own folder a
#      project; another loose file saved into an existing project chosen in
#      the project picker (mitcad#174): the projects with versions listed by
#      name, a name clash shown in it, a Cloud project's folder chosen, the
#      window showing the project's file and the one it came from still
#      there, the version sent and the design selected in the Projects
#      panel; the File menu without Move to a Project; Save with
#      Description's question opening the picker, and Another Folder... for a
#      project typed into the folder dialog. Save As while a project is
#      current: the picker's Another Folder...
#   6. Insert Component (mitcad#176): a design of the current project (the
#      open design not offered) linked, another project's as a copy, and a
#      file through Another File...
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh); Qt's own file dialogs (--no-native-dialogs), so
# paths can be typed into them.
# Usage: tools/ui-project-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

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

# picker_listed title: waits until the project picker titled so has read the
# projects since the mark ("Project picker <title>: <n> projects: ..."),
# and gives them.
picker_listed() {
  local line=""
  for _ in $(seq 1 100); do
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Project picker $1: [0-9]* projects: " | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  echo "${line:-the project picker $1 listed no projects}"
}

# picked_project root kind: the key the project picker logs the project at
# root by (the name the Projects panel shows, or its folder when two
# listed projects have the same name).
picked_project() {
  grep "^Project picker project .*: $2, $1\$" "$UI_LOG" | tail -1 | sed 's/^Project picker project \(.*\): [a-z ]*, .*$/\1/'
}

# save_as_elsewhere path: Save As while a project is current: the project
# picker (mitcad#176), then Another Folder... and the path typed into the
# file dialog.
save_as_elsewhere() {
  ui_step "save as (Ctrl+Shift+S)"       ui_key ctrl+shift+s
  ui_expect_new "Project picker Save in a Project: a place, reading the projects" "the project picker"
  ui_focus_dialog '^Save in a Project$'
  ui_step "another folder (Alt+A)"       ui_key alt+a
  ui_expect_new "Project picker Save in a Project: another folder" "the file dialog instead"
  ui_step "outside projects"             type_path "Save As" "$1"
}

# opened path description [seconds]: waits until a design opened (the
# dialog that made or opened it closed), then gives the main window the
# keyboard again (Xvfb has no window manager to do it).
opened() {
  ui_expect_new "Opened $1" "$2" "${3:-20}"
  ui_focus_main
}

# expect_title title: the main window's title (the design shown).
expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(ui_window_title)
    [ "$title" = "$1" ] && { echo "ok   title '$1'"; return; }
    sleep 0.2
  done
  ui_fail "window title is '$title', expected '$1'"
}

# new_design name: a new project's first design (mitcad#145: New Project
# makes an empty project), File > New Design named in the Projects panel;
# it is recorded as a version and opens.
new_design() {
  ui_focus_main
  ui_step "new design (search)"          ui_command "New Design"
  ui_expect_new "Projects panel: naming a new design in " "named in the Projects panel" 20
  ui_step "type its name"                xdotool type --delay 40 "$1"
  ui_step "take it (Enter)"              ui_key Return
  ui_expect_new "New Design: made " "made as a version" 20
}

# expect_exit description: waits for the app to quit with exit code 0 (a
# crash while quitting fails here, mitcad#825).
expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  if command -v gdb > /dev/null && [ "${UI_GDB:-0}" = 1 ]; then
    grep -q "exited normally" "$UI_LOG" || ui_fail "$1: abnormal exit"
  else
    wait "$UI_RUNNER" || ui_fail "$1: exit code $?"
  fi
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
    if [ "${item//…/...}" = "${1//…/...}" ]; then
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
indicator_menu "Save to Project..." "a loose design's menu: Save to Project"
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
ui_expect_new "Open Project: no designs in $ROBOT" "an empty project: no design named after it" 20
new_design "Robot arm"
opened "$ROBOT/Robot arm.mitcad" "its first design opened" 20
ui_expect_new "Project indicator: Robot arm, local, v1" "the indicator: Local, v1"
expect_git "$ROBOT" "log --format=%s|%an|%ae" "Add Robot arm.mitcad|Ada Tester|ada@example.invalid
Create project Robot arm|Ada Tester|ada@example.invalid" "two versions, by git's author"
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
ui_expect_new "Open Project: no designs in $GEAR" "an empty project" 20
new_design "gear"
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
ui_expect_new "MitCad adds the project beside the repository's files (README.md)." "the README stays" 20
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "New Project: created $PLATE: version " "made beside the README" 30
ui_expect_new "Open Project: no designs in $PLATE" "no designs beside the README" 20
new_design "plate"
opened "$PLATE/plate.mitcad" "its first design opened" 20
expect_git "$PLATE" "log --reverse --format=%s" "Add a README
Create project plate
Add plate.mitcad" "the project's versions after the README's"
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
ui_expect_new "This repository holds a MitCad project: Open It Instead opens it into the folder." "it holds a project" 20
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
ui_expect_new "Project indicator: later, local" "the indicator: Local" 20
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
# open_project folder: Open Project of a folder typed into the folder dialog
# (the project picker's Another Folder..., mitcad#176).
open_project() {
  ui_mark
  ui_step "open project (search)"        ui_command "Open Project"
  ui_expect_new "Project picker Open a Project: a project, reading the projects" "the project picker"
  ui_focus_dialog '^Open a Project$'
  ui_step "another folder (Alt+A)"       ui_key alt+a
  ui_expect_new "Project picker Open a Project: another folder" "the folder dialog instead"
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
ui_expect_new "Open Project: no designs in $OLD" "no design named after it (mitcad#145)"
ui_expect_new "Project indicator: old, local" "Local"
ui_focus_main
open_project "$OLD/parts" inside_project "a folder inside a project"
ui_expect_new "Open Project: no designs in $OLD" "the project opened" 20
ui_focus_main
open_project "$OUTER/inner" project "a project inside another repository"
ui_expect_new "Project: no versions inner at $OUTER/inner (inside $OUTER)" "a project without versions"
ui_expect_new "Project indicator: inner, no versions (inside $OUTER)" "the indicator says so"
ui_focus_main
indicator_menu "Save to Project..." "its menu: Save to Project"
open_project "$EMPTY_FOLDER" empty "an empty folder"
ui_expect_new "New Project dialog: $EMPTY_FOLDER" "New Project with the folder"
ui_focus_dialog '^New Project$'
ui_expect_new "New Project dialog: folder $EMPTY_FOLDER: empty" "an empty folder"
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "Open Project: no designs in $EMPTY_FOLDER" "made an empty project" 20
ui_focus_main
open_project "$REPO" repository "a git repository"
ui_expect_new "New Project dialog: storage cloud" "Storage preset to Cloud: the repository has a remote"
ui_focus_dialog '^New Project$'
ui_expect_new "New Project dialog: folder $REPO: repository" "a repository"
ui_step "local (Alt+L)"                  ui_key alt+l
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "Open Project: no designs in $REPO" "made a project in the repository" 20
ui_focus_main
open_project "$DESIGNS" designs "a folder of designs"
ui_focus_dialog '^New Project$'
ui_expect_new "New Project dialog: folder $DESIGNS: designs" "designs"
ui_step "create (Enter)"                 ui_key Return
# No design chooser (mitcad#145): the newest design opens, the Projects
# panel lists the others.
ui_expect_new "(newest)" "several designs: the newest opens" 20
chosen=$(grep -o "Open Project: [ab]\.mitcad (newest)" "$UI_LOG" | tail -1 | sed 's/Open Project: //; s/ (newest)//')
[ -n "$chosen" ] || ui_fail "no design opened"
opened "$DESIGNS/$chosen" "the newest design opened"
ui_expect_new "Projects tree: designs (local, current): a.mitcad v1" "the Projects panel lists the designs" 20
expect_git "$DESIGNS" "ls-files" ".gitattributes
.gitignore
.mitcad/project.json
a.mitcad
b.mitcad" "the designs are in the first version"
ui_key ctrl+q
expect_exit "exited"

echo "--- Open Project: chosen in the project picker; the design opened last opens"
ui_start_app --no-native-dialogs
ui_mark
ui_step "open project (search)"          ui_command "Open Project"
ui_expect_new "Project picker Open a Project: a project, reading the projects" "the project picker (mitcad#176)"
listed=$(picker_listed "Open a Project")
designs=$(picked_project "$DESIGNS" local)
[ -n "$designs" ] || ui_fail "the folder of designs is not listed: $listed"
[ -n "$(picked_project "$GEAR" cloud)" ] || ui_fail "the Cloud project is not listed: $listed"
[ -n "$(picked_project "$OUTER/inner" "no versions")" ] || ui_fail "the project without versions is not listed: $listed"
echo "ok   the projects listed, also one without versions"
ui_focus_dialog '^Open a Project$'
ui_step "the folder of designs"          ui_click_logged "Project picker row $designs"
ui_expect_new "Project picker selected $designs" "selected"
ui_step "open (Enter)"                   ui_key Return
ui_expect_new "Project picker Open a Project: chose $DESIGNS" "chosen in the picker"
ui_expect_new "Open Project: $DESIGNS: project" "the folder of designs is a project now"
ui_expect_new "Open Project: $chosen (opened last)" "the design opened last" 20
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

echo "--- New Design (Ctrl+N) with a current project: named in the Projects panel"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
ui_expect_new "New Design: in $DESIGNS" "a design of the current project, not an untitled one"
ui_expect_new "Projects panel: naming a new design in " "named in the Projects panel" 20
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Projects panel: new design cancelled" "cancelled: nothing made"
ui_focus_main
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "New document"; then
  ui_fail "New Design in a project opened an untitled design"
fi

echo "--- Save to Project: a loose file's folder made a project"
LOOSE=$WORK/loose
mkdir -p "$LOOSE"
ui_mark
ui_step "close: untitled (Ctrl+W)"       ui_key ctrl+w
ui_expect_new "Document closed" "an untitled design"
save_as_elsewhere "$LOOSE/part.mitcad"
ui_focus_main
ui_expect_new "Saved $LOOSE/part.mitcad" "saved"
ui_expect_new "Project indicator: not in a project" "a loose file"
indicator_choose "Save to Project..."
ui_expect_new "Project picker Save to Project: a place, reading the projects" "the project picker (mitcad#503)"
ui_focus_dialog '^Save to Project$'
ui_step "a new project (Alt+P)"          ui_key alt+p
ui_expect_new "Project picker Save to Project: new project" "New Project... chosen in the picker"
ui_expect_new "New Project dialog: $LOOSE" "its own folder offered"
ui_focus_dialog '^Save to Project$'
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

echo "--- Save to Project: into an existing project (the project picker, mitcad#174), the file it came from stays"
STRAY=$WORK/stray
mkdir -p "$STRAY" "$GEAR/parts"
ui_mark
ui_step "close: untitled (Ctrl+W)"       ui_key ctrl+w
ui_expect_new "Document closed" "an untitled design"
save_as_elsewhere "$STRAY/bolt.mitcad"
ui_focus_main
ui_expect_new "Project indicator: not in a project" "a loose file"
ui_mark
ui_step "file menu (Alt+F)"              ui_key alt+f
ui_expect_new "File menu: " "the File menu"
file_menu=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "File menu: " | tail -1)
case "$file_menu" in
*"Save As | Save to Project | Save with Description"*) echo "ok   Save to Project right after Save As" ;;
*) ui_fail "Save to Project is not after Save As: $file_menu" ;;
esac
case "$file_menu" in
*"Move to a Project"*) ui_fail "the File menu still has Move to a Project: $file_menu" ;;
*) echo "ok   no Move to a Project in the File menu" ;;
esac
ui_step "close the menu (Esc)"           ui_key Escape
indicator_choose "Save to Project..."
ui_expect_new "Project picker Save to Project: a place, reading the projects" "the project picker, no folder dialog"
listed=$(picker_listed "Save to Project")
robot=$(picked_project "$ROBOT" local)
gear=$(picked_project "$GEAR" cloud)
[ -n "$robot" ] || ui_fail "the Local project is not listed: $listed"
[ -n "$gear" ] || ui_fail "the Cloud project is not listed: $listed"
echo "ok   the projects listed by their names: $robot (local), $gear (cloud)"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Project picker project .*, $OUTER/inner\$" && ui_fail "a project without versions is offered: $listed"
echo "ok   only projects with versions"
ui_focus_dialog '^Save to Project$'
ui_step "the Local project"              ui_click_logged "Project picker row $robot"
ui_expect_new "Project picker selected $robot" "selected"
ui_expect_new "Project picker target $robot/bolt.mitcad: free" "the design's name offered, free there"
ui_step "the name field"                 ui_click_logged "Project picker name"
ui_step "a name the project has"         type_text "Robot arm"
ui_expect_new "Project picker target $robot/Robot arm.mitcad: exists" "the clash shown in the dialog"
ui_step "another name"                   type_text "bolt"
ui_expect_new "Project picker target $robot/bolt.mitcad: free" "free again"
ui_step "the Cloud project"              ui_click_logged "Project picker row $gear"
ui_step "its folders (+)"                ui_key plus
ui_sync
ui_step "its folder parts"               ui_click_logged "Project picker row $gear/parts/"
ui_expect_new "Project picker selected $gear/parts/" "a folder in the project"
ui_expect_new "Project picker target $gear/parts/bolt.mitcad: free" "the place in the folder"
ui_step "save (Enter)"                   ui_key Return
ui_expect_new "Project picker Save to Project: chose $GEAR/parts/bolt.mitcad" "chosen in the picker"
ui_expect_new "Saved $GEAR/parts/bolt.mitcad" "saved as Save As saves" 20
ui_expect_new "Save to Project: $GEAR/parts/bolt.mitcad saved in $GEAR" "saved in the project" 20
ui_focus_main
# The window shows the project's file; the one it came from stays.
expect_title "bolt.mitcad - MitCad"
[ -f "$GEAR/parts/bolt.mitcad" ] || ui_fail "the design is not in the project"
[ -f "$STRAY/bolt.mitcad" ] || ui_fail "the file it came from is gone"
echo "ok   the file it came from stays"
expect_git "$GEAR" "log -1 --format=%s" "Save bolt.mitcad" "recorded in the Cloud project"
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent as Save's versions are" 20
expect_sent "$GEAR" "$EMPTY" "the remote has the saved design"
ui_expect_new "Project indicator: gear, cloud" "the indicator: gear"
for _ in $(seq 1 100); do
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Projects panel selected .*parts/bolt\.mitcad\$" && break
  sleep 0.2
done
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Projects panel selected .*parts/bolt\.mitcad\$" ||
  ui_fail "the Projects panel does not show the saved design selected"
echo "ok   the Projects panel shows the saved design selected"

echo "--- Save to Project through Save with Description's question: Another Folder... for a project not listed"
NUTS=$WORK/nuts
mkdir -p "$NUTS"
ui_mark
ui_step "close: untitled (Ctrl+W)"       ui_key ctrl+w
ui_expect_new "Document closed" "an untitled design"
# The new untitled design starts with its Component1 active, as every new
# design does (mitcad#898); an import opens with the root active instead
# (mitcad#426).
ui_expect_new "Opened the new design with Component1 active" "the new design has Component1 active"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Opened with the root active" &&
  ui_fail "the new design after Close opened with the root active"
save_as_elsewhere "$NUTS/nut.mitcad"
ui_focus_main
ui_expect_new "Project indicator: not in a project" "a loose file"
ui_mark
ui_step "save with description (search)" ui_command "Save with Description"
ui_expect_new "Save with Description: no version history for nut.mitcad" "a loose design has no versions"
ui_focus_dialog '^Save with Description$'
ui_step "save to project (Enter)"        ui_key Return
ui_expect_new "Project picker Save to Project: a place, reading the projects" "the question opens the project picker"
ui_focus_dialog '^Save to Project$'
ui_step "another folder (Alt+A)"         ui_key alt+a
ui_expect_new "Project picker Save to Project: another folder" "the folder dialog instead"
ui_step "the project's folder"           type_path "Choose a Project Folder" "$ROBOT"
ui_expect_new "Save to Project: $ROBOT/nut.mitcad saved in $ROBOT" "saved in the project" 20
ui_focus_main
[ -f "$NUTS/nut.mitcad" ] || ui_fail "the file it came from is gone"
echo "ok   the file it came from stays"
expect_git "$ROBOT" "log -1 --format=%s" "Save nut.mitcad" "recorded in the project"
ui_expect_new "Project indicator: Robot arm, local, v1" "the indicator: Robot arm"

echo "--- Insert Component: a design chosen in the project picker (mitcad#176)"
# Designs with a body (the projects' designs so far are empty, and an
# empty design is no component): one in each project and one outside.
cat > "$WORK/block.json" << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
JSON
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli: a design with a body"; }
cp "$WORK/block.mitcad" "$ROBOT/plate.mitcad"
in_git "$ROBOT" add plate.mitcad
in_git "$ROBOT" commit -qm "Add plate.mitcad"
cp "$WORK/block.mitcad" "$GEAR/pin.mitcad"
in_git "$GEAR" add pin.mitcad
in_git "$GEAR" commit -qm "Add pin.mitcad"
# nut.mitcad began as a new design, with its Component1 active
# (mitcad#898): the component goes into Component1, which takes components
# (mitcad#421).
ui_mark
ui_step "insert component (search)"      ui_command "Insert Component"
ui_expect_new "Project picker Insert from a Project: a design, reading the projects" "the project picker"
listed=$(picker_listed "Insert from a Project")
case "$listed" in
"Project picker Insert from a Project: "*" projects: $robot (local)"*) echo "ok   the current project first" ;;
*) ui_fail "the current project is not first: $listed" ;;
esac
ui_focus_dialog '^Insert from a Project$'
ui_expect_new "Project picker row $robot/plate.mitcad at " "the current project expanded"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Project picker row $robot/nut\.mitcad at " &&
  ui_fail "the open design is offered"
echo "ok   the open design is not offered"
ui_step "a design of the project"        ui_click_logged "Project picker row $robot/plate.mitcad"
ui_expect_new "Project picker selected $robot/plate.mitcad" "selected"
ui_step "insert (Enter)"                 ui_key Return
ui_expect_new "Project picker Insert from a Project: chose $ROBOT/plate.mitcad" "chosen in the picker"
ui_focus_dialog '^Insert Component$'
ui_step "linked (Enter)"                 ui_key Return
ui_focus_main
ui_expect_new "(linked) from plate.mitcad" "inserted, linked"
ui_mark
ui_step "insert component (search)"      ui_command "Insert Component"
listed=$(picker_listed "Insert from a Project")
ui_focus_dialog '^Insert from a Project$'
ui_step "another project"                ui_click_logged "Project picker row $gear"
ui_step "its designs (+)"                ui_key plus
ui_sync
ui_step "its design"                     ui_click_logged "Project picker row $gear/pin.mitcad"
ui_expect_new "Project picker selected $gear/pin.mitcad" "a design of another project"
ui_step "insert (Enter)"                 ui_key Return
ui_expect_new "Project picker Insert from a Project: chose $GEAR/pin.mitcad" "chosen in the picker"
ui_focus_dialog '^Insert Component$'
ui_step "a copy (Alt+C)"                 ui_key alt+c
ui_focus_main
ui_expect_new "(copy) from pin.mitcad" "inserted as a copy"
ui_mark
ui_step "insert component (search)"      ui_command "Insert Component"
ui_expect_new "Project picker Insert from a Project: a design, reading the projects" "the project picker"
ui_focus_dialog '^Insert from a Project$'
ui_step "another file (Alt+A)"           ui_key alt+a
ui_expect_new "Project picker Insert from a Project: another file" "the file dialog instead"
ui_step "a design's file"                type_path "Insert Component" "$WORK/block.mitcad"
ui_focus_dialog '^Insert Component$'
ui_step "linked (Enter)"                 ui_key Return
ui_focus_main
ui_expect_new "(linked) from block.mitcad" "inserted from the file dialog"
ui_key ctrl+q
ui_focus_dialog '^MitCad$'
# Don't Save ends the app: no ui_step, whose sync key may come after the
# app is gone (a late xdotool on a busy machine) and whose crash check then
# took the normal end for a crash (mitcad#825); expect_exit checks the end.
ui_key d
expect_exit "don't save (D): exited"

ui_finish "UI project test"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/AppSettings.cpp app/framework/FloatingLayout.cpp app/framework/QuickAccessGroup.cpp
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/projects.rs tools/cli
# The Projects panel (mitcad#145) through the real UI, with a bare
# repository in a folder standing in for the cloud (no network), checked
# with the system's git:
#   1. View > Projects shows and hides the panel. New Project makes an
#      empty project, which the panel shows (no design named after the
#      folder).
#   2. File > New Design names a design in place in the panel: recorded as
#      a version, opened, bold as the open design. New Folder and New
#      Design in the folder from the context menu.
#   3. Save of an untitled design: the project picker starts in the
#      panel's selected folder (mitcad#176).
#   4. Rename (F2) of the open design and a drag into another folder: each
#      a version; the open design follows its new path. Rename from the
#      context menu (cancelled); the toolbar has no Rename button.
#   5. Delete (Del, confirmed) of a design, and of the open design, which
#      closes; the history keeps them. A double-click opens a design.
#   6. A Cloud project added with Add Existing Project Folder is listed
#      at the top of the tree (mitcad#173: no location row above it), shown
#      by its shared folder's path, with its sync state; Remove from List
#      takes it out and deletes nothing.
#      (After step 2: the Projects and Browser trees indent a level by the
#      same amount, mitcad#393.)
#   7. Open Project (the project picker) opens the design opened last;
#      the panel stays shown at the next start. The Floating layout has
#      the panel in a card.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh); Qt's own file dialogs (--no-native-dialogs), so
# paths can be typed into them.
# Usage: tools/ui-projects-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

WORK=$(mktemp -d /tmp/mitcad-ui-projects.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
DOCUMENTS=$HOME/Documents/Mitcad
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"

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

expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  UI_RUNNER=""
  echo "ok   $1"
}

opened() {
  ui_expect_new "Opened $1" "$2" "${3:-20}"
  ui_focus_main
}

# row "key": waits until the panel has logged where a row is (the panel
# logs every row again when one of them moved, so the last place is the
# row's place now).
row() {
  ui_expect_log "Projects row $1 at " "the row $1 is shown"
}

# name_it "text": types a name into the panel's editor and takes it.
name_it() {
  ui_step "type the name"                xdotool type --delay 40 "$1"
  ui_step "take it (Enter)"              ui_key Return
}

ui_start_display

echo "--- View > Projects; New Project makes an empty project the panel shows"
SHOP=$DOCUMENTS/Shop
ui_start_app --no-native-dialogs
ui_mark
ui_step "projects panel (search)"        ui_command "Projects"
ui_expect_new "Projects panel: shown" "shown"
# Logged when it changes: at the start already, while the panel was hidden.
ui_expect_log "Projects panel: 0 projects" "no projects yet"
ui_step "projects panel again (search)"  ui_command "Projects"
ui_expect_new "Projects panel: hidden" "hidden"
ui_mark
ui_step "show the empty projects panel"  ui_command "Projects"
# The plus is in the panel: it is shown before New Project, and stays.
ui_expect_new "Projects panel: shown" "the panel shown again"
ui_step "plain plus with no project"     ui_click_logged "Projects button New Project"
ui_focus_dialog '^New Project$'
ui_step "a cancelled project folder"     ui_key alt+f
ui_step "type the cancelled folder"      ui_type_text "$DOCUMENTS/CancelledProject"
ui_step "cancel creation"                ui_key Escape
ui_focus_main
[ ! -e "$DOCUMENTS/CancelledProject" ] || ui_fail "cancel created a project"
ui_expect_new "New Project cancelled" "the plain plus opened New Project"
ui_mark
ui_step "plain plus creates a project"   ui_click_logged "Projects button New Project"
ui_focus_dialog '^New Project$'
ui_step "the folder (Alt+F)"             ui_key alt+f
ui_step "type the folder"                ui_type_text "$SHOP"
ui_expect_new "New Project dialog: folder $SHOP: missing" "a new folder"
ui_step "create (Enter)"                 ui_key Return
ui_expect_new "New Project: created $SHOP: version " "made" 20
ui_focus_main
ui_expect_new "Open Project: no designs in $SHOP" "no design named after the folder"
ui_expect_new "Projects tree: Shop (local, current): no designs" "an empty Local project" 20
ui_expect_log "Projects top rows: Shop" "the project at the top, no row above it"
ui_expect_log "Projects names: Shop = Shop" "a Local project shown by its folder's name"
# The panel was shown before New Project (its plus) and stays shown.
! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Projects panel: hidden" ||
  ui_fail "the panel was hidden while the project was made"
echo "ok   the panel stays shown with the project"
expect_git "$SHOP" "ls-files" ".gitattributes
.gitignore
.mitcad/project.json" "only the project's own files"

echo "--- Document-plus New Design, named in the panel"
ui_mark
ui_step "document-plus creates a design" ui_click_logged "Projects button New Design"
ui_expect_new "Projects panel: naming a new design in Shop" "named in place" 20
name_it "bracket"
ui_expect_new "Projects panel: new_design recorded as " "recorded" 20
opened "$SHOP/bracket.mitcad" "the new design opened"
expect_git "$SHOP" "log -1 --format=%s" "Add bracket.mitcad" "a version of its own"
ui_expect_new "Projects tree: Shop (local, current): bracket.mitcad v1, open" "listed, the open one" 20

echo "--- The Projects tree indents each level as the Browser does (mitcad#393)"
# Both panels log their tree's indentation per level; one definition
# (framework/PanelTree.hpp) gives it to both.
ui_expect_log "Browser indentation " "the Browser logs its tree's indentation"
ui_expect_log "Projects indentation " "the Projects panel logs its tree's indentation"
projects_indent=$(grep -o "^Projects indentation [0-9]*" "$UI_LOG" | tail -1 | cut -d" " -f3)
browser_indent=$(grep -o "^Browser indentation [0-9]*" "$UI_LOG" | tail -1 | cut -d" " -f3)
{ [ -n "$projects_indent" ] && [ "$projects_indent" = "$browser_indent" ]; } ||
  ui_fail "indentation per level: Projects ${projects_indent:-?}, Browser ${browser_indent:-?}"
echo "ok   both trees indent $projects_indent px per level"

echo "--- New Folder and New Design in it (context menu)"
row "Shop"
ui_mark
ui_step "right-click the project"        ui_click_logged "Projects row Shop" 3
ui_step "new folder"                     ui_menu_choose "New Folder"
ui_expect_new "Projects panel: naming a new folder in Shop" "named in place" 20
name_it "parts"
ui_expect_new "Projects panel: create_folder recorded as " "recorded" 20
expect_git "$SHOP" "log -1 --format=%s" "Add folder parts" "a version of its own"
ui_expect_new "Projects tree: Shop (local, current): parts/ | bracket.mitcad v1, open" "the folder listed" 20
row "Shop/parts/"
ui_mark
ui_step "right-click the folder"         ui_click_logged "Projects row Shop/parts/" 3
ui_step "new design"                     ui_menu_choose "New Design"
ui_expect_new "Projects panel: naming a new design in Shop/parts/" "in the folder" 20
name_it "plate"
opened "$SHOP/parts/plate.mitcad" "the new design opened"
expect_git "$SHOP" "log -1 --format=%s" "Add parts/plate.mitcad" "a version of its own"
ui_expect_new "Projects tree: Shop (local, current): parts/ | bracket.mitcad v1 | parts/plate.mitcad v1, open" \
  "in its folder" 20

echo "--- Save of an untitled design starts in the selected folder"
ui_mark
ui_step "close (Ctrl+W)"                 ui_key ctrl+w
ui_expect_new "Document closed" "an untitled design (New Design would name one here)"
row "Shop/parts/"
ui_step "select the folder"              ui_click_logged "Projects row Shop/parts/"
# Logged when the selection changes: the folder may be selected already
# (the new design's row went, its folder stayed selected).
ui_expect_log "Projects panel selected Shop/parts/" "selected"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
# The project picker (mitcad#176), the selected folder preselected.
ui_expect_new "Project picker target Shop/parts/Untitled.mitcad: free" "the picker starts in the selected folder" 20
ui_focus_dialog '^Save in a Project$'
ui_step "only a name"                    ui_type_text "gear"
ui_expect_new "Project picker target Shop/parts/gear.mitcad: free" "the name typed"
ui_step "save (Enter)"                   ui_key Return
ui_focus_main
ui_expect_new "Saved $SHOP/parts/gear.mitcad" "saved in the selected folder" 20
ui_expect_new "parts/gear.mitcad v1, open" "listed" 20

echo "--- Rename (F2) of the open design: it follows"
row "Shop/parts/gear.mitcad"
ui_mark
ui_step "select it"                      ui_click_logged "Projects row Shop/parts/gear.mitcad"
ui_step "rename (F2)"                    ui_key F2
ui_expect_new "Projects panel: renaming Shop/parts/gear.mitcad" "renamed in place" 20
name_it "cog"
ui_expect_new "Projects panel: move recorded as " "recorded" 20
ui_expect_new "Projects panel: the open design follows: $SHOP/parts/gear.mitcad -> $SHOP/parts/cog.mitcad" \
  "the open design follows"
expect_git "$SHOP" "log -1 --format=%s" "Move parts/gear.mitcad to parts/cog.mitcad" "a version of its own"
[ -f "$SHOP/parts/cog.mitcad" ] && [ ! -e "$SHOP/parts/gear.mitcad" ] || ui_fail "the file was not renamed"
echo "ok   the file renamed"
ui_expect_new "Projects row Shop/parts/cog.mitcad at " "the panel shows the new name" 20

echo "--- Rename from the context menu; no Rename button on the toolbar"
! grep -qF "Projects button Rename " "$UI_LOG" || ui_fail "the toolbar has a Rename button"
echo "ok   no Rename button on the toolbar"
row "Shop/parts/cog.mitcad"
ui_mark
ui_step "right-click it"                 ui_click_logged "Projects row Shop/parts/cog.mitcad" 3
ui_step "rename"                         ui_menu_choose "Rename"
ui_expect_new "Projects panel: renaming Shop/parts/cog.mitcad" "renamed in place from the context menu" 20
ui_step "keep the name (Escape)"         ui_key Escape
ui_expect_new "Projects panel: rename of parts/cog.mitcad cancelled" "cancelled: the name stays"
[ -f "$SHOP/parts/cog.mitcad" ] || ui_fail "the cancelled rename changed the file"
echo "ok   the file kept its name"

echo "--- A drag moves a design within the project"
ui_mark
ui_step "drag it onto the project"       ui_drag_logged "Projects row Shop/parts/cog.mitcad" "Projects row Shop"
ui_expect_new "Projects panel: drag parts/cog.mitcad to cog.mitcad" "dropped on the project" 20
ui_expect_new "Projects panel: the open design follows: $SHOP/parts/cog.mitcad -> $SHOP/cog.mitcad" "it follows" 20
expect_git "$SHOP" "log -1 --format=%s" "Move parts/cog.mitcad to cog.mitcad" "a version of its own"
ui_expect_new "Projects row Shop/cog.mitcad at " "the panel shows it in the project's top" 20
ui_focus_main
# A change first: an unchanged design has nothing to save (mitcad#1090).
ui_step "new component (search)"         ui_command "New Component"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "New component " "a change to save"
ui_save_state enabled "the moved design has a change to save"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $SHOP/cog.mitcad" "Save writes the new path" 20
expect_clean "$SHOP" "git status is clean"

echo "--- Delete (Del): a design, then the open one, which closes"
row "Shop/bracket.mitcad"
ui_mark
ui_step "select bracket"                 ui_click_logged "Projects row Shop/bracket.mitcad"
ui_step "delete (Del)"                   ui_key Delete
ui_expect_new "Delete dialog: bracket.mitcad (1 designs)" "asked first"
ui_focus_dialog '^Delete$'
ui_step "delete (Alt+D)"                 ui_key alt+d
ui_focus_main
ui_expect_new "Projects panel: delete recorded as " "recorded" 20
[ ! -e "$SHOP/bracket.mitcad" ] || ui_fail "bracket.mitcad is still there"
expect_git "$SHOP" "log -1 --format=%s" "Delete bracket.mitcad" "a version of its own"
expect_git "$SHOP" "log --format=%s -- bracket.mitcad" "Delete bracket.mitcad
Add bracket.mitcad" "the history keeps it"
ui_expect_new "Projects tree: Shop (local, current): parts/ | cog.mitcad v" "bracket is no longer listed" 20
row "Shop/cog.mitcad"
ui_mark
ui_step "select cog"                     ui_click_logged "Projects row Shop/cog.mitcad"
ui_step "delete (Del)"                   ui_key Delete
ui_expect_new "Delete dialog: cog.mitcad (1 designs, open)" "asked: it is open"
ui_focus_dialog '^Delete$'
ui_step "delete (Alt+D)"                 ui_key alt+d
ui_focus_main
ui_expect_new "Projects panel: closed the deleted design $SHOP/cog.mitcad" "it closed" 20
[ ! -e "$SHOP/cog.mitcad" ] || ui_fail "cog.mitcad is still there"
ui_expect_new "Projects tree: Shop (local, current): parts/ | parts/plate.mitcad v1" "what is left" 20
row "Shop/parts/plate.mitcad"
ui_double_click_expect "Projects row Shop/parts/plate.mitcad" "Opened $SHOP/parts/plate.mitcad" \
  "a double-click opens a design"
ui_focus_main

echo "--- A Cloud project: Add Existing Project Folder, Remove from List"
mkdir -p "$WORK/remotes"
git init -q --bare "$WORK/remotes/team.git"
TEAM=$WORK/team
"$CLI" project create "$TEAM" --author "Ada Tester <ada@example.invalid>" --url "$WORK/remotes/team.git" \
  > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project create"; }
row "Shop"
ui_mark
ui_step "right-click Shop"               ui_click_logged "Projects row Shop" 3
ui_step "add existing project folder"    ui_menu_choose "Add Existing Project Folder..."
ui_step "type the folder"                ui_type_path "Add Existing Project Folder" "$TEAM"
ui_focus_main
ui_expect_new "Projects panel: added $TEAM" "added to the list"
ui_expect_new "Projects tree: team (cloud, synced): no designs" "keyed by its folder's name, synced" 20
# No location row above it: the project's own row, shown by the shared
# folder's path (sorted before Shop: '/' comes before 'S').
ui_expect_new "Projects top rows: team | Shop" "only the projects at the top" 20
ui_expect_new "Projects names: team = $WORK/remotes/team.git | Shop = Shop" \
  "the Cloud project shown by its shared folder's path" 20
row "team"
ui_mark
ui_step "right-click it"                 ui_click_logged "Projects row team" 3
ui_step "remove from list"               ui_menu_choose "Remove from List"
ui_expect_new "Projects panel: removed $TEAM from the list" "removed"
ui_expect_new "Projects panel: 1 project: Shop" "only Shop is listed" 20
ui_expect_new "Projects top rows: Shop" "only Shop at the top" 20
[ -f "$TEAM/.mitcad/project.json" ] || ui_fail "the project's folder was touched"
echo "ok   nothing deleted"
ui_key ctrl+q
expect_exit "exited"

echo "--- The next start: the panel shown; Open Project opens the design opened last"
ui_start_app --no-native-dialogs
UI_MARK=0 # a new log
ui_expect_log "Projects panel: 1 project: Shop" "the panel is shown again" 20
ui_mark
ui_step "open project (search)"          ui_command "Open Project"
# The project picker lists it (mitcad#176): no folder dialog.
ui_expect_new "Project picker Open a Project: 1 projects: Shop (local)" "the project listed" 20
ui_focus_dialog '^Open a Project$'
ui_expect_new "Project picker selected Shop" "selected"
ui_step "open (Enter)"                   ui_key Return
ui_expect_new "Project picker Open a Project: chose $SHOP" "chosen in the picker"
ui_expect_new "Open Project: parts/plate.mitcad (opened last)" "the design opened last" 20
opened "$SHOP/parts/plate.mitcad" "opened"
ui_key ctrl+q
expect_exit "exited"

echo "--- Floating: the Projects card"
ui_start_app --no-native-dialogs --chrome=floating
UI_MARK=0 # a new log
row "Shop"
ui_expect_log "Projects panel: 1 project: Shop" "the projects listed in the card"
# Not the current project: collapsed until opened.
ui_double_click_expect "Projects row Shop" "Projects row Shop/parts/plate.mitcad at " \
  "a double-click on the project shows its designs"
ui_double_click_expect "Projects row Shop/parts/plate.mitcad" "Opened $SHOP/parts/plate.mitcad" \
  "a double-click in the card opens a design"
ui_key ctrl+q
expect_exit "exited"

ui_finish "UI projects test"

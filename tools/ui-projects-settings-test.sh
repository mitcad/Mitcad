#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/AppSettings.cpp app/framework/FloatingLayout.cpp
# check-all sources: core/vcs core/model/src/file/settings.rs core/ffi/src/vcs.rs core/ffi/src/projects.rs tools/cli
# The Projects panel's defaults and its projects' settings through the real
# UI (mitcad#151, #152, #153, #173), without a network:
#   1. A fresh profile without projects starts with the panel hidden; with
#      projects configured (no choice made yet) it starts hidden too
#      (mitcad#1020: File > Open from Project does its job); shown with
#      View > Projects in the Floating card, it stays shown at the next
#      start, in the Docked dock.
#   2. A flat tree: no row above a project, the projects sorted by the name
#      shown. A Cloud project's row shows its repository's address
#      (github.com/owner/repository), a Local one the folder's name. A
#      double-click on the Cloud project opens its Project Settings (not
#      opening the project); a display name typed there is all its row
#      shows, stays after a restart, and cleared the address is back. The
#      row stays expanded and selected when the display name is set or
#      cleared. The folder and the repository keep their names.
#   3. The settings button opens Project Settings of the selected project
#      for a project row (Local and Cloud), a folder row and a design row
#      (the current project's).
#   4. Hiding the panel is remembered at the next start; so is showing it.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-projects-settings-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

WORK=$(mktemp -d /tmp/mitcad-ui-projects-settings.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
DOCUMENTS=$HOME/Documents/Mitcad
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
AUTHOR="Ada Tester <ada@example.invalid>"

cli() {
  "$CLI" "$@" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli $*"; }
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

quit() {
  ui_focus_main
  ui_key ctrl+q
  expect_exit "exited"
}

row() {
  ui_expect_log "Projects row $1 at " "the row $1 is shown" 20
}

# settings_button "row key" "logged line" description: selects the row,
# then the toolbar's settings button, which must log the line.
settings_button() {
  row "$1"
  ui_mark
  ui_step "select $1"                    ui_click_logged "Projects row $1"
  # Logged when the selection changes: it may be selected already.
  ui_expect_log "Projects panel selected $1" "$1 selected"
  ui_step "the settings button"          ui_click_logged "Projects button Project Settings"
  ui_expect_new "$2" "$3" 20
}

# close_settings "dialog title": Escape closes Project Settings.
close_settings() {
  ui_focus_dialog "$(ui_title_regex "$1")"
  ui_mark
  ui_step "close it (Escape)"            ui_key Escape
  ui_expect_new "Project Settings closed" "closed"
  ui_focus_main
}

ui_start_display

echo "--- A fresh profile without projects: the panel is hidden"
ui_start_app
UI_MARK=0
ui_expect_log "Projects panel at the start: hidden (no choice yet, no projects)" "hidden by default"
quit

echo "--- Projects configured: a Local one, a Cloud one (a remote on a service; no network)"
SHOP=$DOCUMENTS/Shop
GRIPPER=$DOCUMENTS/gripper-work
cli project create "$SHOP" --author "$AUTHOR"
cli project new-design "$SHOP" parts/plate.mitcad --author "$AUTHOR"
cli project create "$GRIPPER" --author "$AUTHOR"
cli project new-design "$GRIPPER" arm.mitcad --author "$AUTHOR"
git -C "$GRIPPER" remote add origin git@github.com:acme/gripper.git
echo "ok   projects made"

echo "--- Floating: hidden by default, shown in its card; a flat tree; a double-click opens the settings"
ui_start_app --chrome=floating
UI_MARK=0
ui_expect_log "Projects panel at the start: hidden (no choice yet, projects configured)" \
  "hidden by default, also with projects configured"
ui_mark
ui_step "projects panel (search)"        ui_command "Projects"
ui_expect_new "Projects panel: shown at the next start" "shown, the choice remembered"
# No row above a project (mitcad#173): the projects are the top rows, by
# the name shown.
ui_expect_log "Projects top rows: gripper-work | Shop" "only the projects at the top, by the name shown" 20
ui_expect_log "Projects names: gripper-work = github.com/acme/gripper | Shop = Shop" \
  "a Cloud project as its repository's address, a Local one as its folder" 20
row "gripper-work"
ui_mark
ui_step "select the Cloud project"       ui_click_logged "Projects row gripper-work"
ui_step "expand it (Right)"              ui_key Right
ui_expect_new "Projects row gripper-work/arm.mitcad at " "its design shown below it" 20
ui_double_click_expect "Projects row gripper-work" \
  "Projects panel: settings of $GRIPPER (double-click on gripper-work)" \
  "a double-click on the Cloud project asks for its settings"
ui_expect_log "Projects panel: Project Settings of $GRIPPER (not the current project)" \
  "opened without opening the project" 20
ui_focus_dialog '^Project Settings - gripper-work$'
ui_expect_log "Project Settings dialog: gripper-work, cloud git@github.com:acme/gripper.git" "its settings"
ui_mark
named=$UI_MARK # close_settings marks again: the rebuild may come before it
ui_step "the display name (Alt+D)"       ui_key alt+d
ui_type_text "Workshop (team)"
ui_step "take it (Tab)"                  ui_key Tab
ui_expect_new "Project Settings: display name Workshop (team)" "the display name set"
close_settings "Project Settings - gripper-work"
ui_expect_log "Projects names: Shop = Shop | gripper-work = Workshop (team) (set)" \
  "the panel shows only the display name" 20
ui_expect_log "Projects top rows: Shop | gripper-work" "sorted by the display name" 20
# Moved below Shop: logged at its new place (since the name was typed),
# still expanded and selected.
UI_MARK=$named
ui_expect_new "Projects row gripper-work/arm.mitcad at " "still expanded after the display name was set" 20
ui_mark
ui_step "the settings button"            ui_click_logged "Projects button Project Settings"
ui_expect_new "Projects panel: settings of $GRIPPER (gripper-work selected)" \
  "still selected after the display name was set" 20
close_settings "Project Settings - gripper-work"
[ -d "$GRIPPER" ] || ui_fail "the folder was renamed"
[ "$(git -C "$GRIPPER" remote get-url origin)" = "git@github.com:acme/gripper.git" ] ||
  ui_fail "the remote changed"
grep -q '"Workshop (team)"' "$GRIPPER/.mitcad/local/name.json" || ui_fail "the display name is not in name.json"
[ -z "$(git -C "$GRIPPER" status --porcelain)" ] || ui_fail "the display name changed the project's files"
echo "ok   the folder, the repository and the versions are untouched"
quit

echo "--- Docked: shown as chosen; the display name stays; the settings button"
ui_start_app
UI_MARK=0
ui_expect_log "Projects panel at the start: shown (as chosen)" "shown at the next start, as the user chose"
ui_expect_log "Projects names: Shop = Shop | gripper-work = Workshop (team) (set)" \
  "the display name after a restart" 20
ui_expect_log "Projects top rows: Shop | gripper-work" "only the projects at the top" 20
settings_button "Shop" "Projects panel: settings of $SHOP (Shop selected)" \
  "the settings button with a project row selected"
ui_expect_log "Project Settings dialog: Shop, local" "Project Settings of the project"
close_settings "Project Settings - Shop"
ui_double_click_expect "Projects row Shop" "Projects row Shop/parts/ at " \
  "a double-click on a Local project shows its folders"
settings_button "Shop/parts/" "Projects panel: settings of $SHOP (Shop/parts/ selected)" \
  "the settings button with a folder row selected"
close_settings "Project Settings - Shop"
row "Shop/parts/plate.mitcad"
ui_double_click_expect "Projects row Shop/parts/plate.mitcad" "Opened $SHOP/parts/plate.mitcad" \
  "a double-click opens the design"
ui_focus_main
settings_button "Shop/parts/plate.mitcad" \
  "Projects panel: settings of $SHOP (Shop/parts/plate.mitcad selected)" \
  "the settings button with a design row selected"
ui_expect_log "Projects panel: Project Settings of the current project $SHOP" "the current project's"
close_settings "Project Settings - Shop"
settings_button "gripper-work" "Projects panel: settings of $GRIPPER (gripper-work selected)" \
  "the settings button with the Cloud project's row selected"
close_settings "Project Settings - gripper-work"
row "gripper-work"
ui_mark
ui_step "select the Cloud project"       ui_click_logged "Projects row gripper-work"
ui_step "expand it (Right)"              ui_key Right
ui_expect_new "Projects row gripper-work/arm.mitcad at " "its design shown below it" 20

echo "--- The display name cleared: the repository's address again"
ui_double_click_expect "Projects row gripper-work" \
  "Projects panel: settings of $GRIPPER (double-click on gripper-work)" \
  "a double-click on the Cloud project asks for its settings"
ui_focus_dialog '^Project Settings - gripper-work$'
ui_mark
cleared=$UI_MARK # close_settings marks again: the rebuild may come before it
ui_step "the display name (Alt+D)"       ui_key alt+d
ui_step "select it"                      ui_key ctrl+a
ui_step "clear it"                       ui_key BackSpace
ui_step "take it (Tab)"                  ui_key Tab
ui_expect_new "Project Settings: display name cleared: github.com/acme/gripper" "cleared"
close_settings "Project Settings - gripper-work"
UI_MARK=$cleared
ui_expect_log "Projects names: gripper-work = github.com/acme/gripper | Shop = Shop" \
  "the repository's address again" 20
ui_expect_new "Projects top rows: gripper-work | Shop" "sorted by the address again, nothing above it" 20
# Moved above Shop: logged at its new place (since the name was cleared),
# still expanded and selected.
ui_expect_new "Projects row gripper-work/arm.mitcad at " "still expanded after the display name was cleared" 20
ui_mark
ui_step "the settings button"            ui_click_logged "Projects button Project Settings"
ui_expect_new "Projects panel: settings of $GRIPPER (gripper-work selected)" \
  "still selected after the display name was cleared" 20
close_settings "Project Settings - gripper-work"
[ ! -e "$GRIPPER/.mitcad/local/name.json" ] || ui_fail "name.json is still there"
echo "ok   no display name kept"

echo "--- Hidden by the user: remembered"
# The view takes the keyboard back from the panel's tree (its letters).
ui_step "click the view"                 ui_click 75 50
ui_mark
ui_step "projects panel (search)"        ui_command "Projects"
ui_expect_new "Projects panel: hidden at the next start" "the choice remembered"
quit
ui_start_app
UI_MARK=0
ui_expect_log "Projects panel at the start: hidden (as chosen)" "hidden at the next start"
ui_mark
ui_step "projects panel (search)"        ui_command "Projects"
ui_expect_new "Projects panel: shown at the next start" "shown again, remembered"
quit
ui_start_app --chrome=floating
UI_MARK=0
ui_expect_log "Projects panel at the start: shown (as chosen)" "shown at the next start (Floating too)"
row "Shop"
quit

ui_finish "UI projects settings test"

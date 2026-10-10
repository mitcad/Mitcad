#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files
# Remote repositories (P12 remote) through the real UI, against a bare
# repository in a folder (no network), checked with the system's git:
#   1. A design saved into Local project A, which Project Settings shares
#      to the empty remote (its address typed in the Cloud section): the
#      versions are sent, the remote's state is synced. A change saved is a
#      version, sent at once.
#   2. Open from Cloud into a new folder (by default
#      Documents/Mitcad/<repository>): project B with the design opened.
#   3. Another user changes the design in B and pushes it with git. A opened
#      again checks the remote: the status bar has the newer version, the
#      notice under the toolbar names it and who saved it; its Sync Now
#      takes it (a fast-forward) and opens the design again.
#   4. Both change the design: the first change checks the remote again,
#      A's save cannot be sent (the remote has a newer one), the notice says
#      a sync will ask; Sync (Ctrl+Alt+Y) stops
#      for the file in Resolve Sync Conflicts, whose default keeps A's as a
#      copy: the file is B's, the copy A's, both sent, the history one line.
#   5. Offline: the remote gone, a saved version waits (the status bar shows
#      it) and is sent once the remote is back, by the retries alone.
#   6. Preferences' Cloud page shows the git program found.
#
# git's configuration and HOME are the test's own (as in
# tools/ui-version-test.sh). Runs headless on Xvfb (see ui-test-lib.sh); Qt's
# own file dialogs (--no-native-dialogs), so paths can be typed into them.
# Usage: tools/ui-sync-test.sh
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

WORK=$(mktemp -d /tmp/mitcad-ui-sync.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
DOCUMENTS=$HOME/Documents/Mitcad
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n' > "$GIT_CONFIG_GLOBAL"
# A failed push is tried again after 2 s (then 4, 8, ...), not a minute.
export MITCAD_TEST_REMOTE_RETRY_SECONDS=2

REMOTE=$WORK/remote.git
A=$DOCUMENTS/bracket
FILE_A=$A/block.mitcad
B=$DOCUMENTS/remote
FILE_B=$B/block.mitcad
git init -q --bare "$REMOTE"
"$CLI" project init "$A" --author "Ada Tester <ada@example.invalid>" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; echo "FAIL: mitcad-cli project init"; exit 1; }
# Without edit locks (mitcad#89): a design whose lock is taken is brought
# up to date at once, while this test is about the notice of a newer
# version, Sync Now and Resolve Sync Conflicts (tools/ui-locks-test.sh has
# the locks).
python3 - "$A/.mitcad/project.json" << 'EOF'
import json, sys

marker = json.load(open(sys.argv[1]))
marker['edit_locks'] = {'enabled': False, 'idle_minutes': 10, 'poll_seconds': 10}
with open(sys.argv[1], 'w') as f:
    f.write(json.dumps(marker, indent=2) + '\n')
EOF
git -C "$A" -c user.name="Ada Tester" -c user.email=ada@example.invalid commit -qam "Edit locks off" ||
  { echo "FAIL: git commit in A"; exit 1; }

# in_git folder git-arguments...
in_git() {
  local folder=$1
  shift
  git -C "$folder" "$@"
}

expect_clean() {
  local status
  status=$(in_git "$1" status --porcelain 2>&1)
  [ -z "$status" ] || ui_fail "$2: git status in $1: $status"
  echo "ok   $2"
}

# expect_sent project description: the remote's branch is the project's.
expect_sent() {
  local mine theirs
  mine=$(in_git "$1" rev-parse HEAD)
  theirs=$(git --git-dir="$REMOTE" rev-parse main 2> /dev/null)
  [ "$mine" = "$theirs" ] || ui_fail "$2: the remote's main is $theirs, $1 has $mine"
  echo "ok   $2"
}

# file_d3 project [value]: prints d3 of a project file, or sets it.
file_d3() {
  python3 - "$@" << 'EOF'
import json, sys

doc = json.load(open(sys.argv[1]))
values = [p for p in doc['parameters'] if p['name'] == 'd3']
if len(sys.argv) > 2:
    values[0]['value'] = float(sys.argv[2])
    values[0]['expression'] = '%g mm' % float(sys.argv[2])
    with open(sys.argv[1], 'w') as f:
        f.write(json.dumps(doc, indent=2) + '\n')
else:
    print([p['value'] for p in values])
EOF
}

# as_other d3 message: the other user changes the design in B with git and
# pushes it.
as_other() {
  file_d3 "$FILE_B" "$1"
  in_git "$B" -c user.name="Bea Other" -c user.email=bea@example.invalid commit -qam "$2" ||
    ui_fail "git commit in B"
  in_git "$B" push -q origin main 2> "$WORK/push.log" || { cat "$WORK/push.log"; ui_fail "git push from B"; }
  echo "ok   the other user pushed: $2"
}

# type_text text: replaces the text of the focused dialog's field (ui_type_text).
type_text() { ui_type_text "$1"; }

# type_path title path: the path typed into Qt's file dialog (ui_type_path), accepted.
type_path() {
  ui_type_path "$1" "$2"
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

ui_start_display

echo "--- Share project A to an empty remote; a saved version is sent"
ui_start_app --demo --no-native-dialogs
ui_mark
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "into project A"                 type_path "Save As" "$FILE_A"
ui_expect_new "Version recorded: block.mitcad " "the design is a version of A"
ui_expect_new "Project indicator: bracket, local, v1" "a Local project"
ui_mark
ui_step "project settings (search)"      ui_command "Project Settings"
ui_expect_new "Project Settings dialog: bracket, local; " "Project Settings"
ui_focus_dialog '^Project Settings - bracket$'
ui_step "cloud (Alt+O)"                  ui_key alt+o
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "the remote's address"           type_text "$REMOTE"
ui_expect_new "Cloud check $REMOTE: " "the address checked" 20
ui_step "share (Alt+S)"                  ui_key alt+s
ui_expect_new "Project Settings: shared bracket to $REMOTE: versions sent" "shared, the versions sent" 30
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Remote status: synced" "synced"
ui_expect_new "Project indicator: bracket, cloud, synced" "the indicator: Cloud, synced"
expect_sent "$A" "the remote has A's versions"
[ "$(in_git "$A" rev-parse --abbrev-ref '@{upstream}')" = "origin/main" ] || ui_fail "main does not follow origin/main"
echo "ok   main follows origin/main"
ui_mark
ui_step "fit (F6)"                       ui_key F6
ui_step "fillet (F)"                     ui_key f
ui_step "pick a face of the body"        ui_view_click 50 50
ui_step "confirm (Enter)"                ui_key Return
ui_expect_new "Added fillet on 4 edge(s)" "a fillet"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version recorded: block.mitcad " "a version"
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent at once"
ui_expect_new "Remote status: synced" "synced again"
expect_sent "$A" "the remote has the new version"
expect_clean "$A" "A's git status is clean"

echo "--- Open from Cloud: project B"
ui_mark
ui_step "open from cloud (search)"       ui_command "Open from Cloud"
ui_expect_new "Open from Cloud dialog: $DOCUMENTS" "the dialog, in Documents/Mitcad"
ui_focus_dialog '^Open from Cloud$'
ui_step "the address (Alt+E)"            ui_key alt+e
ui_step "the remote's address"           type_text "$REMOTE"
ui_expect_new "Open from Cloud dialog: clone, enabled" "a project there" 20
ui_step "open (Enter)"                   ui_key Return
ui_expect_new "Open from Cloud: clone $REMOTE into $B" "into a folder named after the repository"
ui_expect_new "Open from Cloud: opened $B (clone): block.mitcad" "cloned, one design" 20
ui_expect_new "Opened $FILE_B" "the design opened"
ui_focus_main
ui_expect_new "Remote status: synced" "B is synced"
[ "$(in_git "$B" rev-parse HEAD)" = "$(in_git "$A" rev-parse HEAD)" ] || ui_fail "B has not A's versions"
cmp -s "$FILE_A" "$FILE_B" || ui_fail "B's design differs from A's"
echo "ok   B has A's versions and design"
ui_key ctrl+q
expect_exit "exited"

echo "--- A newer version on the remote: the notice, Sync Now"
as_other 30 "Bea's d3"
ui_start_app --open "$FILE_A" --no-native-dialogs
ui_expect_log "Opened $FILE_A" "A opened"
ui_expect_log "Remote check: ahead 0, behind 1" "the remote checked at opening"
ui_expect_log "Remote status: behind 1" "the newer version is known"
ui_expect_log "Project indicator: bracket, cloud, behind 1" "the indicator shows it"
ui_expect_log "Remote notice: A newer version of block.mitcad is on the remote, saved by Bea Other at " \
  "the notice names it"
ui_expect_log "Sync takes it. [Sync Now, Dismiss]" "Sync takes it"
ui_expect_log "Remote notice Sync Now at " "the notice's buttons placed"
ui_mark
ui_step "sync now (the notice)"          ui_click_logged "Remote notice Sync Now"
ui_expect_new "Sync: fast_forward, 0 replayed, nothing sent" "a fast-forward"
ui_expect_new "Sync: opening block.mitcad again" "the design opened again"
ui_expect_new "Opened $FILE_A" "opened"
ui_expect_new "Remote status: synced" "synced"
[ "$(file_d3 "$FILE_A")" = "[30.0]" ] || ui_fail "A's design has d3 $(file_d3 "$FILE_A")"
echo "ok   A has the other user's d3"
expect_sent "$A" "A is at the remote's version"
expect_clean "$A" "A's git status is clean"
ui_key ctrl+q
expect_exit "exited"

echo "--- Both change the design: Resolve Sync Conflicts keeps A's as a copy"
in_git "$B" pull -q --ff-only 2> /dev/null || true
as_other 40 "Bea's second d3"
MITCAD_TEST_REMOTE_CHANGE_CHECK_SECONDS=0 ui_start_app --open "$FILE_A" --set d3=35 --no-native-dialogs
ui_expect_log "Remote check: ahead 0, behind 1" "checked at opening"
ui_expect_log "Remote check: the first change since opening" "checked again at the first change"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version recorded: block.mitcad " "A's version"
ui_expect_new "Remote push failed (rejected)" "it cannot be sent: the remote has a newer one"
ui_expect_new "Remote status: ahead 1, behind 1" "one to send, one newer"
ui_expect_new "Project indicator: bracket, cloud, ahead 1, behind 1" "the indicator shows both"
ui_expect_new "You changed it too: Sync asks which to keep." "the notice says a sync will ask"
ui_mark
ui_step "sync (Ctrl+Alt+Y)"              ui_key ctrl+alt+y
ui_expect_new "Sync: replay (conflict), 0 replayed, nothing sent" "the sync stops for the file"
ui_expect_new "Resolve Sync Conflicts dialog: block.mitcad (modified; remote " "the dialog lists it"
ui_expect_new "by Bea Other; here 1 version(s))" "with the other user's version"
ui_focus_dialog '^Resolve Sync Conflicts$'
ui_step "compare (Alt+M)"                ui_key alt+m
ui_expect_new "Resolve Sync Conflicts compare block.mitcad: From the remote's version " "Compare"
ui_expect_new "d3 (Extrude1 distance): 40 mm -> 35 mm" "it shows d3"
ui_focus_dialog '^Compare block.mitcad: the remote'"'"'s and yours$'
ui_step "close the comparison (Esc)"     ui_key Escape
ui_focus_dialog '^Resolve Sync Conflicts$'
ui_step "apply the default (Enter)"      ui_key Return
ui_focus_main
ui_expect_new "Sync conflict: block.mitcad (modified): copy" "mine as a copy"
ui_expect_new "Sync: block.mitcad kept as block (conflict copy Ada Tester " "the copy named"
ui_expect_new "Sync: opening block.mitcad again" "the design opened again"
ui_expect_new "Remote status: synced" "synced"
[ "$(file_d3 "$FILE_A")" = "[40.0]" ] || ui_fail "A's file has d3 $(file_d3 "$FILE_A")"
copy=$(find "$A" -maxdepth 1 -name 'block (conflict copy Ada Tester *).mitcad')
[ -n "$copy" ] || ui_fail "no copy in A: $(ls "$A")"
[ "$(file_d3 "$copy")" = "[35.0]" ] || ui_fail "the copy has d3 $(file_d3 "$copy")"
echo "ok   the file is the other user's, the copy A's"
expect_sent "$A" "both are on the remote"
expect_clean "$A" "A's git status is clean"
[ "$(in_git "$A" rev-list --merges --count HEAD)" = 0 ] || ui_fail "a merge in A's history"
in_git "$A" for-each-ref --format='%(refname)' refs/mitcad/sync-backup/ | grep -q . ||
  ui_fail "no backup of A's history"
echo "ok   one line of history, the old one kept"

echo "--- Offline: the version waits, and goes when the remote is back"
ui_key ctrl+q
expect_exit "exited"
ui_start_app --open "$FILE_A" --set d3=45 --no-native-dialogs
ui_expect_log "Remote check: ahead 0, behind 0" "checked at opening"
mv "$REMOTE" "$REMOTE.away"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version recorded: block.mitcad " "the version recorded"
ui_expect_new "Remote push failed (not_found)" "it cannot be sent"
ui_expect_new "Remote status: remote not found, ahead 1" "the remote's state: it waits"
ui_expect_new "Project indicator: bracket, cloud, remote not found, ahead 1" "the indicator shows it waiting"
ui_expect_new "Remote push: 1 version(s) wait; trying again in 2 s" "tried again later"
mv "$REMOTE.away" "$REMOTE"
ui_expect_new "Remote push: trying again (1 version(s) waiting)" "tried again" 20
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent once the remote is back" 20
ui_expect_new "Remote status: synced" "synced" 20
expect_sent "$A" "the remote has it"

echo "--- Preferences: the git program"
ui_mark
ui_step "preferences (search)"           ui_command "Preferences"
ui_expect_new "Preferences: cloud: git " "the git program found"
ui_focus_dialog '^Preferences$'
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Preferences: git found automatically, remote checks every 10 min, each version sent on" "saved"

ui_key ctrl+q
expect_exit "exited"
ui_finish "UI sync test"

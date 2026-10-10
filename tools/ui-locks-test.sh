#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs core/ffi/src/live.rs tools/cli
# Edit locks of a Cloud project's designs (mitcad#89) with two (and three)
# instances of Mitcad on clones of one bare repository in a folder (no
# network), checked with the system's git. The lock clock runs faster
# (MITCAD_LOCK_TIME_SCALE), so nothing waits for fixed times:
#   1. Ada opens the design and takes its edit lock; Sam opens it
#      read-only with the question (Continue Read-Only), an edit is
#      refused; Sam asks (Request Edit Access, a message), Ada's Mitcad
#      writes the receipt and asks her, Release saves her change, sends it
#      and hands the lock over; Sam's window is editable with her version.
#   2. Ada asks; Sam declines with a message, which Ada's banner shows.
#   3. Sam's Mitcad is killed: Ada asks again, gets no receipt, and takes
#      the lock ("did not answer"). Then Sam asks Ada and his Mitcad is
#      killed: his request, no longer refreshed, goes stale and is removed
#      by the next Mitcad that sees it (Sam's started again), and Ada's
#      is asked no more.
#   4. Sam opens it read-only again; Ada saves a version, which Sam's
#      window shows ("Updated to Ada Tester's version"), the file left as
#      it was.
#   5. Ada in another window takes the lock over: the first window turns
#      read-only with its unsaved changes, Save as New Version records them
#      (not sent: Send each version at once is off there), Sync asks
#      before sending a file someone else holds (Cancel, then Send
#      Anyway); Save As makes a copy that opens with a lock of its own;
#      Open Read-Only opens a design without a lock.
#   6. The lock's details from the indicator's menu; Project Settings: the
#      remote accepts lock refs; edit locks turned off release the
#      session's lock.
#   7. Idle time (a project of 1 minute, the clock 30 times as fast): no
#      unsaved changes, the lock is released (Edit takes it again); with
#      unsaved changes and autosave on, a version is saved first; with
#      autosave off the lock stays, marked idle, and a request gets it at
#      once, the changes staying in the window.
#   8. A remote that refuses lock refs: no lock, and Project Settings says
#      so.
#   9. The remote out of reach: the design opens for editing, "Edit lock
#      not confirmed (offline)", and the lock is taken once it answers;
#      Release Edit Lock in the indicator's menu.
#  10. With live updates (a mosquitto the test starts; skipped without
#      one): who has the design open, in the banner; a request's receipt at
#      once (the remote is read every 2 minutes of the lock clock while
#      live); the holder's Mitcad killed, its connection's will offers the
#      lock, which is taken.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh); Qt's own file dialogs (--no-native-dialogs).
# Usage: tools/ui-locks-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
SOURCE=$(cd "$(dirname "$0")" && pwd)/cli/tests/v1_boss.mitcad

WORK=$(mktemp -d /tmp/mitcad-ui-locks.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
export MITCAD_TEST_REMOTE_RETRY_SECONDS=2
unset MITCAD_LOCK_TIME_SCALE

in_git() {
  local folder=$1
  shift
  git -C "$folder" "$@"
}

# A failure shows the end of every instance's log.
ui_fail() {
  local instance name file
  echo "FAIL: $1"
  for instance in $UI_INSTANCES; do
    name="UI_INSTANCE_${instance}_LOG"
    file=${!name:-}
    [ "$instance" = "$UI_CURRENT" ] && file=$UI_LOG
    echo "--- the log of $instance ($file):"
    tail -n 80 "$file" 2> /dev/null |
      grep -avE '^(Ribbon|Timeline|Browser|View area|View home|Body F|Layout grid|Sync [0-9]|\[New|\[Thread)' | tail -40
    ui_keep_log "$file" "$instance"
  done
  exit 1
}

# file_d3 project-file: d3 of a design ([value]).
file_d3() {
  python3 - "$1" << 'EOF'
import json, sys

doc = json.load(open(sys.argv[1]))
print([p['value'] for p in doc['parameters'] if p['name'] == 'd3'])
EOF
}

# expect_d3 file value description
expect_d3() {
  local d3
  d3=$(file_d3 "$1")
  [ "$d3" = "[$2]" ] || ui_fail "$3: d3 is $d3 in $1, expected $2"
  echo "ok   $3"
}

# A project of a design in a bare repository: make_project bare idle poll
# [broker] (the edit locks' idle time and poll interval for everyone, and
# the live updates' broker).
make_project() {
  local bare=$1 seed=$WORK/seed-$RANDOM
  git init -q --bare "$bare"
  "$CLI" project init "$seed" --author "Ada Tester <ada@example.invalid>" > "$WORK/cli.log" 2>&1 ||
    { cat "$WORK/cli.log"; ui_fail "mitcad-cli project init"; }
  cp "$SOURCE" "$seed/part.mitcad"
  python3 - "$seed/.mitcad/project.json" "$2" "$3" "${4:-}" << 'EOF'
import json, sys

path = sys.argv[1]
marker = json.load(open(path))
marker['edit_locks'] = {'enabled': True, 'idle_minutes': int(sys.argv[2]), 'poll_seconds': int(sys.argv[3])}
if sys.argv[4]:
    marker['live_updates'] = {'broker': sys.argv[4]}
with open(path, 'w') as f:
    f.write(json.dumps(marker, indent=2) + '\n')
EOF
  in_git "$seed" add -A
  in_git "$seed" commit -qm "Add part.mitcad"
  in_git "$seed" push -q "$bare" HEAD:main 2> /dev/null || ui_fail "git push to $bare"
  rm -rf "$seed"
}

# clone bare folder name email: a clone of someone's, the author in its
# configuration.
clone() {
  git clone -q "$1" "$2" 2> /dev/null || ui_fail "git clone $1"
  in_git "$2" config user.name "$3"
  in_git "$2" config user.email "$4"
}

# The lock refs of a bare repository, and the email of a lock's holder.
lock_refs() { git --git-dir="$1" for-each-ref --format='%(refname)' refs/mitcad/locks/ | wc -l; }
request_refs() { git --git-dir="$1" for-each-ref --format='%(refname)' refs/mitcad/lock-requests/ | wc -l; }
lock_holder() {
  local ref
  ref=$(git --git-dir="$1" for-each-ref --format='%(refname)' refs/mitcad/locks/ | head -1)
  [ -n "$ref" ] && git --git-dir="$1" show "$ref:lock.json" |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["owner"]["email"])'
}
expect_holder() {
  local holder
  holder=$(lock_holder "$1")
  [ "$holder" = "$2" ] || ui_fail "$3: the lock's holder is '$holder', expected '$2'"
  echo "ok   $3"
}

# start instance scale file [arguments...]: an instance of Mitcad with the
# lock clock that many times as fast, the design opened.
start() {
  local name=$1 scale=$2 file=$3
  shift 3
  ui_instance "$name"
  # The log starts afresh: what follows counts from its start.
  UI_MARK=0
  MITCAD_LOCK_TIME_SCALE=$scale ui_start_app --no-native-dialogs --open "$file" "$@"
  ui_expect_log "Opened $file" "$name: $(basename "$file") opened" > /dev/null
}

# quit instance: Ctrl+Q, Mitcad ends (its edit lock released).
quit() {
  ui_instance "$1"
  ui_key ctrl+q
  for _ in $(seq 1 100); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  wait "$UI_RUNNER" 2> /dev/null
  UI_RUNNER=""
  echo "ok   $1 quit"
}

# indicator_choose "entry": the project indicator's menu, an entry picked
# with the arrow keys (from the logged "Project indicator menu: a | b").
indicator_choose() {
  local entries index=0 found="" item
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

# banner_click button description: a click on a button of the banner under
# the toolbar, once the window has settled; once more when the click did
# not reach it (a window that turned read-only a moment ago may still be
# laid out again).
banner_click() {
  local start
  ui_sync
  start=$(wc -l < "$UI_LOG")
  ui_step "$2"                            ui_click_logged "Lock banner $1"
  for _ in $(seq 1 10); do
    tail -n +$((start + 1)) "$UI_LOG" | grep -qF "Lock banner: $1" && return 0
    sleep 0.2
  done
  echo "note: the click did not reach the banner's $1: once more"
  ui_step "$2 (again)"                    ui_click_logged "Lock banner $1"
}

# request message: Request Edit Access in the banner, with a message.
request() {
  banner_click "Request Edit Access..." "request edit access (banner)"
  ui_focus_dialog '^Request Edit Access$'
  if [ -n "$1" ]; then
    ui_step "a message"                   ui_type_text "$1"
  fi
  ui_step "send (Enter)"                  ui_key Return
  ui_focus_main
}

ui_start_display

TEAM=$WORK/team.git
make_project "$TEAM" 10 10
A=$WORK/ada/team
B=$WORK/sam/team
clone "$TEAM" "$A" "Ada Tester" ada@example.invalid
clone "$TEAM" "$B" "Sam Other" sam@example.invalid
# The lock clock 4 times as fast: polls every 2.5 s, a receipt due within
# 5 s, the idle time (10 minutes) 2.5 minutes, longer than any step here.
SCALE=4

echo "--- 1. Ada takes the edit lock; Sam opens the design read-only"
start ada $SCALE "$A/part.mitcad" --set d3=25
ui_expect_new "Edit lock taken: part.mitcad" "Ada takes the edit lock" 20
ui_expect_new "Edit lock state: editing, part.mitcad" "Ada edits"
ui_expect_new ", cloud, " "the indicator" > /dev/null
[ "$(lock_refs "$TEAM")" = 1 ] || ui_fail "the remote has $(lock_refs "$TEAM") lock refs"
expect_holder "$TEAM" ada@example.invalid "the remote's lock ref is Ada's"
start sam $SCALE "$B/part.mitcad"
ui_expect_new "Edit lock held: part.mitcad by Ada Tester (since " "held by Ada" 20
ui_expect_new "Edit lock dialog: part.mitcad is being edited by Ada Tester (since " "the question"
ui_expect_new "[Continue Read-Only, Request Edit Access...]" "with its answers"
ui_expect_new "Read-only: on" "Sam's window is read-only"
ui_expect_new "read-only (ada tester)" "the indicator: read-only (Ada)"
ui_focus_dialog '^Edit Lock$'
ui_step "continue read-only (Alt+C)"    ui_key alt+c
ui_focus_main
ui_expect_new "Edit lock dialog: Continue Read-Only" "read-only it is"
ui_expect_new "Lock banner: Read-only: Ada Tester is editing (since " "the banner"
ui_mark
ui_step "edit Extrude1 (timeline)"      ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Read-only: refused editing " "editing a feature is refused"

echo "--- 1. Sam asks, Ada releases, Sam edits Ada's version"
ui_mark
request "May I add a hole?"
ui_expect_new 'Edit lock request sent: part.mitcad to Ada Tester: "May I add a hole?"' "the request sent" 20
ui_expect_new "Lock banner: Read-only: waiting for Ada Tester's answer." "the banner waits"
ui_instance ada
ui_expect_new "Edit lock receipt: part.mitcad for Sam Other" "Ada's Mitcad writes the receipt" 20
ui_expect_new 'Edit lock request: Sam Other asks to edit part.mitcad: "May I add a hole?" [Release (Save First), Release Without Saving, Keep 15 More Minutes, Decline...]' \
  "Ada is asked" 20
ui_instance sam
ui_expect_new "Edit lock request seen by Ada Tester" "Sam sees the receipt" 20
ui_instance ada
ui_mark
ui_focus_dialog '^Edit Access Request$'
ui_step "release (Alt+R)"               ui_key alt+r
ui_focus_main
ui_expect_new "Edit lock request: Release" "Release"
ui_expect_new "Version recorded: part.mitcad " "Ada's change saved as a version" 20
ui_expect_new "Edit lock handed over: part.mitcad to Sam Other" "handed over to Sam" 30
ui_expect_new "Read-only: on" "Ada's window is read-only"
ui_expect_new "Lock banner: You handed the edit lock of part.mitcad to Sam Other. [Request Edit Access...]" \
  "Ada's banner"
expect_holder "$TEAM" sam@example.invalid "the remote's lock is Sam's"
[ "$(in_git "$A" rev-parse HEAD)" = "$(git --git-dir="$TEAM" rev-parse main)" ] || ui_fail "Ada's version was not sent"
echo "ok   Ada's version is on the remote"
ui_instance sam
ui_expect_new "Edit lock granted: part.mitcad by Ada Tester" "Sam has the edit lock" 20
ui_expect_new "Read-only: off" "Sam's window is editable"
ui_expect_new "Sync: fast_forward" "Sam's copy brought up to date" 30
ui_expect_new "Opened $B/part.mitcad" "the design opened again" 30
expect_d3 "$B/part.mitcad" 25.0 "Sam's design is Ada's version"
ui_focus_main

echo "--- 2. Ada asks, Sam declines"
ui_instance ada
ui_mark
request ""
ui_expect_new "Edit lock request sent: part.mitcad to Sam Other" "Ada's request sent" 20
ui_instance sam
ui_mark
ui_expect_new "Edit lock request: Ada Tester asks to edit part.mitcad. [Release, Keep 15 More Minutes, Decline...]" \
  "Sam is asked" 20
ui_focus_dialog '^Edit Access Request$'
ui_step "decline (Alt+D)"               ui_key alt+d
ui_focus_dialog '^Decline$'
ui_step "a message"                     ui_type_text "Not now"
ui_step "OK (Enter)"                    ui_key Return
ui_focus_main
ui_expect_new "Edit lock request of Ada Tester declined: Not now" "declined" 20
ui_instance ada
ui_expect_new "Edit lock request declined by Sam Other: Not now" "Ada learns it" 20
ui_expect_new 'Sam Other declined your request: "Not now" [Request Edit Access...]' "with Sam's message"

echo "--- 3. Sam's Mitcad killed: no receipt, Ada takes the lock"
ui_instance sam
ui_stop_app
echo "ok   Sam's Mitcad killed"
ui_instance ada
ui_mark
request ""
ui_expect_new "Edit lock request sent: part.mitcad to Sam Other" "asked again" 20
ui_expect_new "Edit lock request: Sam Other did not answer (no_receipt): taking the lock" "no receipt" 30
ui_expect_new "Edit lock taken: part.mitcad from Sam Other (no_receipt)" "Ada has the lock" 20
ui_expect_new "Read-only: off" "Ada's window is editable"
expect_holder "$TEAM" ada@example.invalid "the remote's lock is Ada's"

echo "--- 3b. Sam asks, and his Mitcad is killed: the request goes stale and is removed"
start sam $SCALE "$B/part.mitcad"
ui_expect_new "Edit lock held: part.mitcad by Ada Tester" "held by Ada" 20
ui_focus_dialog '^Edit Lock$'
ui_step "continue read-only (Alt+C)"    ui_key alt+c
ui_focus_main
ui_mark
request ""
ui_expect_new "Edit lock request sent: part.mitcad to Ada Tester" "Sam asks" 20
ui_instance ada
ui_mark
ui_expect_new "Edit lock request: Sam Other asks to edit part.mitcad." "Ada is asked" 20
ui_instance sam
ui_stop_app
echo "ok   Sam's Mitcad killed"
[ "$(request_refs "$TEAM")" = 1 ] || ui_fail "the remote has $(request_refs "$TEAM") request refs, not Sam's one"
# Sam's Mitcad started again, its lock clock 40 times as fast: the killed
# session's request, never refreshed, is stale after the idle time (10
# minutes) and two of its polls (2 × 10 s), 15.5 s here, and the new
# session's poll removes it.
start sam 40 "$B/part.mitcad"
ui_expect_new "Edit lock held: part.mitcad by Ada Tester" "held by Ada" 20
ui_focus_dialog '^Edit Lock$'
ui_step "continue read-only (Alt+C)"    ui_key alt+c
ui_focus_main
ui_expect_new "Edit lock: a stale request removed: part.mitcad by Sam Other" "the stale request removed" 40
[ "$(request_refs "$TEAM")" = 0 ] || ui_fail "a request ref is left: $(request_refs "$TEAM")"
echo "ok   no request refs left"
ui_instance ada
ui_expect_new "Edit lock request withdrawn: part.mitcad" "Ada is asked no more" 20
quit sam

echo "--- 4. Sam read-only again: Ada's saved version shows in Sam's window"
start sam $SCALE "$B/part.mitcad"
ui_expect_new "Edit lock held: part.mitcad by Ada Tester" "held by Ada" 20
ui_focus_dialog '^Edit Lock$'
ui_step "continue read-only (Alt+C)"    ui_key alt+c
ui_focus_main
ui_instance ada
ui_mark
ui_step "undo (Ctrl+Z)"                 ui_key ctrl+z
ui_step "save (Ctrl+S)"                 ui_key ctrl+s
ui_expect_new "Version recorded: part.mitcad " "Ada saves a version" 20
ui_expect_new "Remote push: sent 1 version(s) to origin/main" "sent at once" 20
expect_d3 "$A/part.mitcad" 20.0 "Ada's version has d3 20 again"
ui_instance sam
ui_expect_new "Edit lock: a newer version on the remote: checking part.mitcad" "Sam's Mitcad notices it" 30
ui_expect_new "Updated to Ada Tester's version saved at " "Sam's window shows Ada's version" 30
expect_d3 "$B/part.mitcad" 25.0 "Sam's file is as it was"
ui_expect_new "Lock banner: Read-only: Ada Tester is editing" "still read-only"
quit sam

echo "--- 5. Take Over from another window; the unsaved changes; Sync asks"
quit ada
ui_expect_new "Edit lock released: part.mitcad" "Ada's lock released on quitting"
[ "$(lock_refs "$TEAM")" = 0 ] || ui_fail "a lock ref is left: $(lock_refs "$TEAM")"
echo "ok   no lock refs left"
# Versions are not sent at once in Ada's clone: Sync sends them.
printf '{"send_at_once": false}\n' > "$A/.mitcad/local/sync.json"
start ada $SCALE "$A/part.mitcad" --set d3=33
ui_expect_new "Edit lock taken: part.mitcad" "Ada takes the lock" 20
start ada2 $SCALE "$A/part.mitcad"
ui_expect_new "Edit lock held: part.mitcad by Ada Tester, another session" "held by Ada's other window" 20
ui_expect_new "Edit lock dialog: You have this design open elsewhere (since " "the question"
ui_expect_new "[Take Over, Open Read-Only]" "Take Over or Open Read-Only"
ui_focus_dialog '^Edit Lock$'
ui_step "take over (Alt+T)"             ui_key alt+t
ui_focus_main
ui_expect_new "Edit lock taken: part.mitcad from Ada Tester (take_over)" "taken over" 20
ui_expect_new "Read-only: off" "this window edits"
ui_instance ada
ui_expect_new "Edit lock lost: part.mitcad to Ada Tester (take_over)" "the first window lost it" 30
ui_expect_new "Edit lock lost: the unsaved changes of part.mitcad stay" "its changes stay"
ui_expect_new "Read-only: on" "read-only"
ui_expect_new "You took over the edit lock of part.mitcad in another window at " "the banner says why"
ui_expect_new "[Save as Copy..., Save as New Version, Request Edit Access...]" "with Save as Copy and Save as New Version"
ui_mark
banner_click "Save as New Version" "save as new version (banner)"
ui_expect_new "Save as New Version: $A/part.mitcad (read-only)" "Save as New Version"
ui_expect_new "Version recorded: part.mitcad " "recorded" 20
expect_d3 "$A/part.mitcad" 33.0 "the changes are in the file"
ui_mark
ui_step "sync (Ctrl+Alt+Y)"             ui_key ctrl+alt+y
ui_expect_new "Sync: files someone else holds the edit lock of: part.mitcad (you, in another window): asked" \
  "Sync asks first" 20
ui_focus_dialog '^Sync$'
ui_step "cancel (Esc)"                  ui_key Escape
ui_focus_main
ui_expect_new "Sync: not sent (locked files)" "nothing sent"
[ "$(in_git "$A" rev-parse HEAD)" != "$(git --git-dir="$TEAM" rev-parse main)" ] || ui_fail "the version was sent"
ui_mark
ui_step "sync (Ctrl+Alt+Y)"             ui_key ctrl+alt+y
ui_focus_dialog '^Sync$'
ui_step "send anyway (Alt+A)"           ui_key alt+a
ui_focus_main
ui_expect_new "Sync: Send Anyway" "Send Anyway"
ui_expect_new "Sync: push, 0 replayed, sent" "sent" 30
[ "$(in_git "$A" rev-parse HEAD)" = "$(git --git-dir="$TEAM" rev-parse main)" ] || ui_fail "the version was not sent"
echo "ok   the version is on the remote"
ui_mark
ui_step "save as (Ctrl+Shift+S)"        ui_key ctrl+shift+s
ui_step "a copy"                        ui_type_path "Save As" "$A/copy.mitcad"
ui_focus_main
ui_expect_new "Saved $A/copy.mitcad" "saved as a copy" 20
ui_expect_new "Edit lock taken: copy.mitcad" "the copy has a lock of its own" 20
ui_expect_new "Read-only: off" "and is editable"
ui_mark
ui_step "open read-only (search)"       ui_command "Open Read-Only"
ui_step "the design"                    ui_type_path "Open Read-Only" "$A/part.mitcad"
ui_focus_main
ui_expect_new "Edit lock released: copy.mitcad" "the copy's lock released" 20
ui_expect_new "Edit lock: part.mitcad opened read-only, without an edit lock" "opened without a lock" 20
ui_expect_new "Lock banner: Read-only: part.mitcad was opened without an edit lock. [Edit]" "the banner"
expect_holder "$TEAM" ada@example.invalid "the other window's lock stays"
quit ada

echo "--- 6. The lock's details; Project Settings: the remote takes lock refs; edit locks off"
ui_instance ada2
indicator_choose "Edit Lock Details..."
ui_expect_new "Edit lock details: Design: part.mitcad | Edit lock: yours (this window), since " "the lock's details"
ui_expect_new "Live updates: not connected; the remote is read every 10 s" "how often the remote is read"
ui_focus_dialog '^Edit Lock Details$'
ui_step "close (Esc)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "project settings (search)"     ui_command "Project Settings"
ui_expect_new "Project Settings: edit locks accepted by the remote" "the remote accepts lock refs" 20
ui_focus_dialog '^Project Settings - team$'
ui_step "edit locks off (Alt+E)"        ui_key alt+e
ui_expect_new "Project Settings: shared settings recorded: " "a version for everyone"
ui_expect_new "Edit locks off: part.mitcad" "locking off"
ui_expect_new "Edit locks off: released part.mitcad" "the lock released" 20
ui_step "close (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Edit lock state: none, part.mitcad" "no lock"
[ "$(lock_refs "$TEAM")" = 0 ] || ui_fail "a lock ref is left: $(lock_refs "$TEAM")"
echo "ok   no lock refs left"
quit ada2

echo "--- 7. Idle time: released, saved first, or marked idle"
IDLE=$WORK/idle.git
make_project "$IDLE" 1 60
IA=$WORK/ada/idle
IB=$WORK/sam/idle
clone "$IDLE" "$IA" "Ada Tester" ada@example.invalid
clone "$IDLE" "$IB" "Sam Other" sam@example.invalid
# 30 times as fast: the idle time (1 minute) 2 s, polls every 2 s, a
# receipt due within 4 s.
FAST=30
start ada $FAST "$IA/part.mitcad"
ui_expect_new "Edit lock taken: part.mitcad" "taken" 20
ui_expect_new "Edit lock idle: part.mitcad after 1 min without activity (no unsaved changes)" "idle" 30
ui_expect_new "Edit lock released: part.mitcad" "released" 20
ui_expect_new "Lock banner: Your edit lock was released after 1 minute without activity. [Edit]" "the banner"
ui_mark
banner_click "Edit" "edit (banner)"
ui_expect_new "Edit lock taken: part.mitcad" "Edit takes it again" 20
ui_expect_new "Read-only: off" "editable"
quit ada
start ada $FAST "$IA/part.mitcad" --set d3=31
ui_expect_new "Edit lock taken: part.mitcad" "taken, with a change" 20
ui_expect_new "Edit lock idle: part.mitcad after 1 min without activity (unsaved changes)" "idle" 30
ui_expect_new "Edit lock idle: saving part.mitcad" "autosave on: saved first"
ui_expect_new "Version recorded: part.mitcad " "a version" 20
ui_expect_new "Edit lock released: part.mitcad" "released" 30
git --git-dir="$IDLE" log -1 --format=%s main | grep -q "Saved automatically before releasing the edit lock" ||
  ui_fail "the remote's latest version: $(git --git-dir="$IDLE" log -1 --format=%s main)"
echo "ok   the version is on the remote: Saved automatically before releasing the edit lock"
quit ada
# Autosave off.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[autosave]\nenabled=false\n' >> "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
start ada $FAST "$IA/part.mitcad" --set d3=32
ui_expect_new "Edit lock taken: part.mitcad" "taken, with a change" 20
ui_expect_new "Edit lock idle: part.mitcad kept, marked idle (autosave off)" "autosave off: kept, idle" 30
ui_expect_new "Edit lock state: editing, idle, part.mitcad" "the indicator: idle"
start sam $FAST "$IB/part.mitcad"
ui_expect_new "Edit lock held: part.mitcad by Ada Tester" "held by Ada" 20
ui_expect_new "who is away (idle since " "Ada is away"
ui_focus_dialog '^Edit Lock$'
ui_step "request edit access (Alt+R)"   ui_key alt+r
ui_focus_dialog '^Request Edit Access$'
ui_step "send (Enter)"                  ui_key Return
ui_focus_main
ui_expect_new "Edit lock request sent: part.mitcad to Ada Tester" "asked" 20
ui_expect_new "Edit lock granted: part.mitcad by Ada Tester" "granted at once" 30
ui_expect_new "Read-only: off" "Sam edits"
ui_instance ada
ui_expect_new "Edit lock: Sam Other asks while part.mitcad is idle: handing it over" "Ada's Mitcad hands it over" 20
ui_expect_new "Edit lock handed over: part.mitcad to Sam Other" "handed over" 20
ui_expect_new "Edit lock handed over: the unsaved changes of part.mitcad stay" "Ada's changes stay in her window"
ui_expect_new "Read-only: on" "read-only"
expect_d3 "$IA/part.mitcad" 31.0 "Ada's file has not her unsaved change"
quit sam
ui_instance ada
ui_key ctrl+q
ui_expect_new "Read-only: unsaved changes of part.mitcad asked about" "quitting asks about the changes kept"
ui_focus_dialog '^Mitcad$'
ui_key alt+d # Don't Save: Mitcad ends
for _ in $(seq 1 100); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "ada: Mitcad did not exit"
UI_RUNNER=""
echo "ok   ada quit without saving"

echo "--- 8. A remote that refuses lock refs"
REFUSE=$WORK/refuse.git
make_project "$REFUSE" 10 10
cat > "$REFUSE/hooks/pre-receive" << 'EOF'
#!/bin/sh
while read -r old new ref; do
  case "$ref" in
  refs/mitcad/*) echo "lock refs are not taken here" >&2; exit 1 ;;
  esac
done
EOF
chmod +x "$REFUSE/hooks/pre-receive"
RA=$WORK/ada/refuse
clone "$REFUSE" "$RA" "Ada Tester" ada@example.invalid
start ada $SCALE "$RA/part.mitcad"
ui_expect_new "Edit locks: part.mitcad: This remote does not accept Mitcad's lock references" "no lock with it" 20
ui_expect_new "Edit lock state: none, part.mitcad" "editable without a lock"
ui_mark
ui_step "project settings (search)"     ui_command "Project Settings"
ui_expect_new "Project Settings: edit locks: This remote does not accept Mitcad's lock references" \
  "Project Settings says so" 20
ui_focus_dialog '^Project Settings - refuse$'
ui_step "close (Esc)"                   ui_key Escape
ui_focus_main
quit ada

echo "--- 9. The remote out of reach: editable, the lock taken once it answers"
AWAY=$WORK/away.git
make_project "$AWAY" 10 10
WA=$WORK/ada/away
clone "$AWAY" "$WA" "Ada Tester" ada@example.invalid
mv "$AWAY" "$AWAY.gone"
start ada $SCALE "$WA/part.mitcad"
ui_expect_new "Edit lock not confirmed (" "the lock is not confirmed" 20
ui_expect_new "edit lock not confirmed (offline)" "the indicator says so"
ui_expect_new "Edit lock state: not confirmed, part.mitcad" "editable meanwhile"
mv "$AWAY.gone" "$AWAY"
ui_expect_new "Edit lock taken: part.mitcad" "taken once the remote answers" 30
expect_holder "$AWAY" ada@example.invalid "the remote's lock is Ada's"
indicator_choose "Release Edit Lock"
ui_expect_new "Edit lock released: part.mitcad" "Release Edit Lock" 20
ui_expect_new "Lock banner: You released the edit lock of part.mitcad. [Edit]" "read-only, with Edit"
[ "$(lock_refs "$AWAY")" = 0 ] || ui_fail "a lock ref is left: $(lock_refs "$AWAY")"
echo "ok   no lock refs left"
quit ada

echo "--- 10. Live updates: who has the design open, the receipt at once, the last will"
MOSQUITTO=${MITCAD_MOSQUITTO:-$(command -v mosquitto || ls /usr/sbin/mosquitto /usr/local/sbin/mosquitto 2> /dev/null | head -1)}
if [ -z "$MOSQUITTO" ] || [ ! -x "$MOSQUITTO" ]; then
  echo "note: mosquitto is not installed: no live updates here (tools/dev-env/install-packages.sh)"
else
  PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
  printf 'allow_anonymous true\npersistence false\nlistener %s 127.0.0.1\n' "$PORT" > "$WORK/broker.conf"
  "$MOSQUITTO" -c "$WORK/broker.conf" > "$WORK/broker.out" 2>&1 &
  BROKER=$!
  trap 'ui_cleanup; kill "$BROKER" 2> /dev/null; rm -rf "$WORK"' EXIT
  for _ in $(seq 1 100); do
    (exec 3<> "/dev/tcp/127.0.0.1/$PORT") 2> /dev/null && break
    sleep 0.05
  done
  LIVE=$WORK/live.git
  make_project "$LIVE" 10 10 "mqtt://127.0.0.1:$PORT"
  LA=$WORK/ada/live
  LB=$WORK/sam/live
  clone "$LIVE" "$LA" "Ada Tester" ada@example.invalid
  clone "$LIVE" "$LB" "Sam Other" sam@example.invalid
  start ada $SCALE "$LA/part.mitcad"
  ui_expect_new "Live trust question: live through mqtt://127.0.0.1:$PORT [Connect, Not Now]" "the trust question" 20
  ui_focus_dialog '^Live Updates$'
  ui_step "connect (Enter)"             ui_key Return
  ui_focus_main
  ui_expect_new "Live: live subscribed" "Ada is live" 20
  ui_expect_new "Edit lock taken: part.mitcad" "Ada takes the edit lock" 20
  start sam $SCALE "$LB/part.mitcad"
  ui_expect_new "Live: live subscribed" "Sam is live (the broker trusted already)" 20
  ui_expect_new "Edit lock held: part.mitcad by Ada Tester" "held by Ada" 20
  ui_focus_dialog '^Edit Lock$'
  ui_step "continue read-only (Alt+C)"  ui_key alt+c
  ui_focus_main
  ui_expect_new "Edit lock: also open: Ada Tester (editing, since " "Sam sees who has it open" 20
  ui_expect_new "Also open: Ada Tester (editing, since " "in the banner" 20
  ui_mark
  request ""
  ui_expect_new "Edit lock request sent: part.mitcad to Ada Tester" "asked" 20
  ui_instance ada
  # Live, the remote is read every 2 minutes of the lock clock (30 s here):
  # the request's live message makes Ada's Mitcad read it at once (its log
  # is this phase's alone).
  ui_expect_new "Live event: live: request request " "the request's live message" 10
  ui_expect_new "Edit lock receipt: part.mitcad for Sam Other" "the receipt at once" 10
  ui_expect_new "Edit lock: also open: Sam Other (read-only, since " "Ada sees Sam has it open" 10
  ui_instance sam
  ui_expect_new "Edit lock request seen by Ada Tester" "Sam sees the receipt" 10
  ui_mark
  ui_instance ada
  ui_stop_app
  echo "ok   Ada's Mitcad killed: its connection's will"
  ui_instance sam
  ui_expect_new "Edit lock dialog: Ada Tester's Mitcad lost its connection at " "Sam is asked whether to take the lock" 20
  ui_focus_dialog '^Edit Lock$'
  ui_step "take the edit lock (Alt+T)"  ui_key alt+t
  ui_focus_main
  ui_expect_new "Edit lock taken: part.mitcad from Ada Tester (take_over)" "taken" 20
  ui_expect_new "Read-only: off" "Sam edits"
  expect_holder "$LIVE" sam@example.invalid "the remote's lock is Sam's"
  quit sam
  kill "$BROKER" 2> /dev/null
  wait "$BROKER" 2> /dev/null
fi

ui_finish "UI locks test"

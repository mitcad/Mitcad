#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowEditInPlace.cpp app/files/LockController.cpp app/files/MainWindowVersions.cpp
# check-all sources: app/files/RemoteController.cpp app/files/MainWindowLocks.cpp
# check-all sources: core/vcs core/model/src/document/edit_in_place.rs core/model-api/src/edit_in_place.rs tools/cli
# check-all sources: tools/ui-scripts/edit-in-place-project.mitcad-ui tools/ui-scripts/edit-in-place-part.json tools/ui-scripts/edit-in-place-holder.json
# check-all sources: tools/ui-scripts/edit-in-place-part-newer.json
# Edit in Place in a Cloud project with edit locks (mitcad#2353, #2389,
# #2397): a linked design edited in place is brought up to date when the
# mode takes its edit lock and holds that lock for the whole mode, a save in
# the mode records a version of its own, a design open in its own tab lends
# its lock, another user's request for the lock is answered from the mode,
# and the mode does not start, said in words, while another user holds that
# lock. Two instances of MitCad on clones of one bare repository in a folder
# (no network), checked with the system's git:
#   1. The remote gets a newer version of part.mitcad after the clones. Ada
#      opens holder.mitcad (driven by
#      tools/ui-scripts/edit-in-place-project.mitcad-ui), starts Edit in
#      Place on the linked part.mitcad (its lock taken, the newer version
#      synced in before it is read), edits and saves it: a version of
#      part.mitcad, sent to the remote, whose lock of the part is Ada's
#      while the mode lasts; Finish releases it, no lock of the part left
#      there.
#   2. Ada opens the part in its own tab, which takes its lock, and edits
#      it in place: the tab's lock is held and active on the remote during
#      the mode and still held after Finish; closing the tab releases it.
#   3. Ada edits the part in place again; Sam opens it (read-only) and asks
#      for its lock (Request Edit Access); Ada's MitCad shows the request
#      and Finish and Release hands the lock over: Sam's MitCad gets it.
#   4. While Sam holds it, Ada's next Edit in Place does not start
#      ("part.mitcad is being edited by Sam Other ... it cannot be edited in
#      place"), the file left as it was, no version recorded, Sam's lock
#      kept.
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-edit-in-place-project-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-edit-in-place-project.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset MITCAD_LOCK_TIME_SCALE MITCAD_TEST_INPUT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

# A failure shows the end of both instances' logs.
ui_fail() {
  local instance name file
  echo "FAIL: $1"
  for instance in $UI_INSTANCES; do
    name="UI_INSTANCE_${instance}_LOG"
    file=${!name:-}
    [ "$instance" = "$UI_CURRENT" ] && file=$UI_LOG
    echo "--- the log of $instance ($file):"
    grep -aE 'Edit in Place|Edit lock|Version|TestDriver|Message band: error|FAIL' "$file" 2> /dev/null | tail -40
    ui_keep_log "$file" "$instance"
  done
  exit 1
}

# wait_for log pattern seconds: a line of a log (grep -E), else a failure.
wait_for() {
  local _
  for _ in $(seq 1 $(($3 * 10))); do
    grep -aqE "$2" "$1" && { echo "ok   $2"; return 0; }
    sleep 0.1
  done
  ui_fail "'$2' is not in $1 after $3 s"
}

# wait_count log pattern count seconds: as wait_for, the line there at
# least `count` times.
wait_count() {
  local _
  for _ in $(seq 1 $(($4 * 10))); do
    [ "$(grep -acE "$2" "$1")" -ge "$3" ] && { echo "ok   $2 ($3 times)"; return 0; }
    sleep 0.1
  done
  ui_fail "'$2' is not $3 times in $1 after $4 s"
}

# settle log pattern seconds: as wait_for, but only a note when the line
# does not come.
settle() {
  local _
  for _ in $(seq 1 $(($3 * 10))); do
    grep -aqE "$2" "$1" && { echo "ok   $2"; return 0; }
    sleep 0.1
  done
  echo "note: '$2' is not in $1 after $3 s"
}

# The project: part.mitcad and holder.mitcad (which links it) in a bare
# repository, edit locks on.
team=$work/team.git
seed=$work/seed
git init -q --bare "$team"
"$cli" project init "$seed" --author "Ada Tester <ada@example.invalid>" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli project init"; }
"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$seed/part.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "the part"; }
# The holder made in the part's folder: the link is part.mitcad.
(cd "$seed" && "$cli" run "$here/ui-scripts/edit-in-place-holder.json" --save "$seed/holder.mitcad") \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "the holder"; }
python3 - "$seed/.mitcad/project.json" << 'EOF'
import json, sys

path = sys.argv[1]
marker = json.load(open(path))
marker['edit_locks'] = {'enabled': True, 'idle_minutes': 10, 'poll_seconds': 10}
with open(path, 'w') as f:
    f.write(json.dumps(marker, indent=2) + '\n')
EOF
git -C "$seed" add -A
git -C "$seed" commit -qm "Add the holder and its part"
git -C "$seed" push -q "$team" HEAD:main 2> /dev/null || ui_fail "git push to $team"
ada=$work/ada/team
sam=$work/sam/team
for who in ada sam; do
  folder=$work/$who/team
  git clone -q "$team" "$folder" 2> /dev/null || ui_fail "git clone for $who"
done
git -C "$ada" config user.name "Ada Tester"
git -C "$ada" config user.email ada@example.invalid
git -C "$sam" config user.name "Sam Other"
git -C "$sam" config user.email sam@example.invalid
part_versions() { git -C "$ada" log --format=%H -- part.mitcad | wc -l; }
[ "$(part_versions)" = 1 ] || ui_fail "part.mitcad has $(part_versions) versions at the start"
# A newer version of the part on the remote, which neither clone has: Ada's
# Edit in Place brings it in when it takes the part's lock (mitcad#2397).
"$cli" run "$here/ui-scripts/edit-in-place-part-newer.json" --save "$seed/part.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "the newer part"; }
git -C "$seed" commit -qam "A second cube in the part"
git -C "$seed" push -q "$team" HEAD:main 2> /dev/null || ui_fail "git push of the newer part to $team"

# The holders of the remote's locks: "path owner" lines.
lock_holders() {
  local ref
  for ref in $(git --git-dir="$team" for-each-ref --format='%(refname)' refs/mitcad/locks/); do
    git --git-dir="$team" show "$ref:lock.json" |
      python3 -c 'import json, sys; lock = json.load(sys.stdin); print(lock["path"], lock["owner"]["email"])'
  done
}

# The states of the remote's locks: "path state" lines (active, idle).
lock_states() {
  local ref
  for ref in $(git --git-dir="$team" for-each-ref --format='%(refname)' refs/mitcad/locks/); do
    git --git-dir="$team" show "$ref:lock.json" |
      python3 -c 'import json, sys; lock = json.load(sys.stdin); print(lock["path"], lock.get("state"))'
  done
}

# holds line seconds: the remote's locks have that "path owner" line (an
# extended regex), read again until it does: a lock ref may be rewritten
# while its holder's MitCad syncs or refreshes it. The output is read
# whole before it is matched (no grep -q in a pipe under pipefail).
holds() {
  local _ holders
  for _ in $(seq 1 $(($2 * 2))); do
    holders=$(lock_holders 2> /dev/null)
    grep -qE "$1" <<< "$holders" && return 0
    sleep 0.5
  done
  return 1
}

ui_start_display

echo "--- 1. Ada edits the part in place: brought up to date, its lock held for the mode, a version, the lock released"
ui_instance ada
MITCAD_TEST_INPUT=$here/ui-scripts/edit-in-place-project.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --open "$ada/holder.mitcad"
wait_for "$UI_LOG" '^Edit lock for Edit in Place: part[.]mitcad taken$' 180
wait_for "$UI_LOG" '^Edit in Place: bringing part[.]mitcad up to date [(]sync[)]$' 60
wait_for "$UI_LOG" '^Edit in Place: part[.]mitcad brought up to date$' 120
wait_for "$UI_LOG" '^Edit in Place: saved .*/part[.]mitcad$' 120
wait_for "$UI_LOG" '^Version recorded: part[.]mitcad ' 10
# The first version, the remote's newer one, and Ada's on top of it.
[ "$(part_versions)" = 3 ] || ui_fail "part.mitcad has $(part_versions) versions after Save in place, not 3"
echo "ok   a version of part.mitcad after the remote's: $(git -C "$ada" log -1 --format=%s -- part.mitcad)"
# Still in the mode (the driver waits for ada-held): the lock is Ada's.
holds '^part[.]mitcad ada@example[.]invalid$' 30 ||
  ui_fail "Ada does not hold part.mitcad during Edit in Place: $(lock_holders 2>&1)"
echo "ok   Ada holds the lock of part.mitcad during Edit in Place"
: > "$work/ada-held"
wait_for "$UI_LOG" '^Edit lock for Edit in Place: part[.]mitcad released$' 120
holders=$(lock_holders)
grep -qE '^part[.]mitcad ' <<< "$holders" && ui_fail "a lock of part.mitcad is left after Finish: $holders"
echo "ok   no lock of part.mitcad left on the remote after Finish"
cp "$ada/part.mitcad" "$work/part-saved.mitcad"

echo "--- 2. The part in its own tab lends its lock to Edit in Place: held and active, still the tab's after it"
wait_for "$UI_LOG" '^Edit lock: part[.]mitcad held while it is edited in place$' 180
holds '^part[.]mitcad ada@example[.]invalid$' 30 ||
  ui_fail "the part's tab does not hold part.mitcad during Edit in Place: $(lock_holders 2>&1)"
states=$(lock_states)
grep -qE '^part[.]mitcad active$' <<< "$states" ||
  ui_fail "the lock of part.mitcad is not active during Edit in Place: $states"
echo "ok   the tab's lock of part.mitcad is held and active during Edit in Place"
: > "$work/ada-lent"
wait_for "$UI_LOG" '^Edit lock: part[.]mitcad back from Edit in Place$' 120
holds '^part[.]mitcad ada@example[.]invalid$' 30 ||
  ui_fail "the part's tab lost part.mitcad after Edit in Place: $(lock_holders 2>&1)"
echo "ok   the lock of part.mitcad is still the tab's after Finish"
: > "$work/ada-tab-kept"
wait_for "$UI_LOG" '^Edit lock released: .*part[.]mitcad$' 120

echo "--- 3. Sam asks for the lock Ada's Edit in Place holds: Finish and Release hands it over"
# Ada's second Edit in Place without a tab took the lock.
wait_count "$UI_LOG" '^Edit lock for Edit in Place: part[.]mitcad taken$' 2 180
ui_instance sam
UI_MARK=0
ui_start_app --no-native-dialogs --open "$sam/part.mitcad"
wait_for "$UI_LOG" "^Opened $sam/part[.]mitcad" 30
wait_for "$UI_LOG" '^Edit lock held: part[.]mitcad by Ada Tester' 30
ui_focus_dialog '^Edit Lock$'
ui_step "request edit access (Alt+R)" ui_key alt+r
wait_for "$UI_LOG" '^Request Edit Access dialog: part[.]mitcad, Ada Tester$' 30
ui_focus_dialog '^Request Edit Access$'
ui_step "send the request (Enter)" ui_key Return
wait_for "$UI_LOG" '^Edit lock request sent: part[.]mitcad to Ada Tester' 30
# Ada's MitCad shows the request; her driver chooses Finish and Release.
wait_for "$UI_LOG" '^Edit lock granted: part[.]mitcad by Ada Tester$' 240
wait_for "$UI_LOG" '^Read-only: off' 30
# Ada's version is newer than Sam's copy: his MitCad syncs it right after
# taking the lock, and opens the part again. Settled first.
settle "$UI_LOG" '^Version status: team, main, v3$' 60
holds '^part[.]mitcad sam@example[.]invalid$' 30 ||
  ui_fail "Sam does not hold part.mitcad: $(lock_holders 2>&1)"
echo "ok   Sam holds the lock of part.mitcad, handed over from Ada's Edit in Place"
: > "$work/sam-holds"

echo "--- 4. Sam holds the part's lock: Ada's Edit in Place does not start"
ui_instance ada
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "Ada's driver timed out"
wait "$UI_RUNNER" || ui_fail "Ada's MitCad ended with an error"
UI_RUNNER=""
grep -aq 'TestDriver: quit' "$UI_LOG" || ui_fail "Ada's driver did not finish"
echo "ok   Ada's Edit in Place refused in words while Sam holds the lock"
cmp -s "$work/part-saved.mitcad" "$ada/part.mitcad" || ui_fail "the refused Edit in Place wrote part.mitcad"
echo "ok   part.mitcad as it was saved"
[ "$(part_versions)" = 3 ] ||
  ui_fail "part.mitcad has $(part_versions) versions after the refused Edit in Place, not 3"
echo "ok   no version recorded"
holds '^part[.]mitcad sam@example[.]invalid$' 30 || ui_fail "Sam lost part.mitcad: $(lock_holders 2>&1)"
echo "ok   Sam still holds the lock of part.mitcad"
ui_instance sam
ui_stop_app

ui_finish 'UI Edit in Place in a project'

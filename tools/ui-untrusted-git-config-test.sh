#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs tools/cli
# A received project folder whose own git configuration names a program
# (mitcad#920): a Cloud project with edit locks on, cloned from a bare
# repository in a folder (no network), whose .git/config sets
# remote.origin.uploadpack to a script that writes a marker file (git runs
# upload-pack itself for a remote in a folder). Opening its design takes
# the edit lock and checks the remote at once; MitCad runs no git there:
# the marker is not written, the design opens without an edit lock, and
# the project indicator shows the refusal. Plain git, run on the folder
# afterwards, does write the marker: the setup would have run the script.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-untrusted-git-config-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
SOURCE=$(cd "$(dirname "$0")" && pwd)/cli/tests/v1_boss.mitcad

WORK=$(mktemp -d /tmp/mitcad-ui-untrusted.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset GIT_SSH GIT_SSH_COMMAND GIT_ASKPASS
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
export MITCAD_TEST_REMOTE_RETRY_SECONDS=2
unset MITCAD_LOCK_TIME_SCALE

# The project: a design in a bare repository, edit locks on.
BARE=$WORK/team.git
SEED=$WORK/seed
git init -q --bare "$BARE"
"$CLI" project init "$SEED" --author "Ada Tester <ada@example.invalid>" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli project init"; }
cp "$SOURCE" "$SEED/part.mitcad"
python3 - "$SEED/.mitcad/project.json" << 'EOF'
import json, sys

path = sys.argv[1]
marker = json.load(open(path))
marker['edit_locks'] = {'enabled': True, 'idle_minutes': 10, 'poll_seconds': 10}
with open(path, 'w') as f:
    f.write(json.dumps(marker, indent=2) + '\n')
EOF
git -C "$SEED" add -A
git -C "$SEED" commit -qm "Add part.mitcad"
git -C "$SEED" push -q "$BARE" HEAD:main 2> /dev/null || ui_fail "git push to $BARE"

# The received folder: a clone whose configuration names the script.
RECEIVED=$WORK/received/team
MARKER=$WORK/marker
git clone -q "$BARE" "$RECEIVED" 2> /dev/null || ui_fail "git clone $BARE"
printf 'echo ran > "%s"\nexit 1\n' "$MARKER" > "$WORK/received/program.sh"
git -C "$RECEIVED" config remote.origin.uploadpack "sh \"$WORK/received/program.sh\""

ui_start_display

echo "--- The design of the received project opens"
ui_start_app --no-native-dialogs --open "$RECEIVED/part.mitcad"
ui_expect_log "Opened $RECEIVED/part.mitcad" "part.mitcad opened"
ui_expect_log "Edit locks: part.mitcad: MitCad does not run git in this project" \
  "no edit lock: git is not run"
ui_expect_log "remote.origin.uploadpack" "the refusal names the key"
ui_expect_log "Edit lock state: none, part.mitcad" "editable without a lock"
ui_expect_log "Remote status: untrusted git settings" "the remote's state"
ui_expect_log ", cloud, untrusted git settings" "the project indicator shows the refusal"
# The checks on open (the lock's, the remote's) have had their time.
sleep 3
[ -e "$MARKER" ] && ui_fail "a program of the project's git configuration ran"
echo "ok   no program of the project's git configuration ran"

ui_key ctrl+q
for _ in $(seq 1 400); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.05
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "Mitcad did not exit"
wait "$UI_RUNNER" 2> /dev/null
UI_RUNNER=""
echo "ok   quit"
[ -e "$MARKER" ] && ui_fail "a program of the project's git configuration ran on quit"

echo "--- Plain git on the same folder"
GIT_TERMINAL_PROMPT=0 git -C "$RECEIVED" fetch -q origin > /dev/null 2>&1 < /dev/null
[ -e "$MARKER" ] || ui_fail "plain git did not run the script: the test's setup is wrong"
echo "ok   plain git runs it"
echo "PASS"

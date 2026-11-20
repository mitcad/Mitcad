#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: core/vcs tools/cli tools/ui-scripts/slow-remote.mitcad-ui tools/ui-scripts/slow-ssh.sh
# A Cloud project with a slow server (mitcad#1895) on macOS, through the
# test driver (tools/ui-scripts/slow-remote.mitcad-ui, the same script as
# the Linux and Windows tests'): its server a local bare repository behind
# an ssh that waits 5 s per connection (tools/ui-scripts/slow-ssh.sh), edit
# locks on, a loose design opened; New Folder in Save to Project, Save in
# the new folder and Save again are recorded at once and sent in the
# background, the UI thread never stalls ("UI stall: " lines, logged with
# MITCAD_TEST_UI_STALL_MS), and the server has every version at the end.
# Usage: tools/ui-macos-slow-remote-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-slow-remote}"
UI_TIMEOUT=600
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files/loose" "$files/server"
# git's configuration of the test's own: the versions' author.
export GIT_CONFIG_GLOBAL=$files/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset GIT_SSH GIT_ASKPASS
printf '[user]\n\tname = Mac Tester\n\temail = mac@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Mac Tester <mac@example.invalid>"
export MITCAD_PROJECTS_DIR=$files/projects
shop=$MITCAD_PROJECTS_DIR/Shop
server=$files/server
export GIT_SSH_COMMAND="sh \"$here/ui-scripts/slow-ssh.sh\"" GIT_SSH_VARIANT=ssh SLOW_SSH_ROOT=$server
setup() {
  mkdir -p "$MITCAD_PROJECTS_DIR" &&
    "$2" project init "$shop" --author "$author" &&
    printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
      '  "length": 10, "width": 20, "height": 5, "operation": "new_body"}}]' > "$files/block.json" &&
    "$2" run "$files/block.json" --save "$shop/plate.mitcad" &&
    "$2" version save "$shop/plate.mitcad" -m "The plate" --author "$author" &&
    "$2" run "$files/block.json" --save "$files/loose/gear.mitcad" &&
    git init -q --bare "$server/shop.git" &&
    git -C "$shop" remote add origin ssh://test/shop.git &&
    SLOW_SSH_SECONDS=0 git -C "$shop" push -q -u origin main
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The Projects panel shown at the start, as for a user who turned it on in
# either layout (the Floating top bar has its own choice, mitcad#1990).
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[projects]\npanel=true\npanelTopBar=true\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"

# Every connection waits 5 s; a gap of the UI thread over 4 s is logged.
SLOW_SSH_SECONDS=5 MITCAD_TEST_UI_STALL_MS=4000 MITCAD_TEST_REMOTE_RETRY_SECONDS=2 \
  MITCAD_TEST_WORK=$files MITCAD_TEST_SHOP=$shop MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/slow-remote.mitcad-ui \
  ui_run_app slow-remote --ui=qt --no-recovery --no-mcp --open "$files/loose/gear.mitcad"
ui_expect_log 'UI stall watch: gaps over 4000 ms' "the UI thread's stalls are watched"
ui_expect_log 'TestDriver: done' 'New Folder, Save to Project and Save did not wait for the slow server'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'

# From the first step on, the UI thread never waited for the server.
stall=$(sed -n '/^Project picker Save to Project: /,$p' "$UI_LOG_FILE" | grep -m1 '^UI stall: ')
if [ -z "$stall" ]; then
  ui_pass 'the window kept responding'
else
  ui_fail "the window stopped responding while the server was slow: $stall"
fi

# The server has the folder and both versions of the design.
sent=$(git --git-dir "$server/shop.git" log --format=%s main 2>&1)
if printf '%s\n' "$sent" | grep -qxF "Add folder Parts"; then
  ui_pass "the folder's version is on the server"
else
  ui_fail "the folder's version is not on the server: $sent"
fi
versions=$(git --git-dir "$server/shop.git" log --format=%H main -- Parts/gear.mitcad 2>/dev/null | wc -l | tr -d ' ')
if [ "$versions" -ge 2 ]; then
  ui_pass 'the server has both versions of the design'
else
  ui_fail "the server has $versions version(s) of Parts/gear.mitcad, not 2"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt slow-remote'

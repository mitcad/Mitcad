#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: core/vcs core/ffi/src/vcs.rs tools/cli tools/ui-scripts/slow-remote.mitcad-ui tools/ui-scripts/slow-ssh.sh
# A Cloud project with a slow server (mitcad#1895) through the real UI, by
# the shared driver script tools/ui-scripts/slow-remote.mitcad-ui (the
# Windows counterpart is tools/ui-windows-slow-remote-test.ps1, the macOS
# one tools/ui-macos-slow-remote-test.sh). The project's server is a local
# bare repository behind an ssh that waits 5 s per connection
# (tools/ui-scripts/slow-ssh.sh); edit locks are on; a loose design is open:
#   1. Save to Project's New Folder: the folder is a version at once, sent
#      in the background.
#   2. Save in the new folder: saved and recorded at once, the window takes
#      a change while the version, the edit lock and the folder's version go
#      to the server one after another.
#   3. Save again: recorded at once, sent in the background, the state in
#      words (sending, then up to date with the last sync's time).
# The UI thread never stalls meanwhile ("UI stall: " lines, logged with
# MITCAD_TEST_UI_STALL_MS), and the server has every version at the end.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-slow-remote-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-slow-remote.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
unset GIT_SSH GIT_ASKPASS
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
export MITCAD_PROJECTS_DIR=$work/projects
shop=$MITCAD_PROJECTS_DIR/Shop
loose=$work/loose
server=$work/server

cli() {
  "$CLI" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; }
}

echo "--- A Cloud project with plate.mitcad, its server slow; a loose design"
mkdir -p "$MITCAD_PROJECTS_DIR" "$loose" "$server"
cli project init "$shop" --author "$author"
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
  '  "length": 10, "width": 20, "height": 5, "operation": "new_body"}}]' > "$work/block.json"
cli run "$work/block.json" --save "$shop/plate.mitcad"
cli version save "$shop/plate.mitcad" -m "The plate" --author "$author"
cli run "$work/block.json" --save "$loose/gear.mitcad"
# The server: a bare repository reached through an ssh that waits.
git init -q --bare "$server/shop.git" || ui_fail "git init --bare"
export GIT_SSH_COMMAND="sh \"$here/ui-scripts/slow-ssh.sh\"" GIT_SSH_VARIANT=ssh SLOW_SSH_ROOT=$server
git -C "$shop" remote add origin ssh://test/shop.git || ui_fail "git remote add"
SLOW_SSH_SECONDS=0 git -C "$shop" push -q -u origin main || ui_fail "git push to the test server"
echo "ok   the project is shared"

# The panel shown at the start, as for a user who turned it on.
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

# Every connection waits 5 s; a gap of the UI thread over 4 s is logged.
export SLOW_SSH_SECONDS=5 MITCAD_TEST_UI_STALL_MS=4000 MITCAD_TEST_REMOTE_RETRY_SECONDS=2
export MITCAD_TEST_WORK=$work MITCAD_TEST_SHOP=$shop MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/slow-remote.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$loose/gear.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the driver timed out"
wait "$UI_RUNNER" || { tail -n 60 "$UI_LOG" >&2; ui_fail "the driver failed"; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the steps"
ui_expect_log 'UI stall watch: gaps over 4000 ms' "the UI thread's stalls are watched"

# From the first step on, the UI thread never waited for the server.
stall=$(sed -n '/^Project picker Save to Project: /,$p' "$UI_LOG" | grep -m1 '^UI stall: ')
[ -z "$stall" ] || ui_fail "the window stopped responding while the server was slow: $stall"
echo "ok   the window kept responding"

# The server has the folder and both versions of the design.
sent=$(git --git-dir "$server/shop.git" log --format=%s main 2>&1)
printf '%s\n' "$sent" | grep -qxF "Add folder Parts" || ui_fail "the folder's version is not on the server: $sent"
versions=$(git --git-dir "$server/shop.git" log --format=%H main -- Parts/gear.mitcad 2>/dev/null | wc -l)
[ "$versions" -ge 2 ] || ui_fail "the server has $versions version(s) of Parts/gear.mitcad, not 2: $sent"
echo "ok   the server has every version"

ui_finish "UI slow remote test"

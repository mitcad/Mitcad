#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/AppSettings.cpp core/vcs core/ffi/src/vcs.rs
# check-all sources: core/ffi/src/projects.rs core/ffi/src/remote.rs tools/cli
# Names typed in the Projects panel (mitcad#1268) through the real UI, by the
# shared driver scripts tools/ui-scripts/projects-naming*.mitcad-ui (the
# Windows counterpart is tools/ui-windows-projects-naming-test.ps1). A Cloud
# project of the projects folder (a bare repository in a folder stands in
# for the cloud, no network) with Design1 and Design2 recorded and
# Design3.mitcad only on disk; a git of the test's own takes some seconds
# for a project's fetch, so that a remote fetch runs while a name is typed:
#   1. Floating: New Design proposes Design4, also while a fetch runs; a
#      design written after the rows were read (Design4, by this script) is
#      not proposed (Design5); a name that is there already keeps the editor
#      with the message below the row and no sheet, and a free name then
#      makes the design; with the Projects card collapsed, New Design
#      expands it, the editor has the focus, a name and Enter make that
#      design, and the card collapses again.
#   2. Docked: the hidden dock is shown for New Design, the proposed name
#      is taken with Enter, and the dock is hidden again.
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-projects-naming-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
real_git=$(command -v git)

mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-projects-naming.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export HOME=$work/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$work/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
author="Ada Tester <ada@example.invalid>"
# The projects folder: the panel lists its projects.
export MITCAD_PROJECTS_DIR=$work/projects
shop=$MITCAD_PROJECTS_DIR/Shop

cli() {
  "$CLI" "$@" >> "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli $*"; }
}

echo "--- A Cloud project: Design1 and Design2 recorded, Design3 only on disk"
mkdir -p "$work/remotes" "$MITCAD_PROJECTS_DIR"
git init -q --bare "$work/remotes/shop.git"
cli project create "$shop" --author "$author" --url "$work/remotes/shop.git"
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
  '  "length": 10, "width": 20, "height": 5, "operation": "new_body"}}]' > "$work/block.json"
cli run "$work/block.json" --save "$shop/Design1.mitcad"
cli run "$work/block.json" --save "$shop/Design2.mitcad"
cli version save "$shop/Design1.mitcad" -m "Design1" --author "$author"
cli version save "$shop/Design2.mitcad" -m "Design2" --author "$author"
cli push "$shop"
# Not recorded: only the folder on disk has it.
cli run "$work/block.json" --save "$shop/Design3.mitcad"
echo "ok   the project is made"

# git for MitCad: a project's fetch (fetch --prune --progress, the remote
# check's) takes some seconds; everything else as git does it.
cat > "$work/slow-git" << SLOW
#!/bin/sh
case " \$* " in
  *" fetch --prune --progress "*) sleep 6 ;;
esac
exec "$real_git" "\$@"
SLOW
chmod +x "$work/slow-git"
export MITCAD_GIT=$work/slow-git

# The panel shown at the start, as for a user who turned it on (mitcad#1020:
# hidden until then); New Design replaces the open design (mitcad#1172).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\n[documents]\nopenInCurrentWindow=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

export MITCAD_TEST_WORK=$work MITCAD_TEST_SHOP=$shop MITCAD_TEST_STEP_MS=100
ui_start_display

# run_script name script chrome: one start of MitCad with a driver script,
# which ends it; `written` is set once Design4.mitcad was written.
written=0
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$2
  ui_start_app --ui=qt --no-mcp "--chrome=$3" --open "$shop/Design1.mitcad"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    # A design written after the rows were read: once the second New Design
    # of the Floating steps is cancelled (the script waits for the file).
    if [ "$written" = 0 ] && [ "$(grep -c '^Projects panel: new design cancelled$' "$UI_LOG")" -ge 2 ]; then
      cp "$shop/Design1.mitcad" "$shop/Design4.mitcad"
      written=1
      echo "ok   Design4.mitcad written after the rows were read"
    fi
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: the driver timed out"
  wait "$UI_RUNNER" || { tail -n 60 "$UI_LOG" >&2; ui_fail "$1: the driver failed"; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$1: the steps"
}

echo "--- 1. Floating: the proposals, a refused name, the collapsed card"
run_script "floating" projects-naming.mitcad-ui floating
[ "$written" = 1 ] || ui_fail "Design4.mitcad was not written"
for name in Plate Gear; do
  [ -f "$shop/$name.mitcad" ] || ui_fail "$name.mitcad is not in the project"
done
echo "ok   Plate and Gear made"

echo "--- 2. Docked: the hidden dock shown for the name and hidden again"
run_script "docked" projects-naming-docked.mitcad-ui docked
[ -f "$shop/Design5.mitcad" ] || ui_fail "Design5.mitcad is not in the project"
echo "ok   Design5 made"

ui_finish "UI projects naming test"

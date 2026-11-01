#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp
# check-all sources: app/framework/AppSettings.cpp app/icons core/vcs core/ffi/src/vcs.rs core/ffi/src/projects.rs
# check-all sources: tools/cli
# The Projects panel's rows (mitcad#529) through the real UI. A Cloud
# project in the projects folder (a bare repository in a folder stands in
# for the cloud, no network) with a design whose name is 55 characters long
# and whose edit lock the command line holds as the same author (the panel
# shows it as Editing elsewhere), and a short design; the project's row is
# expanded with a click and Right after each start:
#   1. Floating: the long name is elided in the middle in the narrow card,
#      its start and end kept; its marks are the version and a lock; the
#      tooltip has the full name, the path and "Editing elsewhere". The
#      project's mark is a cloud with a check.
#   2. The card's edge dragged wider: the full name shows, and the width is
#      remembered: the card has it again after a restart.
#   3. Docked: the dock's edge dragged wider is remembered too, and
#      restored after a restart.
# The rows' log lines: "Projects text <key>: "<name as shown>" <marks>;
# tooltip: <lines joined by ' / '>" (app/COMMANDS.md, Logs that UI tests
# read).
#
# git's configuration and HOME are the test's own. Runs headless on Xvfb
# (see ui-test-lib.sh).
# Usage: tools/ui-projects-rows-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}

WORK=$(mktemp -d /tmp/mitcad-ui-projects-rows.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
AUTHOR="Ada Tester <ada@example.invalid>"
# The projects folder: the panel lists its projects.
export MITCAD_PROJECTS_DIR=$WORK/projects
SHOP=$MITCAD_PROJECTS_DIR/Shop
LONG=mounting-bracket-for-the-left-side-motor-assembly-rev-b

cli() {
  "$CLI" "$@" >> "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli $*"; }
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

# The long design's row as logged last, and the name it shows.
text_line() { grep -F "Projects text Shop/$LONG.mitcad: " "$UI_LOG" | tail -1; }
shown_name() {
  local line rest
  line=$(text_line)
  rest=${line#*: \"}
  echo "${rest%%\"*}"
}
# wait_text "text" description: until the long design's row line has it.
wait_text() {
  for _ in $(seq 1 100); do
    text_line | grep -qF -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: '$1' not in '$(text_line)'"
}

# expand_shop: the project's row selected and expanded (Right), its
# designs shown.
expand_shop() {
  ui_expect_log "Projects row Shop at " "the project's row is shown" 20
  ui_step "select the project"           ui_click_logged "Projects row Shop"
  ui_step "expand it (Right)"            ui_key Right
  ui_expect_log "Projects row Shop/$LONG.mitcad at " "its designs are shown"
}

# expect_elided description: the name shown is elided in the middle, both
# of its ends kept.
expect_elided() {
  local shown
  shown=$(shown_name)
  [ "$shown" != "$LONG" ] || ui_fail "$1: the whole name shows: $shown"
  case "$shown" in
    *…*) ;;
    *) ui_fail "$1: no ellipsis in '$shown'" ;;
  esac
  [ "${shown:0:4}" = "${LONG:0:4}" ] || ui_fail "$1: the start is not kept: '$shown'"
  [ "${shown: -4}" = "${LONG: -4}" ] || ui_fail "$1: the end is not kept: '$shown'"
  echo "ok   $1: '$shown'"
}

echo "--- A Cloud project with a long-named design edited elsewhere"
mkdir -p "$WORK/remotes" "$MITCAD_PROJECTS_DIR"
git init -q --bare "$WORK/remotes/shop.git"
cli project create "$SHOP" --author "$AUTHOR" --url "$WORK/remotes/shop.git"
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
  '  "length": 10, "width": 60, "height": 5, "operation": "new_body"}}]' > "$WORK/block.json"
cli run "$WORK/block.json" --save "$SHOP/$LONG.mitcad"
cli run "$WORK/block.json" --save "$SHOP/plate.mitcad"
cli version save "$SHOP/$LONG.mitcad" -m "The bracket" --author "$AUTHOR"
cli version save "$SHOP/plate.mitcad" -m "The plate" --author "$AUTHOR"
cli push "$SHOP"
# The command line's session holds the long design's lock: the same author
# in another session, Editing elsewhere for the app; its status brings the
# lock refs' local copies up to date.
cli lock take "$SHOP/$LONG.mitcad" --author "$AUTHOR"
cli lock status "$SHOP"
echo "ok   the project is made"

# The panel shown at the start, as for a user who turned it on (mitcad#1020:
# hidden until then).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

ui_start_display

echo "--- 1. Floating: the long name elided in the middle, a lock as its mark"
ui_start_app --chrome=floating
expand_shop
wait_text "v1 (lock); tooltip: " "its marks: the version and a lock"
wait_text "; tooltip: $LONG / $SHOP/$LONG.mitcad / v1 · Editing elsewhere / " \
  "the tooltip has the full name, the path and Editing elsewhere"
expect_elided "the name is elided in the narrow card"
ui_expect_log "Projects text Shop/plate.mitcad: \"plate\" v1; tooltip: plate / " "the short name shows whole"
for _ in $(seq 1 100); do
  grep -F "Projects text Shop: " "$UI_LOG" | tail -1 | grep -qF "(cloud-synced); tooltip: " && break
  sleep 0.2
done
grep -F "Projects text Shop: " "$UI_LOG" | tail -1 | grep -qF "(cloud-synced); tooltip: " ||
  ui_fail "the project's mark: $(grep -F "Projects text Shop: " "$UI_LOG" | tail -1)"
echo "ok   the project's mark is a cloud with a check"

echo "--- 2. The card's edge dragged wider: the whole name, remembered"
ui_expect_log "Projects edge at " "the card's edge is logged"
ui_mark
ui_step "drag the card's edge to the right" ui_drag_from "Projects edge" 380 0
ui_expect_new "Projects panel: card width " "the width is remembered"
width=$(grep -o "Projects panel: card width [0-9]* remembered" "$UI_LOG" | tail -1 | cut -d" " -f5)
[ -n "$width" ] && [ "$width" -ge 500 ] || ui_fail "the card is ${width:-?} px wide after the drag"
echo "ok   the card is $width px wide"
wait_text "Projects text Shop/$LONG.mitcad: \"$LONG\" v1 (lock)" "the whole name shows"
ui_key ctrl+q
expect_exit "exited"

ui_start_app --chrome=floating
UI_MARK=0 # a new log
ui_expect_log "Projects panel: card width $width (remembered)" "the width at the next start" 20
expand_shop
for _ in $(seq 1 100); do
  grep -qE "Floating cards: .*projectsDock [0-9]+,[0-9]+ ${width}x" "$UI_LOG" && break
  sleep 0.2
done
grep -qE "Floating cards: .*projectsDock [0-9]+,[0-9]+ ${width}x" "$UI_LOG" ||
  ui_fail "the card is not $width px wide: $(grep -o "projectsDock [0-9,x ]*" "$UI_LOG" | tail -1)"
echo "ok   the card has its width again"
wait_text "Projects text Shop/$LONG.mitcad: \"$LONG\" " "the whole name shows after the restart"
ui_key ctrl+q
expect_exit "exited"

echo "--- 3. Docked: the dock's edge dragged wider, restored at the next start"
ui_start_app
UI_MARK=0 # a new log
expand_shop
ui_expect_log "Projects edge at " "the dock's edge is logged" 20
before=$(grep -o "^Projects width [0-9]*" "$UI_LOG" | tail -1 | cut -d" " -f3)
ui_mark
ui_step "drag the dock's edge to the right" ui_drag_from "Projects edge" 300 0
ui_expect_new "Projects panel: dock width " "the dock's width is remembered"
dock=$(grep -o "Projects panel: dock width [0-9]* remembered" "$UI_LOG" | tail -1 | cut -d" " -f5)
[ -n "$dock" ] && [ "$dock" -ge $((before + 200)) ] || ui_fail "the dock is ${dock:-?} px wide (was $before)"
echo "ok   the dock is $dock px wide (was $before)"
ui_key ctrl+q
expect_exit "exited"

ui_start_app
UI_MARK=0 # a new log
ui_expect_log "Projects panel: dock width $dock restored" "restored at the next start" 20
restored=""
for _ in $(seq 1 100); do
  restored=$(grep -o "^Projects width [0-9]*" "$UI_LOG" | tail -1 | cut -d" " -f3)
  [ -n "$restored" ] && [ "$restored" -ge $((dock - 10)) ] && break
  sleep 0.2
done
[ -n "$restored" ] && [ "$restored" -ge $((dock - 10)) ] || ui_fail "the dock is ${restored:-?} px wide, not $dock"
echo "ok   the dock is $restored px wide again"
ui_key ctrl+q
expect_exit "exited"

ui_finish "UI projects rows test"

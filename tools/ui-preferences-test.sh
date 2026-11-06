#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/files/RemotePreferences.cpp app/files/RemotePreferences.hpp
# check-all sources: app/files/Print3d.cpp core/vcs/src/remote/cli.rs
# check-all sources: app/view/NavigationPreferences.hpp app/framework/PreferencesPageLayout.hpp
# Preferences on a low screen (mitcad#690), with git and without it, on a
# test screen 768 pixels high:
#   1. git found on PATH: the Cloud page names it.
#   2. MITCAD_GIT naming a file that is not there: the Cloud page says so.
#   3. No git on PATH (git not installed): the Cloud page's long message,
#      which once made the window wider than the screen, wraps.
# Each time Preferences opens on its Cloud page: the size it asks for (its
# largest page's) and its minimum size fit a 1366 x 768 screen with a title
# bar and a taskbar, it is resizable, its frame lies within the screen's
# available area, and the page list is as wide as its entries; on its
# General page (Home in the page list) the minimum is that page's, and it
# fits too. Once, Preferences on its Navigation page and on its Cloud page
# (mitcad#1394), each made as low as it goes, then 160 pixels taller: the
# page fills the window's height (its last line ends within
# the bottom margin of the area the window gives it), and its list (the
# bindings table, the known brokers) takes the height the page has to
# spare, at least its 138 or 110 pixels in the lowest window; pictures of
# both heights (navigation-tall.png, navigation-min.png, cloud-tall.png,
# cloud-min.png) in preferences-shots of MITCAD_UI_LOG_DIR.
# tools/ui-windows-preferences-test.ps1 does the same in the Windows VM.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preferences-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

UI_SCREEN_HEIGHT=768
WORK=$(mktemp -d /tmp/mitcad-ui-preferences.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
unset MITCAD_GIT

# since "sed expression": the last line logged since the mark that the
# expression prints.
since() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n "$1" | tail -1; }

# fits what width height: within 1366 x 690, what a 1366 x 768 screen shows
# below a title bar and above a taskbar (mitcad#25).
fits() {
  [ "${2:-0}" -gt 0 ] && [ "$2" -le 1366 ] && [ "${3:-0}" -gt 0 ] && [ "$3" -le 690 ] ||
    ui_fail "Preferences: $1 ${2:-?} x ${3:-?}, more than a 1366 x 768 screen shows"
  echo "ok   Preferences: $1 $2 x $3 fits a 1366 x 768 screen"
}

# expect_cloud "regex" description: the Cloud page's line about the git
# program ("Preferences: cloud: <text>") since the mark matches.
expect_cloud() {
  local text
  for _ in $(seq 1 30); do
    text=$(since 's/^Preferences: cloud: //p')
    if [ -n "$text" ]; then
      grep -qE -- "^$1" <<< "$text" || ui_fail "$2: the Cloud page says '$text'"
      echo "ok   $2: $text"
      return
    fi
    sleep 0.2
  done
  ui_fail "$2: the Cloud page's git line is not in the log"
}

# preferences_fit "Cloud page regex" description: Preferences opened on its
# Cloud page, the Cloud page's text about git, the window's sizes and
# frame, then its General page, then closed.
preferences_fit() {
  local width height x y screen_width screen_height screen_x screen_y state list entries
  ui_mark
  ui_step "Preferences: Cloud (search)"      ui_command "Preferences: Cloud"
  ui_expect_new "Preferences opened: Cloud, " "Preferences on its Cloud page"
  expect_cloud "$1" "$2"
  ui_expect_new "Preferences page list " "Preferences shown"
  read -r width _ height <<< "$(since 's/^Preferences opened: Cloud, \([0-9]* x [0-9]*\)$/\1/p')"
  fits "the size asked for" "$width" "$height"
  read -r width height state <<< "$(since 's/^Preferences minimum \([0-9]*\) x \([0-9]*\), \([a-z]*\)$/\1 \2 \3/p')"
  fits "the minimum size" "$width" "$height"
  [ "$state" = resizable ] || ui_fail "Preferences cannot be resized (${state:-not logged})"
  echo "ok   Preferences can be resized"
  read -r width height x y screen_width screen_height screen_x screen_y <<< "$(since \
    's/^Preferences frame \([0-9]*\) x \([0-9]*\) at \(-*[0-9]*\),\(-*[0-9]*\) on a screen of \([0-9]*\) x \([0-9]*\) at \(-*[0-9]*\),\(-*[0-9]*\)$/\1 \2 \3 \4 \5 \6 \7 \8/p')"
  [ -n "${screen_y:-}" ] || ui_fail "Preferences' frame not logged"
  [ "$screen_height" -le 768 ] || ui_fail "the test screen is $screen_width x $screen_height, not 768 high"
  [ "$x" -ge "$screen_x" ] && [ "$y" -ge "$screen_y" ] &&
    [ $((x + width)) -le $((screen_x + screen_width)) ] && [ $((y + height)) -le $((screen_y + screen_height)) ] ||
    ui_fail "Preferences' frame $width x $height at $x,$y is not within the screen's $screen_width x $screen_height at $screen_x,$screen_y"
  [ "$width" -le 1366 ] || ui_fail "Preferences' frame is $width wide, more than a 1366 x 768 screen"
  echo "ok   Preferences' frame $width x $height at $x,$y lies within the screen ($screen_width x $screen_height)"
  read -r list entries <<< "$(since 's/^Preferences page list \([0-9]*\) wide, its entries \([0-9]*\)$/\1 \2/p')"
  [ "${entries:-0}" -gt 0 ] && [ "${list:-0}" -ge "$entries" ] ||
    ui_fail "the page list is ${list:-?} wide, its entries need ${entries:-?}"
  echo "ok   the page list ($list) shows its entries whole ($entries)"
  ui_focus_dialog '^Preferences$'
  ui_mark
  ui_step "the General page (Home)"          ui_key Home
  ui_expect_new "Preferences page: General, minimum " "Preferences on its General page"
  read -r width _ height <<< "$(since 's/^Preferences page: General, minimum \([0-9]* x [0-9]*\)$/\1/p')"
  fits "the General page's minimum size" "$width" "$height"
  ui_step "close it (Esc)"                   ui_key Escape
  ui_focus_main
}

# page_layout "window" "line": the page's last layout line since the mark,
# "<line> N high, its last line G above the bottom of its area A high"
# (PAGE_LIST, the list's height; PAGE_GAP; PAGE_AREA), its last line within
# that area's bottom margin.
page_layout() {
  read -r PAGE_LIST PAGE_GAP PAGE_AREA <<< "$(since \
    "s/^$2 \([0-9]*\) high, its last line \(-*[0-9]*\) above the bottom of its area \([0-9]*\) high$/\1 \2 \3/p")"
  [ -n "${PAGE_AREA:-}" ] || ui_fail "'$2' in the $1 window is not logged"
  [ "$PAGE_GAP" -ge 0 ] && [ "$PAGE_GAP" -le 32 ] ||
    ui_fail "$2: in the $1 window the page ends $PAGE_GAP pixels above the bottom of its $PAGE_AREA pixels"
  echo "ok   $2: in the $1 window the page ends $PAGE_GAP pixels above the bottom of its $PAGE_AREA pixels, the list $PAGE_LIST high"
}

# page_shot shot "window": the picture the app saved of the window at the
# page area's height PAGE_AREA (MITCAD_TEST_PREFERENCES_SHOTS), kept as
# <shot>-<window>.png for the ticket.
page_shot() {
  [ -s "$SHOTS/$1-$PAGE_AREA.png" ] || ui_fail "no picture $SHOTS/$1-$PAGE_AREA.png"
  cp "$SHOTS/$1-$PAGE_AREA.png" "$SHOTS/$1-$2.png"
  echo "ok   picture $SHOTS/$1-$2.png"
}

# page_fills Page "line" shot least: Preferences opened on the page, made
# as low as it goes (its minimum), then 160 pixels taller (mitcad#1394),
# then closed: the page's list ("line") takes the height the page has to
# spare, at least `least` pixels in the lowest window, and the page fills
# the area the window gives it; pictures <shot>-min.png and <shot>-tall.png.
page_fills() {
  local width low dialog low_list low_area
  ui_mark
  ui_step "Preferences: $1 (search)"         ui_command "Preferences: $1"
  ui_expect_new "Preferences opened: $1, " "Preferences on its $1 page"
  ui_expect_new "Preferences minimum " "Preferences shown"
  ui_expect_new "$2 " "the $1 page laid out"
  read -r width <<< "$(since 's/^Preferences frame \([0-9]*\) x [0-9]* at .*$/\1/p')"
  read -r low <<< "$(since 's/^Preferences minimum [0-9]* x \([0-9]*\), [a-z]*$/\1/p')"
  [ -n "${width:-}" ] && [ -n "${low:-}" ] || ui_fail "Preferences' frame or minimum size is not logged"
  ui_focus_dialog '^Preferences$'
  dialog=$(xdotool search --onlyvisible --name '^Preferences$' 2> /dev/null | head -1)
  [ -n "$dialog" ] || ui_fail "no Preferences window"
  # The page logs a new line only when the height changed (not when
  # Preferences opened at its minimum): the last one since it opened is the
  # lowest window's, once the sync has waited for the page's watched timer.
  ui_step "Preferences as low as it goes"    xdotool windowsize "$dialog" $((width * UI_SCALE)) $((low * UI_SCALE))
  ui_sync
  page_layout lowest "$2"
  page_shot "$3" min
  low_list=$PAGE_LIST
  low_area=$PAGE_AREA
  ui_mark
  ui_step "Preferences 160 pixels taller"    \
    xdotool windowsize "$dialog" $((width * UI_SCALE)) $(((low + 160) * UI_SCALE))
  ui_expect_new "$2 " "the $1 page laid out in the taller window"
  ui_sync
  page_layout taller "$2"
  page_shot "$3" tall
  [ "$PAGE_AREA" -gt "$low_area" ] ||
    ui_fail "the $1 page's area is $PAGE_AREA pixels high in the taller window, $low_area in the lowest"
  [ "$low_list" -ge "$4" ] || ui_fail "$2 is $low_list pixels high in the lowest window, less than $4"
  [ "$PAGE_LIST" -gt "$low_list" ] && [ $((PAGE_LIST - low_list)) -ge $((PAGE_AREA - low_area - 2)) ] ||
    ui_fail "$2 grew from $low_list to $PAGE_LIST pixels while its page grew from $low_area to $PAGE_AREA"
  echo "ok   $2 takes the page's spare height: $low_list pixels lowest, $PAGE_LIST in the taller window"
  ui_step "close it (Esc)"                   ui_key Escape
  ui_focus_main
}

ui_start_display

echo "--- 1. git found on PATH"
command -v git > /dev/null || ui_fail "git is not installed on this machine"
# The pictures of the pages that fill the window's height (mitcad#1394).
SHOTS=${MITCAD_UI_LOG_DIR:-$WORK}/preferences-shots
mkdir -p "$SHOTS"
export MITCAD_TEST_PREFERENCES_SHOTS=$SHOTS
ui_start_app
unset MITCAD_TEST_PREFERENCES_SHOTS
preferences_fit "git [0-9][^ ]* at " "the Cloud page names the git program found"
page_fills Navigation "Navigation page: bindings table" navigation 138
page_fills Cloud "Cloud page: known brokers list" cloud 110
ui_stop_app

echo "--- 2. MITCAD_GIT naming a file that is not there"
export MITCAD_GIT=$WORK/no-such-git
ui_start_app
unset MITCAD_GIT
preferences_fit "the git program set for Mitcad \(MITCAD_GIT\) is unavailable: " "the Cloud page says it is not there"
ui_stop_app

echo "--- 3. No git on PATH: git not installed"
# Mitcad started with a PATH of only the folders without a git program.
NO_GIT_PATH=""
IFS=: read -r -a PATH_FOLDERS <<< "$PATH"
for folder in "${PATH_FOLDERS[@]}"; do
  [ -n "$folder" ] && [ ! -e "$folder/git" ] && NO_GIT_PATH=${NO_GIT_PATH:+$NO_GIT_PATH:}$folder
done
cat > "$WORK/mitcad-without-git" << EOF
#!/bin/sh
PATH='$NO_GIT_PATH' exec '$UI_APP' "\$@"
EOF
chmod +x "$WORK/mitcad-without-git"
APP=$UI_APP
UI_APP=$WORK/mitcad-without-git
ui_start_app
UI_APP=$APP
preferences_fit "git is not installed: " "the Cloud page says git is not installed"
ui_stop_app

ui_finish "UI preferences test"

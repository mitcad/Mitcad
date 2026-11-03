#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccess.hpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/QuickAccessGroup.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/files/FileCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/MainWindow.cpp app/view/ViewController.cpp tools/cli
# The quick-access group (mitcad#1090) through the real UI, in the
# Floating layout (after the workspace tabs in the title row) and the
# Docked one (after the tabs in the toolbar's tab row), on a saved plate that
# mitcad-cli makes:
#   1. Floating, fresh settings: the group shows the default commands this
#      build has (New, Open, Save, Print, Design Properties, Preferences;
#      "not in this build" names those it lacks), each a button in sight.
#      Save is disabled for the unchanged saved design, enabled after an
#      edit (a fillet on the top face), disabled again once saved
#      (Ctrl+S). View > Look At checked in the group's menu (its ⋮) shows
#      a seventh button, before Preferences in the menus' order, and is
#      kept in the settings (quickAccess/commands). In a narrow window the
#      group folds into its menu before the design's name would be cut,
#      and unfolds in a wide one again. Preferences, found by the command
#      search ("quick access", mitcad#1189), Display › Show quick-access
#      toolbar off hides the group at once: the title row's part is gone;
#      on shows it again with the same seven buttons. Hide Quick-Access
#      Toolbar at the end of the group's menu hides it, the choice of
#      commands kept in the settings.
#   2. Docked: the group hidden at the start (the setting read back);
#      Preferences shows it again with the choice read back (Look At
#      still there); Reset to Default in the group's menu (a right-click on
#      the group) brings back the default set; Save disabled, enabled
#      after an edit, disabled once saved; Look At checked again: the
#      seventh button; Preferences hides the group in this layout too.
# The Windows counterpart is tools/ui-windows-quick-access-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-quick-access-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-quick-access.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
# A 200 x 200 x 10 mm plate centred on the origin: Body1 (F2.b0).
PLATE=$WORK/plate.mitcad
cat > "$WORK/plate.json" << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
JSON
"$CLI" run "$WORK/plate.json" --save "$PLATE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
DEFAULTS=(file.new_design file.open file.save file.design_properties file.print tools.preferences)

# group_line: the group's last "Quick access: " line, without its prefix.
group_line() { grep '^Quick access: ' "$UI_LOG" | tail -1 | sed 's/^Quick access: //'; }

# expect_group description extra...: the last "Quick access: " line (waited
# for) shows the default commands this build has, in order, with the extra
# ids inserted before tools.preferences (the menus' order: File, Edit, View,
# Tools), and each of them has a button in sight. SHOWN holds their count.
expect_group() {
  local description=$1 line="" shown missing expected=() id
  shift
  for _ in $(seq 1 25); do
    line=$(group_line)
    [ -n "$line" ] && break
    sleep 0.2
  done
  shown=${line%% (not in this build: *}
  missing=""
  [[ $line == *" (not in this build: "* ]] && missing=${line#* (not in this build: } && missing=${missing%)}
  for id in "${DEFAULTS[@]}"; do
    [ "$id" = tools.preferences ] && expected+=("$@")
    case ", $missing, " in
      *", $id, "*) ;;
      *) expected+=("$id") ;;
    esac
  done
  local want
  want=$(printf '%s, ' "${expected[@]}")
  want=${want%, }
  [ "$shown" = "$want" ] || ui_fail "$description: the group shows '$shown', expected '$want' (line '$line')"
  SHOWN=${#expected[@]}
  # Each a button in sight (logged after the group was made).
  ui_sync
  for id in "${expected[@]}"; do
    grep -E "^Quick access $id (at |out of sight)" "$UI_LOG" | tail -1 | grep -q " at " ||
      ui_fail "$description: no button of $id in sight"
  done
  echo "ok   $description: $SHOWN buttons ($shown)${missing:+; not in this build: $missing}"
}

# edit: a fillet on the plate's top face (F2:end).
edit() {
  ui_mark
  ui_step "fit (F6)"                       ui_key F6
  ui_step "fillet (F)"                     ui_key f
  ui_expect_new "Pick places:" "Fillet logs where its picks are"
  ui_step "pick the top face"              ui_click_pick "face F2.b0/F2:end("
  ui_expect_new "Fillet Edges: 1 face" "the top face picked"
  ui_step "confirm (Enter)"                ui_key Return
  ui_expect_new "Added fillet on 4 edge(s)" "fillet on the face's 4 edges"
}

# save_steps layout file: Save disabled for the unchanged saved design,
# enabled after an edit, disabled again once saved.
save_steps() {
  ui_save_state disabled "$1: Save disabled for the unchanged saved design"
  edit
  ui_save_state enabled "$1: Save enabled after the edit"
  ui_mark
  ui_step "save (Ctrl+S)"                  ui_key ctrl+s
  ui_expect_new "Saved $2" "$1: saved"
  ui_save_state disabled "$1: Save disabled again once saved"
}

# add_look_at layout: View > Look At checked in the group's menu (opened by
# a left click on its ⋮, or with "right" a right click on the group's
# capsule): a seventh button, kept in the settings.
add_look_at() {
  local before=$SHOWN
  open_menu "${2:-}"
  ui_mark
  ui_menu_choose "View" "Look At"
  ui_expect_new "Quick access added view.look_at" "$1: Look At checked"
  ui_expect_new "Quick access: " "$1: the group made again"
  expect_group "$1: Look At added" view.look_at
  [ "$SHOWN" = $((before + 1)) ] || ui_fail "$1: $SHOWN buttons, expected $((before + 1))"
  grep -q "view.look_at" "$CONF" || ui_fail "$1: Look At is not in the settings ($CONF)"
  echo "ok   $1: a button more, kept in the settings"
}

# open_menu [right]: the group's menu, by its ⋮ (or a right click on the
# capsule, left of the ⋮), its entries logged.
open_menu() {
  local at x y
  ui_mark
  if [ "${1:-}" = right ]; then
    at=$(ui_logged_at "Quick access menu")
    read -r x y <<< "$at"
    ui_step "right-click the group"        xdotool mousemove $((x - 12 * UI_SCALE)) "$y" click 3
  else
    ui_step "the group's menu (its dots)"  ui_click_logged "Quick access menu"
  fi
  ui_expect_new "Context menu: " "the group's menu opened"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu: " | tail -1 |
    grep -q "File | Edit | View | Tools | Reset to Default | Hide Quick-Access Toolbar$" ||
    ui_fail "the group's menu: $(grep '^Context menu: ' "$UI_LOG" | tail -1)"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu View: " | tail -1 | grep -q "Look At" ||
    ui_fail "the group's View submenu has no Look At"
}

# toggle_preference layout shown|hidden: Preferences found by the command
# search ("quick access" is a phrase of the Display page's settings,
# mitcad#1189), Show quick-access toolbar (Alt+Q) toggled, OK: the group
# shown or hidden at once.
toggle_preference() {
  ui_mark
  ui_step "Preferences (search: quick access)" ui_command "quick access"
  ui_expect_new "Command search chose tools.preferences.display" "$1: the search finds the setting's page"
  ui_expect_new "Preferences opened: Display, " "$1: Preferences on its Display page"
  ui_focus_dialog '^Preferences$'
  ui_step "Show quick-access toolbar (Alt+Q)" ui_key alt+q
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: quick-access toolbar $2" "$1: the preference saved ($2)"
  ui_expect_new "Quick access toolbar $2" "$1: the group $2 at once"
}

# title_part with|without: the last "Floating cards" line, waited for, has
# the title row's quick-access part (titleQuickAccess) or has not; the
# tabs' part is there either way.
title_part() {
  local line=""
  for _ in $(seq 1 25); do
    line=$(grep -o "^Floating cards: .*" "$UI_LOG" | tail -1)
    if [ "$1" = with ] && grep -qF "; titleQuickAccess " <<< "$line"; then break; fi
    if [ "$1" = without ] && ! grep -qF "; titleQuickAccess " <<< "$line"; then break; fi
    sleep 0.2
  done
  grep -qF "titleTabs " <<< "$line" || ui_fail "no title row tabs among the Floating cards: '$line'"
  if [ "$1" = with ]; then
    grep -qF "; titleQuickAccess " <<< "$line" || ui_fail "the title row has no quick-access part: '$line'"
    echo "ok   the title row's quick-access part is there"
  else
    grep -qF "; titleQuickAccess " <<< "$line" && ui_fail "the title row still has its quick-access part: '$line'"
    echo "ok   the title row's quick-access part is gone"
  fi
}

# expect_hidden_setting layout: the settings say hidden, and the choice of
# commands (with Look At) is kept.
expect_hidden_setting() {
  ui_sync
  grep -q "^visible=false" "$CONF" || ui_fail "$1: the group is not hidden in the settings ($CONF)"
  grep -q "view.look_at" "$CONF" || ui_fail "$1: the choice of commands was not kept ($CONF)"
  echo "ok   $1: hidden in the settings, the choice of commands kept"
}

ui_start_display

echo "--- 1. Floating: the default group, Save, Look At, folding"
cp "$PLATE" "$WORK/floating.mitcad"
ui_start_app --chrome=floating --no-native-dialogs --open "$WORK/floating.mitcad"
ui_expect_log "Opened $WORK/floating.mitcad" "opened"
expect_group "floating: the default group"
# In the title row, right of the tabs.
ui_expect_log "titleQuickAccess " "floating: the group is a part of the title row"
save_steps floating "$WORK/floating.mitcad"
add_look_at floating
ui_mark
ui_step "a narrow window"                xdotool windowsize "$UI_WINDOW" 760 800
ui_expect_new "Quick access folded" "floating: folded in a narrow window"
ui_sync
grep "^Quick access file.save " "$UI_LOG" | tail -1 | grep -q "out of sight" ||
  ui_fail "floating: Save's button still in sight when folded"
ui_mark
ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1280 800
ui_expect_new "Quick access unfolded" "floating: unfolded in a wide window"
title_part with
toggle_preference floating hidden
title_part without
expect_hidden_setting "floating, from Preferences"
toggle_preference floating shown
ui_expect_new "Quick access: " "floating: the group made again"
expect_group "floating: shown again with the same buttons" view.look_at
title_part with
open_menu
ui_mark
ui_menu_choose "Hide Quick-Access Toolbar"
ui_expect_new "Hide Quick-Access Toolbar chosen" "floating: Hide Quick-Access Toolbar chosen"
ui_expect_new "Quick access toolbar hidden" "floating: the group hidden from its menu"
title_part without
expect_hidden_setting "floating, from the group's menu"
ui_stop_app

echo "--- 2. Docked: hidden at the start, shown, Reset to Default, Save, Look At, hidden"
cp "$PLATE" "$WORK/docked.mitcad"
ui_start_app --chrome=docked --no-native-dialogs --open "$WORK/docked.mitcad"
ui_expect_log "Opened $WORK/docked.mitcad" "opened"
ui_expect_log "Quick access toolbar hidden" "docked: the group hidden at the start"
ui_sync
grep -q "^Quick access menu at " "$UI_LOG" && ui_fail "docked: the hidden group's menu is in sight"
toggle_preference docked shown
expect_group "docked: the choice read back from the settings" view.look_at
open_menu right
ui_mark
ui_menu_choose "Reset to Default"
ui_expect_new "Quick access reset to the default" "docked: Reset to Default"
ui_expect_new "Quick access: " "docked: the group made again"
expect_group "docked: the default group"
grep -q "view.look_at" "$CONF" && ui_fail "docked: Look At still in the settings"
save_steps docked "$WORK/docked.mitcad"
add_look_at docked right
toggle_preference docked hidden
expect_hidden_setting "docked, from Preferences"
ui_stop_app

ui_finish "Quick-access group"

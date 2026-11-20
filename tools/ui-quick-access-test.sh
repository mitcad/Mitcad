#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccess.hpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/QuickAccessGroup.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/files/FileCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/MainWindow.cpp app/view/ViewController.cpp tools/cli
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandSearch.cpp
# The quick-access group (mitcad#1090) through the real UI, in the
# Floating layout and the Docked one, both with the top bar of Windows and
# Linux (mitcad#2082: in the title row at the free part's right end, before
# the indicators, where the title box was: mitcad#1989), on a saved plate
# that mitcad-cli makes:
#   1. Floating, fresh settings: the top bar's group has no commands
#      (mitcad#1990: the bar has its own File menu, Save and
#      Preferences) and is not shown, its ⋮ neither: the title row has no
#      quick-access part. Save (the bar's own) is disabled for the
#      unchanged saved design, enabled after an edit (a fillet on the top
#      face), disabled again once saved (Ctrl+S). Add to Quick Access, last
#      in the menu of Measure's row in the SOLID > INSPECT menu, adds
#      Measure: the group shows with its button and ⋮, kept in the
#      settings (quickAccess/commands). View > Look At checked in the
#      group's menu (its ⋮, which lists Measure as a check too) adds a
#      button before Measure (the menus' commands first). In a narrow
#      window the group folds into its menu, and unfolds in a wide one
#      again; the title row has no title box and the group's right end
#      lies just left of the next part. Preferences, found by the command
#      search ("quick access", mitcad#1189), Display › Show quick-access
#      toolbar off hides the group at once; on shows it again with the same
#      buttons. Hide Quick-Access Toolbar at the end of the group's menu
#      hides it, the choice of commands kept in the settings. Shown again,
#      Remove from Quick Access in Measure's row menu takes Measure out,
#      and Look At unchecked in the group's menu leaves it empty: the group
#      is hidden again.
#   2. Docked: the same top bar: the saved empty choice, the group hidden
#      and no ⋮ in sight, no quick-access group anywhere else (not in the
#      toolbar's tab row); Save disabled, enabled after an edit, disabled
#      once saved; Add to Quick Access in Measure's row menu: the group in
#      the title row ("Docked top bar: ... titleQuickAccess ...") at the
#      free part's right end; Look At checked (a right-click on the group):
#      a button more; Preferences hides the group.
#   3. Floating again: the group hidden at the start (the setting read
#      back); Preferences shows it with the choice saved in Docked: the
#      title row's part is there.
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
# The top bar has no default commands in either layout (mitcad#1990,
# mitcad#2082).
DEFAULTS=()

# group_line: the group's last "Quick access: " line, without its prefix.
group_line() { grep '^Quick access: ' "$UI_LOG" | tail -1 | sed 's/^Quick access: //'; }

# expect_group description extra...: the last "Quick access: " line (waited
# for) shows the default commands this build has (DEFAULTS), in order, with
# the extra ids inserted before tools.preferences (the menus' order: File,
# Edit, View, Tools), or after them without it, and each of them has a
# button in sight. SHOWN holds their count.
expect_group() {
  local description=$1 line="" shown missing expected=() id inserted=""
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
    [ "$id" = tools.preferences ] && expected+=("$@") && inserted=1
    case ", $missing, " in
      *", $id, "*) ;;
      *) expected+=("$id") ;;
    esac
  done
  [ -n "$inserted" ] || expected+=("$@")
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

# expect_empty description: the top bar's group without commands
# (mitcad#1990): "Quick access: none", hidden, the title row without its
# part.
expect_empty() {
  ui_expect_new "Quick access: none" "$1: no commands"
  ui_expect_new "Quick access group hidden: no commands" "$1: the empty group hidden"
  title_part without
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

# add_look_at layout how shown...: View > Look At checked in the group's
# menu (opened by a left click on its ⋮, or with how "right" a right click
# on the group's capsule): a button more, kept in the settings; the group
# then shows the defaults with the ids "shown" (expect_group).
add_look_at() {
  local before=$SHOWN layout=$1 how=$2
  shift 2
  open_menu "$how"
  ui_mark
  ui_menu_choose "View" "Look At"
  ui_expect_new "Quick access added view.look_at" "$layout: Look At checked"
  ui_expect_new "Quick access: " "$layout: the group made again"
  expect_group "$layout: Look At added" "$@"
  [ "$SHOWN" = $((before + 1)) ] || ui_fail "$layout: $SHOWN buttons, expected $((before + 1))"
  grep -q "view.look_at" "$CONF" || ui_fail "$layout: Look At is not in the settings ($CONF)"
  echo "ok   $layout: a button more, kept in the settings"
}

# measure_row_menu entry: Measure's row menu in the SOLID > INSPECT menu
# (the ⋮ of its row), which ends with the entry (Add to Quick Access or
# Remove from Quick Access, mitcad#1990), then the entry.
measure_row_menu() {
  ui_mark
  ui_step "the SOLID tab"                  ui_click_tab "SOLID"
  ui_sync
  ui_mark
  ui_step "open the SOLID INSPECT menu"    ui_click_logged "Ribbon SOLID/INSPECT"
  ui_expect_new "Ribbon menu dots SOLID/INSPECT inspect.measure at" "the ⋮ of Measure's row"
  ui_mark
  ui_step "the ⋮ of Measure"               ui_click_logged "Ribbon menu dots SOLID/INSPECT inspect.measure"
  ui_expect_new "Toolbar menu SOLID/INSPECT inspect.measure: " "Measure's row menu"
  grep "^Toolbar menu SOLID/INSPECT inspect.measure: " "$UI_LOG" | tail -1 | grep -qF " | - | $1" ||
    ui_fail "Measure's row menu has no $1 at its end: $(grep '^Toolbar menu SOLID/INSPECT' "$UI_LOG" | tail -1)"
  ui_expect_new "Toolbar menu entry $1 at" "its $1"
  ui_step "$1"                             ui_click_logged "Toolbar menu entry $1"
}

# open_menu [right]: the group's menu, by its ⋮ (or a right click on the
# capsule, left of the ⋮), its entries logged: the File, Edit, View and
# Tools menus, the commands added from elsewhere, Reset to Default and Hide
# Quick-Access Toolbar.
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
  # The commands no menu has (added from the toolbar) as checks after Tools.
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu: " | tail -1 |
    grep -qE "File \| Edit \| View \| Tools( \| [^|]+)* \| Reset to Default \| Hide Quick-Access Toolbar$" ||
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

# title_part with|without: the last "Floating cards" or "Docked top bar"
# line, waited for, has
# the title row's quick-access part (titleQuickAccess) or has not; the
# tabs' part is there either way, a title box never (mitcad#1989), and the
# group's right end lies just left of the indicators or the search field
# (ui_quick_access_place).
title_part() {
  local line=""
  for _ in $(seq 1 25); do
    line=$(grep -oE "^(Floating cards|Docked top bar): .*" "$UI_LOG" | tail -1)
    if [ "$1" = with ] && grep -qF "; titleQuickAccess " <<< "$line"; then break; fi
    if [ "$1" = without ] && ! grep -qF "; titleQuickAccess " <<< "$line"; then break; fi
    sleep 0.2
  done
  grep -qF "titleTabs " <<< "$line" || ui_fail "no title row tabs in the top bar: '$line'"
  if [ "$1" = with ]; then
    grep -qF "; titleQuickAccess " <<< "$line" || ui_fail "the title row has no quick-access part: '$line'"
    echo "ok   the title row's quick-access part is there"
    ui_quick_access_place "the title row" "$line"
  else
    grep -qF "; titleQuickAccess " <<< "$line" && ui_fail "the title row still has its quick-access part: '$line'"
    echo "ok   the title row's quick-access part is gone"
    ui_quick_access_place "the title row" "$line"
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

echo "--- 1. Floating: empty and hidden, Save, Measure and Look At added, folding, hidden, emptied"
cp "$PLATE" "$WORK/floating.mitcad"
ui_start_app --chrome=floating --no-native-dialogs --open "$WORK/floating.mitcad"
ui_expect_log "Opened $WORK/floating.mitcad" "opened"
# The Floating top bar shows no quick-access group by default (mitcad#1990).
ui_expect_log "Quick access: none" "floating: no commands by default"
ui_expect_log "Quick access group hidden: no commands" "floating: the empty group hidden"
title_part without
ui_sync
grep -q "^Quick access menu at " "$UI_LOG" && ui_fail "floating: the empty group's ⋮ is in sight"
echo "ok   floating: no quick-access group in the top bar by default"
save_steps floating "$WORK/floating.mitcad"
# Wide enough for the SOLID groups with their buttons.
ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
ui_sync
measure_row_menu "Add to Quick Access"
ui_expect_new "Quick access added inspect.measure" "floating: Measure added from its row menu"
ui_expect_new "Quick access group shown: it has commands" "floating: the group shown"
expect_group "floating: Measure's button" inspect.measure
title_part with
grep -q "inspect.measure" "$CONF" || ui_fail "floating: Measure is not in the settings ($CONF)"
add_look_at floating "" view.look_at inspect.measure
ui_mark
# Narrow enough with the compact indicators (mitcad#1990).
ui_step "a narrow window"                xdotool windowsize "$UI_WINDOW" 520 800
ui_expect_new "Quick access folded" "floating: folded in a narrow window"
ui_sync
grep -E "^Quick access view.look_at (at |out of sight)" "$UI_LOG" | tail -1 | grep -q "out of sight" ||
  ui_fail "floating: Look At's button still in sight when folded"
title_part with
ui_mark
ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
ui_expect_new "Quick access unfolded" "floating: unfolded in a wide window"
title_part with
toggle_preference floating hidden
title_part without
expect_hidden_setting "floating, from Preferences"
toggle_preference floating shown
ui_expect_new "Quick access: " "floating: the group made again"
expect_group "floating: shown again with the same buttons" view.look_at inspect.measure
title_part with
open_menu
ui_mark
ui_menu_choose "Hide Quick-Access Toolbar"
ui_expect_new "Hide Quick-Access Toolbar chosen" "floating: Hide Quick-Access Toolbar chosen"
ui_expect_new "Quick access toolbar hidden" "floating: the group hidden from its menu"
title_part without
expect_hidden_setting "floating, from the group's menu"
toggle_preference floating shown
ui_expect_new "Quick access: " "floating: the group made again"
expect_group "floating: shown again" view.look_at inspect.measure
measure_row_menu "Remove from Quick Access"
ui_expect_new "Quick access removed inspect.measure" "floating: Measure removed from its row menu"
ui_expect_new "Quick access: " "floating: the group made again"
expect_group "floating: Look At left" view.look_at
open_menu
ui_mark
ui_menu_choose "View" "Look At"
ui_expect_new "Quick access removed view.look_at" "floating: Look At unchecked"
expect_empty "floating, emptied"
grep -q -e "view.look_at" -e "inspect.measure" "$CONF" && ui_fail "floating: removed commands still in the settings ($CONF)"
ui_stop_app

echo "--- 2. Docked: the same top bar: the saved empty choice, Save, Measure and Look At, hidden"
cp "$PLATE" "$WORK/docked.mitcad"
ui_start_app --chrome=docked --no-native-dialogs --open "$WORK/docked.mitcad"
ui_expect_log "Opened $WORK/docked.mitcad" "opened"
ui_expect_log "Quick access: none" "docked: the saved choice (none)"
ui_expect_log "Quick access group hidden: no commands" "docked: the empty group hidden, as in the Floating top bar"
title_part without
ui_sync
grep -q "^Quick access menu at " "$UI_LOG" && ui_fail "docked: the empty group's ⋮ is in sight"
grep -qE "^(Quick access group: flat|Ribbon tab row end: )" "$UI_LOG" && ui_fail "docked: a quick-access group in the toolbar's tab row"
echo "ok   docked: no quick-access group in the top bar by default, none in the tab row"
save_steps docked "$WORK/docked.mitcad"
ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
ui_sync
measure_row_menu "Add to Quick Access"
ui_expect_new "Quick access added inspect.measure" "docked: Measure added from its row menu"
ui_expect_new "Quick access group shown: it has commands" "docked: the group shown"
expect_group "docked: Measure's button" inspect.measure
title_part with
add_look_at docked right view.look_at inspect.measure
toggle_preference docked hidden
title_part without
expect_hidden_setting "docked, from Preferences"
ui_stop_app

echo "--- 3. Floating: the choice saved in Docked"
cp "$PLATE" "$WORK/floating2.mitcad"
ui_start_app --chrome=floating --no-native-dialogs --open "$WORK/floating2.mitcad"
ui_expect_log "Opened $WORK/floating2.mitcad" "opened"
ui_expect_log "Quick access toolbar hidden" "floating: the group hidden at the start"
title_part without
toggle_preference floating shown
ui_expect_new "Quick access: " "floating: the group made again"
expect_group "floating: the saved choice read back" view.look_at inspect.measure
title_part with
ui_stop_app

ui_finish "Quick-access group"

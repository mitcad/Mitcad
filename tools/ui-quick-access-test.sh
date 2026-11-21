#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccess.hpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/QuickAccessGroup.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/files/FileCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/MainWindow.cpp app/view/ViewController.cpp tools/cli
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandSearch.cpp app/DocumentFrame.cpp
# The top bar's one left group (mitcad#2129, the quick-access group of
# mitcad#1090) through the real UI, in the Floating layout and in the Docked
# one, both with the top bar of Windows and Linux, on a saved plate that
# mitcad-cli makes. In each layout:
#   1. Fresh settings: after the fixed ☰ and Home, the default items in
#      order: File ˅ (menu.file), New Design, Open, Save, Design Properties,
#      Print, Undo and Redo ("Quick access: " and their places left to
#      right); Print has no button while nothing prints ("Quick access
#      file.print hidden: nothing to print"). The group is at the row's left
#      end, the ☰ first in it, the document tabs after it.
#   2. ☰ and Home offer no remove: their right-click gives the group's menu
#      without Remove from Quick Access.
#   3. File ˅ removed with Remove from Quick Access in its right-click menu,
#      then added back with File Menu checked in the add button's (⋮) menu:
#      at its place again, first after Home.
#   4. Save disabled for the unchanged saved design, enabled after an edit,
#      disabled once saved.
#   5. Add to Quick Access in Measure's row menu (the SOLID > INSPECT
#      toolbar's menu): Measure at the group's end, after Redo.
#   6. Hide Quick-Access Toolbar (the ⋮'s menu) hides only Measure: the
#      default items stay in sight; Show Quick-Access Toolbar (Floating) or
#      Preferences › Display › Show quick-access toolbar (Docked) shows it
#      again.
#   7. Redo removed (its right-click while it is disabled, nothing to
#      redo), then a drawing opened (Create
#      Drawing): Print shows in the group.
#   8. A restart: Redo stays removed, Measure stays (quickAccess/items).
#   9. A profile with an earlier version's quickAccess/commands (Look At,
#      Preferences, Open): Look At after the default items, once ("Quick
#      access migrated"), Preferences left out (the bar has its own), Open
#      not twice; the new key written.
#  10. At 1024 px with two designs open (Ctrl+N) and Measure and Look At
#      added (the defaults alone fit there): the group folds its last
#      items into its ⋮ ("Quick access folded: ... view.look_at"), Look At
#      first in the ⋮'s menu, File still in sight, and both document tabs
#      stay in sight, not elided ("Title row tabs: ").
# The Windows counterpart is tools/ui-windows-quick-access-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-quick-access-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-quick-access.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
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
# The left group's default items (QuickAccessSettings::topBarItems).
DEFAULTS=(menu.file file.new_design file.open file.save file.design_properties file.print edit.undo edit.redo)

# settings text: the settings file before a start (an empty text: fresh).
settings() {
  mkdir -p "$(dirname "$CONF")"
  printf '%b' "$1" > "$CONF"
}

# group_line: the group's last "Quick access: " line, without its prefix.
group_line() { grep '^Quick access: ' "$UI_LOG" | tail -1 | sed 's/^Quick access: //'; }

# expect_group description id...: the last "Quick access: " line (waited
# for) lists the ids, in order, less those not in this build; each of them
# but Print (while nothing prints) has a button in sight, left to right
# after Home.
expect_group() {
  local description=$1 line="" shown missing="" expected=() id want x last home where
  shift
  for _ in $(seq 1 25); do
    line=$(group_line)
    [ -n "$line" ] && break
    sleep 0.2
  done
  shown=${line%% (not in this build: *}
  [[ $line == *" (not in this build: "* ]] && missing=${line#* (not in this build: } && missing=${missing%)}
  for id in "$@"; do
    case ", $missing, " in
      *", $id, "*) ;;
      *) expected+=("$id") ;;
    esac
  done
  want=$(printf '%s, ' "${expected[@]}")
  want=${want%, }
  [ "$shown" = "$want" ] || ui_fail "$description: the group shows '$shown', expected '$want' (line '$line')"
  ui_sync
  home=$(ui_logged_at "Quick access leading titleHome" | awk '{ print $1 }')
  last=$home
  for id in "${expected[@]}"; do
    where=$(grep -E "^Quick access $id (at |out of sight)" "$UI_LOG" | tail -1)
    if [[ $where == *" out of sight" ]]; then
      [ "$id" = file.print ] || ui_fail "$description: no button of $id in sight"
      continue
    fi
    x=$(sed -n 's/.* at \([0-9]*\),.*/\1/p' <<< "$where")
    [ -n "$x" ] && [ "$x" -gt "$last" ] || ui_fail "$description: $id ($x) is not right of the item before it ($last)"
    last=$x
  done
  echo "ok   $description: $want, left to right after Home"
}

# expect_defaults description [extra...]: the default items, then the extra
# ids.
expect_defaults() {
  local description=$1
  shift
  expect_group "$description" "${DEFAULTS[@]}" "$@"
}

# parts_line: the top bar's parts as logged last, in either layout.
parts_line() { grep -oE "^(Floating cards|Docked top bar): .*" "$UI_LOG" | tail -1; }

# place description: the group at the row's left end, the ☰ first in it,
# the document tabs after it (ui_quick_access_place).
place() {
  local line=""
  for _ in $(seq 1 25); do
    line=$(parts_line)
    grep -qF "; titleQuickAccess " <<< "$line" && break
    sleep 0.2
  done
  ui_quick_access_place "$1" "$line"
}

# item_menu id entry: an item's right-click menu (Remove from Quick
# Access), its entry chosen; the item in sight first (a folded one has no
# right-click). Also for a disabled item, which gets no context menu event
# from Qt: its right press opens the menu.
item_menu() {
  in_sight "$1" yes
  ui_mark
  ui_step "right-click $1"                 ui_click_logged "Quick access $1" 3
  ui_expect_new "Quick access item menu: $1" "$1's right-click menu"
  ui_expect_new "Context menu: Remove from Quick Access" "its Remove from Quick Access"
  ui_mark
  ui_menu_choose "$2"
}

# no_remove name: a leading widget's right-click (☰ or Home) offers no
# Remove from Quick Access.
no_remove() {
  ui_mark
  ui_step "right-click $1"                 ui_click_logged "Quick access leading $1" 3
  ui_sync
  sleep 0.5
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q -e "^Quick access item menu: " -e "^Context menu: Remove from Quick Access" &&
    ui_fail "$1 offers Remove from Quick Access"
  ui_key Escape
  ui_sync
  echo "ok   $1 offers no Remove from Quick Access"
}

# open_menu: the add button's (⋮) menu, its entries logged: the File Menu
# check, the File, Edit, View and Tools menus, the commands added from
# elsewhere, Reset to Default and Hide (or Show) Quick-Access Toolbar.
open_menu() {
  ui_mark
  ui_step "the group's add button (⋮)"     ui_click_logged "Quick access menu"
  ui_expect_new "Context menu: " "the group's menu opened"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu: " | tail -1 |
    grep -qE "File Menu \| File \| Edit \| View \| Tools( \| [^|]+)* \| Reset to Default \| (Hide|Show) Quick-Access Toolbar$" ||
    ui_fail "the group's menu: $(grep '^Context menu: ' "$UI_LOG" | tail -1)"
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

# toggle_preference layout shown|hidden: Preferences found by the command
# search ("quick access" is a phrase of the Display page's settings,
# mitcad#1189), Show quick-access toolbar (Alt+Q) toggled, OK.
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
  ui_expect_new "Quick access toolbar $2" "$1: the added commands $2 at once"
}

# in_sight id yes|no: the item's last place is in sight or not.
in_sight() {
  local last
  ui_sync
  last=$(grep -E "^Quick access $1 (at |out of sight)" "$UI_LOG" | tail -1)
  if [ "$2" = yes ]; then
    [[ $last == *" at "* ]] || ui_fail "$1 is not in sight: '$last'"
  else
    [[ $last == *" out of sight" ]] || ui_fail "$1 is still in sight: '$last'"
  fi
}

# layout_steps layout: steps 1 to 10 in the layout.
layout_steps() {
  local layout=$1 file
  file=$WORK/$layout.mitcad
  echo "=== $layout"

  echo "--- 1. the default items after ☰ and Home"
  settings ""
  cp "$PLATE" "$file"
  ui_start_app --chrome="$layout" --no-native-dialogs --open "$file"
  ui_expect_log "Opened $file" "$layout: opened"
  ui_expect_log "Quick access file.print hidden: nothing to print" "$layout: Print hidden while nothing prints"
  ui_expect_log "Quick access leading menuBarButton at " "$layout: the ☰ in the group"
  ui_expect_log "Quick access leading titleHome at " "$layout: Home in the group"
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
  ui_sync
  expect_defaults "$layout: the defaults"
  in_sight file.print no
  place "$layout: the left group"

  echo "--- 2. ☰ and Home offer no remove"
  no_remove menuBarButton
  no_remove titleHome

  echo "--- 3. File ˅ removed and added back"
  item_menu menu.file "Remove from Quick Access"
  ui_expect_new "Quick access removed menu.file" "$layout: File removed"
  ui_expect_new "Quick access: " "$layout: the group made again"
  expect_group "$layout: without File" "${DEFAULTS[@]:1}"
  open_menu
  ui_mark
  ui_menu_choose "File Menu"
  ui_expect_new "Quick access added menu.file" "$layout: File Menu checked"
  ui_expect_new "Quick access: " "$layout: the group made again"
  expect_defaults "$layout: File back at its place"

  echo "--- 4. Save"
  ui_save_state disabled "$layout: Save disabled for the unchanged saved design"
  edit
  ui_save_state enabled "$layout: Save enabled after the edit"
  ui_mark
  ui_step "save (Ctrl+S)"                  ui_key ctrl+s
  ui_expect_new "Saved $file" "$layout: saved"
  ui_save_state disabled "$layout: Save disabled again once saved"

  echo "--- 5. Add to Quick Access from the toolbar"
  measure_row_menu "Add to Quick Access"
  ui_expect_new "Quick access added inspect.measure" "$layout: Measure added from its row menu"
  ui_expect_new "Quick access: " "$layout: the group made again"
  expect_defaults "$layout: Measure at the group's end" inspect.measure
  grep -q "^items=.*inspect.measure" "$CONF" || ui_fail "$layout: Measure is not in quickAccess/items ($CONF)"

  echo "--- 6. Hide Quick-Access Toolbar hides only the added commands"
  open_menu
  ui_mark
  ui_menu_choose "Hide Quick-Access Toolbar"
  ui_expect_new "Hide Quick-Access Toolbar chosen" "$layout: Hide Quick-Access Toolbar chosen"
  ui_expect_new "Quick access toolbar hidden" "$layout: the added commands hidden"
  in_sight inspect.measure no
  for id in menu.file file.open file.save edit.undo edit.redo; do
    in_sight "$id" yes
  done
  place "$layout: the left group stays"
  echo "ok   $layout: only Measure hidden, the default items in sight"
  if [ "$layout" = floating ]; then
    open_menu
    ui_mark
    ui_menu_choose "Show Quick-Access Toolbar"
    ui_expect_new "Show Quick-Access Toolbar chosen" "$layout: Show Quick-Access Toolbar chosen"
    ui_expect_new "Quick access toolbar shown" "$layout: the added commands shown"
  else
    toggle_preference "$layout" shown
  fi
  ui_expect_new "Quick access: " "$layout: the group made again"
  expect_defaults "$layout: Measure in sight again" inspect.measure

  echo "--- 7. Redo removed; Print with a drawing"
  item_menu edit.redo "Remove from Quick Access"
  ui_expect_new "Quick access removed edit.redo" "$layout: Redo removed"
  ui_expect_new "Quick access: " "$layout: the group made again"
  expect_group "$layout: without Redo" "${DEFAULTS[@]:0:7}" inspect.measure
  ui_mark
  ui_step "Create Drawing (search)"        ui_command "Create Drawing"
  ui_expect_new "Create Drawing dialog opened" "$layout: its dialog"
  ui_focus_dialog '^Create Drawing$'
  ui_step "OK"                             ui_key Return
  ui_focus_main
  ui_expect_new "Drawing opened: " "$layout: a drawing open"
  ui_expect_new "Quick access file.print shown" "$layout: Print shown while the drawing is open"
  in_sight file.print yes
  ui_stop_app

  echo "--- 8. a restart: Redo stays removed"
  ui_start_app --chrome="$layout" --no-native-dialogs --open "$file"
  ui_expect_log "Opened $file" "$layout: opened again"
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
  ui_sync
  expect_group "$layout: Redo still removed after a restart" "${DEFAULTS[@]:0:7}" inspect.measure
  grep -q "^items=.*edit.redo" "$CONF" && ui_fail "$layout: Redo is in quickAccess/items again ($CONF)"
  ui_stop_app

  echo "--- 9. an earlier version's quickAccess/commands"
  settings '[quickAccess]\ncommands="view.look_at,tools.preferences,file.open"\n'
  cp "$PLATE" "$file"
  ui_start_app --chrome="$layout" --no-native-dialogs --open "$file"
  ui_expect_log "Opened $file" "$layout: opened"
  ui_expect_log "Quick access migrated: 3 commands" "$layout: the old choice migrated"
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
  ui_sync
  expect_defaults "$layout: Look At after the defaults, Preferences left out" view.look_at
  [ "$(grep -c '^Quick access migrated: ' "$UI_LOG")" = 1 ] || ui_fail "$layout: migrated more than once"
  grep -q "^items=" "$CONF" || ui_fail "$layout: quickAccess/items not written ($CONF)"
  grep -q "^itemsMigrated=true" "$CONF" || ui_fail "$layout: the migration not marked ($CONF)"
  ui_stop_app

  echo "--- 10. 1024 px with two designs"
  # The defaults alone fit at 1024 px with two designs (Print hidden): two
  # added commands make the group wider than its room there.
  settings '[quickAccess]\nitems="menu.file,file.new_design,file.open,file.save,file.design_properties,file.print,edit.undo,edit.redo,inspect.measure,view.look_at"\n'
  cp "$PLATE" "$file"
  ui_start_app --chrome="$layout" --no-native-dialogs --open "$file"
  ui_expect_log "Opened $file" "$layout: opened"
  ui_mark
  ui_step "a new design (Ctrl+N)"          ui_key ctrl+n
  ui_expect_new "Title row tabs: " "$layout: the document tabs"
  ui_mark
  ui_step "a 1024 px window"               xdotool windowsize "$UI_WINDOW" 1024 800
  ui_expect_new "Quick access folded: " "$layout: the group folds its last items"
  ui_sync
  sleep 0.5
  local tabs folded
  folded=$(grep "^Quick access folded: " "$UI_LOG" | tail -1 | sed 's/^Quick access folded: //')
  [[ ", $folded" != *", menu.file"* ]] || ui_fail "$layout: File folded before the last items: '$folded'"
  [[ $folded == *"view.look_at" ]] || ui_fail "$layout: the last item (Look At) is not folded: '$folded'"
  in_sight view.look_at no
  in_sight menu.file yes
  # Folded into the add button: its menu lists Look At first, before File Menu.
  open_menu
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu: " | tail -1 | sed 's/ | File Menu | .*//' | grep -q "Look At" ||
    ui_fail "$layout: the add button's menu does not list the folded Look At first: $(grep '^Context menu: ' "$UI_LOG" | tail -1)"
  ui_key Escape
  ui_sync
  tabs=$(grep "^Title row tabs: " "$UI_LOG" | tail -1)
  [ "$(tr ';' '\n' <<< "${tabs#*; }" | grep -c " elided no$")" = 2 ] ||
    ui_fail "$layout: the two document tabs are not both whole in sight: '$tabs'"
  place "$layout: the left group at 1024 px"
  echo "ok   $layout: folded ($folded), both document tabs in sight"
  ui_stop_app
}

ui_start_display
layout_steps floating
layout_steps docked
ui_finish "The top bar's left group"

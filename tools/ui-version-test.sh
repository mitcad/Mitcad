#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Saving versions (P12d) through the real UI, checked with the system's git:
#   1. Start Version History of an untitled design: the New Project dialog
#      (by default Documents/Mitcad/Project1), the author git's settings
#      give shown once, Save As in the project's folder; the first version
#      has the undo steps' names as its message, git's author and Mitcad's
#      trailers, and git status is clean. Save without a change, and a
#      change of the display state only (the Origin shown, kept apart in
#      .mitcad/local), record no version.
#   2. Autosave writes the changed design to the recovery folder but
#      records no version and leaves the file. Save Version (Ctrl+Alt+S)
#      records one with the description typed.
#   3. Opened again with a change, Ctrl+S records it without asking. A
#      change made to the file by another program: Compare shows it, Save
#      as New Version records it first and then the design on top. A newer
#      version committed with git: Cancel writes nothing, Save as New
#      Version builds on it.
#   4. Preferences, General: versions by the name and email there instead
#      of git's.
#   5. A file renamed outside Mitcad: opened, its display state follows,
#      and Save records the rename as a version of its own, then the change.
#   6. Start Version History of a file outside projects: its folder made a
#      project; another one moved into a new project.
#   7. Without git's user.name and user.email the first version asks for
#      them: cancelled, what was added to the folder (or the new project's
#      folder) goes again; given, Mitcad's settings keep them, and New
#      Project's design is saved in the project's folder.
#   8. Version History (P12e; each Save above kept a preview): the renamed
#      file's ten versions newest first with what each changed, the latest
#      with its preview compared with the one before; an older one compared
#      with the open design (a change not saved) and with the geometry;
#      Save Copy As outside the project; Restore with the unsaved change
#      saved first: git's log has the restore as a new version, the file
#      is the old version's, and Save asks nothing after it; Open of an
#      older version as an untitled design, which Save As keeps.
#
# No test uses the user's identity: git's global configuration is the test's
# own (GIT_CONFIG_GLOBAL, GIT_CONFIG_NOSYSTEM), and HOME is the test's, so
# the projects' default folder (Documents/Mitcad) is too.
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so paths can be typed into them.
# Usage: tools/ui-version-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v git > /dev/null || { echo "git is required (apt-get install git)" >&2; exit 2; }

WORK=$(mktemp -d /tmp/mitcad-ui-version.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
DOCUMENTS=$HOME/Documents/Mitcad
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE
printf '[user]\n\tname = Git Test\n\temail = git-test@example.invalid\n' > "$GIT_CONFIG_GLOBAL"
export MITCAD_TEST_AUTOSAVE_SECONDS=2
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
# Versions' previews (P12e), in the test's application data.
THUMBNAILS=$XDG_DATA_HOME/Mitcad/Mitcad/thumbnails
PROJECT=$DOCUMENTS/bracket
FILE=$PROJECT/block.mitcad

# in_git folder git-arguments...: the system's git in a project.
in_git() {
  local folder=$1
  shift
  git -C "$folder" "$@"
}

# expect_git folder "git arguments" expected description
expect_git() {
  local actual
  # shellcheck disable=SC2086
  actual=$(in_git "$1" $2 2>&1) || ui_fail "$4: git $2: $actual"
  [ "$actual" = "$3" ] || ui_fail "$4: git $2 gives '$actual', expected '$3'"
  echo "ok   $4"
}

# commits folder: the number of versions in the project.
commits() { in_git "$1" rev-list --count HEAD; }

expect_commits() {
  [ "$(commits "$1")" = "$2" ] || ui_fail "$3: $(commits "$1") versions in $1, expected $2"
  echo "ok   $3"
}

expect_clean() {
  local status
  status=$(in_git "$1" status --porcelain 2>&1)
  [ -z "$status" ] || ui_fail "$2: git status in $1: $status"
  echo "ok   $2"
}

# file_d3 project [value]: prints d3 of a project file, or sets it (as
# another program would change the file).
file_d3() {
  python3 - "$@" << 'EOF'
import json, sys

doc = json.load(open(sys.argv[1]))
values = [p for p in doc['parameters'] if p['name'] == 'd3']
if len(sys.argv) > 2:
    values[0]['value'] = float(sys.argv[2])
    values[0]['expression'] = '%g mm' % float(sys.argv[2])
    with open(sys.argv[1], 'w') as f:
        f.write(json.dumps(doc, indent=2) + '\n')
else:
    print([p['value'] for p in values])
EOF
}

# type_text text: replaces the focused field's text.
type_text() {
  ui_key ctrl+a
  xdotool type --delay 20 "$1"
  ui_sync
}

# type_path path: replaces the file name in Qt's file dialog and accepts.
type_path() {
  type_text "$1"
  ui_key Return
  ui_focus_main
}

# no_new_since "text" description: nothing logged with the text since the mark.
no_new_since() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
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

ui_start_display

echo "--- Start Version History of an untitled design: a new project"
ui_start_app --demo --no-native-dialogs
ui_step "fit (F6)"                       ui_key F6
ui_step "fillet (F)"                     ui_key f
ui_step "pick a face of the body"        ui_view_click 50 50
ui_step "confirm (Enter)"                ui_key Return
ui_expect_log "Added fillet on 4 edge(s)" "a fillet: a change to save"
ui_mark
ui_step "start version history (search)" ui_command "Start Version History"
ui_expect_new "New Project dialog: $DOCUMENTS/Project1" "a new project, by default in Documents/Mitcad"
ui_focus_dialog '^Save in a New Project$'
ui_step "name it bracket, Enter"         type_text bracket
ui_key Return
ui_expect_new "Version author dialog: git's Git Test <git-test@example.invalid>" \
  "the author git's settings give is shown"
ui_focus_dialog '^Version Author$'
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Version author: Git Test <git-test@example.invalid> (git)" "git's author"
ui_expect_new "Version history started in $PROJECT: version " "the project has version history"
ui_focus_dialog '^Save As$'
ui_step "a name in the project's folder" type_path block.mitcad
ui_expect_new "Saved $FILE" "saved in the project's folder"
ui_expect_new "Version recorded: block.mitcad " "a version recorded"
ui_expect_new "Version status: bracket, main, v1" "the status bar shows the version"
ui_expect_new "Version preview saved: " "with a preview of the version"
blob=$(in_git "$PROJECT" rev-parse HEAD:block.mitcad)
[ -f "$THUMBNAILS/$blob.png" ] || ui_fail "no preview $THUMBNAILS/$blob.png"
echo "ok   the preview is kept by the file's blob id, outside the project"
expect_commits "$PROJECT" 2 "two versions: the project's files and the design"
expect_git "$PROJECT" "log -1 --format=%s" "Save block.mitcad: Add Fillet1" "the message has the undo step's name"
expect_git "$PROJECT" "log -1 --format=%an|%ae|%cn" "Git Test|git-test@example.invalid|Git Test" "by git's author"
expect_git "$PROJECT" "log -1 --format=%(trailers:key=Mitcad-Format,valueonly)" "3" "with Mitcad's trailers"
expect_git "$PROJECT" "log --reverse --format=%s" "Create project bracket
Save block.mitcad: Add Fillet1" "the project's first version"
expect_git "$PROJECT" "ls-files" ".gitattributes
.gitignore
.mitcad/project.json
block.mitcad" "the versioned files"
expect_clean "$PROJECT" "git status is clean"
grep -q '^confirmed=true' "$SETTINGS" || ui_fail "the author is not marked shown in the settings"
echo "ok   the author is shown once"

echo "--- No change, or only the display state: no version"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version unchanged: block.mitcad (v1)" "nothing changed: no version"
no_new_since "Version author dialog" "the author is not asked again"
ui_mark
ui_step "show the origin"                ui_click_logged "Browser eye Root/Origin"
ui_expect_new "Visibility Root/Origin: shown" "origin shown"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version unchanged: block.mitcad (v1)" "the display state is not versioned"
[ -f "$PROJECT/.mitcad/local/display/block.mitcad.json" ] || ui_fail "no display state in .mitcad/local"
echo "ok   the display state is kept in .mitcad/local"
expect_commits "$PROJECT" 2 "still two versions"
expect_clean "$PROJECT" "git status is clean"

echo "--- Autosave records no version; Save Version"
checksum=$(sha256sum "$FILE")
ui_mark
ui_step "undo Show Origin (Ctrl+Z)"      ui_key ctrl+z
ui_step "undo the fillet (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Autosaved block.mitcad: " "the change autosaved" 10
expect_commits "$PROJECT" 2 "autosave recorded no version"
[ "$(sha256sum "$FILE")" = "$checksum" ] || ui_fail "autosave changed the project file"
echo "ok   the project file is as saved"
expect_clean "$PROJECT" "git status is clean"
ui_mark
ui_step "save version (Ctrl+Alt+S)"      ui_key ctrl+alt+s
ui_expect_new "Save Version dialog: Save block.mitcad: Undo Add Fillet1" \
  "the dialog offers the automatic message, without the display state's step"
ui_focus_dialog '^Save Version$'
ui_step "type the description's summary" xdotool type --delay 20 "No fillet"
ui_step "a new line (Enter)"             ui_key Return
ui_step "and more"                       xdotool type --delay 20 "The plate without its rounding."
ui_step "save (Ctrl+Enter)"              ui_key ctrl+Return
ui_focus_main
ui_expect_new "Version recorded: block.mitcad " "the version recorded"
ui_expect_new " v2: No fillet" "with the description's first line"
expect_git "$PROJECT" "log -1 --format=%B" "No fillet
The plate without its rounding.

Mitcad-Version: $(in_git "$PROJECT" log -1 --format=%B | sed -n 's/^Mitcad-Version: //p')
Mitcad-Format: 3" "the whole description"
expect_commits "$PROJECT" 3 "three versions"
[ ! -f "$PROJECT/.mitcad/local/display/block.mitcad.json" ] || ui_fail "the default display state is still kept"
echo "ok   the display state back at its default: nothing kept"
ui_key ctrl+q # exit
expect_exit "exited"

echo "--- Opened with a change: Ctrl+S"
ui_start_app --open "$FILE" --set d3=30 --no-native-dialogs
ui_expect_log "Opened $FILE" "opened"
ui_expect_log "Version status: bracket, main, v2" "the status bar shows the version opened"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version recorded: block.mitcad " "a version recorded"
no_new_since "Version author dialog" "without asking for the author"
expect_git "$PROJECT" "log -1 --format=%s" "Save block.mitcad: Change d3" "named after the change"

echo "--- A change outside Mitcad: compare, then a version of it first"
file_d3 "$FILE" 40 # another program
ui_mark
ui_step "undo the change of d3 (Ctrl+Z)" ui_key ctrl+z
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Save conflict: $FILE changed outside Mitcad (the file)" "Save notices the change"
ui_focus_dialog '^Mitcad$'
ui_step "compare (O)"                    ui_key o
ui_expect_new "Save conflict: compare: Changes from the file in the folder to the open design:" "Compare"
ui_expect_new "d3 (Extrude1 distance): 40 mm -> 20 mm" "it shows d3"
ui_focus_dialog '^Compare block.mitcad$'
ui_step "close the comparison (Esc)"     ui_key Escape
ui_focus_dialog '^Mitcad$'
ui_step "save as a new version (N)"      ui_key n
ui_focus_main
ui_expect_new "Save conflict: new version" "a new version"
ui_expect_new "Version recorded: block.mitcad " "versions recorded"
expect_git "$PROJECT" "log -2 --format=%s" "Save block.mitcad: Undo Change d3
Save block.mitcad as changed outside Mitcad" "the change outside first, the design on top"
[ "$(file_d3 "$FILE")" = "[20.0]" ] || ui_fail "the file has d3 $(file_d3 "$FILE")"
[ "$(in_git "$PROJECT" show HEAD~1:block.mitcad | python3 -c 'import json, sys; print([p["value"] for p in json.load(sys.stdin)["parameters"] if p["name"] == "d3"])')" = "[40.0]" ] ||
  ui_fail "the version before has not the other program's d3"
echo "ok   both are in the history"
expect_clean "$PROJECT" "git status is clean"

echo "--- A newer version from git: cancel, then on top of it"
file_d3 "$FILE" 45
in_git "$PROJECT" -c user.name="Git Elsewhere" -c user.email=elsewhere@example.invalid commit -qam "Elsewhere"
elsewhere=$(sha256sum "$FILE")
ui_mark
ui_step "redo the change of d3 (Ctrl+Y)" ui_key ctrl+y
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Save conflict: $FILE changed outside Mitcad (a newer version)" "Save notices the newer version"
ui_focus_dialog '^Mitcad$'
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Save conflict: cancelled" "cancelled"
[ "$(sha256sum "$FILE")" = "$elsewhere" ] || ui_fail "the file was written"
echo "ok   nothing written"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^Mitcad$'
ui_step "save as a new version (N)"      ui_key n
ui_focus_main
ui_expect_new "Version recorded: block.mitcad " "recorded"
expect_git "$PROJECT" "log -2 --format=%s|%an" "Save block.mitcad: Change d3|Git Test
Elsewhere|Git Elsewhere" "on top of the newer version"
[ "$(file_d3 "$FILE")" = "[30.0]" ] || ui_fail "the file has d3 $(file_d3 "$FILE")"
echo "ok   the file has the design's d3"

echo "--- Preferences: the author's name and email instead of git's"
ui_mark
ui_step "preferences, General (search)"  ui_command "Preferences: General"
ui_focus_dialog '^Preferences$'
ui_step "not git's (Alt+G)"              ui_key alt+g
ui_step "name (Alt+N)"                   ui_key alt+n
ui_step "type the name"                  type_text "Settings Author"
ui_step "email (Alt+M)"                  ui_key alt+m
ui_step "type the email"                 type_text "settings@example.invalid"
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Preferences: versions by Settings Author <settings@example.invalid>" "saved"
grep -q '^useGit=false' "$SETTINGS" || ui_fail "useGit is not saved off"
ui_mark
ui_step "undo the change of d3 (Ctrl+Z)" ui_key ctrl+z
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version author: Settings Author <settings@example.invalid> (settings)" "the settings' author"
ui_expect_new "Version recorded: block.mitcad " "recorded"
expect_git "$PROJECT" "log -1 --format=%an|%ae" "Settings Author|settings@example.invalid" "by the settings' author"
ui_mark
ui_step "show the origin"                ui_click_logged "Browser eye Root/Origin"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version unchanged: block.mitcad" "the display state kept, no version"
[ -f "$PROJECT/.mitcad/local/display/block.mitcad.json" ] || ui_fail "no display state in .mitcad/local"
ui_key ctrl+q # exit
expect_exit "exited"

echo "--- Renamed outside Mitcad"
mv "$FILE" "$PROJECT/plate.mitcad"
FILE=$PROJECT/plate.mitcad
versions=$(commits "$PROJECT")
ui_start_app --open "$FILE" --set d3=33 --no-native-dialogs
ui_expect_log "Renamed from block.mitcad: its display state moved along" "the rename is found"
ui_expect_log "Opened $FILE" "opened"
[ -f "$PROJECT/.mitcad/local/display/plate.mitcad.json" ] && [ ! -e "$PROJECT/.mitcad/local/display/block.mitcad.json" ] ||
  ui_fail "the display state did not follow the file"
echo "ok   the display state followed the file"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version recorded: plate.mitcad " "recorded"
expect_git "$PROJECT" "log -2 --format=%s" "Save plate.mitcad: Change d3
Rename block.mitcad to plate.mitcad" "the rename, then the change"
expect_commits "$PROJECT" $((versions + 2)) "two versions more"
expect_git "$PROJECT" "log --follow --format=%s -- plate.mitcad" "$(in_git "$PROJECT" log --format=%s | grep -v '^Create project')" \
  "git follows the rename back to the first version"
expect_clean "$PROJECT" "git status is clean"

echo "--- Start Version History of a file outside projects: its folder"
SINGLE=$WORK/single
mkdir -p "$SINGLE"
ui_mark
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_focus_dialog '^Save As$'
ui_step "a folder outside projects"      type_path "$SINGLE/part.mitcad"
ui_expect_new "Saved $SINGLE/part.mitcad" "saved"
ui_expect_new "Version status: none" "no version history there"
no_new_since "Version recorded" "and no version"
ui_mark
ui_step "start version history (search)" ui_command "Start Version History"
ui_expect_new "Start Version History dialog: $SINGLE" "the dialog"
ui_focus_dialog '^Start Version History$'
ui_step "use the folder (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Version history started in $SINGLE: version " "the folder has version history"
ui_expect_new "Version recorded: part.mitcad " "the design recorded"
expect_git "$SINGLE" "log --reverse --format=%s|%an" "Create project single|Settings Author
Save part.mitcad|Settings Author" "the folder's versions"
expect_clean "$SINGLE" "git status is clean"

echo "--- Start Version History: moved into a new project"
OTHER=$WORK/other
mkdir -p "$OTHER"
ui_mark
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_focus_dialog '^Save As$'
ui_step "another folder"                 type_path "$OTHER/gear.mitcad"
ui_expect_new "Saved $OTHER/gear.mitcad" "saved"
ui_mark
ui_step "start version history (search)" ui_command "Start Version History"
ui_focus_dialog '^Start Version History$'
ui_step "move to a new project (M)"      ui_key m
ui_expect_new "New Project dialog: $DOCUMENTS/gear" "a new project named after the design"
ui_focus_dialog '^Move to a New Project$'
ui_step "create (Enter)"                 ui_key Return
ui_focus_main
ui_expect_new "Moved $OTHER/gear.mitcad to $DOCUMENTS/gear/gear.mitcad" "moved"
[ ! -e "$OTHER/gear.mitcad" ] || ui_fail "the file it came from is still there"
expect_git "$DOCUMENTS/gear" "log --format=%s" "Save gear.mitcad
Create project gear" "the new project's versions"
expect_clean "$DOCUMENTS/gear" "git status is clean"
ui_key ctrl+q # exit
expect_exit "exited"

echo "--- Without git's user: cancelled, nothing is left behind"
: > "$GIT_CONFIG_GLOBAL"
export XDG_CONFIG_HOME=$WORK/config
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
KEEP=$WORK/keep
mkdir -p "$KEEP"
cp "$SINGLE/part.mitcad" "$KEEP/part.mitcad"
ui_start_app --open "$KEEP/part.mitcad" --no-native-dialogs
ui_expect_log "Opened $KEEP/part.mitcad" "a file outside projects opened"
ui_mark
ui_step "start version history (search)" ui_command "Start Version History"
ui_focus_dialog '^Start Version History$'
ui_step "use the folder (Enter)"         ui_key Return
ui_expect_new "Version author dialog: git has none" "the author is asked"
ui_focus_dialog '^Version Author$'
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Version author dialog cancelled" "cancelled"
[ "$(ls -A "$KEEP")" = "part.mitcad" ] || ui_fail "left in the folder: $(ls -A "$KEEP")"
echo "ok   the folder is as it was"
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_focus_dialog '^New Project$'
ui_step "name it gone, Enter"            type_text gone
ui_key Return
ui_focus_dialog '^Version Author$'
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Version author dialog cancelled" "cancelled"
[ ! -e "$DOCUMENTS/gone" ] || ui_fail "the new project's folder is left"
echo "ok   the new project's folder went again"
no_new_since "New project " "no new project"

echo "--- Without git's user: asked once, kept in Mitcad's settings"
ui_mark
ui_step "new project (search)"           ui_command "New Project"
ui_expect_new "New Project dialog: $DOCUMENTS/Project1" "Project1 is free"
ui_focus_dialog '^New Project$'
ui_step "name it nogit, Enter"           type_text nogit
ui_key Return
ui_expect_new "Version author dialog: git has none" "the author is asked"
ui_focus_dialog '^Version Author$'
ui_step "type the name"                  type_text "Asked Author"
ui_step "the email (Tab)"                ui_key Tab
ui_step "type the email"                 type_text "asked@example.invalid"
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Version author: Asked Author <asked@example.invalid> (settings)" "the author given"
ui_expect_new "New project $DOCUMENTS/nogit" "a new design for the project"
grep -q '^name=Asked Author' "$SETTINGS" && grep -q '^email=asked@example.invalid' "$SETTINGS" ||
  ui_fail "the author is not in the settings"
echo "ok   kept in the settings"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^Save As$'
ui_step "accept the name offered"        ui_key Return
ui_focus_main
ui_expect_new "Saved $DOCUMENTS/nogit/nogit.mitcad" "saved in the project, named after it"
ui_expect_new "Version recorded: nogit.mitcad " "recorded"
expect_git "$DOCUMENTS/nogit" "log --format=%s|%an|%ae" "Save nogit.mitcad|Asked Author|asked@example.invalid
Create project nogit|Asked Author|asked@example.invalid" "by the author given"
ui_key ctrl+q # exit
expect_exit "exited"

echo "--- Version History: the versions, what each changed, comparisons"
FILE=$PROJECT/plate.mitcad
# The file's versions as git sees them (the same linear history): their
# short ids by number, v1 the oldest.
version_id() {
  local count
  count=$(in_git "$PROJECT" log --follow --format=%H -- plate.mitcad | wc -l)
  in_git "$PROJECT" log --follow --format=%H -- plate.mitcad | sed -n "$((count - $1 + 1))p" | cut -c1-7
}
count=$(in_git "$PROJECT" log --follow --format=%H -- plate.mitcad | wc -l)
[ "$count" = 10 ] || ui_fail "plate.mitcad has $count versions, expected 10"
v8=$(version_id 8)
ui_start_app --open "$FILE" --set d3=36 --no-native-dialogs
ui_expect_log "Opened $FILE" "opened, with a change not saved"
ui_mark
ui_step "version history (Ctrl+Shift+H)" ui_key ctrl+shift+h
ui_expect_new "Version History dialog: plate.mitcad, reading its versions" "the window"
ui_expect_new "Version History: 10 versions of plate.mitcad: v10 $(version_id 10) Save plate.mitcad: Change d3 | v9 $(version_id 9) Rename block.mitcad to plate.mitcad (renamed from block.mitcad) | v8 " \
  "the versions newest first, through the rename"
ui_expect_new "Version History changes: v10 d3 20 mm -> 33 mm, 1 feature modified | v9 no changes | v8 d3 30 mm -> 20 mm" \
  "what each version changed against the one before"
ui_expect_new " | v1 first version" "the first version"
ui_expect_new "Version History selected v10 ($(version_id 10)): preview" "the latest selected, with its preview"
ui_expect_new "Version History compare v10 with v9: Changes from v9 ($(version_id 9)) to v10 ($(version_id 10)):" \
  "compared with the version before"
ui_expect_new "d3 (Extrude1 distance): 20 mm -> 33 mm" "the change of d3"
ui_focus_dialog '^Version History - plate.mitcad$'
ui_mark
ui_step "two versions back (Down Down)"  ui_key Down Down
ui_expect_new "Version History selected v8 ($v8)" "v8 selected"
ui_expect_new "Version History compare v8 with v7: " "compared with v7"
ui_expect_new "d3 (Extrude1 distance): 30 mm -> 20 mm" "what v8 changed"
ui_mark
ui_step "with the open design (Alt+D)"   ui_key alt+d
ui_expect_new "Version History compare v8 with the open design: Changes from v8 ($v8) to the open design (with its unsaved changes):" \
  "compared with the open design"
ui_expect_new "d3 (Extrude1 distance): 20 mm -> 36 mm" "the unsaved change of d3"
ui_mark
ui_step "compare the geometry (Alt+G)"   ui_key alt+g
ui_expect_new "Version History geometry v8 with the open design: Summary: d3 20 mm -> 36 mm" "both computed" 30
ui_expect_new "Geometry: |   Body1 (F2.b0): volume 48000 -> 86400 mm^3 (+38400, +80%)" "the body's volume compared"

echo "--- Version History: Save Copy As"
COPIES=$WORK/copies
mkdir -p "$COPIES"
ui_focus_dialog '^Version History - plate.mitcad$'
ui_mark
ui_step "save a copy (Alt+C)"            ui_key alt+c
ui_expect_new "Save Copy As dialog: plate v8.mitcad" "named after the version"
ui_focus_dialog '^Save Copy As$'
ui_step "a folder outside projects"      type_text "$COPIES/plate-v8.mitcad"
ui_key Return
ui_expect_new "Saved a copy of plate.mitcad v8 ($v8) as $COPIES/plate-v8.mitcad" "saved"
[ "$(file_d3 "$COPIES/plate-v8.mitcad")" = "[20.0]" ] || ui_fail "the copy has d3 $(file_d3 "$COPIES/plate-v8.mitcad")"
echo "ok   the copy is v8's design"
expect_commits "$PROJECT" 11 "a copy outside the project is no version of it"

echo "--- Version History: Restore, the unsaved change saved first"
ui_focus_dialog '^Version History - plate.mitcad$'
ui_mark
ui_step "restore (Alt+R)"                ui_key alt+r
ui_expect_new "Version History restore v8 ($v8)" "Restore chosen"
ui_expect_new "Restore dialog: v8 ($v8) of plate.mitcad (unsaved changes)" "asked, with the unsaved change"
ui_focus_dialog '^Restore Version$'
ui_step "save and restore (Enter)"       ui_key Return
ui_focus_main
ui_expect_new "Version recorded: plate.mitcad " "the unsaved change recorded first"
ui_expect_new "Version restored: plate.mitcad v8 ($v8) as v12 (" "v8 restored as a new version"
ui_expect_new "Opened $FILE" "the restored design opened"
ui_expect_new "Version status: bracket, main, v12" "the status bar shows it"
expect_git "$PROJECT" "log -2 --format=%s|%an" "Restore v8 of plate.mitcad ($v8)|Asked Author
Save plate.mitcad: Change d3|Asked Author" "the restore on top of the saved change"
[ "$(file_d3 "$FILE")" = "[20.0]" ] || ui_fail "the file has d3 $(file_d3 "$FILE")"
[ "$(in_git "$PROJECT" rev-parse HEAD:plate.mitcad)" = "$(in_git "$PROJECT" rev-parse "$v8:block.mitcad")" ] ||
  ui_fail "the restored file is not v8's"
echo "ok   the file is v8's again"
expect_clean "$PROJECT" "git status is clean"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Version unchanged: plate.mitcad (v12)" "nothing to save: the restored file is no change outside Mitcad"
no_new_since "Save conflict" "Save does not ask"

echo "--- Version History: Open an older version as a new design"
ui_mark
ui_step "version history (Ctrl+Shift+H)" ui_key ctrl+shift+h
ui_expect_new "Version History: 12 versions of plate.mitcad: v12 $(version_id 12) Restore v8 of plate.mitcad" "the restore listed"
ui_expect_new "Version History changes: v12 d3 36 mm -> 20 mm, 1 feature modified | v11 d3 33 mm -> 36 mm" \
  "what the restore changed"
ui_expect_new "Version History selected v12 ($(version_id 12)): preview" "the restored version has v8's preview"
ui_focus_dialog '^Version History - plate.mitcad$'
ui_mark
ui_step "the version before (Down)"      ui_key Down
ui_expect_new "Version History selected v11 ($(version_id 11))" "v11 selected"
ui_step "open (Alt+O)"                   ui_key alt+o
ui_focus_main
ui_expect_new "Opened version v11 ($(version_id 11)) of plate.mitcad as plate v11" "opened as a new design"
ui_expect_new "Version status: none" "untitled: no version history"
xdotool search --name '^plate v11 - Mitcad$' > /dev/null || ui_fail "the window is not titled plate v11"
echo "ok   the window is titled plate v11"
ui_mark
ui_step "version history (Ctrl+Shift+H)" ui_key ctrl+shift+h
ui_expect_new "Version History: no version history for plate v11" "an untitled design has none"
ui_focus_dialog '^Version History$'
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^Save As$'
ui_step "a name in the project"          type_path "$PROJECT/plate-v11.mitcad"
ui_expect_new "Saved $PROJECT/plate-v11.mitcad" "Save asks for a name"
ui_expect_new "Version recorded: plate-v11.mitcad " "a file of its own in the project"
[ "$(file_d3 "$PROJECT/plate-v11.mitcad")" = "[36.0]" ] || ui_fail "the file has d3 $(file_d3 "$PROJECT/plate-v11.mitcad")"
echo "ok   it is v11's design"
expect_git "$PROJECT" "log -1 --format=%s" "Save plate-v11.mitcad" "recorded"
expect_clean "$PROJECT" "git status is clean"
ui_key ctrl+q # exit
expect_exit "exited"

ui_finish "UI version test"

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files
# Autosave (P8) through the real UI, every 2 s (MITCAD_TEST_AUTOSAVE_SECONDS):
#   1. A changed document is written to the recovery folder in the user's
#      data ($XDG_DATA_HOME/Mitcad/Mitcad/autosave): the project file and
#      its metadata, under the session's lock. An unchanged one is not
#      written again; undone back to the state saved, its files go; an
#      open edit that rolled the timeline back is written with the marker
#      it restores; a normal end removes everything.
#   2. MITCAD_AUTOSAVE_DIR moves the folder. Save As removes the files and
#      a change after it is written with the file's path and digest. Another
#      instance does not offer the session of one that runs. Killed, the
#      session leaves its files and lock. A second instance (--no-recovery:
#      nothing asked) writes a session of its own and leaves the first's;
#      while it opens a design on the worker, autosave waits.
#   3. Recovery (P8c): at start-up the dialog offers the killed session;
#      Later keeps it. File > Recover Documents offers it again, telling
#      that its file changed since; Restore opens it modified, as that
#      file, and this session writes it and removes the earlier session's
#      files. Save over the changed file asks: Cancel writes nothing,
#      Overwrite writes. A lock left without files goes; a damaged session
#      can only be discarded; an untitled one is restored untitled, and a
#      normal end leaves nothing.
#   4. Preferences, General: autosave off in the settings writes nothing;
#      switched on, it writes; switched off again, it is saved off.
#
# Runs headless on Xvfb (see ui-test-lib.sh, which gives every test a data
# folder of its own; UI_RECOVERY=1 lets the app offer recovery at start-up).
# The app uses Qt's own file dialog (--no-native-dialogs), so a path can be
# typed into it.
# Usage: tools/ui-autosave-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-autosave.XXXXXX)
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_TEST_AUTOSAVE_SECONDS=2
FOLDER=$XDG_DATA_HOME/Mitcad/Mitcad/autosave

# pair_value folder session expression: evaluates a Python expression on a
# session's metadata (meta), its project file (doc) and their names in the
# folder; ok tells whether the metadata's size and digest are the project
# file's. Session "-": the folder's only one. fnv(path) is the digest of a
# file.
pair_value() {
  python3 - "$1" "$2" "$3" << 'EOF'
import glob, json, os, sys

folder, session, expression = sys.argv[1:4]


def fnv_bytes(data):
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return '%016x' % value


def fnv(path):
    with open(path, 'rb') as f:
        return fnv_bytes(f.read())


if session == '-':
    found = glob.glob(os.path.join(folder, '*.json'))
    if len(found) != 1:
        sys.exit('%d sessions in %s' % (len(found), folder))
    session = os.path.basename(found[0])[:-len('.json')]
with open(os.path.join(folder, session + '.json')) as f:
    meta = json.load(f)
with open(os.path.join(folder, meta['project']), 'rb') as f:
    data = f.read()
doc = json.loads(data)
ok = meta['size'] == len(data) and meta['digest'] == fnv_bytes(data)
print(eval(expression))
EOF
}

# expect_pair folder session expression expected description
expect_pair() {
  local actual
  actual=$(pair_value "$1" "$2" "$3" 2>&1) || ui_fail "$5: $actual"
  [ "$actual" = "$4" ] || ui_fail "$5: $3 is '$actual', expected '$4'"
  echo "ok   $5"
}

# expect_files folder description pattern...: the folder holds files
# matching exactly these patterns (one each), and nothing else.
expect_files() {
  local folder=$1 description=$2 count
  shift 2
  count=$(find "$folder" -mindepth 1 2> /dev/null | wc -l)
  [ "$count" = $# ] || ui_fail "$description: $count files in $folder: $(ls -A "$folder" 2> /dev/null)"
  for pattern in "$@"; do
    compgen -G "$folder/$pattern" > /dev/null || ui_fail "$description: no $pattern in $folder"
  done
  echo "ok   $description"
}

# no_new "text" description seconds: nothing logged with the text meanwhile.
no_new() {
  ui_mark
  sleep "$3"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

focus_dialog() { ui_focus_dialog "$1"; }

# type_path title path: the path typed into Qt's file dialog (ui_type_path), accepted.
type_path() {
  ui_type_path "$1" "$2"
  ui_focus_main
}

# expect_exit description: waits for the app to quit normally.
expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  if command -v gdb > /dev/null; then
    grep -q "exited normally" "$UI_LOG" || ui_fail "$1: abnormal exit"
  else
    wait "$UI_RUNNER" || ui_fail "$1: exit code $?"
  fi
  UI_RUNNER=""
  echo "ok   $1"
}

d3_of() { echo "[p['value'] for p in doc['parameters'] if p['name'] == 'd3']"; }

# file_d3 project [value]: prints d3 of a project file, or sets it (as
# another program would change the file).
file_d3() {
  python3 - "$@" << 'EOF'
import json, sys

doc = json.load(open(sys.argv[1]))
values = [p for p in doc['parameters'] if p['name'] == 'd3']
if len(sys.argv) > 2:
    values[0]['value'] = float(sys.argv[2])
    json.dump(doc, open(sys.argv[1], 'w'))
else:
    print([p['value'] for p in values])
EOF
}

expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(xdotool getwindowname "$UI_WINDOW" 2> /dev/null)
    [ "$title" = "$1" ] && { echo "ok   title '$1'"; return; }
    sleep 0.2
  done
  ui_fail "window title is '$title', expected '$1'"
}

# no_new_since "text" description: nothing logged with the text since the mark.
no_new_since() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

ui_start_display

echo "--- A change is written to the user's recovery folder"
ui_start_app --demo --set d3=30
ui_expect_log "Autosave every 2 s to $FOLDER" "autosave every 2 s in the user's data"
ui_expect_log "Autosaved Untitled: " "the changed document was written"
expect_files "$FOLDER" "a project file, its metadata and the lock" '*.mitcad' '*.json' '*.lock'
expect_pair "$FOLDER" - 'meta["format"], meta["version"], meta["document"], meta["path"], meta["base_digest"], ok' \
  "('mitcad-autosave', 2, 'Untitled', '', '', True)" "the metadata describes the project file"
expect_pair "$FOLDER" - 'meta["project"].startswith(meta["session"] + ".") and meta["project"].endswith(".mitcad"), os.path.exists(os.path.join(folder, meta["session"] + ".lock"))' \
  "(True, True)" "the files are named by the session"
expect_pair "$FOLDER" - 'os.path.exists("/proc/%d" % meta["pid"]), meta["saved_at"].endswith("Z"), meta["application"]' \
  "(True, True, 'Mitcad')" "the process and the time"
expect_pair "$FOLDER" - "$(d3_of)" "[30.0]" "the project file has the change"
no_new "Autosaved" "an unchanged document is not written again" 5
ui_mark
ui_step "undo the change (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Autosave removed for Untitled" "undone to the state saved, its files go"
expect_files "$FOLDER" "only the session's lock is left" '*.lock'
ui_mark
ui_step "redo it (Ctrl+Y)"               ui_key ctrl+y
ui_expect_new "Autosaved Untitled: " "redone, it is written again"
expect_pair "$FOLDER" - "$(d3_of)" "[30.0]" "with the change"

echo "--- An open edit is written as it will be"
ui_expect_log "Timeline Extrude1 at" "the timeline is logged"
ui_mark
ui_step "double-click Extrude1"          ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "editing Extrude1 rolls the timeline back"
ui_expect_new "Autosaved Untitled: " "the rolled-back document is written"
expect_pair "$FOLDER" - '"marker" in doc, len(doc["features"])' "(False, 2)" \
  "with the marker at the end, as the edit restores it"
ui_step "cancel the edit (Esc)"          ui_key Escape
ui_expect_new "Command Extrude cancelled" "the edit cancelled"

echo "--- A normal end removes the session's files"
ui_step "exit (Ctrl+Q)"                  ui_key ctrl+q
focus_dialog '^Mitcad$'
ui_key d # Don't Save: the app ends
expect_exit "exited without saving"
grep -qF "Autosave removed for Untitled" "$UI_LOG" || ui_fail "the files were not removed"
expect_files "$FOLDER" "nothing is left in the folder"

echo "--- MITCAD_AUTOSAVE_DIR; Save As; a change after it"
export MITCAD_AUTOSAVE_DIR=$WORK/autosave
FOLDER=$MITCAD_AUTOSAVE_DIR
ui_start_app --demo --set d3=30 --no-native-dialogs
ui_expect_log "Autosave every 2 s to $FOLDER" "autosave in the folder given"
ui_expect_log "Autosaved Untitled: " "the changed document was written"
first=$(pair_value "$FOLDER" - 'meta["session"]') || ui_fail "no session: $first"
ui_mark
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "type the path, Enter"           type_path "Save As" "$FILE"
ui_expect_new "Saved $FILE" "saved"
ui_expect_new "Autosave removed for Untitled" "saving removes the files"
expect_files "$FOLDER" "only the session's lock is left" "$first.lock"
ui_mark
ui_step "undo the change (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Autosaved block.mitcad: " "the change after saving was written"
expect_pair "$FOLDER" "$first" 'meta["document"], meta["path"], meta["base_digest"] == fnv(meta["path"])' \
  "('block.mitcad', '$FILE', True)" "with the file's path and digest as saved"
expect_pair "$FOLDER" "$first" "$(d3_of)" "[20.0]" "the change: d3 back to 20"
first_snapshot=$(pair_value "$FOLDER" "$first" 'meta["project"]') || ui_fail "no snapshot"
checksum=$(sha256sum "$FOLDER/$first.json" "$FOLDER/$first_snapshot")

echo "--- Another instance does not offer a running one's session"
"$UI_APP" > "$WORK/other.log" 2>&1 &
other=$!
for _ in $(seq 1 150); do
  grep -q "Recovery: " "$WORK/other.log" && break
  sleep 0.2
done
kill -KILL "$other" 2> /dev/null
wait "$other" 2> /dev/null
grep -qF "Recovery: none" "$WORK/other.log" || { cat "$WORK/other.log"; ui_fail "the running session was offered"; }
echo "ok   nothing to recover while the session runs"
[ "$(sha256sum "$FOLDER/$first.json" "$FOLDER/$first_snapshot")" = "$checksum" ] ||
  ui_fail "the running session's files changed"
expect_files "$FOLDER" "its files and lock are as they were" "$first.lock" "$first.json" "$first_snapshot"

echo "--- Killed, the session's files stay; a second session"
ui_stop_app
expect_files "$FOLDER" "the killed session's files and lock stay" "$first.lock" "$first.json" "$first_snapshot"
# The second instance opens the design on the worker, slowly: autosave
# waits for it.
export MITCAD_TEST_RECOMPUTE_DELAY_MS=2500
ui_start_app --open "$FILE" --set d3=25 # --no-recovery (ui_start_app)
unset MITCAD_TEST_RECOMPUTE_DELAY_MS
ui_expect_log "Autosave skipped: busy" "autosave waited while the design was computed"
ui_expect_log "Opened $FILE" "opened"
ui_expect_log "Autosaved block.mitcad: " "the second session's change was written"
grep -q "Recovery: " "$UI_LOG" && ui_fail "--no-recovery: recovery was offered"
echo "ok   --no-recovery offers nothing"
second=$(find "$FOLDER" -name '*.json' ! -name "$first.json" -printf '%f\n' | sed 's/\.json$//')
[ -n "$second" ] && [ "$second" != "$first" ] || ui_fail "no second session in $FOLDER"
echo "ok   a session of its own: $second"
expect_pair "$FOLDER" "$second" "$(d3_of)" "[25.0]" "with its change"
[ "$(sha256sum "$FOLDER/$first.json" "$FOLDER/$first_snapshot")" = "$checksum" ] ||
  ui_fail "the first session's files changed"
echo "ok   the first session's files are as they were"
ui_step "exit (Ctrl+Q)"                  ui_key ctrl+q
focus_dialog '^Mitcad$'
ui_key d # Don't Save: the app ends
expect_exit "the second session ended"
expect_files "$FOLDER" "only the killed session's files are left" "$first.lock" "$first.json" "$first_snapshot"
expect_pair "$FOLDER" "$first" 'ok' "True" "and they are a complete pair"
base=$(pair_value "$FOLDER" "$first" 'meta["base_digest"]') || ui_fail "no base digest: $base"

echo "--- Recovery at start-up: Later"
UI_RECOVERY=1 ui_start_app
ui_expect_log "Recovery: 1 document(s) found" "the killed session is found"
ui_expect_log "Recoverable block.mitcad: $FILE, autosaved " "with its file and time"
grep -F "Recoverable block.mitcad" "$UI_LOG" | grep -qF "changed since" && ui_fail "the file has not changed"
focus_dialog '^Recover Unsaved Work$'
ui_mark
ui_step "later (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Recovery: later, 1 document(s) kept" "Later keeps it"
expect_title "Untitled - Mitcad"
expect_files "$FOLDER" "its files stay, unlocked" "$first.json" "$first_snapshot"

echo "--- File > Recover Documents: the file changed since; Restore"
file_d3 "$FILE" 40 # another program changes the file
changed=$(sha256sum "$FILE")
ui_mark
ui_step "recover documents (search)"     ui_command "Recover Documents"
ui_expect_new "Recovery: 1 document(s) found" "offered again"
ui_expect_new "Recoverable block.mitcad: $FILE, autosaved " "the session"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Recoverable block.mitcad" | grep -qF ", the file has changed since" ||
  ui_fail "the dialog does not tell that the file changed"
echo "ok   the file has changed since"
focus_dialog '^Recover Unsaved Work$'
ui_step "restore (Enter)"                ui_key Return
ui_focus_main
ui_expect_new "Recovered block.mitcad from autosave of " "restored"
expect_title "block.mitcad* - Mitcad"
ui_expect_new "Autosaved block.mitcad: " "this session wrote it"
ui_expect_new "Recovered session $first removed" "and removed the killed session's files"
third=$(pair_value "$FOLDER" - 'meta["session"]') || ui_fail "no session: $third"
expect_files "$FOLDER" "only this session's files are left" "$third.lock" "$third.json" "$third.*.mitcad"
expect_pair "$FOLDER" "$third" "meta['document'], meta['path'], meta['base_digest'] == '$base'" \
  "('block.mitcad', '$FILE', True)" "as its file, with the digest the file was opened with"
expect_pair "$FOLDER" "$third" "$(d3_of)" "[20.0]" "with the recovered change"

echo "--- Save over the changed file asks"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
focus_dialog '^Mitcad$'
ui_expect_new "Save conflict: $FILE changed since it was opened" "Save tells that the file changed"
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Save conflict: cancelled" "cancelled"
[ "$(sha256sum "$FILE")" = "$changed" ] || ui_fail "the file was written"
echo "ok   the file is as the other program left it"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
focus_dialog '^Mitcad$'
ui_step "overwrite (O)"                  ui_key o
ui_focus_main
ui_expect_new "Save conflict: overwrite" "overwrite"
ui_expect_new "Saved $FILE" "saved over it"
ui_expect_new "Autosave removed for block.mitcad" "saving removed this session's files"
expect_title "block.mitcad - Mitcad"
[ "$(file_d3 "$FILE")" = "[20.0]" ] || ui_fail "the file has d3 $(file_d3 "$FILE")"
echo "ok   the file has the recovered change"
expect_files "$FOLDER" "only this session's lock is left" "$third.lock"
ui_mark
ui_step "save again (Ctrl+S)"            ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved again"
no_new_since "Save conflict" "without asking again"

echo "--- A lock without files goes; a damaged session; an untitled one"
ui_stop_app
expect_files "$FOLDER" "killed when saved, the session left its lock" "$third.lock"
ui_start_app --demo --set d3=33
ui_expect_log "Autosaved Untitled: " "an untitled change was written"
fourth=$(pair_value "$FOLDER" - 'meta["session"]') || ui_fail "no session: $fourth"
ui_stop_app
# A session whose project file is not the one its metadata describes.
broken=$(python3 -c 'import uuid; print(uuid.uuid4())')
python3 - "$FOLDER" "$broken" << 'EOF'
import json, os, sys

folder, session = sys.argv[1:3]
with open(os.path.join(folder, session + '.mitcad'), 'w') as f:
    f.write('{"format": "mitcad"}')
meta = {'format': 'mitcad-autosave', 'version': 1, 'session': session, 'document': 'broken.mitcad',
        'path': '', 'base_digest': '', 'saved_at': '2026-01-01T00:00:00Z', 'revision': 3,
        'project': session + '.mitcad', 'size': 1, 'digest': '0000000000000000'}
with open(os.path.join(folder, session + '.json'), 'w') as f:
    json.dump(meta, f)
EOF
UI_RECOVERY=1 ui_start_app
ui_expect_log "Recovery: removed the lock of session $third" "the lock left without files went"
ui_expect_log "Recovery: 2 document(s) found" "two sessions found"
ui_expect_log "Recoverable Untitled: never saved, autosaved " "the untitled one"
ui_expect_log "Recoverable broken.mitcad: never saved, autosaved 2026-01-01T00:00:00Z, 1 bytes, damaged: the project file is not the one its metadata describes" \
  "the damaged one, older"
focus_dialog '^Recover Unsaved Work$'
ui_step "select the damaged one (Down)"  ui_key Down
ui_step "discard (Alt+D)"                ui_key alt+d
focus_dialog '^Discard$'
ui_mark
ui_step "confirm (D)"                    ui_key d
ui_expect_new "Discarded recovery of broken.mitcad" "the damaged one discarded"
[ -e "$FOLDER/$broken.json" ] || [ -e "$FOLDER/$broken.mitcad" ] || [ -e "$FOLDER/$broken.lock" ] &&
  ui_fail "the damaged session's files are left"
echo "ok   its files went"
focus_dialog '^Recover Unsaved Work$'
ui_step "restore the untitled one (Enter)" ui_key Return
ui_focus_main
ui_expect_new "Recovered Untitled from autosave of " "the untitled one restored"
expect_title "Untitled* - Mitcad"
ui_expect_new "Recovered session $fourth removed" "this session took it over"
expect_pair "$FOLDER" - 'meta["document"], meta["path"], meta["session"] != "'"$fourth"'"' \
  "('Untitled', '', True)" "untitled, in this session"
expect_pair "$FOLDER" - "$(d3_of)" "[33.0]" "with its change"
ui_step "exit (Ctrl+Q)"                  ui_key ctrl+q
focus_dialog '^Mitcad$'
ui_key d # Don't Save: the app ends
expect_exit "exited without saving"
expect_files "$FOLDER" "nothing is left in the folder"
unset MITCAD_AUTOSAVE_DIR
FOLDER=$XDG_DATA_HOME/Mitcad/Mitcad/autosave

echo "--- Preferences: General"
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[autosave]\nenabled=false\nminutes=7\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
ui_start_app --demo --set d3=30
ui_expect_log "Autosave off" "autosave off in the settings"
no_new "Autosaved" "nothing is written" 4
ui_mark
ui_step "preferences, General (search)"  ui_command "Preferences: General"
ui_focus_dialog '^Preferences$'
ui_step "switch autosave on (Alt+S)"     ui_key alt+s
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Preferences: navigation " "Preferences saved"
ui_expect_new ", autosave every 7 min" "autosave on, every 7 minutes"
ui_expect_new "Autosave every 2 s to $FOLDER" "the timer started (2 s for the test)"
ui_expect_new "Autosaved Untitled: " "the changed document was written"
grep -q '^enabled=true' "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf" || ui_fail "autosave is not saved on"
grep -q '^minutes=7' "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf" || ui_fail "the minutes are not kept"
echo "ok   saved on, the minutes kept"
ui_mark
ui_step "preferences, General (search)"  ui_command "Preferences: General"
ui_focus_dialog '^Preferences$'
ui_step "switch autosave off (Alt+S)"    ui_key alt+s
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new ", autosave off" "autosave off"
ui_expect_new "Autosave off" "the timer stopped"
grep -q '^enabled=false' "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf" || ui_fail "autosave is not saved off"
echo "ok   saved off"
ui_step "exit (Ctrl+Q)"                  ui_key ctrl+q
focus_dialog '^Mitcad$'
ui_key d # Don't Save: the app ends
expect_exit "exited without saving"
expect_files "$FOLDER" "nothing is left in the folder"

ui_finish "UI autosave test"

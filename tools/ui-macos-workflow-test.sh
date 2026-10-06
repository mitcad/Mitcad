#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# macOS workflow test: the counterpart of tools/ui-windows-workflow-test.ps1
# (the basic workflows through the UI: sketch, extrude, fillet, autosave and
# recovery, save, open, STEP export and import, version history), with the
# same assertions. The input comes from the application itself: the in-process
# test driver (app/framework/TestDriver.hpp) runs a script of key presses,
# clicks and menu commands, synthesised as Qt events, so no Accessibility
# permission is needed and the test runs over SSH. The scripts are in
# tools/ui-scripts/, in three runs of the application because a script cannot
# start it again:
#   workflow.mitcad-ui           sketch, extrude, fillet, autosave; ends the
#                                application as a kill would (no clean-up)
#   workflow-recovery.mitcad-ui  the killed session is offered and restored;
#                                Save As, New, Open, Export, Import, undo
#   workflow-versions.mitcad-ui  the saved design opened with a change:
#                                version history, two versions, one restored
# This script checks what lies outside the application between and after the
# runs: the autosave files the killed session left (their metadata, the
# project file's features and the lock), git's log of the versions.
#
# The window opens, so a window server is needed (see ui-macos-test.sh).
# Exit status: 0 passed, 1 failed, 2 wrong usage or environment, 77 skipped
# (no window server).
#
# Usage: tools/ui-macos-workflow-test.sh <app> [out directory]
#   <app>  .../mitcad.app/Contents/MacOS/mitcad
#   <out>  logs and the test's files (default: ui-macos-workflow in the
#          current directory)
# Environment: UI_WORKFLOW_TIMEOUT seconds per run of the application (180);
# MITCAD_UI_REQUIRE_GUI and MITCAD_UI_LLDB as in ui-macos-test.sh;
# MITCAD_TEST_STEP_MS and MITCAD_TEST_SETTLE_MS slow the driver down
# (TestDriver.hpp).

set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
scripts=$here/ui-scripts

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-workflow}"
UI_TIMEOUT=${UI_WORKFLOW_TIMEOUT:-180}

# A daemon's session (launchd's System domain) has no window server at all.
# An SSH login says Background and can still work: the run decides.
if command -v launchctl > /dev/null; then
  session=$(launchctl managername 2> /dev/null || true)
  echo "     session: ${session:-unknown}"
  if [ "$session" = System ] && [ "${MITCAD_UI_REQUIRE_GUI:-0}" != 1 ]; then
    echo "SKIP: no window server in the System session"
    exit 77
  fi
fi

ui_finish() {
  echo "Logs and files: $UI_OUT"
  if [ "$UI_FAILURES" -gt 0 ]; then
    echo "FAIL: UI macOS workflow test ($UI_FAILURES failures)"
    exit 1
  fi
  echo 'PASS: UI macOS workflow test'
  exit 0
}

# run_script name script app-arguments...: runs the application with the
# driver's script; the script's steps assert the UI (the driver exits with 1
# and logs "TestDriver: FAIL line n: ..." at the first that fails). The test
# ends at the first failed run, as the next one needs its files.
run_script() {
  local name=$1 script=$2
  shift 2
  MITCAD_TEST_INPUT=$scripts/$script ui_run_app "$name" "$@"
  ui_expect_no_log 'TestDriver: FAIL' "$name: no step of the script failed"
  ui_expect_no_log 'OCCT view event failed' "$name: no failed view operation"
  if [ "$UI_FAILURES" -gt 0 ]; then ui_finish; fi
}

# The test's own folder for its files, as in the Windows test; the app logs
# paths with forward slashes, as here.
work=$UI_OUT/files
rm -rf "$work"
mkdir -p "$work"
export MITCAD_TEST_WORK=$work
# Autosave every 2 s for the test.
export MITCAD_TEST_AUTOSAVE_SECONDS=2
# Qt's own file dialogs, which the script types into (the platform's are
# not Qt widgets); no recovery offered unless a run asks for it.
dialogs=--no-native-dialogs

echo '--- Sketch, extrude, fillet, autosave'
run_script workflow workflow.mitcad-ui "$dialogs" --no-recovery
ui_expect_log 'TestDriver: kill' 'the script ran to its end, which killed the application'
ui_expect_log 'Added rectangle 40 x 25 mm at \(0, 0\)' 'rectangle 40 x 25 at the origin'
ui_expect_log 'Added extrude \(New Body\), distance 20' 'extrusion built'
ui_expect_log 'Added fillet on 4 edge\(s\)' "fillet on the top face's 4 edges"
ui_expect_log 'Autosaved Untitled: ' 'the changed design was autosaved'
if [ "$UI_FAILURES" -gt 0 ]; then ui_finish; fi

# The files the killed session left: one session with its metadata, the
# project file with the three features, the lock.
cat > "$UI_OUT/check-autosave.py" << 'EOF'
import glob, json, os, sys

folder = sys.argv[1]
found = sorted(glob.glob(os.path.join(folder, '*.json')))
if len(found) != 1:
    sys.exit('%d autosave sessions in %s' % (len(found), folder))
about = json.load(open(found[0]))
if about.get('format') != 'mitcad-autosave' or about.get('version') != 1 or about.get('document') != 'Untitled':
    sys.exit('autosave metadata: %s' % json.dumps(about))
project = os.path.join(folder, about['project'])
if not os.path.exists(project) or os.path.getsize(project) != about['size']:
    sys.exit('the autosaved project file is not the size its metadata gives (%s)' % about.get('size'))
features = [f['type'] for f in json.load(open(project))['features']]
if features != ['sketch', 'extrude', 'fillet']:
    sys.exit('autosaved features: %s' % ','.join(features))
if not os.path.exists(os.path.join(folder, about['session'] + '.lock')):
    sys.exit('no lock of the session')
print(about['session'])
EOF
if session_id=$(python3 "$UI_OUT/check-autosave.py" "$MITCAD_AUTOSAVE_DIR" 2> "$UI_OUT/check-autosave.err"); then
  ui_pass "the project file with the fillet, its metadata and the lock (session $session_id)"
else
  ui_fail "autosave: $(cat "$UI_OUT/check-autosave.err")"
  ui_finish
fi
# What the first script measured, for the next: the filleted volume.
filleted=$(ui_log_value 'Body Body1 (F2\.b0): volume 20000\.000 -> \([0-9.][0-9.]*\) mm3')
if [ -z "$filleted" ]; then
  ui_fail 'no volume of the filleted body in the log'
  ui_finish
fi
export MITCAD_TEST_SESSION=$session_id
export MITCAD_TEST_FILLETED=$filleted

echo '--- Recovery, save, open, export and import'
# Started again, the application finds the killed session's lock stale (the
# process is gone) and offers the design.
run_script recovery workflow-recovery.mitcad-ui "$dialogs"
ui_expect_log 'Recovered Untitled from autosave of ' 'the killed session was restored'
ui_expect_log "Saved .*/block\\.mitcad" 'saved through the dialog'
ui_expect_log 'Imported block\.step as' 'the STEP came in as a base feature'
ui_expect_log 'TestDriver: done' 'the script ran to its end'
if [ ! -s "$work/block.mitcad" ] || [ ! -s "$work/block.step" ]; then
  ui_fail 'no project file or STEP file'
  ui_finish
fi

echo '--- Version history'
# Git's configuration is the test's own (ui_init), without a user: the author
# is asked once and kept in the test's settings, never the machine's identity.
run_script versions workflow-versions.mitcad-ui "$dialogs" --no-recovery --open "$work/block.mitcad" --set d3=25
ui_expect_log 'Version restored: block\.mitcad v1 \(' 'the first version restored as a new one'
ui_expect_log 'TestDriver: done' 'the script ran to its end'
if command -v git > /dev/null; then
  author='macOS Tester|macos@example.invalid'
  log=$(git -C "$work" log --format='%s|%an|%ae' 2>&1)
  expected="Save block.mitcad: Undo Change d3|$author
Save block.mitcad: Change d3|$author
Create project files|$author"
  # The restore is the newest version: "Restore v1 of block.mitcad (hash)".
  newest=$(printf '%s\n' "$log" | sed -n 1p)
  older=$(printf '%s\n' "$log" | sed -n '2,$p')
  if printf '%s' "$newest" | grep -Eq '^Restore v1 of block\.mitcad \([0-9a-f]{7}\)\|macOS Tester\|'; then
    ui_pass "git's log has the restore"
  else
    ui_fail "git log, newest: $newest"
  fi
  if [ "$older" = "$expected" ]; then
    ui_pass "git's log has the three versions before it"
  else
    ui_fail "git log: $(printf '%s' "$older" | tr '\n' '/')"
  fi
  status=$(git -C "$work" status --porcelain 2>&1)
  # The STEP file of the export is the user's, not a version's.
  if [ "$status" = '?? block.step' ]; then
    ui_pass 'git status shows only the file Mitcad does not record'
  else
    ui_fail "git status: $(printf '%s' "$status" | tr '\n' '/')"
  fi
else
  echo 'note: git is not installed; the versions are checked through the log only'
fi

ui_finish

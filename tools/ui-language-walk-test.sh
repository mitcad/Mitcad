#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/translations app/framework/Language.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/TestDriverWalk.cpp app/commands app/files/FileCommands.cpp
# check-all sources: tools/ui-scripts/language-walk.mitcad-ui
# The Finnish interface walked (mitcad#1473): MitCad started with
# --language fi and MITCAD_LOG_UNTRANSLATED=1 runs
# tools/ui-scripts/language-walk.mitcad-ui (every menu filled, the Browser's
# context menus, every command opened and closed again, the sketch's and a
# drawing's commands), and fails on each text of MitCad's it shows that no
# catalogue translated ("Untranslated: <context>: <text>") or that was found
# only without its context ("Fallback: ..."; docs/translations.md,
# "Contexts"). Qt's own texts that Qt's Finnish catalogue lacks (its file
# dialog's, its key names) are not MitCad's and are left out; the labels of
# its file dialog that Qt's catalogue has must be Finnish (mitcad#1674).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-language-walk-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_LOG_UNTRANSLATED=1
export MITCAD_TEST_INPUT=$here/ui-scripts/language-walk.mitcad-ui

# The contexts of Qt's own texts.
QT_OWN='^(Untranslated|Fallback): (QShortcut|QKeySequence|QFileDialog|QFileSystemModel|QAbstractFileIconProvider|QProgressBar|QGuiApplication|QPlatformTheme|QDialogButtonBox|QMessageBox|QLineEdit|QWidgetTextControl|QUnicodeControlCharacterMenu|QScrollBar|QColorDialog|QInputDialog|QSpinBox|QAbstractSpinBox|QCalendarWidget|QToolButton|QDockWidget|QMdiSubWindow|QTabBar|QComboBox|QErrorMessage|QSystemTrayIcon|QAccessibleButton|QAccessibleActionInterface|QPlainTextEdit|QTextControl): '

ui_start_display
ui_start_app --ui=qt --no-mcp --language=fi
for _ in $(seq 1 9000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the walk timed out"
wait "$UI_RUNNER" || ui_fail "the walk failed: $(grep -E 'TestDriver' "$UI_LOG" | tail -8)"
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the steps of language-walk"
grep -E '^Walk: [0-9]+ (menus|rows|commands)$' "$UI_LOG" | sed 's/^/     /'
grep -qE '^Walk: [0-9]+ commands$' "$UI_LOG" || ui_fail "no command was walked"

# Each text with the step that showed it.
found=$(awk -v own="$QT_OWN" '
  /^Walk: (command|context menu|menu )/ { where = $0; next }
  /^TestDriver: line / { where = $0; next }
  /^(Untranslated|Fallback): / && $0 !~ own { print "     " $0 "  (" where ")" }
' "$UI_LOG")
if [ -n "$found" ]; then
  echo "$found"
  ui_fail "$(printf '%s\n' "$found" | wc -l) text(s) of MitCad not translated or found without their context"
fi
echo "ok   every text the walk showed is translated in its context"

# Qt's file dialog (Open, Save As) takes its labels from Qt's own catalogue
# (qtbase_fi.qm), which has them: one of them left English means that
# catalogue was not there or not loaded (mitcad#1674).
qt_dialog=$(grep -E '^Untranslated: QFileDialog: (Look in:|Files of type:|File &name:)' "$UI_LOG" | sort -u)
[ -z "$qt_dialog" ] || ui_fail "Qt's file dialog is not in Finnish (qtbase_fi.qm): $qt_dialog"
echo "ok   no label of Qt's file dialog left English"

ui_finish 'UI language walk test'

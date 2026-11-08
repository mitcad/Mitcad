#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Language.cpp app/framework/Language.hpp app/framework/CommandSearch.cpp
# check-all sources: app/framework/Ribbon.cpp app/main.cpp app/view/ViewController.cpp app/translations cmake/Translations.cmake
# check-all sources: tools/translations app/framework/TestDriver.cpp tools/ui-scripts/language-fi.mitcad-ui
# check-all sources: tools/ui-scripts/language-en.mitcad-ui tools/ui-scripts/language-pseudo.mitcad-ui
# The interface language (mitcad#1460) through the real UI. Four starts:
#   1. default: as every UI test starts MitCad (the test library sets
#      MITCAD_LANGUAGE=en): English (tools/ui-scripts/language-en.mitcad-ui).
#   2. en: --language en over a stored Finnish preference: English.
#   3. fi: --language fi: the tabs in Finnish, the command search finds a
#      command by its English name, Preferences › General › Language set to
#      English is saved for the next start (language-fi.mitcad-ui).
#   4. pseudo: --language xx, the tests' pseudo-language, where the build
#      has it (a test build): every text marked and longer, no group
#      caption cut (language-pseudo.mitcad-ui).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-language-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

# run name script [arguments...]: one start of MitCad with the steps of
# tools/ui-scripts/<script>.mitcad-ui; the app ends with them.
run() {
  local name=$1 script=$2
  shift 2
  export MITCAD_TEST_INPUT=$here/ui-scripts/$script.mitcad-ui
  echo "--- $name"
  ui_start_app --ui=qt --no-mcp "$@"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the language driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$name: the language driver failed: $(grep -E 'TestDriver|Language:' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$name: the steps of $script"
  grep -E '^Language: |^Preferences: language ' "$UI_LOG" | sed 's/^/     /'
}

ui_start_display

[ "${MITCAD_LANGUAGE:-}" = en ] || ui_fail "the test library does not set MITCAD_LANGUAGE=en"
export MITCAD_TEST_LANGUAGE_REASON=MITCAD_LANGUAGE
run default language-en
echo "ok   default: English, as every UI test starts"

mkdir -p "$(dirname "$CONF")"
printf '[interface]\nlanguage=fi\n' > "$CONF"
export MITCAD_TEST_LANGUAGE_REASON=--language
run en language-en --language=en
echo "ok   en: --language en over a stored Finnish preference"

run fi language-fi --language=fi
grep -q '^language=en$' "$CONF" || ui_fail "fi: English is not saved as interface/language"
echo "ok   fi: Finnish tabs, the command search by the English name, the preference saved"

translations=$(dirname "$UI_APP")/translations
if [ -f "$translations/mitcad_xx.qm" ]; then
  run pseudo language-pseudo --language=xx
  echo "ok   pseudo: every text marked and longer, no group caption cut"
else
  echo "skip pseudo: no $translations/mitcad_xx.qm (not a test build)"
fi

ui_finish 'UI language test'

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BoostButton.cpp app/framework/Ribbon.cpp app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: tools/cli tools/ui-scripts/boost-button.mitcad-ui tools/ui-scripts/boost-button.json
# check-all sources: app/framework/DiscordButton.cpp app/framework/DiscordButton.hpp app/icons/community.svg
# check-all sources: app/resources/layout/default.json
# The Boost development button (mitcad#1990) in the Qt frontend on macOS,
# as tools/ui-boost-button-test.sh on Linux: the shared steps
# tools/ui-scripts/boost-button.mitcad-ui on a box that mitcad-cli makes
# (tools/ui-scripts/boost-button.json), in the Docked layout only (the
# tabs' row's right end; the Floating layout's macOS toolbar row has no
# Boost button): the button shown, a click logs the Ko-fi link (the tests'
# URL log, no browser), its × hides it and saves it, the Settings window's
# Display › Show the Boost development button brings it back; the Discord
# button right of it in the tab row (mitcad#2123): a click logs MitCad's
# Discord link, it stays while Boost is hidden, Help › MitCad on Discord
# logs the same link.
# Usage: tools/ui-macos-boost-button-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-boost-button-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/boost.mitcad
"$cli" run "$here/ui-scripts/boost-button.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run boost-button.json' >&2; exit 1; }
MITCAD_TEST_LOG_URLS=1 MITCAD_TEST_INPUT=$here/ui-scripts/boost-button.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app boost-button-docked --ui=qt --no-recovery --no-mcp --chrome=docked --open "$design"
ui_expect_log '^Boost button shown$' 'the button shown'
ui_expect_log '^Boost link opened: https://ko-fi[.]com/mitcad$' 'a click opens the Ko-fi page'
ui_expect_log '^Open URL [(]test, not opened[)]: https://ko-fi[.]com/mitcad$' 'only logged in the test'
ui_expect_log '^Boost button hidden$' 'the × hides it'
ui_expect_log '^Preferences: Boost development button shown$' 'the Settings window shows it again'
# The shared Qt UI's Docked top bar (mitcad#2082): the Discord button right
# of the Boost button and left of the project indicator (mitcad#2123), from
# the parts logged last while Boost showed, as on Linux.
shown=$(grep -E '^Docked top bar: .*; titleBoost ' "$UI_LOG_FILE" | tail -1)
part() { tr ';' '
' <<< "${shown#*; }" | sed 's/^ *//' | awk -v name="$1" '$1 == name { print $2 }'; }
boost=$(part titleBoost)
discord=$(part titleDiscord)
indicator=$(part titleIndicator)
if [ -n "$boost" ] && [ -n "$discord" ] && [ -n "$indicator" ] &&
   [ "${boost%,*}" -lt "${discord%,*}" ] && [ "${discord%,*}" -lt "${indicator%,*}" ]; then
  ui_pass 'the Discord button right of the Boost button in the top bar'
else
  ui_fail "the Discord button is not between the Boost button and the project indicator: '$shown'"
fi
ui_expect_log '^Discord link opened: https://mitcad[.]org/discord$' 'the Discord button and Help open the Discord link'
ui_expect_log '^Open URL [(]test, not opened[)]: https://mitcad[.]org/discord$' 'the Discord link only logged in the test'
[ "$(grep -c '^Discord link opened: https://mitcad[.]org/discord$' "$UI_LOG_FILE")" -ge 3 ] ||
  ui_fail 'the Discord link not opened three times (the button, the button without Boost, Help)'
ui_expect_log '^TestDriver: done' 'the Boost development button'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Boost development button'

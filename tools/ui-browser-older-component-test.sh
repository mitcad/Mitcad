#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/webbrowser/MainWindowComponentWeb.cpp app/webbrowser/BrowserComponent.cpp
# check-all sources: app/webbrowser/BrowserComponent.hpp app/webbrowser/BrowserModuleLoad.cpp
# check-all sources: app/webbrowser/ComponentBrowserHost.hpp
# check-all sources: tools/ui-scripts/browser-older-component.mitcad-ui
# check-all sources: tools/ui-scripts/browser-broken-module.mitcad-ui
# Component from Web with a web browser component MitCad cannot use
# (mitcad#2289), without CEF: MITCAD_BROWSER_COMPONENT_DIR names a component
# folder the test makes, so nothing is downloaded and nothing of the user's
# is read.
#   older   component.json of the interface before this MitCad's
#           (kComponentBrowserAbi - 1): told in words, nothing loaded
#           (tools/ui-scripts/browser-older-component.mitcad-ui);
#   broken  component.json of this MitCad's interface, its module not a
#           library: told in words that it could not be loaded
#           (tools/ui-scripts/browser-broken-module.mitcad-ui).
# Each run: the script's steps pass, the app quits with status 0 and no
# crash report is written.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-older-component-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
build=$(cd "$(dirname "$UI_APP")/.." && pwd)
cli=${UI_CLI:-$build/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-older-component.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100

abi=$(sed -n 's/.*constexpr int kComponentBrowserAbi = \([0-9]*\);.*/\1/p' "$root/app/webbrowser/ComponentBrowserHost.hpp")
[ -n "$abi" ] && [ "$abi" -gt 1 ] || { ui_fail "no interface above 1 in ComponentBrowserHost.hpp ('$abi')"; exit 1; }

# A one-box design (the 3D view tests' own, tools/ui-scripts/gl-failure.json).
"$cli" run "$here/ui-scripts/gl-failure.json" --save "$work/design.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli could not make the design'; exit 1; }

# component name abi: a component folder whose module is not a library.
component() {
  local folder=$work/$1/component
  mkdir -p "$folder"
  printf '{"abi": %s, "version": "2026.1101.100", "cef": "test", "platform": "linux-x64", "module": "mitcad-webbrowser.so", "executables": []}\n' \
    "$2" > "$folder/component.json"
  printf 'not a library\n' > "$folder/mitcad-webbrowser.so"
}

# run name script: one run until the app exits.
run() {
  local name=$1 script=$2 status
  export MITCAD_TEST_WORK=$work/$name
  mkdir -p "$MITCAD_TEST_WORK/crashes"
  cp "$work/design.mitcad" "$MITCAD_TEST_WORK/design.mitcad"
  export MITCAD_CRASH_DIR=$MITCAD_TEST_WORK/crashes
  export MITCAD_TEST_INPUT=$here/ui-scripts/$script.mitcad-ui
  export MITCAD_BROWSER_COMPONENT_DIR=$work/$name/component
  export MITCAD_BROWSER_COMPONENTS_DIR=$work/$name/components
  export MITCAD_WEB_PROFILE_DIR=$work/$name/profile
  export MITCAD_WEB_DOWNLOADS_DIR=$work/$name/downloads
  ui_start_app --ui=qt --no-mcp --chrome=floating --open "$MITCAD_TEST_WORK/design.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$name: the driver timed out"; exit 1; }
  wait "$UI_RUNNER"
  status=$?
  UI_RUNNER=""
  if grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG"; then
    grep -E '^TestDriver: FAIL' "$UI_LOG"
    ui_fail "$name: a step failed"
    exit 1
  fi
  grep -Eq '^TestDriver: quit' "$UI_LOG" || { ui_fail "$name: the script did not reach its quit"; exit 1; }
  [ "$status" = 0 ] || { ui_fail "$name: the app did not quit cleanly (status $status)"; exit 1; }
  if compgen -G "$MITCAD_CRASH_DIR/*.crash" > /dev/null; then
    ls -l "$MITCAD_CRASH_DIR"
    ui_fail "$name: a crash report was written"
    exit 1
  fi
}

ui_start_display
component older $((abi - 1))
run older browser-older-component
grep -Eq "^Component from Web: the build's component .* has interface $((abi - 1)), not $abi\$" "$UI_LOG" ||
  { ui_fail "older: the interfaces were not logged as $((abi - 1)) and $abi"; exit 1; }
echo "ok   older: a component of interface $((abi - 1)) told in words, nothing loaded, quit cleanly"
component broken "$abi"
run broken browser-broken-module
echo "ok   broken: a module that does not load told in words, quit cleanly"
ui_finish 'UI Component from Web with a component it cannot use'

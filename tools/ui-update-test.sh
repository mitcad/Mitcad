#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/update
# Automatic updates (mitcad#9) through the real UI, against a local HTTPS
# server (tools/update-test-server.py) with a certificate and a release key
# made for the test (MITCAD_UPDATE_TEST_CA, MITCAD_UPDATE_TEST_KEY); no
# other host is contacted. The app takes itself for version 0.0.0
# (MITCAD_TEST_VERSION), so that the build's own version is the release:
#   1. Checks off: with the setting off, with the administrator's switch
#      (MITCAD_NO_UPDATE_CHECK, Help > Check for Updates too), and in a
#      build without a release key (the placeholder), Mitcad makes no
#      request at all.
#   2. At start-up a development build finds the release (through a
#      redirect, as GitHub's latest release redirects) and only announces
#      it, with where checks are turned off; the request tells the version,
#      the platform and the channel, and nothing else. Skip This Version;
#      the next start makes no request (once a day); Check for Updates
#      offers the skipped version; Later.
#   3. Refused: a tampered manifest, one signed with another key, a redirect
#      to HTTP; an older release is no update.
#   4. Preferences: the pre-release channel finds a beta through GitHub's
#      list of releases.
#   5. A per-user AppImage (a script that stands for one, setting APPIMAGE
#      as the AppImage runtime does): a tampered download is refused and
#      deleted; Install downloads and verifies the release, asks to save the
#      changed design (saved), replaces the AppImage with an atomic rename
#      once Mitcad has quit and starts it, and the new one reports the
#      update.
#   6. An AppImage in a folder the user cannot write to only announces.
#
# Runs headless on Xvfb (see ui-test-lib.sh); needs python3 and openssl.
# Usage: tools/ui-update-test.sh
# check-all sources: core/update tools/update-test-server.py

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
REAL_APP=$UI_APP
RELEASE_TOOL=${UI_RELEASE_TOOL:-$(dirname "$REAL_APP")/../core/mitcad-release}
WORK=$(mktemp -d /tmp/mitcad-ui-update.XXXXXX)
WWW=$WORK/www
APPS=$WORK/apps
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
SERVER=""
cleanup() {
  ui_cleanup
  [ -n "$SERVER" ] && kill "$SERVER" 2> /dev/null
  chmod -R u+w "$WORK" 2> /dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

for tool in python3 openssl; do
  command -v "$tool" > /dev/null || ui_fail "$tool is required"
done
[ -x "$RELEASE_TOOL" ] || ui_fail "no release tool at $RELEASE_TOOL (UI_RELEASE_TOOL)"

ui_start_display
VERSION=$("$REAL_APP" --version | sed -n 's/^Mitcad //p')
[ -n "$VERSION" ] || ui_fail "the build's version is unknown"
echo "ok   the build is Mitcad $VERSION; the app takes itself for 0.0.0"

# A certificate for 127.0.0.1 and release keys, made for this run.
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 2 -subj /CN=localhost \
  -addext "subjectAltName=IP:127.0.0.1,DNS:localhost" -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
  > /dev/null 2>&1 || ui_fail "openssl could not make a certificate"
SEED=$(openssl rand -hex 32)
OTHER_SEED=$(openssl rand -hex 32)
PUBLIC=$(MITCAD_RELEASE_KEY=$SEED "$RELEASE_TOOL" public-key) || ui_fail "no public key"

mkdir -p "$WWW" "$APPS" "$WORK/new"
python3 "$ROOT/tools/update-test-server.py" "$WWW" "$WORK/cert.pem" "$WORK/key.pem" "$WORK/requests.log" \
  "$WORK/port" &
SERVER=$!
for _ in $(seq 1 100); do
  [ -s "$WORK/port" ] && break
  sleep 0.1
done
[ -s "$WORK/port" ] || ui_fail "the HTTPS server did not start"
BASE=https://127.0.0.1:$(cat "$WORK/port")
echo "ok   HTTPS server at $BASE"

# release folder version seed [platform file]...: a manifest signed with
# the seed's key at $BASE/<folder>/update-manifest.json, the files next to
# it as the downloads.
release() {
  local folder=$1 version=$2 seed=$3 assets=()
  shift 3
  mkdir -p "$WWW/$folder"
  while [ $# -gt 0 ]; do
    cp "$2" "$WWW/$folder/"
    assets+=(--asset "$1" "$2" "$BASE/$folder/$(basename "$2")")
    shift 2
  done
  MITCAD_RELEASE_KEY=$seed "$RELEASE_TOOL" manifest --version "$version" --date 2026-10-06 \
    --notes "$BASE/$folder/notes.html" "${assets[@]}" --out "$WWW/$folder/update-manifest.json" > /dev/null ||
    ui_fail "mitcad-release could not sign $folder"
}

requests() { wc -l < "$WORK/requests.log"; }

# no_requests seconds description: the server got no request meanwhile.
no_requests() {
  local before
  before=$(requests)
  sleep "$1"
  [ "$(requests)" = "$before" ] || ui_fail "$2: $(tail -n +$((before + 1)) "$WORK/requests.log")"
  echo "ok   $2"
}

fresh_settings() {
  rm -f "$SETTINGS"
  mkdir -p "$(dirname "$SETTINGS")"
}

setting() { sed -n "s/^$1=//p" "$SETTINGS"; }

# A stand-in for Mitcad's AppImage: sets APPIMAGE as the runtime does and
# runs the build.
fake_appimage() {
  cat > "$1" << EOF
#!/bin/sh
# Mitcad $2 as an AppImage, for tools/ui-update-test.sh.
echo "Fake AppImage $2 started" >&2
APPIMAGE=\$(readlink -f "\$0")
export APPIMAGE
exec "$REAL_APP" "\$@"
EOF
  chmod 755 "$1"
}
fake_appimage "$APPS/Mitcad.AppImage" 0.0.0
fake_appimage "$WORK/new/Mitcad.AppImage" "$VERSION"
cp "$APPS/Mitcad.AppImage" "$WORK/old.AppImage"

release stable "$VERSION" "$SEED" linux-x64 "$WORK/new/Mitcad.AppImage"
export MITCAD_UPDATE_URL=$BASE/redirect/stable/update-manifest.json
export MITCAD_UPDATE_RELEASES_URL=$BASE/releases.json
export MITCAD_UPDATE_TEST_KEY=$PUBLIC MITCAD_UPDATE_TEST_CA=$WORK/cert.pem MITCAD_TEST_VERSION=0.0.0
unset MITCAD_NO_UPDATE_CHECK

echo "--- 1. Checks off: no request"
fresh_settings
printf '[updates]\nautomatic=false\n' > "$SETTINGS"
ui_start_app
ui_expect_log "Automatic update checks are off" "the setting is off"
no_requests 4 "no request at start-up"
ui_stop_app
fresh_settings
export MITCAD_NO_UPDATE_CHECK=1
ui_start_app
ui_expect_log "Update checks are turned off by the administrator" "the administrator turned checks off"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Update notice (message): Update checks are turned off by your administrator. [Close]" \
  "Check for Updates says so"
no_requests 4 "no request, at start-up or asked"
ui_stop_app
unset MITCAD_NO_UPDATE_CHECK
if ! grep -qE '^[0-9a-fA-F]{64}$' "$ROOT/packaging/update-key.txt"; then
  # The placeholder: without the test's key the build has none.
  fresh_settings
  MITCAD_UPDATE_TEST_KEY="" ui_start_app
  ui_mark
  ui_step "check for updates (search)"   ui_command "Check for Updates"
  ui_expect_new "Update notice (error): This build of Mitcad has no release key, so it cannot verify updates. [Close]" \
    "a build without a release key says so"
  no_requests 4 "no request without a release key, at start-up or asked"
  ui_stop_app
fi

echo "--- 2. A development build announces the release"
fresh_settings
ui_start_app
ui_expect_log "Updates: Mitcad 0.0.0 for linux-x64, announce only: not an AppImage" "a development build announces only"
ui_expect_log "Update request redirected to https://127.0.0.1" "the HTTPS redirect is followed"
ui_expect_log "Update notice (offer): Mitcad $VERSION is available (this is 0.0.0). Download it from its release page. Mitcad checks for updates once a day; Tools > Preferences turns that off. [Release Notes, Skip This Version, Later]" \
  "the notice: no Install, the release page, where checks are turned off"
grep -qxF "GET /redirect/stable/update-manifest.json ua=Mitcad/0.0.0 (linux-x64; stable) lang=* cookie=-" \
  "$WORK/requests.log" || ui_fail "the request said more: $(cat "$WORK/requests.log")"
grep -qxF "GET /stable/update-manifest.json ua=Mitcad/0.0.0 (linux-x64; stable) lang=* cookie=-" \
  "$WORK/requests.log" || ui_fail "the redirect was not followed: $(cat "$WORK/requests.log")"
[ "$(requests)" = 2 ] || ui_fail "more requests than the manifest: $(cat "$WORK/requests.log")"
echo "ok   only the version, the platform and the channel are sent"
[ -n "$(setting lastCheck)" ] || ui_fail "the check's time is not kept"
ui_mark
ui_step "skip this version (Alt+K)"      ui_key alt+k
ui_expect_new "Update: skipping Mitcad $VERSION" "skipped"
[ "$(setting skipped)" = "$VERSION" ] || ui_fail "the skipped version is not kept"
echo "ok   the skipped version is kept"
ui_stop_app
ui_start_app
ui_expect_log "Update check: the last was at" "the next start does not check (once a day)"
no_requests 4 "no request at the next start"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Update notice (offer): Mitcad $VERSION is available (this is 0.0.0). Download it from its release page. You skipped this version. [Release Notes, Skip This Version, Later]" \
  "asked, the skipped version is offered"
ui_step "later (Alt+L)"                  ui_key alt+l
ui_expect_new "Update notice closed" "Later closes the notice"

echo "--- 3. Refused manifests"
cp "$WWW/stable/update-manifest.json" "$WORK/good-manifest.json"
sed -i 's/"date": "2026-10-06"/"date": "2026-10-07"/' "$WWW/stable/update-manifest.json"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Update notice (error): The update check failed: The update manifest was refused: the update manifest's signature does not match Mitcad's release key. [Close]" \
  "a tampered manifest is refused"
release stable "$VERSION" "$OTHER_SEED" linux-x64 "$WORK/new/Mitcad.AppImage"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "the update manifest's signature does not match Mitcad's release key. [Close]" \
  "a manifest signed with another key is refused"
release stable 0.0.0 "$SEED"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Update notice (message): Mitcad 0.0.0 is up to date. [Close]" "the same version is no update"
cp "$WORK/good-manifest.json" "$WWW/stable/update-manifest.json"
ui_stop_app
MITCAD_UPDATE_URL=$BASE/redirect-http/stable/update-manifest.json ui_start_app
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Update notice (error): The update check failed: The release server redirected to an address that is not HTTPS (http://127.0.0.1). [Close]" \
  "a redirect to HTTP is not followed"
ui_stop_app

echo "--- 4. The pre-release channel"
# Above the build's own version, whatever it is: a beta, and a newer draft
# that is not taken
IFS=. read -r major minor _ <<< "$VERSION"
BETA=$major.$((minor + 1)).0-beta.1
DRAFT=$major.$((minor + 2)).0
release beta "$BETA" "$SEED"
cat > "$WWW/releases.json" << EOF
[{"tag_name": "v$VERSION", "draft": false, "prerelease": false,
  "assets": [{"name": "update-manifest.json", "browser_download_url": "$BASE/stable/update-manifest.json"}]},
 {"tag_name": "v$DRAFT", "draft": true,
  "assets": [{"name": "update-manifest.json", "browser_download_url": "$BASE/draft/update-manifest.json"}]},
 {"tag_name": "v$BETA", "draft": false, "prerelease": true,
  "assets": [{"name": "update-manifest.json", "browser_download_url": "$BASE/beta/update-manifest.json"}]}]
EOF
ui_start_app
ui_mark
ui_step "preferences, Updates (search)"  ui_command "Preferences: Updates"
ui_focus_dialog '^Preferences$'
ui_step "the channel (Alt+H)"            ui_key alt+h
ui_step "pre-releases too (Down)"        ui_key Down
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Preferences: update checks on, channel prerelease" "the pre-release channel chosen"
[ "$(setting channel)" = prerelease ] || ui_fail "the channel is not kept"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Update notice (offer): Mitcad $BETA is available (this is 0.0.0). It is a pre-release. Download it from its release page. [Release Notes, Skip This Version, Later]" \
  "the newest release of the list, a beta"
grep -qxF "GET /releases.json ua=Mitcad/0.0.0 (linux-x64; prerelease) lang=* cookie=-" "$WORK/requests.log" ||
  ui_fail "the list of releases was not asked for: $(cat "$WORK/requests.log")"
echo "ok   the list of releases, then the beta's manifest"
ui_stop_app

echo "--- 5. A per-user AppImage updates itself"
release appimage "$VERSION" "$SEED" linux-x64 "$WORK/new/Mitcad.AppImage"
# The download as served first: changed, the same size.
sed -i 's/started/STARTED/' "$WWW/appimage/Mitcad.AppImage"
cat > "$WORK/block.mitcad" << 'EOF'
{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0 }, { "name": "d2", "value": 40.0 },
    { "name": "d3", "value": 20.0 }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" } ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" }
  ]
}
EOF
fresh_settings
export UI_GDB=0
MITCAD_UPDATE_URL=$BASE/appimage/update-manifest.json UI_APP=$APPS/Mitcad.AppImage \
  ui_start_app --open "$WORK/block.mitcad" --set d3=35
ui_expect_log "Updates: Mitcad 0.0.0 for linux-x64, the AppImage $APPS/Mitcad.AppImage" "a per-user AppImage can update itself"
ui_expect_log "Update notice (offer): Mitcad $VERSION is available (this is 0.0.0). Mitcad checks for updates once a day; Tools > Preferences turns that off. [Release Notes, Install, Skip This Version, Later]" \
  "the notice offers Install"
ui_mark
ui_step "install (Alt+I)"                ui_key alt+i
ui_expect_new "Update rejected: the download is not the release's: its SHA-256 differs from the release's (deleted $APPS/.Mitcad.AppImage.update)" \
  "a tampered download is refused"
ui_expect_new "Update notice (error): Mitcad $VERSION was not installed: the download is not the release's: its SHA-256 differs from the release's. The download was deleted. [Close]" \
  "and the user is told"
[ -e "$APPS/.Mitcad.AppImage.update" ] && ui_fail "the tampered download is left"
cmp -s "$APPS/Mitcad.AppImage" "$WORK/old.AppImage" || ui_fail "the AppImage changed"
echo "ok   deleted, the AppImage unchanged"
ui_step "close the notice (Alt+O)"       ui_key alt+o
cp "$WORK/new/Mitcad.AppImage" "$WWW/appimage/Mitcad.AppImage"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "[Release Notes, Install, Skip This Version, Later]" "asked again, Install"
OLD=$UI_RUNNER
ui_key alt+i
ui_expect_new "Update verified: $APPS/.Mitcad.AppImage.update" "the download is verified"
ui_expect_new "Update staged: 0.0.0 -> $VERSION" "the update waits for Mitcad to quit"
ui_focus_dialog '^Mitcad$'
ui_key Return # Save, the default
for _ in $(seq 1 100); do
  kill -0 "$OLD" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$OLD" 2> /dev/null && ui_fail "Mitcad did not quit"
echo "ok   asked to save, saved, quit"
ui_expect_new "Update: replaced $APPS/Mitcad.AppImage" "the AppImage replaced once Mitcad quit"
ui_expect_new "Fake AppImage $VERSION started" "the new AppImage started"
ui_expect_new "Update installed: 0.0.0 -> $VERSION" "the new version reports the update" 30
ui_expect_new "Update notice (message): Mitcad was updated to $VERSION. [Close]" "and tells the user"
cmp -s "$APPS/Mitcad.AppImage" "$WORK/new/Mitcad.AppImage" || ui_fail "the AppImage is not the release's"
[ -x "$APPS/Mitcad.AppImage" ] || ui_fail "the new AppImage is not executable"
[ -e "$APPS/.Mitcad.AppImage.update" ] && ui_fail "the download is left"
d3=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print([p["value"] for p in doc["parameters"] if p["name"] == "d3"])' \
  "$WORK/block.mitcad")
[ "$d3" = "[35.0]" ] || ui_fail "the design was not saved: d3 $d3"
grep -q "^lastResult=installed 0.0.0 -> $VERSION" "$SETTINGS" || ui_fail "no result kept"
echo "ok   the release's AppImage, executable, the design saved"
# The new process is the AppImage's own, no child of this script.
UI_WINDOW=""
for _ in $(seq 1 100); do
  UI_WINDOW=$(xdotool search --onlyvisible --name ' - Mitcad$' 2> /dev/null | head -1)
  [ -n "$UI_WINDOW" ] && break
  sleep 0.2
done
[ -n "$UI_WINDOW" ] || ui_fail "the new Mitcad shows no window"
UI_RUNNER=$(xdotool getwindowpid "$UI_WINDOW")
[ -n "$UI_RUNNER" ] && [ "$UI_RUNNER" != "$OLD" ] || ui_fail "no new process"
ui_stop_app

echo "--- 6. An AppImage the user cannot replace only announces"
chmod 555 "$APPS"
UI_APP=$APPS/Mitcad.AppImage ui_start_app
ui_expect_log "Updates: Mitcad 0.0.0 for linux-x64, announce only: the user cannot write to the AppImage $APPS/Mitcad.AppImage or its folder" \
  "a read-only AppImage announces only"
ui_mark
ui_step "check for updates (search)"     ui_command "Check for Updates"
ui_expect_new "Download it from its release page. [Release Notes, Skip This Version, Later]" "no Install"
ui_stop_app
chmod 755 "$APPS"

ui_finish "UI update test"

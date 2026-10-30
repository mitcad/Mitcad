#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/update core/update tools/update-test-server.py
# Run in the approved macOS guest only. All replacements are copies owned
# by this test; the input bundle and Applications are never modified.
# Only a test build reads the update tests' environment variables (the
# CMake option MITCAD_UPDATE_TEST_HOOKS, on in macos-debug; mitcad#921;
# UI_UPDATE_TEST_HOOKS from the ctest or the build's CMakeCache.txt tells).
# A build without it (macos-release) is checked to ignore them instead,
# after the atomic exchange: its own version, GitHub's addresses, the
# built-in key only, no request to the test's server.
# Usage: bash tools/ui-macos-update-test.sh <app executable> <mitcad-release> [out]
set -eu
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
[ "$#" -ge 2 ] && [ "$#" -le 3 ] || { echo "usage: $0 <app executable> <mitcad-release> [out]" >&2; exit 2; }
release_tool=$2
ui_init "$1" "${3:-ui-macos-update}"
set -eE
trap 'fixture_error=$?; case $- in *e*) printf "FAIL: line %s, status %s: %s\n" "$LINENO" "$fixture_error" "$BASH_COMMAND" >&2 ;; esac' ERR
# UI_OUT is reused by ctest. Keep prior evidence separately so an old
# helper log cannot satisfy this run's assertions (the helper appends).
previous_run=''
for previous_log in readonly.log translocation-policy.log reject.log install.log \
  reject-helper.log rollback-helper.log updater.log relaunch.log requests.log server.log; do
  if [ -f "$UI_OUT/$previous_log" ]; then
    if [ -z "$previous_run" ]; then
      mkdir -p "$UI_OUT/history"
      previous_run=$(mktemp -d "$UI_OUT/history/run.XXXXXX")
    fi
    mv "$UI_OUT/$previous_log" "$previous_run/$previous_log"
  fi
done
original_app=$UI_APP
swapper=$(dirname "$original_app")/mitcad-updater
[ -x "$swapper" ] || { echo 'FAIL: macOS atomic exchange helper is missing'; exit 1; }
bundle=$(dirname "$(dirname "$(dirname "$original_app")")")
executable=$(basename "$original_app")
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$bundle/Contents/Info.plist")
work=$UI_ROOT/update
mkdir -p "$work/www" "$work/image" "$work/apps"
# macOS aliases /var to /private/var. Launch Services starts the canonical
# executable path recorded by Installation::detect, so use the same path.
work=$(cd "$work" && pwd -P)
fixture_executable=$work/apps/Mitcad.app/Contents/MacOS/$executable
server='' mounted='' restarted=''
fixture_pids() {
  python3 - "$fixture_executable" <<'PY'
import pathlib
import subprocess
import sys

expected = pathlib.Path(sys.argv[1]).resolve()
processes = subprocess.check_output(["/bin/ps", "-axo", "pid=,comm="], text=True)
for line in processes.splitlines():
    fields = line.strip().split(maxsplit=1)
    if len(fields) == 2 and fields[0].isdigit():
        if pathlib.Path(fields[1]).resolve() == expected:
            print(fields[0])
PY
}
stop_restarted() {
  local pid remaining i=0
  # Discover afresh even after an assertion fails before setting restarted.
  restarted=$(fixture_pids)
  while IFS= read -r pid; do
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; fi
  done <<< "$restarted"
  while [ "$i" -lt 50 ]; do
    remaining=$(fixture_pids)
    if [ -z "$remaining" ]; then restarted=''; return 0; fi
    sleep 0.1
    i=$((i + 1))
  done
  while IFS= read -r pid; do
    if [ -n "$pid" ]; then kill -KILL "$pid" 2>/dev/null || true; fi
  done <<< "$remaining"
  sleep 0.1
  restarted=''
  [ -z "$(fixture_pids)" ] || { echo 'FAIL: could not stop the owned fixture application'; return 1; }
}
cleanup_update() {
  set +e
  stop_restarted
  if [ -n "$mounted" ]; then /usr/bin/hdiutil detach "$mounted" >/dev/null 2>&1 || true; fi
  if [ -n "$server" ]; then kill "$server" 2>/dev/null || true; fi
  ui_cleanup
}
trap cleanup_update EXIT
wait_for() {
  local file=$1 pattern=$2 i=0
  while [ "$i" -lt 150 ]; do
    if [ -f "$file" ] && grep -Eq -- "$pattern" "$file"; then return 0; fi
    sleep 0.2
    i=$((i + 1))
  done
  echo "FAIL: missing $pattern in $file" >&2
  [ ! -f "$file" ] || tail -60 "$file" >&2
  exit 1
}
check_run() {
  ui_expect_no_log 'TestDriver: FAIL' "$1: driver completed"
  [ "$UI_FAILURES" -eq 0 ] || exit 1
}
run_app() {
  # The shared helper must inspect nonzero exits itself (including SKIP for
  # an unavailable window server), before restoring this script's fail-fast.
  set +e
  ui_run_app "$@"
  set -e
}
fresh_settings() { rm -rf "$MITCAD_SETTINGS_DIR"; mkdir -p "$MITCAD_SETTINGS_DIR"; }
sign_image() {
  MITCAD_RELEASE_KEY=$seed "$release_tool" manifest --version "$version" \
    --notes "$base/notes.html" --asset macos-arm64 "$work/www/update.dmg" "$base/update.dmg" \
    --out "$work/www/update-manifest.json" > /dev/null
}

echo '--- Atomic bundle exchange, rollback, and failure leaving the old path intact'
mkdir "$work/first.app" "$work/second.app"
printf 'old\n' > "$work/first.app/marker"
printf 'new\n' > "$work/second.app/marker"
"$swapper" --swap "$work/first.app" "$work/second.app"
grep -qx new "$work/first.app/marker"
grep -qx old "$work/second.app/marker"
"$swapper" --swap "$work/first.app" "$work/second.app"
grep -qx old "$work/first.app/marker"
if "$swapper" --swap "$work/first.app" "$work/missing.app"; then
  echo 'FAIL: atomic exchange accepted a missing staged bundle'; exit 1
fi
grep -qx old "$work/first.app/marker"

# A certificate and Ed25519 key belonging only to this run.
cat > "$work/cert.cnf" <<'EOF'
[req]
distinguished_name=dn
x509_extensions=ext
prompt=no
[dn]
CN=localhost
[ext]
subjectAltName=IP:127.0.0.1,DNS:localhost
basicConstraints=critical,CA:TRUE
EOF
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -config "$work/cert.cnf" \
  -keyout "$work/key.pem" -out "$work/cert.pem" > /dev/null 2>&1
seed=$(openssl rand -hex 32)
public=$(MITCAD_RELEASE_KEY=$seed "$release_tool" public-key)
python3 "$here/update-test-server.py" "$work/www" "$work/cert.pem" "$work/key.pem" \
  "$UI_OUT/requests.log" "$work/port" > "$UI_OUT/server.log" 2>&1 &
server=$!
wait_for "$work/port" '^[0-9]+$'
base=https://127.0.0.1:$(cat "$work/port")
unset MITCAD_NO_UPDATE_CHECK
export MITCAD_UPDATE_URL=$base/update-manifest.json
export MITCAD_UPDATE_RELEASES_URL=$base/releases.json
export MITCAD_UPDATE_TEST_CA=$work/cert.pem MITCAD_UPDATE_TEST_KEY=$public MITCAD_TEST_VERSION=0.0.0

hooks=$(ui_update_test_hooks "$original_app")
if [ "$hooks" = OFF ]; then
  echo "--- A build without MITCAD_UPDATE_TEST_HOOKS ignores the update tests' variables"
  printf '%s\n' "wait-log '^Update source: this build ignores '" > "$work/ignored.script"
  UI_APP=$original_app
  fresh_settings
  # Its check would go to GitHub: checks off, the source is logged anyway.
  MITCAD_NO_UPDATE_CHECK=1 MITCAD_TEST_INPUT=$work/ignored.script run_app ignored --ui=qt --no-recovery
  ui_expect_log "^Updates: Mitcad $version for macos-arm64, " 'its own version, not MITCAD_TEST_VERSION'
  ui_expect_log '^Update source: manifest at https://github[.]com/mitcad/Mitcad/releases/latest/download/update-manifest[.]json, releases at https://api[.]github[.]com/repos/mitcad/Mitcad/releases[?]per_page=20, ' \
    "GitHub's addresses, not MITCAD_UPDATE_URL nor MITCAD_UPDATE_RELEASES_URL"
  ui_expect_log '^Update source: this build ignores MITCAD_UPDATE_URL, MITCAD_UPDATE_RELEASES_URL, MITCAD_UPDATE_TEST_KEY, MITCAD_UPDATE_TEST_CA, MITCAD_TEST_VERSION [(]MITCAD_UPDATE_TEST_HOOKS off[)]' \
    "the update tests' variables are ignored"
  ui_expect_no_log 'and the test key' 'the built-in key only'
  ui_expect_no_log 'test certificate' 'no test certificate'
  check_run ignored
  if [ -s "$UI_OUT/requests.log" ]; then echo "FAIL: requests to the test's server"; exit 1; fi
  echo "PASS: macOS automatic update (a build without the test hooks ignores the update tests' variables)"
  echo "Logs: $UI_OUT"
  exit 0
fi

echo '--- Build and sign a disk image of the test-owned new application'
/usr/bin/ditto "$bundle" "$work/image/Mitcad.app"
mkdir -p "$work/image/Mitcad.app/Contents/Resources"
printf 'new %s\n' "$version" > "$work/image/Mitcad.app/Contents/Resources/update-fixture.txt"
# Re-sign only this disposable fixture after adding its marker. Do not
# remove quarantine attributes or change system assessment policies.
/usr/bin/codesign --force --deep --sign - "$work/image/Mitcad.app"
/usr/bin/codesign --verify --deep --strict "$work/image/Mitcad.app"
/usr/bin/hdiutil create -quiet -format UDZO -srcfolder "$work/image" "$work/good.dmg"
cp "$work/good.dmg" "$work/www/update.dmg"
sign_image
/usr/bin/ditto "$bundle" "$work/apps/Mitcad.app"
printf 'old\n' > "$work/apps/Mitcad.app/Contents/Resources/update-fixture.txt"
/usr/bin/codesign --force --deep --sign - "$work/apps/Mitcad.app"

cat > "$work/announce.script" <<'EOF'
command help.check_updates
wait-log '^Update notice \(offer\): .*Download it from its release page\.'
expect-log '\[Release Notes, Skip This Version, Later\]'
EOF
echo '--- A genuine read-only disk-image run announces only'
mounted=$work/readonly
mkdir "$mounted"
/usr/bin/hdiutil attach -quiet -readonly -nobrowse -noautoopen -mountpoint "$mounted" "$work/good.dmg"
UI_APP=$mounted/Mitcad.app/Contents/MacOS/$executable
MITCAD_TEST_INPUT=$work/announce.script run_app readonly --ui=qt --no-recovery
ui_expect_log 'announce only: the application bundle is on a read-only volume' 'read-only reason'
check_run readonly
/usr/bin/hdiutil detach -quiet "$mounted"
mounted=''

echo '--- App Translocation path policy (synthetic path, not an OS translocation claim)'
mkdir -p "$work/AppTranslocation/fixture/d"
/usr/bin/ditto "$bundle" "$work/AppTranslocation/fixture/d/Mitcad.app"
UI_APP=$work/AppTranslocation/fixture/d/Mitcad.app/Contents/MacOS/$executable
fresh_settings
MITCAD_TEST_INPUT=$work/announce.script run_app translocation-policy --ui=qt --no-recovery
ui_expect_log 'announce only: the application is running from App Translocation' 'translocation policy reason'
check_run translocation-policy
rm -rf "$work/AppTranslocation"

echo '--- A changed downloaded image is refused before mounting'
# Change a byte without changing size; the signed manifest still names the
# original digest. The image will never reach hdiutil.
python3 - "$work/www/update.dmg" <<'PY'
import sys
with open(sys.argv[1], "r+b") as image:
    byte = image.read(1)
    image.seek(0)
    image.write(bytes([byte[0] ^ 1]))
PY
cat > "$work/reject.script" <<'EOF'
command help.check_updates
wait-log '^Update notice \(offer\): .*\[Release Notes, Install, Skip This Version, Later\]'
button '^Install$'
wait-log '^Update rejected: .*SHA-256 differs'
EOF
UI_APP=$work/apps/Mitcad.app/Contents/MacOS/$executable
fresh_settings
before_requests=$(wc -l < "$UI_OUT/requests.log")
MITCAD_TEST_INPUT=$work/reject.script run_app reject --ui=qt --no-recovery
ui_expect_log '^Update rejected: .*SHA-256 differs' 'changed image rejected'
ui_expect_log '^Update source: this test build reads MITCAD_UPDATE_URL, ' "a test build reads the update tests' variables"
check_run reject
after_requests=$(wc -l < "$UI_OUT/requests.log")
[ "$((after_requests - before_requests))" -eq 2 ] || {
  echo 'FAIL: expected one manifest and one download, without a duplicate startup check'; exit 1;
}
grep -qx old "$work/apps/Mitcad.app/Contents/Resources/update-fixture.txt"
cp "$work/good.dmg" "$work/www/update.dmg"

echo '--- A signed image containing a broken bundle seal is refused'
printf 'changed after signing\n' > "$work/image/Mitcad.app/Contents/Resources/update-fixture.txt"
/usr/bin/hdiutil create -quiet -format UDZO -srcfolder "$work/image" "$work/tampered-bundle.dmg"
mkdir "$work/reject-helper"
cp "$work/tampered-bundle.dmg" "$work/reject-helper/update.dmg"
cp "$here/../app/update/UpdaterMacos.sh" "$work/reject-helper/updater.sh"
cp "$swapper" "$work/reject-helper/mitcad-updater"
MITCAD_RELEASE_KEY=$seed "$release_tool" manifest --version "$version" \
  --notes "$base/notes.html" --asset macos-arm64 "$work/reject-helper/update.dmg" "$base/rejected.dmg" \
  --out "$work/reject-helper/manifest.json" > /dev/null
"$release_tool" verify --manifest "$work/reject-helper/manifest.json" --key "$public" \
  --asset macos-arm64 "$work/reject-helper/update.dmg" > /dev/null
# Disable checks in the restarted old fixture and keep all data in UI_ROOT.
MITCAD_NO_UPDATE_CHECK=1 /bin/bash "$work/reject-helper/updater.sh" 99999999 \
  "$work/reject-helper/update.dmg" "$work/apps/Mitcad.app" "$version" "$UI_OUT/reject-helper.log" && {
    echo 'FAIL: accepted a broken code signature'; exit 1;
  }
grep -q 'Update failed: the new bundle code signature is invalid' "$UI_OUT/reject-helper.log"
grep -qx old "$work/apps/Mitcad.app/Contents/Resources/update-fixture.txt"
# The helper restarts the old fixture on failure; stop only our exact path.
wait_for "$work/reject-helper/relaunch.log" 'Updates:'
restarted=$(fixture_pids)
stop_restarted

echo '--- A Launch Services refusal atomically rolls back to the old bundle'
# A correctly sealed, but deliberately incompatible, fixture exercises the
# failure after the actual filesystem exchange without changing OS policy.
/usr/libexec/PlistBuddy -c 'Set :LSMinimumSystemVersion 99.0' "$work/image/Mitcad.app/Contents/Info.plist"
/usr/bin/codesign --force --deep --sign - "$work/image/Mitcad.app"
/usr/bin/hdiutil create -quiet -format UDZO -srcfolder "$work/image" "$work/incompatible.dmg"
mkdir "$work/rollback-helper"
cp "$work/incompatible.dmg" "$work/rollback-helper/update.dmg"
cp "$here/../app/update/UpdaterMacos.sh" "$work/rollback-helper/updater.sh"
cp "$swapper" "$work/rollback-helper/mitcad-updater"
if MITCAD_NO_UPDATE_CHECK=1 /bin/bash "$work/rollback-helper/updater.sh" 99999999 \
  "$work/rollback-helper/update.dmg" "$work/apps/Mitcad.app" "$version" "$UI_OUT/rollback-helper.log"; then
  echo 'FAIL: Launch Services accepted an incompatible application'; exit 1
fi
grep -q 'Update: atomically exchanging the old and new bundles' "$UI_OUT/rollback-helper.log"
grep -q 'Update: restored' "$UI_OUT/rollback-helper.log"
grep -q 'Update failed: Launch Services could not restart the application' "$UI_OUT/rollback-helper.log"
grep -qx old "$work/apps/Mitcad.app/Contents/Resources/update-fixture.txt"
wait_for "$work/rollback-helper/relaunch.log" 'Updates:'
restarted=$(fixture_pids)
stop_restarted

echo '--- Offer, download, verify, quit, replace, and relaunch the new version'
cat > "$work/install.script" <<'EOF'
command help.check_updates
wait-log '^Update notice \(offer\): .*\[Release Notes, Install, Skip This Version, Later\]'
button '^Install$'
wait-log '^Update staged:'
EOF
fresh_settings
MITCAD_TEST_INPUT=$work/install.script run_app install --ui=qt --no-recovery
ui_expect_log '^Update verified:' 'download verified'
ui_expect_log "^Update staged: 0.0.0 -> $version" 'staged before quit'
check_run install
download=$(sed -n 's/.*Update verified: //p' "$UI_OUT/install.log" | tail -1)
[ -n "$download" ] || { echo 'FAIL: no verified download path'; exit 1; }
download_dir=$(dirname "$download")
wait_for "$download_dir/updater.log" 'Update installed:'
wait_for "$download_dir/relaunch.log" "Update installed: 0.0.0 -> $version"
cp "$download_dir/updater.log" "$UI_OUT/updater.log"
cp "$download_dir/relaunch.log" "$UI_OUT/relaunch.log"
restarted=$(fixture_pids)
[ "$(printf '%s\n' "$restarted" | wc -w)" -eq 1 ] || {
  echo 'FAIL: expected exactly one relaunched process at the canonical fixture executable path'; exit 1;
}
kill -0 "$restarted" || { echo 'FAIL: the relaunched fixture application exited'; exit 1; }
grep -qx "new $version" "$work/apps/Mitcad.app/Contents/Resources/update-fixture.txt"
/usr/bin/codesign --verify --deep --strict "$work/apps/Mitcad.app"
if /usr/bin/xattr -p com.apple.quarantine "$work/apps/Mitcad.app" > /dev/null 2>&1; then
  echo 'FAIL: the updater added quarantine to the test bundle'; exit 1
fi
[ ! -e "$download" ] || { echo 'FAIL: the downloaded image was left'; exit 1; }
stop_restarted
echo 'PASS: macOS automatic update (local manifest and disk image, synthetic translocation policy)'
echo "Logs: $UI_OUT"

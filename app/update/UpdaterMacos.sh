#!/bin/bash
# SPDX-License-Identifier: MIT
# Called only for an image verified by UpdateController, after the window
# and its document have closed. No privileged service or elevated install.
set -u
export PATH=/usr/bin:/bin:/usr/sbin:/sbin
export LC_ALL=C
umask 077

[ "$#" -eq 5 ] || exit 2
old_pid=$1 image=$2 target=$3 version=$4 log=$5
exec >> "$log" 2>&1
case "$old_pid" in ''|*[!0-9]*) echo 'Update failed: invalid process ID'; exit 2 ;; esac
case "$target" in /*.[aA][pP][pP]) ;; *) echo 'Update failed: invalid bundle path'; exit 2 ;; esac
work=$(/usr/bin/dirname "$image")
parent=$(/usr/bin/dirname "$target")
mount=$work/mount
swap=''
attached=0
replaced=0
success=0
failure='the macOS update helper stopped'

restart() {
  local args=(-n --stdout "$work/relaunch.log" --stderr "$work/relaunch.log")
  # Launch Services otherwise drops these overrides. Preserve the application's
  # storage locations, including the isolated UI fixtures, across the restart.
  # Never forward credentials or the test's pretend version/input script.
  local name
  for name in MITCAD_SETTINGS_DIR MITCAD_AUTOSAVE_DIR MITCAD_PROJECTS_DIR \
    MITCAD_THUMBNAIL_DIR MITCAD_CRASH_DIR MITCAD_MCP_DISCOVERY_DIR MITCAD_RESULT_STORE \
    MITCAD_DEBUG_LOG MITCAD_UPDATE_URL MITCAD_UPDATE_TEST_KEY MITCAD_UPDATE_TEST_CA \
    MITCAD_UPDATE_RELEASES_URL MITCAD_NO_UPDATE_CHECK QT_FORCE_STDERR_LOGGING; do
    if [ "${!name+x}" ]; then args+=(--env "$name=${!name}"); fi
  done
  /usr/bin/open "${args[@]}" "$target"
}

cleanup() {
  local result=$?
  trap - EXIT
  if [ "$success" -eq 0 ] && [ "$replaced" -eq 1 ]; then
    # Never delete the only intact old bundle, even if a rollback fails.
    if "$work/mitcad-updater" --swap "$target" "$swap/new.app"; then
      echo "Update: restored $target"
      replaced=0
    else
      failure="$failure; the previous application is retained at $swap/new.app"
    fi
  fi
  if [ "$attached" -eq 1 ]; then
    if /usr/bin/hdiutil detach "$mount"; then
      echo 'Update: detached the disk image'
      attached=0
    else
      echo 'Update failed: cannot detach the update disk image'
    fi
  fi
  if [ "$success" -eq 0 ]; then
    echo "Update failed: $failure"
    # The old app reads the saved failure on restart. Do not restart while
    # the original process is still alive (for example after the timeout).
    if ! kill -0 "$old_pid" 2>/dev/null && [ -d "$target" ]; then restart || true; fi
  fi
  if [ -n "$swap" ] && { [ "$success" -eq 1 ] || [ "$replaced" -eq 0 ]; }; then /bin/rm -rf "$swap"; fi
  /bin/rm -f "$image" "$0" "$work/mitcad-updater"
  if [ "$attached" -eq 0 ]; then /bin/rmdir "$mount" 2>/dev/null || true; fi
  exit "$result"
}
trap cleanup EXIT
fail() { failure=$1; exit 1; }
trap 'fail "the macOS update was interrupted"' HUP INT TERM

echo "Update: waiting for process $old_pid to quit"
count=0
while kill -0 "$old_pid" 2>/dev/null; do
  [ "$count" -lt 300 ] || fail 'MitCad did not quit within five minutes'
  /bin/sleep 1
  count=$((count + 1))
done
[ -f "$image" ] && [ ! -L "$image" ] || fail 'the verified disk image is no longer a regular file'
[ -d "$target" ] && [ ! -L "$target" ] && [ -w "$parent" ] || fail 'the application bundle can no longer be replaced'
/bin/mkdir "$mount" || fail 'cannot prepare the disk image mount point'
echo 'Update: mounting the verified disk image read-only'
/usr/bin/hdiutil attach -readonly -nobrowse -noautoopen -mountpoint "$mount" "$image" ||
  fail 'cannot mount the verified disk image'
attached=1
source=$mount/Mitcad.app
[ -d "$source" ] && [ ! -L "$source" ] || fail 'the disk image contains no Mitcad.app'
plist=$source/Contents/Info.plist
identifier=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$target/Contents/Info.plist") ||
  fail 'cannot read the installed application identifier'
new_identifier=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist") || fail 'the new bundle has no identifier'
new_version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist") || fail 'the new bundle has no version'
executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist") || fail 'the new bundle has no executable'
[ "$new_identifier" = "$identifier" ] && [ "$new_version" = "$version" ] || fail 'the new bundle identifier or version does not match'
case "$executable" in ''|*/*|.|..) fail 'the new bundle executable name is invalid' ;; esac
[ -x "$source/Contents/MacOS/$executable" ] || fail 'the new bundle executable is missing'

# The release signature authenticates the image, including unsigned local
# builds. When the bundle has a code signature, require it to remain valid.
# No re-signing, xattr removal, quarantine addition or Gatekeeper changes.
signed=0
if /usr/bin/codesign -d "$source" > "$work/signature.txt" 2>&1; then
  signed=1
  /usr/bin/codesign --verify --deep --strict "$source" || fail 'the new bundle code signature is invalid'
elif [ -e "$source/Contents/_CodeSignature" ] ||
     ! /usr/bin/grep -Fq 'code object is not signed at all' "$work/signature.txt"; then
  fail 'the new bundle code signature cannot be read'
fi
swap=$(/usr/bin/mktemp -d "$parent/.mitcad-update.XXXXXX") || fail 'cannot prepare a copy beside the application'
echo 'Update: copying the new bundle beside the installed application'
/usr/bin/ditto --rsrc --extattr "$source" "$swap/new.app" || fail 'cannot copy the new application'
if [ "$signed" -eq 1 ]; then
  /usr/bin/codesign --verify --deep --strict "$swap/new.app" || fail 'the copied bundle code signature is invalid'
fi
echo 'Update: atomically exchanging the old and new bundles'
"$work/mitcad-updater" --swap "$target" "$swap/new.app" || fail 'cannot atomically exchange the application bundles'
replaced=1
if /usr/bin/hdiutil detach "$mount"; then
  attached=0
  echo 'Update: detached the disk image'
else
  fail 'cannot detach the update disk image'
fi
echo "Update: relaunching $target"
restart || fail 'Launch Services could not restart the application'
success=1
echo "Update installed: $version at $target"
# The atomic exchange and the relaunch succeeded; cleanup discards the old bundle.
exit 0

#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Mitcad's Linux AppImage (cmake/AppImage.cmake, mitcad#18) as a user gets
# it, on a hidden Xvfb display (ui-test-lib.sh); no host but this one is
# contacted:
#   1. What it holds: the executables, the desktop file and the icon, Qt's
#      plugins (X11, OpenGL on X11, SVG images and icons, TLS) with a
#      qt.conf, OCCT and the licences; every ELF file finds its libraries
#      inside it or in the system's library folders (ldd with an empty
#      environment), none in the build's, Qt's or vcpkg's folders, and its
#      RUNPATHs stay inside it. Prints the newest glibc version it needs.
#   2. Copied into a folder of its own and started with an empty
#      environment (env -i; PATH /usr/bin:/bin, and update checks off):
#      --version, then the demo block's screenshot (--screenshot), while
#      every library the running Mitcad has loaded (/proc/<pid>/maps) is
#      the AppImage's or the system's, and its desktop file (Exec the
#      AppImage) and icons are in the user's data folder.
#   2b. With the render worker (MITCAD_RENDER): its libraries and their
#      licences are in it, mitcad links none of them, and View > Rendered
#      renders the demo block with them (on the GPU, with the AppImage's
#      kernels, when the worker has a CUDA device; mitcad#50).
#   3. The automatic update (docs/updates.md): the AppImage, taking itself
#      for 0.0.0 (MITCAD_TEST_VERSION), finds a release whose manifest signs
#      this AppImage as the linux-x64 download (mitcad-release, with a key
#      made for the test), served over HTTPS from 127.0.0.1 with a
#      certificate made for the test (tools/update-test-server.py; needs
#      python3 and openssl); Install downloads and verifies it, Mitcad
#      quits, the download replaces the AppImage (a new file of the same
#      name) and starts as the release, which reports the update.
# Without a working FUSE the AppImage runs with APPIMAGE_EXTRACT_AND_RUN=1.
#
# Usage: tools/appimage-test.sh [--skip-stale] <AppImage> [<built mitcad>] [<mitcad-release>]
#   --skip-stale (the ctest app.appimage): exit 77, skipped, when there is
#   no AppImage newer than the built mitcad.
# The release tool defaults to build/dev/core/mitcad-release.

SKIP_STALE=0
if [ "${1:-}" = --skip-stale ]; then
  SKIP_STALE=1
  shift
fi
IMAGE=${1:?usage: tools/appimage-test.sh [--skip-stale] <AppImage> [<built mitcad>] [<mitcad-release>]}
BUILT=${2:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
RELEASE_TOOL=${3:-$ROOT/build/dev/core/mitcad-release}
if [ "$SKIP_STALE" = 1 ] && { [ ! -f "$IMAGE" ] || { [ -n "$BUILT" ] && [ "$IMAGE" -ot "$BUILT" ]; }; }; then
  echo "SKIP: no AppImage of this build; run: cmake --build --preset dev --target appimage"
  exit 77
fi
[ -f "$IMAGE" ] || { echo "FAIL: no AppImage $IMAGE"; exit 1; }
IMAGE=$(readlink -f "$IMAGE")

source "$ROOT/tools/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-appimage.XXXXXX)
APPS=$WORK/apps
COPY=$APPS/Mitcad.AppImage
WWW=$WORK/www
SERVER=""
cleanup() {
  ui_cleanup
  [ -n "$SERVER" ] && kill "$SERVER" 2> /dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

for tool in python3 openssl readelf ldd; do
  command -v "$tool" > /dev/null || ui_fail "$tool is required"
done
[ -x "$RELEASE_TOOL" ] || ui_fail "no release tool at $RELEASE_TOOL"
echo "AppImage: $IMAGE ($(du -h "$IMAGE" | cut -f1))"

# library_ok path dir: a library inside dir (the AppImage's) or in one of the
# system's library folders.
library_ok() {
  case $1 in
    "$2"/*) return 0 ;;
    /lib/* | /lib64/* | /usr/lib/* | /usr/lib64/* | /lib32/* | /usr/lib32/*) return 0 ;;
  esac
  return 1
}

echo "--- 1. What the AppImage holds"
(cd "$WORK" && "$IMAGE" --appimage-extract > /dev/null) || ui_fail "--appimage-extract failed"
DIR=$WORK/squashfs-root
for file in AppRun mitcad.desktop mitcad.png .DirIcon usr/bin/mitcad usr/bin/mitcad-cli usr/bin/qt.conf \
  usr/share/applications/mitcad.desktop usr/share/icons/hicolor/256x256/apps/mitcad.png \
  usr/plugins/platforms/libqxcb.so usr/plugins/xcbglintegrations/libqxcb-glx-integration.so \
  usr/plugins/imageformats/libqsvg.so usr/plugins/iconengines/libqsvgicon.so \
  usr/plugins/tls/libqopensslbackend.so \
  usr/share/doc/mitcad/LICENSE usr/share/doc/mitcad/THIRD-PARTY-NOTICES.txt \
  usr/share/doc/mitcad/licenses/droid-sans/LICENSE.txt; do
  [ -e "$DIR/$file" ] || ui_fail "the AppImage has no $file"
done
for library in libQt6Core libQt6Widgets libQt6Network libQt6Svg libQt6XcbQpa libTKernel libTKV3d libTKOpenGl; do
  compgen -G "$DIR/usr/lib/$library.so*" > /dev/null || ui_fail "the AppImage has no $library"
done
grep -qx 'Exec=mitcad' "$DIR/mitcad.desktop" || ui_fail "the desktop file does not start mitcad"
echo "ok   the executables, the desktop file, the icon, Qt's plugins, OCCT and the licences"
RENDER=0
if [ -e "$DIR/usr/bin/mitcad-render" ]; then
  RENDER=1
  for file in usr/share/doc/mitcad/licenses/cycles/LICENSE usr/share/doc/mitcad/licenses/cycles/BSD-3-Clause-license.txt \
    usr/share/doc/mitcad/licenses/openimagedenoise/LICENSE.txt usr/share/doc/mitcad/licenses/vcpkg/embree/copyright; do
    [ -e "$DIR/$file" ] || ui_fail "the AppImage has the render worker but no $file"
  done
  for library in libembree4 libtbb libOpenImageIO libOpenColorIO libOpenImageDenoise libOpenImageDenoise_core \
    libOpenImageDenoise_device_cpu; do
    compgen -G "$DIR/usr/lib/$library.so*" > /dev/null || ui_fail "the AppImage has the render worker but no $library"
  done
  for name in Cycles "Open Image Denoise" Embree OpenImageIO OpenColorIO; do
    grep -q "$name" "$DIR/usr/share/doc/mitcad/THIRD-PARTY-NOTICES.txt" || ui_fail "the notices do not list $name"
  done
  # Only the worker loads the renderer's libraries.
  render_libraries='libembree|libtbb|libOpenImageIO|libOpenColorIO|libOpenImageDenoise|libOpenEXR|libcrypto'
  if env -i /usr/bin/ldd "$DIR/usr/bin/mitcad" | grep -E "$render_libraries"; then
    ui_fail "mitcad links the renderer's libraries"
  fi
  echo "ok   the render worker mitcad-render, its libraries and their licences; mitcad links none of them"
else
  echo "note no render worker (a build without MITCAD_RENDER)"
fi

elves=0
glibc=0
while IFS= read -r -d '' file; do
  [ "$(head -c 4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ] || continue
  elves=$((elves + 1))
  relative=${file#"$DIR"/}
  while IFS= read -r path; do
    case $path in
      '$ORIGIN' | '$ORIGIN/'*) ;;
      *) ui_fail "$relative: RUNPATH $path is not inside the AppImage" ;;
    esac
  done < <(readelf -d "$file" | sed -n 's/.*(R*U*N*PATH).*\[\(.*\)\]$/\1/p' | tr ':' '\n')
  # The runtime loader's view of it, without the caller's environment.
  needs=$(env -i /usr/bin/ldd "$file" 2>&1) || continue # not dynamic
  while read -r name arrow path _; do
    [ "$arrow" = "=>" ] || continue
    [ "$path" = not ] && ui_fail "$relative: $name not found"
    library_ok "$path" "$DIR" || ui_fail "$relative: $name from $path, outside the AppImage and the system"
  done <<< "$needs"
  version=$(objdump -T "$file" 2> /dev/null | grep -o 'GLIBC_2\.[0-9]*' | cut -d. -f2 | sort -n | tail -1)
  [ -n "$version" ] && [ "$version" -gt "$glibc" ] && glibc=$version
done < <(find "$DIR" -type f -print0)
[ "$elves" -gt 50 ] || ui_fail "only $elves ELF files"
echo "ok   $elves ELF files: libraries inside the AppImage or the system's, RUNPATHs inside it"
echo "note the AppImage needs glibc 2.$glibc or newer"
rm -rf "$DIR"

echo "--- 2. Started with an empty environment"
ui_start_display
mkdir -p "$APPS" "$WORK/home"
cp "$IMAGE" "$COPY"
chmod 755 "$COPY"
plain() {
  env -i DISPLAY="$DISPLAY" HOME="$WORK/home" PATH=/usr/bin:/bin MITCAD_NO_UPDATE_CHECK=1 \
    ${APPIMAGE_EXTRACT_AND_RUN:+APPIMAGE_EXTRACT_AND_RUN=1} "$@"
}
if ! output=$(cd "$WORK" && plain "$COPY" --version 2>&1); then
  echo "note FUSE does not work here ($(echo "$output" | head -1)); APPIMAGE_EXTRACT_AND_RUN=1"
  export APPIMAGE_EXTRACT_AND_RUN=1
  output=$(cd "$WORK" && plain "$COPY" --version 2>&1) || ui_fail "--version failed: $output"
fi
VERSION=$(echo "$output" | sed -n 's/^Mitcad //p')
[ -n "$VERSION" ] || ui_fail "no version: $output"
if [ -n "$BUILT" ]; then
  [ "$("$BUILT" --version | sed -n 's/^Mitcad //p')" = "$VERSION" ] || ui_fail "the AppImage is not the build's version"
fi
echo "ok   --version: Mitcad $VERSION"

(cd "$WORK" && plain "$COPY" --demo --screenshot "$WORK/demo.png" > "$WORK/plain.log" 2>&1) &
starter=$!
window=""
for _ in $(seq 1 150); do
  window=$(xdotool search --onlyvisible --name ' - Mitcad$' 2> /dev/null | head -1)
  [ -n "$window" ] && break
  sleep 0.1
done
[ -n "$window" ] || { cat "$WORK/plain.log"; ui_fail "the AppImage showed no window"; }
pid=$(xdotool getwindowpid "$window")
appdir=$(tr '\0' '\n' < "/proc/$pid/environ" | sed -n 's/^APPDIR=//p')
[ -n "$appdir" ] || ui_fail "Mitcad runs without the AppImage's APPDIR"
loaded=$(grep -oE '/[^ ]+\.so[.0-9]*$' "/proc/$pid/maps" | sort -u)
count=0
while IFS= read -r library; do
  library_ok "$library" "$appdir" || ui_fail "Mitcad loaded $library"
  count=$((count + 1))
done <<< "$loaded"
grep -q "^$appdir/usr/lib/libTKernel" <<< "$loaded" || ui_fail "OCCT is not the AppImage's"
grep -q "^$appdir/usr/plugins/platforms/libqxcb.so" <<< "$loaded" || ui_fail "Qt's X11 plugin is not the AppImage's"
echo "ok   $count libraries loaded, from $appdir and the system's folders"
wait "$starter" || { cat "$WORK/plain.log"; ui_fail "the screenshot run failed"; }
file "$WORK/demo.png" | grep -q 'PNG image data, [1-9]' || ui_fail "no screenshot"
echo "ok   the demo block's screenshot ($(stat -c %s "$WORK/demo.png") bytes)"
entry=$WORK/home/.local/share/applications/mitcad.desktop
grep -qx "Exec=\"$COPY\"" "$entry" || ui_fail "no desktop file running the AppImage in $entry"
[ -f "$WORK/home/.local/share/icons/hicolor/256x256/apps/mitcad.png" ] || ui_fail "no icon in the user's data folder"
echo "ok   the desktop file and the icons in the user's data folder"

if [ "$RENDER" = 1 ]; then
  echo "--- 2b. View > Rendered with the AppImage's render worker"
  export MITCAD_RENDER_SAMPLES=4
  UI_GDB=0 UI_APP=$COPY ui_start_app --demo
  ui_step "View > Rendered" ui_command "Rendered"
  ui_expect_log "Render worker ready: Cycles" "the AppImage's worker runs Cycles"
  worker=$(grep -o "Render worker started (pid [0-9]*): .*" "$UI_LOG" | tail -1)
  pid=$(echo "$worker" | sed 's/.*(pid \([0-9]*\)).*/\1/')
  appdir=$(tr '\0' '\n' < "/proc/$(xdotool getwindowpid "$UI_WINDOW")/environ" | sed -n 's/^APPDIR=//p')
  [ "${worker##*: }" = "$appdir/usr/bin/mitcad-render" ] || ui_fail "the worker is not the AppImage's: $worker"
  for _ in $(seq 1 300); do
    grep -qE "Render view [0-9]+: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" && break
    ui_crashed && ui_fail "the app crashed"
    sleep 0.2
  done
  grep -qE "Render view [0-9]+: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" || ui_fail "the render did not finish"
  grep -qE "Render view [0-9]+: first frame" "$UI_LOG" || ui_fail "no rendered frame shown"
  loaded=$(grep -oE '/[^ ]+\.so[.0-9]*$' "/proc/$pid/maps" | sort -u)
  while IFS= read -r library; do
    library_ok "$library" "$appdir" || ui_fail "the worker loaded $library"
  done <<< "$loaded"
  for library in libembree4 libOpenImageDenoise_device_cpu; do
    grep -q "^$appdir/usr/lib/$library" <<< "$loaded" || ui_fail "the worker did not load the AppImage's $library"
  done
  echo "ok   rendered ($(grep -oE "Render view [0-9]+: $MITCAD_RENDER_SAMPLES samples in .*" "$UI_LOG" | tail -1)), with the AppImage's libraries"
  # The render device (mitcad#50): with a GPU build on a machine with a
  # GPU, the automatic choice renders on it with the AppImage's kernels.
  device=$(grep -o "Render device: .*" "$UI_LOG" | tail -1)
  if "$DIR/usr/bin/mitcad-render" --list-devices 2> /dev/null | grep -q '"type":"CUDA"'; then
    compgen -G "$DIR/usr/lib/mitcad/cycles/lib/kernel_*.cubin.zst" > /dev/null ||
      ui_fail "the worker has a CUDA device but the AppImage no kernels"
    grep -q "(CUDA) (asked for auto)" <<< "$device" || ui_fail "the AppImage's worker did not render on the GPU: $device"
  fi
  echo "ok   ${device:-no render device logged}"
  ui_stop_app
  unset MITCAD_RENDER_SAMPLES
fi

echo "--- 3. The AppImage updates itself"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 2 -subj /CN=localhost \
  -addext "subjectAltName=IP:127.0.0.1,DNS:localhost" -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
  > /dev/null 2>&1 || ui_fail "openssl could not make a certificate"
SEED=$(openssl rand -hex 32)
PUBLIC=$(MITCAD_RELEASE_KEY=$SEED "$RELEASE_TOOL" public-key) || ui_fail "no public key"
mkdir -p "$WWW/release"
python3 "$ROOT/tools/update-test-server.py" "$WWW" "$WORK/cert.pem" "$WORK/key.pem" "$WORK/requests.log" \
  "$WORK/port" &
SERVER=$!
for _ in $(seq 1 100); do
  [ -s "$WORK/port" ] && break
  sleep 0.1
done
[ -s "$WORK/port" ] || ui_fail "the HTTPS server did not start"
BASE=https://127.0.0.1:$(cat "$WORK/port")
asset=Mitcad-$VERSION-x86_64.AppImage
cp "$IMAGE" "$WWW/release/$asset"
MITCAD_RELEASE_KEY=$SEED "$RELEASE_TOOL" manifest --version "$VERSION" --date 2026-10-06 \
  --notes "$BASE/release/notes.html" --asset linux-x64 "$WWW/release/$asset" "$BASE/release/$asset" \
  --out "$WWW/release/update-manifest.json" > /dev/null || ui_fail "mitcad-release could not sign the AppImage"
echo "ok   the AppImage signed as linux-x64 with a key made for the test"

export MITCAD_UPDATE_URL=$BASE/release/update-manifest.json
export MITCAD_UPDATE_TEST_KEY=$PUBLIC MITCAD_UPDATE_TEST_CA=$WORK/cert.pem MITCAD_TEST_VERSION=0.0.0
unset MITCAD_NO_UPDATE_CHECK
inode=$(stat -c %i "$COPY")
UI_GDB=0 UI_APP=$COPY ui_start_app
ui_expect_log "Updates: Mitcad 0.0.0 for linux-x64, the AppImage $COPY" "the AppImage can update itself"
ui_expect_log "Update notice (offer): Mitcad $VERSION is available (this is 0.0.0). Mitcad checks for updates once a day; Tools > Preferences turns that off. [Release Notes, Install, Skip This Version, Later]" \
  "the notice offers Install"
OLD=$(xdotool getwindowpid "$UI_WINDOW")
ui_mark
ui_key alt+i
ui_expect_new "Update verified: $APPS/.Mitcad.AppImage.update" "the download is verified" 30
ui_expect_new "Update staged: 0.0.0 -> $VERSION" "the update waits for Mitcad to quit"
for _ in $(seq 1 100); do
  kill -0 "$OLD" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$OLD" 2> /dev/null && ui_fail "Mitcad did not quit"
ui_expect_new "Update: replaced $COPY" "the AppImage replaced once Mitcad quit"
ui_expect_new "Update installed: 0.0.0 -> $VERSION" "the new AppImage reports the update" 30
ui_expect_new "Update notice (message): Mitcad was updated to $VERSION. [Close]" "and tells the user"
cmp -s "$COPY" "$WWW/release/$asset" || ui_fail "the AppImage is not the release's"
[ "$(stat -c %i "$COPY")" != "$inode" ] || ui_fail "the AppImage was not replaced"
[ -x "$COPY" ] || ui_fail "the new AppImage is not executable"
[ -e "$APPS/.Mitcad.AppImage.update" ] && ui_fail "the download is left"
grep -q "^GET /release/$asset " "$WORK/requests.log" || ui_fail "the AppImage was not downloaded"
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
tr '\0' '\n' < "/proc/$UI_RUNNER/environ" | grep -qx "APPIMAGE=$COPY" || ui_fail "the new Mitcad is not the AppImage"
echo "ok   the new file, executable, started as the AppImage"
ui_stop_app

ui_finish "AppImage test"

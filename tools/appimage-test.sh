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
#      RUNPATHs stay inside it. Static checks of what a user's system must
#      have (mitcad#789), in seconds: no ELF file (nor the AppImage's
#      runtime) needs a glibc or libstdc++ symbol version newer than
#      GLIBC_FLOOR, GLIBCXX_FLOOR and CXXABI_FLOOR, those of the oldest
#      supported system (Ubuntu 24.04), and every ELF
#      file that links Qt finds each libQt6 library inside the AppImage,
#      through its own $ORIGIN RUNPATH and in ldd's view, never a system's
#      Qt. Prints the newest glibc version it needs. OpenSSL (mitcad#954):
#      libssl.so.3 and libcrypto.so.3 are both bundled or both absent, and
#      no file names a library folder of the build machine (vcpkg's install
#      trees, the render libraries' prefix, the built executables'
#      RUNPATHs), as OpenSSL's ENGINESDIR and MODULESDIR did.
#   2. Copied into a folder of its own and started with an empty
#      environment (env -i; PATH /usr/bin:/bin, and update checks off):
#      --version, then the demo block's screenshot (--screenshot), while
#      every library the running Mitcad has loaded (/proc/<pid>/maps) is
#      the AppImage's or the system's, and its desktop file (Exec the
#      AppImage) and icons are in the user's data folder.
#   2b. The render worker mitcad-render: its libraries and their
#      licences are in it, mitcad links none of them, and View > Rendered
#      renders the demo block with them (on the GPU, with the AppImage's
#      kernels, when the worker has a CUDA device; mitcad#50). A release
#      build (mitcad#202: configured with MITCAD_RENDER_REQUIRED_DEVICES
#      naming CUDA, or --require-cuda) fails without the CUDA kernels and
#      cuew's licence, whether or not this machine has a GPU.
#   3. The automatic update (docs/updates.md): the AppImage, taking itself
#      for 0.0.0 (MITCAD_TEST_VERSION), finds a release whose manifest signs
#      this AppImage as the linux-x64 download (mitcad-release, with a key
#      made for the test), served over HTTPS from 127.0.0.1 with a
#      certificate made for the test (tools/update-test-server.py; needs
#      python3 and openssl), with the AppImage's OpenSSL when it has one
#      (the logged TLS library is the bundled libcrypto's version, and the
#      running Mitcad has loaded libssl and libcrypto from its usr/lib
#      only), else one system's pair; Install downloads and verifies it, Mitcad
#      quits, the download replaces the AppImage (a new file of the same
#      name) and starts as the release, which reports the update.
#      Only a test build reads the tests' variables (the CMake option
#      MITCAD_UPDATE_TEST_HOOKS, mitcad#921; the built mitcad's
#      CMakeCache.txt tells, or without it the AppImage's log). A release's
#      AppImage, without the option, is checked to ignore them instead:
#      its own version, GitHub's addresses, the built-in key only, no
#      request to the test's server.
# Without a working FUSE the AppImage runs with APPIMAGE_EXTRACT_AND_RUN=1.
#
# Usage: tools/appimage-test.sh [--skip-stale] [--require-cuda] <AppImage> [<built mitcad>] [<mitcad-release>]
#   --skip-stale (the ctest app.appimage): exit 77, skipped, when there is
#   no AppImage newer than the built mitcad.
#   --require-cuda: the AppImage must have the CUDA kernels; implied when
#   the built mitcad's build folder (its CMakeCache.txt) requires CUDA.
# The release tool defaults to build/dev/core/mitcad-release.

SKIP_STALE=0
REQUIRE_CUDA=0
while [ $# -gt 0 ]; do
  case $1 in
    --skip-stale) SKIP_STALE=1 ;;
    --require-cuda) REQUIRE_CUDA=1 ;;
    *) break ;;
  esac
  shift
done
IMAGE=${1:?usage: tools/appimage-test.sh [--skip-stale] [--require-cuda] <AppImage> [<built mitcad>] [<mitcad-release>]}
BUILT=${2:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
RELEASE_TOOL=${3:-$ROOT/build/dev/core/mitcad-release}
# The newest glibc the AppImage may need: Ubuntu 24.04's (and the
# distributions based on it), which docs/user-guide.md promises. A release
# built on a newer system needs that system's glibc (mitcad#789); build it
# on Ubuntu 24.04 (docs/development.md#linux-appimage).
GLIBC_FLOOR=2.39
# Likewise libstdc++, which stays the system's too: the newest symbol
# versions the libstdc++ of Ubuntu 24.04's libstdc++6 package (GCC 14's)
# provides, so a build with g++-14 there is fine.
GLIBCXX_FLOOR=3.4.33
CXXABI_FLOOR=1.3.15
# The build folder of the built mitcad (<build>/app/mitcad): a release
# configure asks for CUDA there (cmake/Render.cmake).
if [ -n "$BUILT" ] && grep -qE '^MITCAD_RENDER_REQUIRED_DEVICES:[A-Z]+=.*\bCUDA\b' \
  "$(dirname "$BUILT")/../CMakeCache.txt" 2> /dev/null; then
  REQUIRE_CUDA=1
fi
if [ "$SKIP_STALE" = 1 ] && { [ ! -f "$IMAGE" ] || { [ -n "$BUILT" ] && [ "$IMAGE" -ot "$BUILT" ]; }; }; then
  echo "SKIP: no AppImage of this build; run: cmake --build --preset dev --target appimage"
  exit 77
fi
[ -f "$IMAGE" ] || { echo "FAIL: no AppImage $IMAGE"; exit 1; }
IMAGE=$(readlink -f "$IMAGE")

source "$ROOT/tools/ui-test-lib.sh"
# Whether the AppImage reads the update tests' variables (mitcad#921): ON,
# OFF, or empty (no build folder: section 3 asks the AppImage).
HOOKS=""
[ -n "$BUILT" ] && HOOKS=$(ui_update_test_hooks "$BUILT")

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

for tool in python3 openssl readelf objdump ldd; do
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
  usr/share/doc/mitcad/licenses/droid-sans/LICENSE.txt usr/share/doc/mitcad/licenses/qtkeychain/COPYING; do
  [ -e "$DIR/$file" ] || ui_fail "the AppImage has no $file"
done
# The annotations' lettering (mitcad#902) and its licence, when the source
# has it (third_party/fonts/iso-lettering).
if [ -e "$(dirname "$0")/../third_party/fonts/iso-lettering/lettering.ttf" ]; then
  for file in usr/share/mitcad/fonts/lettering.ttf usr/share/doc/mitcad/licenses/iso-lettering/LICENSE.txt; do
    [ -e "$DIR/$file" ] || ui_fail "the AppImage has no $file"
  done
fi
# Qt's D-Bus module: the keychain's (KWallet, the Secret Service).
for library in libQt6Core libQt6Widgets libQt6Network libQt6Svg libQt6XcbQpa libQt6DBus libTKernel libTKV3d libTKOpenGl; do
  compgen -G "$DIR/usr/lib/$library.so*" > /dev/null || ui_fail "the AppImage has no $library"
done
grep -qx 'Exec=mitcad' "$DIR/mitcad.desktop" || ui_fail "the desktop file does not start mitcad"
# The HEIF decoder of image references (mitcad#850) when mitcad links it
# (MITCAD_HEIF): shared libraries of their own, and their licence texts.
if readelf -d "$DIR/usr/bin/mitcad" | grep -q 'NEEDED.*libheif'; then
  for library in libheif libde265; do
    compgen -G "$DIR/usr/lib/$library.so*" > /dev/null || ui_fail "the AppImage has no $library"
    [ -e "$DIR/usr/share/doc/mitcad/licenses/vcpkg/$library/copyright" ] ||
      ui_fail "the AppImage has no licence text of $library"
  done
  grep -q "libheif" "$DIR/usr/share/doc/mitcad/THIRD-PARTY-NOTICES.txt" || ui_fail "the notices do not list libheif"
  echo "ok   libheif and libde265 as shared libraries, with their licence texts"
fi
echo "ok   the executables, the desktop file, the icon, Qt's plugins, OCCT and the licences"
[ -e "$DIR/usr/bin/mitcad-render" ] || ui_fail "the AppImage has no render worker usr/bin/mitcad-render"
for file in usr/share/doc/mitcad/licenses/cycles/LICENSE usr/share/doc/mitcad/licenses/cycles/BSD-3-Clause-license.txt \
  usr/share/doc/mitcad/licenses/openimagedenoise/LICENSE.txt usr/share/doc/mitcad/licenses/vcpkg/embree/copyright; do
  [ -e "$DIR/$file" ] || ui_fail "the AppImage has no $file"
done
for library in libembree4 libtbb libOpenImageIO libOpenColorIO libOpenImageDenoise libOpenImageDenoise_core \
  libOpenImageDenoise_device_cpu; do
  compgen -G "$DIR/usr/lib/$library.so*" > /dev/null || ui_fail "the AppImage has no $library"
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
# OpenSSL (mitcad#954, packaging/linux/make-appimage.sh): libssl.so.3 and
# libcrypto.so.3 both bundled, which Qt's TLS plugin then loads from
# usr/lib, or both the system's; never one of each, which mixes two
# OpenSSL builds in one process. The bundled libcrypto's run-time folders
# (OPENSSLDIR, ENGINESDIR, MODULESDIR) are none of the build machine's.
openssl_dir() { grep -aoE "$1: \"[^\"]*\"" "$2" | head -1 | sed -E "s/^$1: \"(.*)\"\$/\1/"; }
BUNDLED_TLS=""
ssl_bundled=$(compgen -G "$DIR/usr/lib/libssl.so*")
crypto_bundled=$(compgen -G "$DIR/usr/lib/libcrypto.so*")
if [ -n "$ssl_bundled" ] && [ -n "$crypto_bundled" ]; then
  [ -f "$DIR/usr/lib/libssl.so.3" ] && [ -f "$DIR/usr/lib/libcrypto.so.3" ] ||
    ui_fail "the AppImage's OpenSSL is not libssl.so.3 and libcrypto.so.3: $(basename -a $ssl_bundled $crypto_bundled | tr '\n' ' ')"
  BUNDLED_TLS=$(grep -aoE 'OpenSSL [0-9]+\.[0-9]+\.[0-9]+[^ [:cntrl:]]* +[0-9]+ [A-Z][a-z]+ [0-9]{4}' \
    "$DIR/usr/lib/libcrypto.so.3" | head -1)
  [ -n "$BUNDLED_TLS" ] || ui_fail "no version text in the bundled libcrypto.so.3"
  folders=""
  for name in OPENSSLDIR ENGINESDIR MODULESDIR; do
    folder=$(openssl_dir "$name" "$DIR/usr/lib/libcrypto.so.3")
    case $folder in
      "${HOME:-/no-home-set}"/* | "$ROOT"/* | */vcpkg_installed/* | */vcpkg/*)
        ui_fail "the bundled libcrypto's $name is the build machine's $folder" ;;
    esac
    folders="$folders, $name ${folder:-none}"
  done
  echo "ok   libssl.so.3 and libcrypto.so.3 bundled ($BUNDLED_TLS$folders)"
elif [ -n "$ssl_bundled$crypto_bundled" ]; then
  ui_fail "the AppImage bundles $(basename -a $ssl_bundled $crypto_bundled | tr '\n' ' ')without its pair, so Qt's TLS would mix it with the system's"
else
  echo "ok   no bundled OpenSSL: Qt's TLS loads the system's libssl and libcrypto"
fi
# No file names a folder of the build machine that libraries are looked up
# in (mitcad#954: the bundled libcrypto's ENGINESDIR and MODULESDIR were
# vcpkg's install folder): vcpkg's install trees, the render libraries'
# prefix and the built executables' RUNPATHs. Source file names in
# messages (OCCT's, Cycles', the Rust crates') are no such folders.
lookup=(-e /vcpkg_installed/)
if [ -n "$BUILT" ]; then
  for program in "$BUILT" "$(dirname "$BUILT")/mitcad-render"; do
    [ -f "$program" ] || continue
    while IFS= read -r path; do
      case $path in
        '' | '$ORIGIN'*) ;;
        *) lookup+=(-e "$path") ;;
      esac
    done < <(readelf -d "$program" | sed -n 's/.*(R*U*N*PATH).*\[\(.*\)\]$/\1/p' | tr ':' '\n')
  done
  deps=$(sed -n 's/^MITCAD_RENDER_DEPS:[A-Z]*=//p' "$(dirname "$BUILT")/../CMakeCache.txt" 2> /dev/null)
  [ -n "$deps" ] && lookup+=(-e "$deps/install/" -e "$deps/vcpkg_installed")
fi
carriers=$(grep -rlaF "${lookup[@]}" "$DIR" | sed "s|^$DIR/||")
[ -z "$carriers" ] || ui_fail "these files name the build machine's library folders ($(printf '%s ' "${lookup[@]}" | sed 's/-e //g')): $(echo $carriers)"
echo "ok   no file names the build machine's library folders"
# The CUDA kernels (mitcad#202): a release must have them, with cuew's
# licence, whatever this machine's GPU.
CUBINS=$(compgen -G "$DIR/usr/lib/mitcad/cycles/lib/kernel_sm_*.cubin.zst" | wc -l)
if [ "$CUBINS" -gt 0 ]; then
  [ -e "$DIR/usr/share/doc/mitcad/licenses/cycles/cuew-license.txt" ] || ui_fail "the AppImage has CUDA kernels but no cuew licence"
  echo "ok   $CUBINS CUDA kernels ($(du -ch "$DIR"/usr/lib/mitcad/cycles/lib/* | tail -1 | cut -f1)) and cuew's licence"
elif [ "$REQUIRE_CUDA" = 1 ]; then
  ui_fail "a release AppImage without CUDA kernels (usr/lib/mitcad/cycles/lib/kernel_sm_*.cubin.zst)"
else
  echo "note no CUDA kernels (a renderer without its CUDA device)"
fi

elves=0
glibc=2.0
qt_users=0
REAL_DIR=$(readlink -f "$DIR")
# newer a b: whether version a is newer than version b.
newer() {
  [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]
}
# check_versions name file: fails when the file needs a GLIBC_, GLIBCXX_
# or CXXABI_ symbol version newer than its floor; a static file needs none.
check_versions() {
  local symbols prefix floor newest
  symbols=$(objdump -T "$2" 2> /dev/null)
  for prefix in GLIBC GLIBCXX CXXABI; do
    floor=${prefix}_FLOOR
    floor=${!floor}
    newest=$(grep -oE "\b${prefix}_[0-9]+(\.[0-9]+)*" <<< "$symbols" | sed "s/^${prefix}_//" | sort -V | tail -1)
    [ -n "$newest" ] || continue
    newer "$newest" "$floor" &&
      ui_fail "$1 needs ${prefix}_$newest, newer than ${prefix}_FLOOR $floor; build the release on Ubuntu 24.04"
    [ "$prefix" = GLIBC ] && newer "$newest" "$glibc" && glibc=$newest
  done
  return 0
}
# The AppImage's runtime: a static one needs no glibc.
check_versions "the AppImage's runtime" "$IMAGE"
while IFS= read -r -d '' file; do
  [ "$(head -c 4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ] || continue
  elves=$((elves + 1))
  relative=${file#"$DIR"/}
  dynamic=$(readelf -d "$file" 2> /dev/null)
  runpath=$(sed -n 's/.*(R*U*N*PATH).*\[\(.*\)\]$/\1/p' <<< "$dynamic")
  runpaths=$(tr ':' '\n' <<< "$runpath")
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    case $path in
      '$ORIGIN' | '$ORIGIN/'*) ;;
      *) ui_fail "$relative: RUNPATH $path is not inside the AppImage" ;;
    esac
  done <<< "$runpaths"
  # Qt only from the AppImage (mitcad#789): each libQt6 library the file
  # links is found through its own RUNPATH, inside the AppImage, so the
  # loader never takes a system's Qt instead (whatever was loaded before).
  while IFS= read -r library; do
    [ -n "$library" ] || continue
    found=""
    while IFS= read -r path; do
      [ -n "$path" ] || continue
      below=${path#\$ORIGIN} # every entry starts with $ORIGIN (above)
      candidate=$(readlink -f "$(dirname "$file")$below/$library")
      case $candidate in
        "$REAL_DIR"/*) [ -f "$candidate" ] && found=$candidate ;;
      esac
    done <<< "$runpaths"
    [ -n "$found" ] || ui_fail "$relative: its RUNPATH ($runpath) does not find $library in the AppImage, so a system's Qt would be loaded"
    qt_users=$((qt_users + 1))
  done < <(sed -n 's/.*(NEEDED).*\[\(libQt6[^]]*\)\]$/\1/p' <<< "$dynamic")
  check_versions "$relative" "$file"
  # The runtime loader's view of it, without the caller's environment
  # (no LD_LIBRARY_PATH). A file that links nothing (a static one) has none;
  # ldd failing on any other is a fault, not a file to skip.
  grep -q '(NEEDED)' <<< "$dynamic" || continue
  needs=$(env -i /usr/bin/ldd "$file" 2>&1) || ui_fail "$relative: ldd failed: $(head -3 <<< "$needs")"
  while read -r name arrow path _; do
    [ "$arrow" = "=>" ] || continue
    [ "$path" = not ] && ui_fail "$relative: $name not found"
    library_ok "$path" "$DIR" || ui_fail "$relative: $name from $path, outside the AppImage and the system"
    case $name:$path in
      libQt6*:"$DIR"/*) ;;
      libQt6*) ui_fail "$relative: $name from $path, not the AppImage's Qt" ;;
    esac
  done <<< "$needs"
done < <(find "$DIR" -type f -print0)
[ "$elves" -gt 50 ] || ui_fail "only $elves ELF files"
[ "$qt_users" -gt 0 ] || ui_fail "no ELF file links Qt"
echo "ok   $elves ELF files: libraries inside the AppImage or the system's, RUNPATHs inside it, Qt only the AppImage's"
echo "ok   the AppImage needs glibc $glibc or newer (at most $GLIBC_FLOOR), libstdc++ at most GLIBCXX_$GLIBCXX_FLOOR and CXXABI_$CXXABI_FLOOR"
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
  window=$(xdotool search --onlyvisible --name ' - MitCad$' 2> /dev/null | head -1)
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
# The running AppImage's worker (section 1's extracted copy is gone).
if env -i PATH=/usr/bin:/bin "$appdir/usr/bin/mitcad-render" --list-devices 2> /dev/null | grep -q '"type":"CUDA"'; then
  [ "$CUBINS" -gt 0 ] || ui_fail "the worker has a CUDA device but the AppImage no kernels"
  grep -q "(CUDA) (asked for auto)" <<< "$device" || ui_fail "the AppImage's worker did not render on the GPU: $device"
fi
echo "ok   ${device:-no render device logged}"
ui_stop_app
unset MITCAD_RENDER_SAMPLES

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
if [ -z "$HOOKS" ]; then
  # No build folder: the AppImage's log says whether it reads the variables
  # (checks off, its source is logged anyway).
  MITCAD_NO_UPDATE_CHECK=1 UI_GDB=0 UI_APP=$COPY ui_start_app
  ui_expect_log "Update source: " "the AppImage logs where its updates come from"
  if grep -qF "Update source: this test build reads" "$UI_LOG"; then HOOKS=ON; else HOOKS=OFF; fi
  ui_stop_app
  echo "note no build folder given: the AppImage's test hooks are $HOOKS by its own log"
fi
# A release (--require-cuda or its build's CUDA requirement) never has them.
[ "$REQUIRE_CUDA" = 1 ] && [ "$HOOKS" = ON ] &&
  ui_fail "a release's AppImage reads the update tests' variables (configure it with -DMITCAD_UPDATE_TEST_HOOKS=OFF)"
if [ "$HOOKS" = OFF ]; then
  # A release's AppImage (mitcad#921): its check would go to GitHub, so
  # checks off; its source is logged anyway.
  MITCAD_NO_UPDATE_CHECK=1 UI_GDB=0 UI_APP=$COPY ui_start_app
  ui_expect_log "Updates: Mitcad $VERSION for linux-x64, the AppImage $COPY" "its own version, not MITCAD_TEST_VERSION"
  ui_expect_log "Update source: manifest at https://github.com/mitcad/Mitcad/releases/latest/download/update-manifest.json, releases at https://api.github.com/repos/mitcad/Mitcad/releases?per_page=20, " \
    "GitHub's addresses, not MITCAD_UPDATE_URL"
  ui_expect_log "Update source: this build ignores MITCAD_UPDATE_URL, MITCAD_UPDATE_TEST_KEY, MITCAD_UPDATE_TEST_CA, MITCAD_TEST_VERSION (MITCAD_UPDATE_TEST_HOOKS off)" \
    "a release ignores the update tests' variables"
  grep -qE "and the test key|test certificate" "$UI_LOG" && ui_fail "the AppImage took the test's key or certificate"
  echo "ok   the built-in key only, no test certificate"
  sleep 4
  [ -s "$WORK/requests.log" ] && ui_fail "requests to the test's server: $(cat "$WORK/requests.log")"
  echo "ok   no request to the test's server"
  ui_stop_app
  ui_finish "AppImage test (a release build: the update tests' variables ignored)"
  exit 0
fi
inode=$(stat -c %i "$COPY")
UI_GDB=0 UI_APP=$COPY ui_start_app
ui_expect_log "Updates: Mitcad 0.0.0 for linux-x64, the AppImage $COPY" "the AppImage can update itself"
ui_expect_log "Update source: this test build reads MITCAD_UPDATE_URL, MITCAD_UPDATE_TEST_KEY, MITCAD_UPDATE_TEST_CA, MITCAD_TEST_VERSION (MITCAD_UPDATE_TEST_HOOKS)" \
  "a test build reads the update tests' variables"
ui_expect_log "Update notice (offer): MitCad $VERSION is available (this is 0.0.0). MitCad checks for updates once a day; Tools › Preferences turns that off. [Release Notes, Install, Skip This Version, Later]" \
  "the notice offers Install"
OLD=$(xdotool getwindowpid "$UI_WINDOW")
# The TLS of the check (mitcad#954): the bundled OpenSSL, libssl and
# libcrypto both from the AppImage's usr/lib and no other; without one,
# both from one system folder.
if [ -n "$BUNDLED_TLS" ]; then
  ui_expect_log "Update check: TLS $BUNDLED_TLS (openssl backend)" "the update check's TLS is the bundled $BUNDLED_TLS"
else
  ui_expect_log "Update check: TLS OpenSSL " "the update check's TLS is the system's OpenSSL"
fi
appdir=$(tr '\0' '\n' < "/proc/$OLD/environ" | sed -n 's/^APPDIR=//p')
openssl_loaded=$(grep -oE '/[^ ]+/lib(ssl|crypto)\.so[.0-9]*$' "/proc/$OLD/maps" | sort -u)
for library in libssl.so.3 libcrypto.so.3; do
  grep -q "/$library\$" <<< "$openssl_loaded" || ui_fail "Mitcad has not loaded $library: $(echo $openssl_loaded)"
done
if [ -n "$BUNDLED_TLS" ]; then
  [ "$openssl_loaded" = "$(printf '%s\n' "$appdir/usr/lib/libcrypto.so.3" "$appdir/usr/lib/libssl.so.3")" ] ||
    ui_fail "Mitcad's OpenSSL is not only the AppImage's: $(echo $openssl_loaded)"
else
  [ "$(dirname $openssl_loaded | sort -u | wc -l)" = 1 ] ||
    ui_fail "Mitcad's libssl and libcrypto come from different folders: $(echo $openssl_loaded)"
fi
echo "ok   libssl and libcrypto loaded from $(dirname $openssl_loaded | sort -u)"
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
ui_expect_new "Update notice (message): MitCad was updated to $VERSION. [Close]" "and tells the user"
cmp -s "$COPY" "$WWW/release/$asset" || ui_fail "the AppImage is not the release's"
[ "$(stat -c %i "$COPY")" != "$inode" ] || ui_fail "the AppImage was not replaced"
[ -x "$COPY" ] || ui_fail "the new AppImage is not executable"
[ -e "$APPS/.Mitcad.AppImage.update" ] && ui_fail "the download is left"
grep -q "^GET /release/$asset " "$WORK/requests.log" || ui_fail "the AppImage was not downloaded"
# The new process is the AppImage's own, no child of this script.
UI_WINDOW=""
for _ in $(seq 1 100); do
  UI_WINDOW=$(xdotool search --onlyvisible --name ' - MitCad$' 2> /dev/null | head -1)
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

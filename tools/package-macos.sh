#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Packages a built Mitcad as a macOS disk image: installs the application
# bundle with Qt and the vcpkg libraries inside (cmake --install, see
# app/packaging/macos/Bundle.cmake), signs it, checks it
# (tools/check-bundle-macos.sh) and makes a compressed .dmg with an
# /Applications link.
#
# By default the bundle is signed AD HOC (`codesign --sign -`): it runs on
# Apple Silicon, which requires some signature, but Gatekeeper rejects a
# downloaded copy until the user allows it in System Settings (Privacy and
# Security) or removes the quarantine attribute. That is right for
# development builds and CI.
#
# A release for other people needs an Apple Developer account (paid; the
# Developer ID Application certificate and notarisation). Both are PREPARED
# BUT OFF: nothing below runs unless these variables are set.
#   MITCAD_SIGN_IDENTITY   the certificate, e.g. "Developer ID Application:
#                          Name (TEAMID)", from `security find-identity -v -p
#                          codesigning`. Signs with the hardened runtime,
#                          app/packaging/macos/entitlements.plist and a
#                          secure timestamp (needs network).
#   MITCAD_NOTARY_PROFILE  a keychain profile made once with
#                          `xcrun notarytool store-credentials <profile>`
#                          (Apple ID, team ID and an app-specific password, or
#                          an App Store Connect API key). Submits the .dmg to
#                          Apple's notary service, waits and staples the
#                          ticket. Needs MITCAD_SIGN_IDENTITY: an ad-hoc
#                          signature cannot be notarised.
# The bundle identifier (MITCAD_BUNDLE_ID in app/packaging/macos/Bundle.cmake)
# must be final before the first signed release.
#
# Other variables: MITCAD_CONFIG (the configuration for a multi-config
# generator, passed to cmake --install), MITCAD_PACKAGE_DIR (output directory,
# default <build dir>/package).
#
# Runs on macOS only, where Mitcad was built (the macOS build environment,
# see docs/development.md). Bash 3.2 compatible.
# Usage: tools/package-macos.sh [<build dir, default build/macos-release>]
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$(pwd -P)

if [ "$(uname -s)" != Darwin ]; then
  echo "package-macos.sh runs on macOS only" >&2
  exit 1
fi

build_dir=${1:-build/macos-release}
if [ ! -f "$build_dir/CMakeCache.txt" ]; then
  echo "$build_dir is not a CMake build directory (build Mitcad first)" >&2
  exit 1
fi
build_dir=$(cd "$build_dir" && pwd -P)

sign_identity=${MITCAD_SIGN_IDENTITY:-}
notary_profile=${MITCAD_NOTARY_PROFILE:-}
if [ -n "$notary_profile" ] && [ -z "$sign_identity" ]; then
  echo "MITCAD_NOTARY_PROFILE needs MITCAD_SIGN_IDENTITY: an ad-hoc signature cannot be notarised" >&2
  exit 1
fi

out_dir=${MITCAD_PACKAGE_DIR:-$build_dir/package}
mkdir -p "$out_dir"
out_dir=$(cd "$out_dir" && pwd -P)
stage="$out_dir/stage"
dmg_root="$out_dir/dmg-root"
rm -rf "$stage" "$dmg_root"
mkdir -p "$stage" "$dmg_root"

# --- Install the bundle -------------------------------------------------------
install_args=(--install "$build_dir" --prefix "$stage")
if [ -n "${MITCAD_CONFIG:-}" ]; then
  install_args+=(--config "$MITCAD_CONFIG")
fi
echo "== Installing into $stage"
cmake "${install_args[@]}"

# The bundle is installed as mitcad.app, after the executable (the tests use
# it); the product is called Mitcad. A rename in two steps because the file
# system ignores case.
[ -d "$stage/mitcad.app" ] || { echo "cmake --install left no mitcad.app in $stage" >&2; exit 1; }
mv "$stage/mitcad.app" "$stage/Mitcad.app.tmp"
mv "$stage/Mitcad.app.tmp" "$stage/Mitcad.app"
app="$stage/Mitcad.app"

version=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$app/Contents/Info.plist")
executable=$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$app/Contents/Info.plist")
archs=$(lipo -archs "$app/Contents/MacOS/$executable" | tr ' ' '+')

# --- Remove build paths from the rpaths ---------------------------------------
# macdeployqt leaves absolute rpaths (of Qt's or vcpkg's build) in some
# binaries; they are of no use in the bundle and leak the build machine's
# paths. The signature is made afterwards.
rpaths_of() {
  otool -l "$1" | awk '
    $1 == "cmd" { in_rpath = ($2 == "LC_RPATH") }
    in_rpath && $1 == "path" {
      sub(/^[ \t]*path /, ""); sub(/ \(offset [0-9]+\)$/, "")
      if (!seen[$0]++) print
      in_rpath = 0
    }'
}
echo "== Removing absolute rpaths"
while IFS= read -r -d '' file_path; do
  case "$(file -b "$file_path")" in
    Mach-O*) ;;
    *) continue ;;
  esac
  while IFS= read -r rpath; do
    case "$rpath" in
      '' | @*) ;;
      *)
        echo "  $rpath: ${file_path#"$app"/}"
        install_name_tool -delete_rpath "$rpath" "$file_path"
        ;;
    esac
  done < <(rpaths_of "$file_path")
done < <(find "$app" -type f -print0)

# --- Sign ---------------------------------------------------------------------
# --deep signs the nested code (frameworks, plug-ins, dylibs) first.
if [ -z "$sign_identity" ]; then
  echo "== Signing ad hoc (set MITCAD_SIGN_IDENTITY for a Developer ID signature)"
  codesign --force --deep --sign - --timestamp=none "$app"
else
  echo "== Signing with the identity '$sign_identity' (hardened runtime)"
  codesign --force --deep --sign "$sign_identity" --options runtime \
    --entitlements "$repo/app/packaging/macos/entitlements.plist" --timestamp "$app"
fi

echo "== Verifying the signature"
codesign --verify --deep --strict --verbose=2 "$app"

echo "== Checking the bundle"
"$repo/tools/check-bundle-macos.sh" "$app"

# --- Disk image -----------------------------------------------------------------
dmg="$out_dir/Mitcad-$version-macos-$archs.dmg"
rm -f "$dmg"
echo "== Creating $dmg"
ditto "$app" "$dmg_root/Mitcad.app"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -volname "Mitcad $version" -srcfolder "$dmg_root" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$dmg_root"

# --- Developer ID and notarisation (off unless the variables are set) --------------
if [ -n "$sign_identity" ]; then
  codesign --force --sign "$sign_identity" --timestamp "$dmg"
fi

if [ -n "$notary_profile" ]; then
  echo "== Notarising (Apple's notary service; this can take minutes)"
  log=$(xcrun notarytool submit "$dmg" --keychain-profile "$notary_profile" --wait 2>&1 | tee /dev/stderr)
  if ! printf '%s\n' "$log" | grep -q 'status: Accepted'; then
    submission=$(printf '%s\n' "$log" | awk '$1 == "id:" { print $2; exit }')
    echo "Notarisation was not accepted; the notary log:" >&2
    [ -z "$submission" ] || xcrun notarytool log "$submission" --keychain-profile "$notary_profile" >&2 || true
    exit 1
  fi
  echo "== Stapling the ticket to the disk image"
  xcrun stapler staple "$dmg"
  xcrun stapler validate "$dmg"
  # Gatekeeper's verdict on the app, which only a notarised bundle passes.
  spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
fi

echo "== Done: $dmg"
shasum -a 256 "$dmg"

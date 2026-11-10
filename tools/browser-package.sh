#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The web browser component's package for a release (mitcad#1579,
# docs/updates.md "The web browser component"), on Linux and macOS
# (tools/browser-package.ps1 on Windows): built only when what it is built
# from changed since the package the latest release's manifest lists.
#
#   tools/browser-package.sh <build dir> <latest update-manifest.json or ""> <release public key>
#
# 1. The fingerprint: SHA-256 over the pinned CEF version (versions.sh), the
#    module interface (ComponentBrowserHost.hpp) and every file the module
#    is built from (app/webbrowser, app/files/FileFormats.*,
#    packaging/browser).
# 2. The latest manifest's package for this platform and interface
#    (mitcad-release browser): the same fingerprint means unchanged, and the
#    release keeps that package (mitcad-release manifest --keep-browser).
# 3. Otherwise a new package: the build configured with the component
#    required and a version of the build's time (YYYY.MDD.HMM, a semantic
#    version without leading zeros), the module, helper and CEF's runtime
#    built, on macOS signed (MITCAD_SIGN_IDENTITY as tools/package-macos.sh,
#    else ad hoc), and zipped (target browser-package).
#
# Writes <build dir>/browser-package.txt, one line the release passes to
# mitcad-release manifest as `--browser <line> <url>` for a new package:
#   PLATFORM ABI VERSION CEF FINGERPRINT FILE
# or `unchanged` when the latest manifest's package stays. Needs CEF
# (tools/dev-env/fetch-cef.sh) and mitcad-release in the build.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
build=${1:?usage: browser-package.sh <build dir> <latest update-manifest.json or ""> <release public key>}
latest=${2-}
key=${3-}
# shellcheck source=tools/dev-env/versions.sh
. "$repo/tools/dev-env/versions.sh"

case "$(uname -s)" in
  Linux) platform=linux-x64 ;;
  Darwin) platform=macos-arm64 ;;
  *) echo "browser-package.sh: Linux and macOS only (Windows: browser-package.ps1)" >&2; exit 1 ;;
esac
abi=$(sed -n 's/.*constexpr int kComponentBrowserAbi = \([0-9]*\);.*/\1/p' "$repo/app/webbrowser/ComponentBrowserHost.hpp")
[ -n "$abi" ] || { echo "browser-package.sh: no kComponentBrowserAbi in ComponentBrowserHost.hpp" >&2; exit 1; }

sha256() { if command -v sha256sum > /dev/null 2>&1; then sha256sum | cut -d ' ' -f 1; else shasum -a 256 | cut -d ' ' -f 1; fi; }
fingerprint=$(
  cd "$repo"
  {
    echo "cef $CEF_VERSION"
    echo "abi $abi"
    find app/webbrowser packaging/browser app/files/FileFormats.cpp app/files/FileFormats.hpp -type f |
      LC_ALL=C sort | while IFS= read -r file; do
        echo "file $file $(sha256 < "$file")"
      done
  } | sha256
)
echo "browser-package.sh: $platform, interface $abi, CEF $CEF_VERSION, fingerprint $fingerprint"

release_tool=$(find "$build" -maxdepth 4 -type f -name mitcad-release -perm -u+x | head -1)
if [ -n "$latest" ] && [ -f "$latest" ]; then
  [ -n "$release_tool" ] || { echo "browser-package.sh: no mitcad-release in $build" >&2; exit 1; }
  listed=$("$release_tool" browser --manifest "$latest" --key "$key" --platform "$platform" --abi "$abi" |
    sed -n 's/^fingerprint //p')
  if [ "$listed" = "$fingerprint" ]; then
    echo "browser-package.sh: unchanged since the latest release's package: kept"
    echo unchanged > "$build/browser-package.txt"
    exit 0
  fi
fi

version="$(date -u +%Y).$((10#$(date -u +%m) * 100 + 10#$(date -u +%d))).$((10#$(date -u +%H) * 100 + 10#$(date -u +%M)))"
echo "browser-package.sh: building the package $version"
cmake -S "$repo" -B "$build" -DMITCAD_BROWSER_COMPONENT=ON -DMITCAD_BROWSER_VERSION="$version" > /dev/null
cmake --build "$build" --target mitcad-webbrowser
component="$build/browser-component"
if [ "$platform" = macos-arm64 ]; then
  identity=${MITCAD_SIGN_IDENTITY:--}
  options=()
  [ "$identity" = - ] || options=(--options runtime --timestamp)
  # The framework, the helper apps, then the module.
  codesign --force --sign "$identity" "${options[@]}" "$component/Chromium Embedded Framework.framework"
  for helper in "$component"/*.app; do
    codesign --force --sign "$identity" "${options[@]}" "$helper"
  done
  codesign --force --sign "$identity" "${options[@]}" "$component/mitcad-webbrowser.so"
fi
cmake --build "$build" --target browser-package
file="$build/mitcad-browser-$version-$platform.zip"
[ -f "$file" ] || { echo "browser-package.sh: no $file" >&2; exit 1; }
echo "$platform $abi $version $CEF_VERSION $fingerprint $file" > "$build/browser-package.txt"
echo "browser-package.sh: $file ($(wc -c < "$file" | tr -d ' ') bytes)"

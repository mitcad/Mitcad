#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Fetches the Chromium Embedded Framework's binary distribution pinned in
# versions.sh (CEF_VERSION, the "minimal" distribution) for this platform
# (Linux x86-64, macOS arm64), checks its SHA-1 (the pin, else the digest
# cef-builds publishes beside the archive), and unpacks it into
# ~/cef/<CEF_VERSION>/; prints the folder, which the build takes as
# MITCAD_CEF_ROOT (Insert > Component from Web's web browser component,
# mitcad#1579, docs/development.md "The web browser component").
# Running it again with the folder there does nothing.
# Usage: tools/dev-env/fetch-cef.sh [--print-root]
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=tools/dev-env/versions.sh
. "$SCRIPT_DIR/versions.sh"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux64 pin=$CEF_LINUX64_SHA1 ;;
  Darwin-arm64) platform=macosarm64 pin=$CEF_MACOSARM64_SHA1 ;;
  *) echo "fetch-cef.sh: no CEF distribution for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

name="cef_binary_${CEF_VERSION}_${platform}_minimal"
root="$HOME/cef/$CEF_VERSION/$name"
if [ "${1:-}" = --print-root ]; then
  echo "$root"
  exit 0
fi
if [ -f "$root/cmake/FindCEF.cmake" ]; then
  echo "$root"
  exit 0
fi

# The archive's address: '+' of the version escaped.
encoded=${name//+/%2B}
url="https://cef-builds.spotifycdn.com/$encoded.tar.bz2"
work="$HOME/cef/$CEF_VERSION/download"
mkdir -p "$work"
archive="$work/$name.tar.bz2"
echo "== CEF $CEF_VERSION ($platform) from $url" >&2
curl --proto '=https' --tlsv1.2 -fsSL -o "$archive" "$url"
sha1() {
  if command -v sha1sum > /dev/null 2>&1; then sha1sum "$1" | cut -d ' ' -f 1; else shasum -a 1 "$1" | cut -d ' ' -f 1; fi
}
actual=$(sha1 "$archive")
if [ -n "$pin" ]; then
  expected=$pin
else
  expected=$(curl --proto '=https' --tlsv1.2 -fsSL "$url.sha1" | tr -d ' \r\n')
  echo "fetch-cef.sh: CEF_$(echo "$platform" | tr '[:lower:]' '[:upper:]')_SHA1 is not pinned in versions.sh;" \
    "the published digest is $expected" >&2
fi
if [ "$actual" != "$expected" ]; then
  echo "fetch-cef.sh: $archive has SHA-1 $actual, not $expected" >&2
  rm -f "$archive"
  exit 1
fi
rm -rf "$root.partial"
mkdir -p "$root.partial"
tar -xjf "$archive" -C "$root.partial" --strip-components=1
mv "$root.partial" "$root"
rm -f "$archive"
echo "$root"

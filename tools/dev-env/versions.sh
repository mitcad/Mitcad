# SPDX-License-Identifier: MIT
# The tool versions pinned for the isolated build environments, sourced by
# setup-user.sh (Linux) and setup-macos.sh. setup-windows.ps1 cannot source
# this file and mirrors the shared pins by hand: keep them in step.

QT_VERSION=6.12.0
# aqtinstall 3.3.0 cannot install Qt 6.11+ on Windows; the fix (PR #1000)
# is unreleased, so a pinned commit is used on every platform.
AQT_SPEC="aqtinstall @ git+https://github.com/miurahr/aqtinstall@076e1659807d0b362a3ed684d54c2e9c775eb9c7"
CARGO_DENY_VERSION=0.20.2
# Must match "builtin-baseline" in vcpkg.json.
VCPKG_COMMIT=3aea538b2bb21a586502c67b00eb474fdd2e3098

# macOS only: CMake and Ninja come from the upstream release archives, not
# Homebrew, checked against these SHA-256 pins. CMake's is from
# cmake-<version>-SHA-256.txt next to its release files, Ninja's from the
# digest GitHub lists for the release asset.
CMAKE_VERSION=3.31.6
CMAKE_MACOS_SHA256=330b9514f5112e5ed4fb08b8b05803b776fd9b539a6ae12927d14dcc0ee2ba8d
NINJA_VERSION=1.13.2
NINJA_MAC_SHA256=c99048673aa765960a99cf10c6ddb9f1fad506099ff0a0e137ad8960a88f321b
# A standalone CPython for aqtinstall (python-build-standalone), since the
# Command Line Tools' python3 is too old; the digest is GitHub's for the asset.
PYTHON_VERSION=3.12.15
PYTHON_BUILD=20261003
PYTHON_MAC_SHA256=316a463172740e71d8dca1f2730784e325f3f720941137b5d674d5801a632213
# pkgconf for vcpkg; its distfiles have no checksum files, so the pin is the
# hash Homebrew's formula records for the same archive.
PKGCONF_VERSION=3.0.7
PKGCONF_SHA256=c926ff491cbd9a331a589160811bd97ab1749b4d5198a519338f2cdfabe6940a

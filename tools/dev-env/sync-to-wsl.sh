#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Mirrors this working tree (tracked and untracked files that git does not
# ignore) into the isolated WSL distro for building and testing.
#
# The copy goes one way only: the distro never writes back to Windows, and it
# needs no access to Windows drives. The tree is unpacked into a staging
# folder (~/.cache/mitcad-sync/<dir>), and rsync then replaces only the
# files whose content differs, which get the time of the sync: a build
# directory last built from another checkout then rebuilds every changed
# file, however old its time here, and unchanged files keep their times, so
# they are not rebuilt. Files deleted here are deleted there too; the
# target's build/ directory and its CMakeUserPresets.json are kept.
#
# Usage (Git Bash on Windows): tools/dev-env/sync-to-wsl.sh [distro] [dir]
#   dir is relative to the distro user's home (default src/mitcad).
set -euo pipefail

DISTRO=${1:-mitcad-dev}
TARGET=${2:-src/mitcad}
STAGE=.cache/mitcad-sync/${TARGET//\//_}
cd "$(dirname "$0")/../.."

# --exec runs sh without a shell around it, so the script below reaches the
# distro as it is (with "wsl -- ..." the distro's shell would expand it
# first). rsync -c compares contents; without -t a replaced file gets the
# current time; -p takes over the executable bits. Its itemised output
# counts the files it replaced (">f...") and deleted.
# shellcheck disable=SC2016
git ls-files -z --cached --others --exclude-standard |
  tar --null -T - -cf - |
  MSYS_NO_PATHCONV=1 wsl.exe -d "$DISTRO" --cd "~" --exec sh -c '
    set -e
    rm -rf "$2" && mkdir -p "$2" "$1" && tar -xf - -C "$2"
    rsync -rlpc --delete --exclude=/build --exclude=/CMakeUserPresets.json --out-format=%i \
      "$2/" "$1/" > "$2.changes"
    echo "$(grep -c "^>f" "$2.changes") files changed, $(grep -c "^.deleting" "$2.changes") removed"
    rm -rf "$2" "$2.changes"' sync "$TARGET" "$STAGE"
echo "Synced to $DISTRO:~/$TARGET"

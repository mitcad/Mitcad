#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Mirrors this working tree (tracked and untracked files that git does not
# ignore) into the macOS build VM over SSH. The copy goes one way only; the
# VM's build directory and CMakeUserPresets.json are kept, and file times are
# preserved so unchanged files are not rebuilt.
#
# Usage: tools/dev-env/sync-to-mac.sh [user@host] [dir]
# Defaults come from MITCAD_MAC (no default), MITCAD_MAC_DIR (~/src/mitcad on
# the guest) and MITCAD_MAC_KEY (~/.ssh/mitcad_mac).
set -euo pipefail

HOST=${1:-${MITCAD_MAC:-}}
DIR=${2:-${MITCAD_MAC_DIR:-'~/src/mitcad'}}
KEY=${MITCAD_MAC_KEY:-$HOME/.ssh/mitcad_mac}
if [ -z "$HOST" ]; then
  echo "Usage: $0 user@host [dir]  (or set MITCAD_MAC)" >&2
  exit 1
fi
# A leading ~ is expanded by the guest's shell, not here.
case "$DIR" in
  '~') DIR='$HOME' ;;
  '~/'*) DIR='$HOME/'${DIR#'~/'} ;;
esac
SSH=(ssh -i "$KEY" -o IdentitiesOnly=yes -o BatchMode=yes -o LogLevel=ERROR "$HOST")
cd "$(dirname "$0")/../.."

# From a Mac host: no AppleDouble (._*) files or extended attributes in the
# archive. bsdtar takes the options, GNU tar on a Linux host does not need them.
export COPYFILE_DISABLE=1
TAR_OPTS=()
for opt in --no-mac-metadata --no-xattrs; do
  if tar "$opt" -cf /dev/null -T /dev/null 2> /dev/null; then
    TAR_OPTS+=("$opt")
  fi
done

# Remove what is not kept, so files deleted here disappear there too.
"${SSH[@]}" "d=\"$DIR\"; mkdir -p \"\$d\" && find \"\$d\" -mindepth 1 -maxdepth 1 ! -name build ! -name CMakeUserPresets.json -exec rm -rf {} +"

git ls-files -z --cached --others --exclude-standard |
  tar ${TAR_OPTS[@]+"${TAR_OPTS[@]}"} --null -T - -cf - |
  "${SSH[@]}" "d=\"$DIR\"; tar -xf - -C \"\$d\""
echo "Synced to $HOST:$DIR"

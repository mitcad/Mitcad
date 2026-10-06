#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Mirrors this working tree (tracked and untracked files that git does not
# ignore) into the Windows build VM over SSH. The copy goes one way only.
#
# The tree is unpacked into a staging folder next to the target (<dir>.sync),
# and sync-apply.ps1 then replaces only the files whose content differs,
# which get the time of the sync: a build directory last built from another
# checkout then rebuilds every changed file, however old its time here, and
# unchanged files keep their times, so they are not rebuilt. Files deleted
# here are deleted there too; the VM's build directory and
# CMakeUserPresets.json are kept.
#
# Usage (Git Bash): tools/dev-env/sync-to-vm.sh [user@host] [dir]
# Defaults come from MITCAD_VM, MITCAD_VM_DIR and MITCAD_VM_KEY.
set -euo pipefail

HOST=${1:-${MITCAD_VM:-User@WinDev2407Eval.mshome.net}}
DIR=${2:-${MITCAD_VM_DIR:-C:/dev/src/mitcad}}
DIR=${DIR%/}
STAGE=$DIR.sync
KEY=${MITCAD_VM_KEY:-$HOME/.ssh/mitcad_vm}
SSH=(ssh -i "$KEY" -o BatchMode=yes -o LogLevel=ERROR "$HOST")
cd "$(dirname "$0")/../.."

# An empty staging folder (a failed sync may have left one).
"${SSH[@]}" "powershell -NoProfile -NonInteractive -Command \"if (Test-Path '$STAGE') { Remove-Item -Recurse -Force '$STAGE' }; New-Item -ItemType Directory -Force '$STAGE','$DIR' | Out-Null\""

git ls-files -z --cached --others --exclude-standard |
  tar --null -T - -cf - |
  "${SSH[@]}" "tar -xf - -C \"$STAGE\""

# The staged copy of sync-apply.ps1 compares, moves and removes the stage.
"${SSH[@]}" "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"$STAGE/tools/dev-env/sync-apply.ps1\" -Stage \"$STAGE\" -Target \"$DIR\""
echo "Synced to $HOST:$DIR"

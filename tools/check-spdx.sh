#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
#
# Checks that every source file of Mitcad's own code carries an SPDX licence
# identifier in its first lines (licence policy in docs/development.md).
# Formats without comments (JSON) and documents are exempt.
# Third-party code under third_party/ keeps its own headers.
# Also checks that PowerShell scripts (*.ps1) are pure ASCII and that shell
# scripts with a #! line are executable in git.
set -euo pipefail
cd "$(dirname "$0")/.."

list_files() {
  if git rev-parse --git-dir >/dev/null 2>&1; then
    git ls-files
  else
    find . -type f -not -path './build/*' -not -path '*/target/*' | sed 's|^\./||'
  fi
}

missing=0
while IFS= read -r file; do
  case "$file" in
    third_party/*) continue ;;
    *.rs | *.cpp | *.hpp | *.h | *.c | *.sh | *.py | *.ps1 | *.cmd | *.cmake | *.nsh | CMakeLists.txt | */CMakeLists.txt | *.toml | *.conf | *.svg) ;;
    *) continue ;;
  esac
  if ! head -n 5 "$file" | grep -q 'SPDX-License-Identifier:'; then
    echo "missing SPDX-License-Identifier: $file"
    missing=1
  fi
  # PowerShell scripts stay pure ASCII: Windows PowerShell 5.1 reads a file
  # without a byte order mark as ANSI, so a UTF-8 character in an expected
  # text never matches (mitcad#1071; tools/ui-windows-lib.ps1's $Times).
  case "$file" in
    *.ps1)
      if LC_ALL=C grep -q $'[\x80-\xff]' "$file"; then
        echo "non-ASCII bytes in a PowerShell script: $file"
        missing=1
      fi ;;
  esac
done < <(list_files)

# A shell script with a #! line is executable in git: the exported source
# tree keeps git's modes, and a release build runs some scripts directly
# (tools/dev-env/fetch-cef.sh among them). Scripts without one are only
# sourced (versions.sh, the UI tests' libraries) and stay as they are.
if git rev-parse --git-dir >/dev/null 2>&1; then
  while read -r mode _ _ file; do
    case "$file" in third_party/*) continue ;; esac
    if [ "$mode" = 100644 ] && [ "$(head -c 2 "$file")" = '#!' ]; then
      echo "a script with #! not executable in git (git update-index --chmod=+x): $file"
      missing=1
    fi
  done < <(git ls-files -s -- '*.sh')
fi

exit "$missing"

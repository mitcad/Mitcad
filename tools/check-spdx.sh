#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
#
# Checks that every source file of Mitcad's own code carries an SPDX licence
# identifier in its first lines (licence policy in docs/development.md).
# Formats without comments (JSON) and documents are exempt.
# Third-party code under third_party/ keeps its own headers.
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
done < <(list_files)

exit "$missing"

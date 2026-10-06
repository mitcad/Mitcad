#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Checks that a macOS application bundle is self-contained and runs on Apple
# Silicon: for every Mach-O file in it (executables, dylibs, frameworks,
# Qt plug-ins)
#   - `otool -L` lists only system libraries (/System, /usr/lib) and
#     libraries that are inside the bundle (@rpath, @executable_path,
#     @loader_path), never a path of the build or of the machine that made
#     it (/opt, /Users, /usr/local, a build directory);
#   - every such @-reference resolves to a file in the bundle, the way dyld
#     searches it;
#   - the rpaths are relative (@...), so that they leak no build path;
#   - `lipo -archs` contains arm64.
# It also checks Info.plist, the executable, the icon and the licence
# texts, and that no symbolic link leaves the bundle.
#
# Runs on macOS only (otool, lipo, plutil). Bash 3.2 compatible.
# Usage: tools/check-bundle-macos.sh <path to Mitcad.app>
set -euo pipefail

if [ $# -ne 1 ] || [ ! -d "$1" ]; then
  echo "usage: $0 <path to the .app bundle>" >&2
  exit 2
fi
bundle=$(cd "$1" && pwd -P)
contents="$bundle/Contents"

failures=0
fail() {
  echo "FAIL: ${1#"$bundle"/}: $2"
  failures=$((failures + 1))
}

# --- The bundle's own files ---------------------------------------------------
plist="$contents/Info.plist"
exe=""
if [ ! -f "$plist" ]; then
  fail "$plist" "Info.plist is missing"
elif ! plutil -lint "$plist" >/dev/null; then
  fail "$plist" "Info.plist is not a valid property list"
else
  plist_value() { /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null || true; }
  exe_name=$(plist_value CFBundleExecutable)
  icon_name=$(plist_value CFBundleIconFile)
  exe="$contents/MacOS/$exe_name"
  if [ -z "$exe_name" ] || [ ! -x "$exe" ]; then
    fail "$plist" "CFBundleExecutable '$exe_name' is not an executable in Contents/MacOS"
    exe=""
  fi
  case "$icon_name" in *.icns) ;; *) icon_name="$icon_name.icns" ;; esac
  if [ "$icon_name" = ".icns" ] || [ ! -f "$contents/Resources/$icon_name" ]; then
    fail "$plist" "the icon (CFBundleIconFile) is missing from Contents/Resources"
  fi
  if [ -z "$(plist_value CFBundleIdentifier)" ]; then
    fail "$plist" "CFBundleIdentifier is empty"
  fi
fi
# The native look of the widgets is a plug-in that macdeployqt leaves out
# unless it is asked for; without it Qt falls back to its own cross-platform style.
if [ ! -f "$contents/PlugIns/styles/libqmacstyle.dylib" ]; then
  fail "$contents/PlugIns/styles/libqmacstyle.dylib" "the macOS widget style plug-in is missing"
fi
if ! ls "$contents/Resources/licenses/"* >/dev/null 2>&1; then
  fail "$contents/Resources/licenses" "the licence texts are missing"
fi

# Symbolic links must stay inside the bundle: an absolute target is a path
# of the machine that built it.
while IFS= read -r -d '' link; do
  target=$(readlink "$link")
  case "$target" in
    /*) fail "$link" "absolute symbolic link to $target" ;;
  esac
done < <(find "$bundle" -type l -print0)

# --- Mach-O files ---------------------------------------------------------------
# The rpaths of a file, one per line (LC_RPATH entries).
rpaths_of() {
  otool -l "$1" | awk '
    $1 == "cmd" { in_rpath = ($2 == "LC_RPATH") }
    in_rpath && $1 == "path" {
      sub(/^[ \t]*path /, ""); sub(/ \(offset [0-9]+\)$/, "")
      if (!seen[$0]++) print # a universal file lists them once per slice
      in_rpath = 0
    }'
}

# Expands @executable_path and @loader_path in a path of a file.
expand_at() { # <path> <loader file>
  case "$1" in
    @executable_path/*) echo "$contents/MacOS/${1#@executable_path/}" ;;
    @loader_path/*) echo "$(dirname "$2")/${1#@loader_path/}" ;;
    *) echo "$1" ;;
  esac
}

# The rpaths of the main executable also apply to every library it loads.
exe_rpaths=""
if [ -n "$exe" ]; then
  exe_rpaths=$(rpaths_of "$exe")
fi

# Does an @rpath/... reference of file $2 resolve to a file in the bundle,
# searching the rpaths of the file and then those of the executable, as dyld
# does?
rpath_resolves() { # <@rpath/rest> <file>
  local rest=${1#@rpath/} entry
  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    [ -e "$(expand_at "$entry" "$2")/$rest" ] && return 0
  done <<EOF
$(rpaths_of "$2")
EOF
  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    [ -e "$(expand_at "$entry" "$exe")/$rest" ] && return 0
  done <<EOF
$exe_rpaths
EOF
  return 1
}

# Is $1 the install name of the file being checked (own_ids, set per file)?
nl='
'
is_own_id() {
  case "$nl$own_ids$nl" in
    *"$nl$1$nl"*) return 0 ;;
  esac
  return 1
}

macho_count=0
dep_count=0
while IFS= read -r -d '' file_path; do
  case "$(file -b "$file_path")" in
    Mach-O*) ;;
    *) continue ;;
  esac
  macho_count=$((macho_count + 1))

  # Architectures.
  archs=$(lipo -archs "$file_path" 2>/dev/null || true)
  case " $archs " in
    *" arm64 "*) ;;
    *) fail "$file_path" "no arm64 slice (architectures: ${archs:-unknown})" ;;
  esac

  # Own install name (the first line `otool -L` prints for a library).
  own_ids=$(otool -D "$file_path" 2>/dev/null | awk 'NR > 1 && !/:$/' | sort -u)

  # Linked libraries.
  while IFS= read -r dep; do
    [ -n "$dep" ] || continue
    dep_count=$((dep_count + 1))
    case "$dep" in
      /System/* | /usr/lib/*) ;;
      @rpath/*)
        if ! is_own_id "$dep" && ! rpath_resolves "$dep" "$file_path"; then
          fail "$file_path" "$dep is not found in the bundle by the rpaths"
        fi
        ;;
      @executable_path/* | @loader_path/*)
        if ! is_own_id "$dep" && [ ! -e "$(expand_at "$dep" "$file_path")" ]; then
          fail "$file_path" "$dep is not in the bundle"
        fi
        ;;
      *)
        fail "$file_path" "refers to $dep, which is outside the bundle and not a system library"
        ;;
    esac
  done <<EOF
$(otool -L "$file_path" | awk '/^[ \t]/ { print $1 }' | sort -u)
EOF

  # Rpaths.
  while IFS= read -r rpath; do
    [ -n "$rpath" ] || continue
    case "$rpath" in
      @*) ;;
      *) fail "$file_path" "absolute rpath $rpath" ;;
    esac
  done <<EOF
$(rpaths_of "$file_path")
EOF
done < <(find "$bundle" -type f -print0)

if [ "$macho_count" -eq 0 ]; then
  fail "$bundle" "no Mach-O files found"
fi

echo "$(basename "$bundle"): $macho_count Mach-O files, $dep_count library references checked, $failures problem(s)"
[ "$failures" -eq 0 ]

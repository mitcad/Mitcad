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
# texts, signed OIDN CPU/Metal modules (including the embedded Metal kernels),
# Cycles' complete Metal runtime source tree against its manifest,
# and that no symbolic link leaves the bundle.
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
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=tools/dev-env/versions.sh
. "$SCRIPT_DIR/dev-env/versions.sh"

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
  # Keep Finder/Open With and Dock drops aligned with the actual opener.
  # Every document type lists extensions explicitly, including the native
  # type, and non-system UTIs must have an exported/imported declaration.
  document_extensions=""
  document_index=0
  while [ -n "$(plist_value "CFBundleDocumentTypes:$document_index:CFBundleTypeName")" ]; do
    document="CFBundleDocumentTypes:$document_index"
    role=$(plist_value "$document:CFBundleTypeRole")
    rank=$(plist_value "$document:LSHandlerRank")
    extension_index=0
    while extension=$(plist_value "$document:CFBundleTypeExtensions:$extension_index") && [ -n "$extension" ]; do
      document_extensions="$document_extensions
$extension"
      if [ "$extension" = mitcad ]; then
        if [ "$role" != Editor ] || [ "$rank" != Owner ]; then
          fail "$plist" ".mitcad must remain Editor/Owner"
        fi
      elif [ "$role" != Viewer ] || [ "$rank" != Alternate ]; then
        fail "$plist" ".$extension must be Viewer/Alternate"
      fi
      extension_index=$((extension_index + 1))
    done
    if [ "$extension_index" -eq 0 ]; then
      fail "$plist" "document type $document_index has no explicit extensions"
    fi
    type_index=0
    while type_id=$(plist_value "$document:LSItemContentTypes:$type_index") && [ -n "$type_id" ]; do
      case "$type_id" in
        public.*) ;;
        *)
          declared=0
          for type_group in UTExportedTypeDeclarations UTImportedTypeDeclarations; do
            declaration_index=0
            while identifier=$(plist_value "$type_group:$declaration_index:UTTypeIdentifier") && [ -n "$identifier" ]; do
              if [ "$identifier" = "$type_id" ]; then declared=1; fi
              declaration_index=$((declaration_index + 1))
            done
          done
          if [ "$declared" -ne 1 ]; then fail "$plist" "document UTI $type_id has no declaration"; fi
          ;;
      esac
      type_index=$((type_index + 1))
    done
    if [ "$type_index" -eq 0 ]; then fail "$plist" "document type $document_index has no UTI"; fi
    document_index=$((document_index + 1))
  done
  supported_extensions=$(sed -n '/^FileKind fileKind(/,/^}/p' "$SCRIPT_DIR/../app/files/FileFormats.cpp" |
    grep -oE 'QStringLiteral\("[a-z0-9]+"\)' | sed 's/QStringLiteral("//; s/")//' | sort -u)
  declared_extensions=$(printf '%s\n' "$document_extensions" | sed '/^$/d' | tr '[:upper:]' '[:lower:]' | sort -u)
  if [ "$declared_extensions" != "$supported_extensions" ]; then
    fail "$plist" "document extensions differ from FileFormats.cpp (expected: $(echo "$supported_extensions" | tr '\n' ' '); declared: $(echo "$declared_extensions" | tr '\n' ' '))"
  fi
fi
# The native look of the widgets is a plug-in that macdeployqt leaves out
# unless it is asked for; without it Qt falls back to its own cross-platform style.
if [ ! -f "$contents/PlugIns/styles/libqmacstyle.dylib" ]; then
  fail "$contents/PlugIns/styles/libqmacstyle.dylib" "the macOS widget style plug-in is missing"
fi
# File › Print (mitcad#1089): Qt's print support, which macdeployqt copies
# as the app links it.
if [ ! -d "$contents/Frameworks/QtPrintSupport.framework" ]; then
  fail "$contents/Frameworks/QtPrintSupport.framework" "Qt's print support is missing"
fi
if ! ls "$contents/Resources/licenses/"* >/dev/null 2>&1; then
  fail "$contents/Resources/licenses" "the licence texts are missing"
fi
# The interface languages (mitcad#1460): the catalogue of each language of
# app/translations/languages.txt in Resources/translations, Qt's own for
# Finnish, and never the tests' pseudo-language.
while read -r code _; do
  case $code in '' | '#'*) continue ;; esac
  if [ ! -s "$contents/Resources/translations/mitcad_$code.qm" ]; then
    fail "$contents/Resources/translations/mitcad_$code.qm" "the catalogue of the language $code is missing"
  fi
done < "$SCRIPT_DIR/../app/translations/languages.txt"
if [ ! -s "$contents/Resources/translations/qtbase_fi.qm" ]; then
  fail "$contents/Resources/translations/qtbase_fi.qm" "Qt's Finnish catalogue is missing"
fi
if [ -e "$contents/Resources/translations/mitcad_xx.qm" ]; then
  fail "$contents/Resources/translations/mitcad_xx.qm" "the tests' pseudo-language is in the bundle"
fi
# QtKeychain is linked statically (cmake/Keychain.cmake): its licence text.
if [ ! -f "$contents/Resources/licenses/QtKeychain-BSD-3-Clause.txt" ]; then
  fail "$contents/Resources/licenses/QtKeychain-BSD-3-Clause.txt" "QtKeychain's licence text is missing"
fi

# Every bundle includes the render worker, its dynamically opened denoiser
# devices and their notices; the Mach-O checks below cover their linked
# dependency graph.
licenses="$contents/Resources/licenses"
worker="$contents/MacOS/mitcad-render"
# Old incremental build bundles can retain removed experiment targets. CMake's
# BUNDLE install copies the directory, so reject these even if signed correctly.
for obsolete in mitcad-frame-bootstrap-probe libmitcad-render-probe-runtime.dylib; do
  if [ -e "$contents/MacOS/$obsolete" ] || [ -L "$contents/MacOS/$obsolete" ]; then
    fail "$contents/MacOS/$obsolete" "obsolete bootstrap experiment must not be shipped"
  fi
done
if [ ! -x "$worker" ]; then
  fail "$worker" "the renderer is not an executable in Contents/MacOS"
fi
render_service="$contents/XPCServices/org.mitcad.renderer.xpc"
if [ ! -x "$render_service/Contents/MacOS/mitcad-render-service" ]; then
  fail "$render_service" "the private interactive renderer service is missing"
elif ! codesign --verify --strict "$render_service" 2>/dev/null; then
  fail "$render_service" "the private interactive renderer service has no valid code signature"
fi
if [ ! -f "$render_service/Contents/Info.plist" ] || ! plutil -lint "$render_service/Contents/Info.plist" >/dev/null; then
  fail "$render_service" "the private renderer's Info.plist is missing or invalid"
elif [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$render_service/Contents/Info.plist")" != org.mitcad.renderer ] || \
     [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundlePackageType' "$render_service/Contents/Info.plist")" != 'XPC!' ]; then
  fail "$render_service" "the private renderer's bundle identity or type is wrong"
fi
# Finder's thumbnails of designs (mitcad#1696): the Quick Look extension,
# signed with the App Sandbox (macOS loads app extensions only sandboxed),
# for the .mitcad type the application exports.
quicklook="$contents/PlugIns/MitCadThumbnails.appex"
quicklook_plist="$quicklook/Contents/Info.plist"
if [ ! -x "$quicklook/Contents/MacOS/MitCadThumbnails" ]; then
  fail "$quicklook" "the Quick Look thumbnail extension is missing"
elif ! codesign --verify --strict "$quicklook" 2>/dev/null; then
  fail "$quicklook" "the Quick Look thumbnail extension has no valid code signature"
elif ! codesign --display --entitlements - --xml "$quicklook" 2>/dev/null | grep -q com.apple.security.app-sandbox; then
  fail "$quicklook" "the Quick Look thumbnail extension is not signed with the App Sandbox"
fi
if [ ! -f "$quicklook_plist" ] || ! plutil -lint "$quicklook_plist" >/dev/null; then
  fail "$quicklook" "the Quick Look extension's Info.plist is missing or invalid"
elif [ "$(/usr/libexec/PlistBuddy -c 'Print :NSExtension:NSExtensionPointIdentifier' "$quicklook_plist")" != \
       com.apple.quicklook.thumbnail ] || \
     [ "$(/usr/libexec/PlistBuddy -c 'Print :NSExtension:NSExtensionAttributes:QLSupportedContentTypes:0' "$quicklook_plist")" != \
       "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist").project" ]; then
  fail "$quicklook" "the Quick Look extension is not a thumbnail extension for the application's .mitcad type"
fi
for license in cycles/LICENSE openimagedenoise/LICENSE.txt openimagedenoise/third-party-programs.txt \
               embree-copyright.txt tbb-copyright.txt openimageio-copyright.txt \
               opencolorio-copyright.txt cgltf-copyright.txt sse2neon-copyright.txt; do
  if [ ! -s "$licenses/$license" ]; then
    fail "$licenses/$license" "the renderer's licence text is missing or empty"
  fi
done
if ! ls "$licenses/cycles/"*license*.txt >/dev/null 2>&1; then
  fail "$licenses/cycles" "the licence texts of Cycles' bundled code are missing"
fi
for device in cpu metal; do
  module="$contents/Frameworks/libOpenImageDenoise_device_$device.$OIDN_VERSION.dylib"
  if [ ! -s "$module" ]; then
    fail "$module" "Open Image Denoise's runtime-loaded $device device is missing or empty"
  elif ! codesign --verify --strict "$module" 2>/dev/null; then
    fail "$module" "Open Image Denoise's $device device has no valid code signature"
  fi
done
# OIDN 2.5.1's oidn_metal.cmake converts metal_kernels.metallib to C++ and
# embeds it in the signed Metal module. No external OIDN kernels are needed;
# bundling this module also covers the kernel resource and its integrity.

# Metal compiles from source at run time. The manifest covers every file
# exported from the pinned Cycles kernel and util trees, including the
# .metal entry point that a header-only or flat-kernel copy would lose.
cycles_runtime="$contents/Resources/cycles"
for resource in source/kernel/device/metal/kernel.metal source/kernel/device/metal/compat.h \
                source/kernel/device/metal/globals.h source/kernel/device/gpu/kernel.h \
                source/util/types.h source.sha256; do
  if [ ! -s "$cycles_runtime/$resource" ]; then
    fail "$cycles_runtime/$resource" "Cycles' Metal runtime source is missing or empty"
  fi
done
if [ -s "$cycles_runtime/source.sha256" ] && \
   ! (cd "$cycles_runtime" && shasum -a 256 -c source.sha256 >/dev/null); then
  fail "$cycles_runtime/source.sha256" "Cycles' runtime sources do not match the complete source manifest"
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
  local executable_dir="$contents/MacOS"
  case "$2" in
    "$contents"/XPCServices/*.xpc/Contents/MacOS/*) executable_dir=$(dirname "$2") ;;
  esac
  case "$1" in
    @executable_path/*) echo "$executable_dir/${1#@executable_path/}" ;;
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

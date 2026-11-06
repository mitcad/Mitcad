#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The macOS release check for the shared Qt UI (branch
# experiment/macos-shared-qt-ui; docs/macos-shared-qt.md), the macOS
# counterpart of tools/check-all.sh --full on Linux and the Windows VM's
# warning-free build with the full ctest:
#
#   1. configure and build the release configuration with every core
#      (compiler warnings fail the check, as in check-all.sh);
#   2. check that this is the only macOS UI: the AppKit frontend is off,
#      none of its sources was compiled and no app.ui-macos-native* test is
#      registered;
#   3. every ctest through tools/test-macos.sh (mitcad-testrun --schedule:
#      the cores and the memory less the desktop reserve; the GUI tests one
#      at a time under the macos_desktop lock, the headless and harness
#      tests beside them);
#   4. package the .dmg (tools/package-macos.sh: install, ad hoc or
#      Developer ID signature, tools/check-bundle-macos.sh, hdiutil).
#
# Runs on macOS only: the macOS VM for testing, a native Mac for releases.
# The GUI tests need a logged-in console session (docs/development.md).
#
# Usage: tools/release-check-macos.sh [--build DIR] [--jobs N]
#          [--disk-floor SIZE] [--only build,native,ctest,package]
#          [-- extra mitcad-testrun arguments, e.g. -R REGEX]
# Environment: VCPKG_ROOT, QT_ROOT_DIR (as for the macos-release preset),
# MITCAD_RENDER_DEPS (optional), MITCAD_SIGN_IDENTITY and
# MITCAD_NOTARY_PROFILE (package-macos.sh). Logs: <build>/release-check/.
# Bash 3.2 compatible.
set -uo pipefail
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != Darwin ]; then
  echo "release-check-macos.sh runs on macOS only" >&2
  exit 2
fi

build=build/macos-release
jobs=$(sysctl -n hw.ncpu)
disk_floor=""
only=build,native,ctest,package
extra=()
while [ "$#" -gt 0 ]; do
  case $1 in
    --build) build=${2:?--build needs a directory}; shift ;;
    --jobs) jobs=${2:?--jobs needs a number}; shift ;;
    --disk-floor) disk_floor=${2:?--disk-floor needs a size}; shift ;;
    --only) only=${2:?--only needs steps}; shift ;;
    --) shift; extra=("$@"); break ;;
    *) echo "release-check-macos: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done
logs=$build/release-check
mkdir -p "$logs"
failed=0
row() { printf '%-10s %s\n' "$1" "$2" | tee -a "$logs/summary.txt"; }
wants() { case ",$only," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
: > "$logs/summary.txt"

if wants build; then
  render=()
  [ -n "${MITCAD_RENDER_DEPS:-}" ] && render=(-DMITCAD_RENDER_DEPS="$MITCAD_RENDER_DEPS")
  export CMAKE_BUILD_PARALLEL_LEVEL=$jobs CARGO_BUILD_JOBS=$jobs VCPKG_MAX_CONCURRENCY=$jobs
  if ! cmake --preset macos-release -B "$build" ${render[@]+"${render[@]}"} > "$logs/configure.log" 2>&1; then
    row configure FAIL; failed=1
  elif ! cmake --build "$build" --parallel "$jobs" > "$logs/build.log" 2>&1; then
    row build FAIL; failed=1
  else
    # What it compiled must be warning-free; vcpkg's own notes are not.
    warnings=$(grep -E '(^| )warning(\[[^]]*\])?: ' "$logs/build.log" | grep -vc 'vcpkg' || true)
    if [ "$warnings" -gt 0 ]; then
      grep -E '(^| )warning(\[[^]]*\])?: ' "$logs/build.log" | grep -v vcpkg | head -20
      row build "FAIL($warnings warnings)"; failed=1
    else
      row build ok
    fi
  fi
fi

if wants native; then
  cache=$build/CMakeCache.txt
  problems=""
  grep -q '^MITCAD_MACOS_NATIVE_UI:BOOL=OFF' "$cache" || problems="$problems native-option"
  grep -q '^MITCAD_MACOS_SHARED_QT_UI:BOOL=ON' "$cache" || problems="$problems shared-option"
  if find "$build/app" -name 'NativeMacWindow.mm.o' -o -name 'NativeForms.mm.o' -o -name 'NativeDialogs.mm.o' |
      grep -q .; then
    problems="$problems appkit-objects"
  fi
  if ctest --test-dir "$build" -N 2>/dev/null | grep -q 'app\.ui-macos-native'; then
    problems="$problems native-tests"
  fi
  if [ -z "$problems" ]; then row native "ok (the shared Qt UI only)"; else row native "FAIL($problems )"; failed=1; fi
fi

if wants ctest; then
  floor=()
  [ -n "$disk_floor" ] && floor=(--disk-floor "$disk_floor")
  if tools/test-macos.sh --build "$build" -j "$jobs" ${floor[@]+"${floor[@]}"} \
      ${extra[@]+"${extra[@]}"} > "$logs/ctest.log" 2>&1; then
    row ctest ok
  else
    row ctest FAIL; failed=1
  fi
  tail -40 "$logs/ctest.log"
fi

if wants package; then
  if tools/package-macos.sh "$build" > "$logs/package.log" 2>&1; then
    row package "ok ($(grep -o '[^ ]*\.dmg$' "$logs/package.log" | tail -1))"
  else
    row package FAIL; failed=1
  fi
fi

echo "logs: $logs"
exit "$failed"

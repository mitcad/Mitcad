#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds the CPU render worker's dependencies in the isolated Apple Silicon
# macOS guest, never on the host (mitcad#51, docs/rendering.md). Mirrors
# build-cycles.sh/build-cycles.ps1 with AppleClang and Ninja. Metal is a
# separate follow-up (mitcad#50); every GPU device is disabled here.
#
# Usage: tools/dev-env/build-cycles-macos.sh [prefix]
# Prefix: MITCAD_RENDER_DEPS, else ~/mitcad-render-deps; the same downloads/,
# src/, build/, vcpkg_installed/ and install/{oidn,cycles}/ layout as Linux.
# Pins: versions.sh, including ISPC's native arm64 release archive.
# Environment: MITCAD_RENDER_JOBS (default 8), VCPKG_ROOT (default ~/vcpkg).
# Requires the toolchain of setup-macos.sh and Xcode Command Line Tools.
# Re-running skips completed steps; remove the prefix's build/ and install/
# folders by their literal paths to rebuild. Bash 3.2 compatible.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$SCRIPT_DIR/../.." && pwd)
# shellcheck source=tools/dev-env/versions.sh
. "$SCRIPT_DIR/versions.sh"

case ${1:-} in
  -h|--help) sed -n '2,/^set -euo/p' "$0" | sed -e '/^set -euo/d' -e 's/^# \{0,1\}//'; exit 0 ;;
  -*) echo "Unknown option $1 (see --help)" >&2; exit 2 ;;
esac
if [ $# -gt 1 ]; then
  echo "Usage: $0 [prefix]" >&2
  exit 2
fi
if [ "$(uname -s)" != Darwin ] || [ "$(uname -m)" != arm64 ]; then
  echo "build-cycles-macos.sh requires an Apple Silicon (arm64) macOS guest." >&2
  exit 1
fi
if [ "$(id -u)" = 0 ]; then
  echo "Run as the developer user, not root." >&2
  exit 1
fi

PREFIX=${1:-${MITCAD_RENDER_DEPS:-$HOME/mitcad-render-deps}}
JOBS=${MITCAD_RENDER_JOBS:-8}
VCPKG_ROOT=${VCPKG_ROOT:-$HOME/vcpkg}
TRIPLET=arm64-osx-dynamic
# Matches CMakePresets.json and triplets/arm64-osx-dynamic.cmake.
export MACOSX_DEPLOYMENT_TARGET=14.4
mkdir -p "$PREFIX"
PREFIX=$(cd "$PREFIX" && pwd)
DOWNLOADS=$PREFIX/downloads
SRC=$PREFIX/src
BUILD=$PREFIX/build
INSTALL=$PREFIX/install
VCPKG_INSTALLED=$PREFIX/vcpkg_installed
mkdir -p "$DOWNLOADS" "$SRC" "$BUILD" "$INSTALL"

fetch() {
  local url=$1 file=$2 sha256=$3 actual
  if [ ! -f "$DOWNLOADS/$file" ]; then
    echo "   downloading $url"
    curl --proto '=https' --tlsv1.2 -fsSL -o "$DOWNLOADS/$file.part" "$url"
    mv "$DOWNLOADS/$file.part" "$DOWNLOADS/$file"
  fi
  actual=$(shasum -a 256 "$DOWNLOADS/$file" | cut -d ' ' -f 1)
  if [ "$actual" != "$sha256" ]; then
    echo "$file does not match its SHA-256 pin $sha256 ($actual)" >&2
    exit 1
  fi
}

echo "== vcpkg: the manifest's render feature ($TRIPLET)"
"$VCPKG_ROOT/vcpkg" install --x-manifest-root="$REPO" --x-feature=render \
  --x-install-root="$VCPKG_INSTALLED" --overlay-triplets="$REPO/triplets" \
  --triplet "$TRIPLET" --no-print-usage > "$PREFIX/vcpkg.log" 2>&1 || {
  tail -40 "$PREFIX/vcpkg.log" >&2
  exit 1
}
DEPS=$VCPKG_INSTALLED/$TRIPLET

echo "== ISPC $ISPC_VERSION (build tool)"
ISPC_NAME=ispc-v$ISPC_VERSION-macOS.arm64
fetch "https://github.com/ispc/ispc/releases/download/v$ISPC_VERSION/$ISPC_NAME.tar.gz" \
  "$ISPC_NAME.tar.gz" "$ISPC_MACOS_ARM64_SHA256"
ISPC_DIR=$SRC/$ISPC_NAME
if [ ! -x "$ISPC_DIR/bin/ispc" ]; then
  tar -xzf "$DOWNLOADS/$ISPC_NAME.tar.gz" -C "$SRC"
fi
if [ ! -x "$ISPC_DIR/bin/ispc" ]; then
  echo "No $ISPC_DIR/bin/ispc in ISPC's archive" >&2
  exit 1
fi

echo "== Open Image Denoise $OIDN_VERSION"
fetch "https://github.com/RenderKit/oidn/releases/download/v$OIDN_VERSION/oidn-$OIDN_VERSION.src.tar.gz" \
  "oidn-$OIDN_VERSION.src.tar.gz" "$OIDN_SRC_SHA256"
OIDN_SRC=$SRC/oidn-$OIDN_VERSION
if [ ! -f "$OIDN_SRC/CMakeLists.txt" ]; then
  tar -xzf "$DOWNLOADS/oidn-$OIDN_VERSION.src.tar.gz" -C "$SRC"
fi
# Upstream overrides the caller's deployment target with 11.0 on arm64.
# Preserve 14.4, as used by Qt and vcpkg; the patch stays in our source tree.
OIDN_PATCH=$REPO/third_party/oidn/respect-macos-deployment-target.patch
if [ ! -f "$OIDN_SRC/.mitcad-deployment-target" ]; then
  patch -d "$OIDN_SRC" -p1 < "$OIDN_PATCH"
  touch "$OIDN_SRC/.mitcad-deployment-target"
fi
OIDN_DONE=$INSTALL/oidn/.done-$OIDN_VERSION-arm64-macos14.4
if [ ! -f "$OIDN_DONE" ]; then
  cmake -S "$OIDN_SRC" -B "$BUILD/oidn" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=14.4 \
    -DCMAKE_INSTALL_PREFIX="$INSTALL/oidn" -DCMAKE_PREFIX_PATH="$DEPS" \
    -DISPC_EXECUTABLE="$ISPC_DIR/bin/ispc" \
    -DOIDN_DEVICE_CPU=ON -DOIDN_DEVICE_CPU_BNNS=ON -DOIDN_DEVICE_METAL=OFF \
    -DOIDN_DEVICE_CUDA=OFF -DOIDN_DEVICE_HIP=OFF -DOIDN_DEVICE_SYCL=OFF \
    -DOIDN_FILTER_RT=ON -DOIDN_FILTER_RTLIGHTMAP=OFF \
    -DOIDN_APPS=OFF -DOIDN_INSTALL_DEPENDENCIES=OFF \
    -DCMAKE_INSTALL_RPATH="@loader_path;$DEPS/lib" > "$PREFIX/oidn-configure.log"
  cmake --build "$BUILD/oidn" --parallel "$JOBS"
  cmake --install "$BUILD/oidn" > "$PREFIX/oidn-install.log"
  touch "$OIDN_DONE"
fi

echo "== Cycles $CYCLES_VERSION"
CYCLES_SRC=$SRC/cycles-$CYCLES_VERSION
if [ ! -d "$CYCLES_SRC/.git" ]; then
  git clone --quiet --depth 1 --branch "v$CYCLES_VERSION" \
    https://projects.blender.org/blender/cycles.git "$CYCLES_SRC"
fi
if [ "$(git -C "$CYCLES_SRC" rev-parse HEAD)" != "$CYCLES_COMMIT" ]; then
  echo "Cycles v$CYCLES_VERSION is not at the pinned commit $CYCLES_COMMIT" >&2
  exit 1
fi
CYCLES_DONE=$INSTALL/cycles/.done-$CYCLES_VERSION-arm64-macos14.4-cpu
if [ ! -f "$CYCLES_DONE" ]; then
  cmake -S "$CYCLES_SRC" -B "$BUILD/cycles" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=14.4 \
    -DCMAKE_PREFIX_PATH="$DEPS;$INSTALL/oidn" \
    -DCMAKE_INSTALL_PREFIX="$BUILD/cycles/install" -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
    -DWITH_LIBS_PRECOMPILED=OFF -DWITH_STRICT_BUILD_OPTIONS=ON \
    -DWITH_CYCLES_EMBREE=ON -DWITH_CYCLES_OPENIMAGEDENOISE=ON \
    -DWITH_CYCLES_ALEMBIC=OFF -DWITH_CYCLES_OPENSUBDIV=OFF -DWITH_CYCLES_OPENVDB=OFF \
    -DWITH_CYCLES_NANOVDB=OFF -DWITH_CYCLES_OSL=OFF -DWITH_CYCLES_USD=OFF \
    -DWITH_CYCLES_HYDRA_RENDER_DELEGATE=OFF -DWITH_CYCLES_LOGGING=OFF \
    -DWITH_CYCLES_PATH_GUIDING=OFF -DWITH_CYCLES_STANDALONE_GUI=OFF \
    -DWITH_CYCLES_DEVICE_CUDA=OFF -DWITH_CYCLES_DEVICE_OPTIX=OFF -DWITH_CYCLES_DEVICE_HIP=OFF \
    -DWITH_CYCLES_DEVICE_HIPRT=OFF -DWITH_CYCLES_DEVICE_METAL=OFF -DWITH_CYCLES_DEVICE_ONEAPI=OFF \
    -DEMBREE_ROOT_DIR="$DEPS" -DTBB_ROOT_DIR="$DEPS" -DOPENIMAGEDENOISE_ROOT_DIR="$INSTALL/oidn" \
    -DSSE2NEON_ROOT_DIR="$DEPS" -DZSTD_ROOT_DIR="$DEPS" -DPUGIXML_ROOT_DIR="$DEPS" \
    -DCMAKE_BUILD_RPATH="$DEPS/lib;$INSTALL/oidn/lib" > "$PREFIX/cycles-configure.log"
  cmake --build "$BUILD/cycles" --parallel "$JOBS"

  # BSD cp has no --parents: keep each header's path with Python instead.
  rm -rf "${INSTALL:?}/cycles"
  python3 - "$CYCLES_SRC" "$BUILD/cycles" "$INSTALL/cycles" <<'PY'
import pathlib, shutil, sys
source, build, out = map(pathlib.Path, sys.argv[1:])
for directory, target in ((source / "src", out / "include"),
                          (source / "third_party", out / "include/third_party")):
    for header in directory.rglob("*.h"):
        destination = target / header.relative_to(directory)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(header, destination)
(out / "lib").mkdir(parents=True, exist_ok=True)
for pattern in ("libcycles_*.a", "libextern_*.a"):
    for library in build.rglob(pattern):
        shutil.copy2(library, out / "lib" / library.name)
shutil.copy2(source / "LICENSE", out / "LICENSE")
PY
  printf '%s\n' cpu > "$INSTALL/cycles/devices.txt"
  touch "$CYCLES_DONE"
fi
mkdir -p "$INSTALL/cycles/licenses"
cp "$CYCLES_SRC"/src/doc/license/*license*.txt "$INSTALL/cycles/licenses/"

# The compile definitions and host options must match Cycles' headers.
# Export architecture-neutral options and ARM -m flags, never x86 SSE flags.
python3 - "$BUILD/cycles/compile_commands.json" "$INSTALL/cycles" "$INSTALL/oidn" <<'PY'
import json, pathlib, shlex, sys
commands, out, oidn = sys.argv[1:]
with open(commands) as stream:
    entry = next(e for e in json.load(stream) if e["file"].endswith("src/session/session.cpp"))
args = shlex.split(entry["command"]) if "command" in entry else entry["arguments"]
defines = sorted({a[2:] for a in args if a.startswith("-D")})
options = [a for a in args if (a.startswith("-m") or a.startswith("-f")) and a != "-fPIC"]
libraries = sorted(pathlib.Path(out, "lib").glob("*.a"))
if not libraries:
    raise SystemExit("No Cycles static libraries were installed")
with open(pathlib.Path(out, "mitcad-cycles.cmake"), "w") as stream:
    stream.write("# SPDX-License-Identifier: MIT\n")
    stream.write("# Written by tools/dev-env/build-cycles-macos.sh for cmake/Render.cmake.\n")
    stream.write("set(MITCAD_CYCLES_ROOT %s)\n" % json.dumps(out))
    stream.write("set(MITCAD_OIDN_ROOT %s)\n" % json.dumps(oidn))
    for name, values in (("DEFINITIONS", defines), ("OPTIONS", options)):
        stream.write("set(MITCAD_CYCLES_%s\n" % name)
        for value in values:
            stream.write("  %s\n" % json.dumps(value))
        stream.write(")\n")
    stream.write("set(MITCAD_CYCLES_DEVICES CPU)\nset(MITCAD_CYCLES_KERNELS)\n")
    stream.write("set(MITCAD_CYCLES_LIBRARIES\n")
    for library in libraries:
        stream.write('  "${MITCAD_CYCLES_ROOT}/lib/%s"\n' % library.name)
    stream.write(")\n")
PY

echo "== Check: Cycles' standalone program renders a test scene"
mkdir -p "$BUILD/check"
"$BUILD/cycles/bin/cycles" --background --quiet --samples 4 --width 64 --height 48 \
  --output "$BUILD/check/scene.png" "$CYCLES_SRC/examples/scene_cube_surface.xml"
test -s "$BUILD/check/scene.png"
echo "Done: configure Mitcad with -DMITCAD_RENDER=ON -DMITCAD_RENDER_DEPS=$PREFIX"

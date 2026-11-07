#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds the CPU and Metal render worker's dependencies in the isolated Apple Silicon
# macOS guest, never on the host (mitcad#51, docs/rendering.md). Mirrors
# build-cycles.sh/build-cycles.ps1 with AppleClang and Ninja. CPU/BNNS and
# Metal denoising are enabled; the other GPU backends stay disabled. Every
# Mitcad build has the render worker and needs this prefix: setup-macos.sh
# runs this once, and Mitcad's configure finds the default prefix itself.
#
# Usage: tools/dev-env/build-cycles-macos.sh [prefix]
# Prefix: MITCAD_RENDER_DEPS, else ~/mitcad-render-deps; the same downloads/,
# src/, build/, vcpkg_installed/ and install/{oidn,cycles}/ layout as Linux.
# Pins: versions.sh, including ISPC's native arm64 release archive.
# Environment: MITCAD_RENDER_JOBS (default 8), VCPKG_ROOT (default ~/vcpkg),
# PATH/DEVELOPER_DIR/TOOLCHAINS (optional xcrun toolchain selection).
# Requires setup-macos.sh's toolchain, macOS SDK >= 14 and Apple's Metal
# compiler/linker available through xcrun (Xcode or the Metal toolchain).
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

# OIDN compiles and embeds its Metal kernels at build time (unlike Cycles'
# runtime source compilation). Fail before downloading/building dependencies
# when only Command Line Tools without the Metal compiler are installed.
metal_toolchain_error() {
  echo "Open Image Denoise Metal requires macOS SDK >= 14 and working xcrun metal/metallib tools." >&2
  echo "Install Apple's Metal toolchain or Xcode in this build environment; select it with" >&2
  echo "PATH, DEVELOPER_DIR, TOOLCHAINS or xcode-select, then verify: xcrun --sdk macosx metal --version." >&2
  exit 1
}
SDK_VERSION=$(xcrun --sdk macosx --show-sdk-version 2>/dev/null) || metal_toolchain_error
SDK_ROOT=$(xcrun --sdk macosx --show-sdk-path 2>/dev/null) || metal_toolchain_error
SDK_MAJOR=${SDK_VERSION%%.*}
case "$SDK_MAJOR" in ''|*[!0-9]*) metal_toolchain_error ;; esac
if [ "$SDK_MAJOR" -lt 14 ]; then metal_toolchain_error; fi
METAL_TOOL=$(xcrun --sdk macosx --find metal 2>/dev/null) || metal_toolchain_error
METALLIB_TOOL=$(xcrun --sdk macosx --find metallib 2>/dev/null) || metal_toolchain_error
if [ ! -x "$METAL_TOOL" ] || [ ! -x "$METALLIB_TOOL" ]; then metal_toolchain_error; fi
METAL_VERSION=$(xcrun --sdk macosx metal --version 2>&1) || metal_toolchain_error
METALLIB_VERSION=$(xcrun --sdk macosx metallib --version 2>&1) || metal_toolchain_error
CLANG_VERSION=$(xcrun --sdk macosx clang --version 2>&1) || metal_toolchain_error
TOOLCHAIN_SHA256=$({
  printf '%s\n' "$SDK_VERSION" "$METAL_VERSION" "$METALLIB_VERSION" "$CLANG_VERSION"
  shasum -a 256 "$METAL_TOOL" "$METALLIB_TOOL" | cut -d ' ' -f 1
} | shasum -a 256 | cut -d ' ' -f 1)

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

echo "== vcpkg: the manifest's libraries ($TRIPLET)"
"$VCPKG_ROOT/vcpkg" install --x-manifest-root="$REPO" \
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
# The current macOS linker rejects OIDN's legacy eager-binding option with
# a deprecation warning. Preserve it only for older deployment targets.
OIDN_BINDING_PATCH=$REPO/third_party/oidn/modern-macos-binding.patch
if [ ! -f "$OIDN_SRC/.mitcad-modern-binding" ]; then
  patch -d "$OIDN_SRC" -p1 < "$OIDN_BINDING_PATCH"
  touch "$OIDN_SRC/.mitcad-modern-binding"
fi
# The source/tool pins and build inputs are part of the completion stamp.
# A changed archive or dependency baseline must never reuse an old export.
BUILDER_SHA256=$(shasum -a 256 "$0" | cut -d ' ' -f 1)
CYCLES_PATCH=$REPO/third_party/cycles/metal-resource-usage.patch
CYCLES_DIAGNOSTICS_PATCH=$REPO/third_party/cycles/denoiser-diagnostics.patch
CYCLES_PIPELINE_PATCH=$REPO/third_party/cycles/metal-pipeline-failure.patch
CYCLES_CANCELLATION_PATCH=$REPO/third_party/cycles/metal-cancellation-diagnostic.patch
CYCLES_PREVIEW_BATCH_PATCH=$REPO/third_party/cycles/denoiser-preview-batches.patch
CYCLES_EXTERNAL_BUFFER_PATCH=$REPO/third_party/cycles/metal-external-buffer-usage.patch
CYCLES_NATIVE_DISPLAY_PATCH=$REPO/third_party/cycles/native-display-passes.patch
RENDER_INPUT_SHA256=$(shasum -a 256 "$REPO/vcpkg.json" "$REPO/triplets/$TRIPLET.cmake" \
  "$OIDN_PATCH" "$OIDN_BINDING_PATCH" "$CYCLES_PATCH" "$CYCLES_DIAGNOSTICS_PATCH" "$CYCLES_PIPELINE_PATCH" \
  "$CYCLES_CANCELLATION_PATCH" "$CYCLES_PREVIEW_BATCH_PATCH" "$CYCLES_EXTERNAL_BUFFER_PATCH" \
  "$CYCLES_NATIVE_DISPLAY_PATCH" \
  | cut -d ' ' -f 1 | shasum -a 256 | cut -d ' ' -f 1)
RENDER_BUILD_KEY=$({
  printf '%s\n' "$CYCLES_VERSION" "$CYCLES_COMMIT" "$OIDN_VERSION" "$OIDN_SRC_SHA256" \
    "$ISPC_VERSION" "$ISPC_MACOS_ARM64_SHA256" "$VCPKG_COMMIT" "$BUILDER_SHA256" \
    "$RENDER_INPUT_SHA256" "$TOOLCHAIN_SHA256" \
    'arm64;macos14.4;cycles=CPU,METAL;oidn=CPU,BNNS,METAL;oidn-metal=embedded;runtime-source-v1'
} | shasum -a 256 | cut -d ' ' -f 1)
OIDN_DONE=$INSTALL/oidn/.done-$OIDN_VERSION-arm64-macos14.4-$RENDER_BUILD_KEY
if [ ! -f "$OIDN_DONE" ]; then
  cmake -S "$OIDN_SRC" -B "$BUILD/oidn" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=14.4 -DCMAKE_OSX_SYSROOT="$SDK_ROOT" \
    -DCMAKE_INSTALL_PREFIX="$INSTALL/oidn" -DCMAKE_PREFIX_PATH="$DEPS" \
    -DISPC_EXECUTABLE="$ISPC_DIR/bin/ispc" \
    -DOIDN_DEVICE_CPU=ON -DOIDN_DEVICE_CPU_BNNS=ON -DOIDN_DEVICE_METAL=ON \
    -DOIDN_STATIC_LIB=OFF -DOIDN_LIBRARY_VERSIONED=ON \
    -DOIDN_DEVICE_CUDA=OFF -DOIDN_DEVICE_HIP=OFF -DOIDN_DEVICE_SYCL=OFF \
    -DOIDN_FILTER_RT=ON -DOIDN_FILTER_RTLIGHTMAP=OFF \
    -DOIDN_APPS=OFF -DOIDN_INSTALL_DEPENDENCIES=OFF \
    -DCMAKE_INSTALL_RPATH="@loader_path;$DEPS/lib" > "$PREFIX/oidn-configure.log"
  cmake --build "$BUILD/oidn" --parallel "$JOBS"
  # OIDN's install rules edit Mach-O rpaths even for an up-to-date file.
  # Recopy pristine build outputs before those edits on a changed build key.
  rm -rf "${INSTALL:?}/oidn"
  cmake --install "$BUILD/oidn" > "$PREFIX/oidn-install.log"
fi
# These exact names are used by OIDN's runtime module loader. A stamp alone
# cannot establish a complete prefix after a partial install or cleanup.
for device in cpu metal; do
  if [ ! -s "$INSTALL/oidn/lib/libOpenImageDenoise_device_$device.$OIDN_VERSION.dylib" ]; then
    echo "Incomplete OIDN $device module; remove $OIDN_DONE and rerun this script." >&2
    exit 1
  fi
done
# The pinned oidn_metal.cmake embeds this compiled blob in the Metal module;
# no .metallib sidecar or SDK is needed by the deployed application.
if [ ! -s "$BUILD/oidn/devices/metal/metal_kernels.metallib" ]; then
  echo "OIDN's embedded Metal kernel library is missing; remove $OIDN_DONE and rebuild." >&2
  exit 1
fi
touch "$OIDN_DONE"

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
# The pinned backend still declares sampled textures with an API deprecated
# in macOS 13. The existing Read usage covers sampling at our 14.4 minimum.
# The diagnostics accessor lets the worker report the actual selected denoiser.
# Failed/cancelled Metal pipelines must stop waiting and report a device error.
# Later patches can change an earlier patch's context, so an individual
# reverse check cannot recognize a valid complete stack. Build the expected
# source from the pinned revision, then compare actual bytes and modes.
cycles_patches=("$CYCLES_PATCH" "$CYCLES_DIAGNOSTICS_PATCH" "$CYCLES_PIPELINE_PATCH"
                "$CYCLES_CANCELLATION_PATCH" "$CYCLES_PREVIEW_BATCH_PATCH"
                "$CYCLES_EXTERNAL_BUFFER_PATCH" "$CYCLES_NATIVE_DISPLAY_PATCH")
cycles_expected=$(mktemp -d "$SRC/cycles-patch-check.XXXXXX")
if ! git -c advice.detachedHead=false clone --quiet --no-hardlinks "$CYCLES_SRC" "$cycles_expected"; then
  rm -rf "${cycles_expected:?}"
  exit 1
fi
for cycles_patch in "${cycles_patches[@]}"; do
  if ! git -C "$cycles_expected" apply --check "$cycles_patch" ||
     ! git -C "$cycles_expected" apply "$cycles_patch"; then
    rm -rf "${cycles_expected:?}"
    exit 1
  fi
done
if python3 - "$CYCLES_SRC" "$cycles_expected" <<'PY'
import filecmp, os, pathlib, stat, subprocess, sys
source, expected = map(pathlib.Path, sys.argv[1:])
for directory, dirs, files in os.walk(expected):
    dirs[:] = [name for name in dirs if name != ".git"]
    for name in dirs + files:
        target = pathlib.Path(directory, name)
        actual = source / target.relative_to(expected)
        if target.is_symlink():
            if not actual.is_symlink() or os.readlink(actual) != os.readlink(target):
                sys.exit(1)
        elif target.is_file():
            if (not actual.is_file() or actual.is_symlink()
                    or not filecmp.cmp(actual, target, shallow=False)
                    or bool(actual.stat().st_mode & stat.S_IXUSR) != bool(target.stat().st_mode & stat.S_IXUSR)):
                sys.exit(1)
# A file removed by the stack must also be absent from the working source.
tracked = subprocess.check_output(["git", "-C", str(source), "ls-files", "-z"]).decode().split("\0")
for name in filter(None, tracked):
    target, actual = expected / name, source / name
    if not (target.exists() or target.is_symlink()) and (actual.exists() or actual.is_symlink()):
        sys.exit(1)
PY
then
  echo "   complete patch stack verified"
elif git -C "$CYCLES_SRC" diff --quiet HEAD --; then
  for cycles_patch in "${cycles_patches[@]}"; do
    if ! git -C "$CYCLES_SRC" apply --check "$cycles_patch" ||
       ! git -C "$CYCLES_SRC" apply "$cycles_patch"; then
      rm -rf "${cycles_expected:?}"
      exit 1
    fi
  done
else
  echo "Cycles source does not match the pinned revision with the current patch stack; use a fresh prefix." >&2
  rm -rf "${cycles_expected:?}"
  exit 1
fi
rm -rf "${cycles_expected:?}"
CYCLES_DONE=$INSTALL/cycles/.done-$CYCLES_VERSION-arm64-macos14.4-cpu-metal-$RENDER_BUILD_KEY
if [ ! -f "$CYCLES_DONE" ]; then
  # vcpkg nests this header under include/sse2neon; Cycles' find module
  # searches only include. Without it the ARM branch is skipped and an
  # AppleClang SSE4.2 probe can enable unsupported x86 compiler options.
  SSE2NEON_INCLUDE=$DEPS/include/sse2neon
  if [ ! -f "$SSE2NEON_INCLUDE/sse2neon.h" ]; then
    echo "No sse2neon.h in $SSE2NEON_INCLUDE; install the arm64 vcpkg dependencies." >&2
    exit 1
  fi
  cmake -S "$CYCLES_SRC" -B "$BUILD/cycles" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=14.4 -DCMAKE_OSX_SYSROOT="$SDK_ROOT" \
    -DCMAKE_PREFIX_PATH="$DEPS;$INSTALL/oidn" \
    -DCMAKE_INSTALL_PREFIX="$BUILD/cycles/install" -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
    -DWITH_LIBS_PRECOMPILED=OFF -DWITH_STRICT_BUILD_OPTIONS=ON \
    -DWITH_CYCLES_EMBREE=ON -DWITH_CYCLES_OPENIMAGEDENOISE=ON \
    -DWITH_CYCLES_ALEMBIC=OFF -DWITH_CYCLES_OPENSUBDIV=OFF -DWITH_CYCLES_OPENVDB=OFF \
    -DWITH_CYCLES_NANOVDB=OFF -DWITH_CYCLES_OSL=OFF -DWITH_CYCLES_USD=OFF \
    -DWITH_CYCLES_HYDRA_RENDER_DELEGATE=OFF -DWITH_CYCLES_LOGGING=OFF \
    -DWITH_CYCLES_PATH_GUIDING=OFF -DWITH_CYCLES_STANDALONE_GUI=OFF \
    -DWITH_CYCLES_DEVICE_CUDA=OFF -DWITH_CYCLES_DEVICE_OPTIX=OFF -DWITH_CYCLES_DEVICE_HIP=OFF \
    -DWITH_CYCLES_DEVICE_HIPRT=OFF -DWITH_CYCLES_DEVICE_METAL=ON -DWITH_CYCLES_DEVICE_ONEAPI=OFF \
    -DEMBREE_ROOT_DIR="$DEPS" -DTBB_ROOT_DIR="$DEPS" -DOPENIMAGEDENOISE_ROOT_DIR="$INSTALL/oidn" \
    -DSSE2NEON_INCLUDE_DIR="$SSE2NEON_INCLUDE" -DZSTD_ROOT_DIR="$DEPS" -DPUGIXML_ROOT_DIR="$DEPS" \
    -DCMAKE_BUILD_RPATH="$DEPS/lib;$INSTALL/oidn/lib" > "$PREFIX/cycles-configure.log"
  cmake --build "$BUILD/cycles" --parallel "$JOBS"

  # BSD cp has no --parents: keep each header's path with Python instead.
  rm -rf "${INSTALL:?}/cycles"
  python3 - "$CYCLES_SRC" "$BUILD/cycles" "$INSTALL/cycles" <<'PY'
import hashlib, pathlib, shutil, sys
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
# Metal compiles source at runtime. Preserve every nested kernel/util path,
# including .metal and non-.h include files; flattening CUDA kernels is not enough.
runtime = out / "runtime"
for directory in ("kernel", "util"):
    shutil.copytree(source / "src" / directory, runtime / "source" / directory)
with (runtime / "source.sha256").open("w") as manifest:
    for path in sorted((runtime / "source").rglob("*")):
        if path.is_file():
            manifest.write(hashlib.sha256(path.read_bytes()).hexdigest()
                           + "  " + path.relative_to(runtime).as_posix() + "\n")
PY
  printf '%s\n' cpu metal > "$INSTALL/cycles/devices.txt"
fi
mkdir -p "$INSTALL/cycles/licenses"
cp "$CYCLES_SRC"/src/doc/license/*license*.txt "$INSTALL/cycles/licenses/"

# The compile definitions and host options must match Cycles' headers.
# Export architecture-neutral options and ARM -m flags, never x86 SSE flags.
python3 - "$BUILD/cycles/compile_commands.json" "$INSTALL/cycles" "$INSTALL/oidn" \
  "$RENDER_BUILD_KEY" "$BUILDER_SHA256" "$RENDER_INPUT_SHA256" "$CYCLES_VERSION" "$CYCLES_COMMIT" "$OIDN_VERSION" \
  "$OIDN_SRC_SHA256" "$ISPC_VERSION" "$ISPC_MACOS_ARM64_SHA256" "$VCPKG_COMMIT" \
  "$TOOLCHAIN_SHA256" "$BUILD/oidn/CMakeCache.txt" <<'PY'
import json, pathlib, re, shlex, sys
commands, out, oidn, build_key, builder_sha, input_sha = sys.argv[1:7]
pins = dict(zip(("CYCLES_VERSION", "CYCLES_COMMIT", "OIDN_VERSION", "OIDN_SRC_SHA256",
                 "ISPC_VERSION", "ISPC_MACOS_ARM64_SHA256", "VCPKG_COMMIT"), sys.argv[7:14]))
toolchain_sha, oidn_cache_path = sys.argv[14:16]
oidn_cache = pathlib.Path(oidn_cache_path).read_text()
for option in ("OIDN_DEVICE_CPU", "OIDN_DEVICE_CPU_BNNS", "OIDN_DEVICE_METAL",
               "OIDN_LIBRARY_VERSIONED"):
    if not re.search(r"^" + option + r":BOOL=ON$", oidn_cache, re.MULTILINE):
        raise SystemExit("OIDN was not built with " + option + "; rebuild its dependencies")
oidn_modules = ["libOpenImageDenoise_device_" + device + "." + pins["OIDN_VERSION"] + ".dylib"
                for device in ("cpu", "metal")]
for module in oidn_modules:
    if not pathlib.Path(oidn, "lib", module).is_file():
        raise SystemExit("Incomplete OIDN runtime module: " + module)
with open(commands) as stream:
    entry = next(e for e in json.load(stream) if e["file"].endswith("src/session/session.cpp"))
args = shlex.split(entry["command"]) if "command" in entry else entry["arguments"]
defines = sorted({a[2:] for a in args if a.startswith("-D")})
define_names = {d.split("=", 1)[0] for d in defines}
if "WITH_SSE2NEON" not in define_names:
    raise SystemExit("Cycles was not built with SSE2NEON for arm64; rebuild its dependencies")
if "WITH_METAL" not in define_names:
    raise SystemExit("Cycles was not built with Metal; remove its completion stamp and rebuild")
defines.append("MITCAD_CYCLES_DENOISER_DIAGNOSTICS")
defines.append("MITCAD_CYCLES_NATIVE_DISPLAY_PASSES")
for resource in ("source/kernel/device/metal/kernel.metal", "source/util/types.h", "source.sha256"):
    if not pathlib.Path(out, "runtime", resource).is_file():
        raise SystemExit("Incomplete Cycles Metal runtime resources: " + resource)
x86_defines = [d for d in define_names if d.startswith(("WITH_KERNEL_SSE", "WITH_KERNEL_AVX",
                                                       "__SSE", "__AVX", "__MMX"))]
x86_option = re.compile(r"^-m(?:no-)?(?:sse|ssse|avx|mmx|f16c|fma|fpmath|lzcnt|bmi|"
                        r"aes|pclmul|3dnow|xop|rdrnd|rdseed|cx16|rtm|xsave)")
x86_options = [a for a in args if x86_option.match(a) or a in ("-m32", "-m64")
               or a.startswith(("-march=x86", "-march=i386", "-march=i486", "-march=i586", "-march=i686"))]
if x86_defines or x86_options:
    raise SystemExit("Refusing x86 Cycles definitions/options in the arm64 export: "
                     + ", ".join(sorted(x86_defines) + x86_options))
options = [a for a in args if (a.startswith("-m") or a.startswith("-f")) and a != "-fPIC"]
libraries = sorted(pathlib.Path(out, "lib").glob("*.a"))
if not libraries:
    raise SystemExit("No Cycles static libraries were installed")
with open(pathlib.Path(out, "mitcad-cycles.cmake"), "w") as stream:
    stream.write("# SPDX-License-Identifier: MIT\n")
    stream.write("# Written by tools/dev-env/build-cycles-macos.sh for cmake/Render.cmake.\n")
    stream.write('set(MITCAD_CYCLES_ROOT "${CMAKE_CURRENT_LIST_DIR}")\n')
    stream.write('get_filename_component(MITCAD_OIDN_ROOT "${CMAKE_CURRENT_LIST_DIR}/../oidn" ABSOLUTE)\n')
    stream.write("set(MITCAD_CYCLES_EXPORT_VERSION 3)\n")
    stream.write("set(MITCAD_CYCLES_BUILD_KEY %s)\n" % json.dumps(build_key))
    stream.write("set(MITCAD_CYCLES_BUILDER_SHA256 %s)\n" % json.dumps(builder_sha))
    stream.write("set(MITCAD_CYCLES_INPUT_SHA256 %s)\n" % json.dumps(input_sha))
    stream.write("set(MITCAD_RENDER_TOOLCHAIN_SHA256 %s)\n" % json.dumps(toolchain_sha))
    stream.write("set(MITCAD_OIDN_DEVICES CPU BNNS METAL)\n")
    stream.write("set(MITCAD_OIDN_METAL_RESOURCES EMBEDDED)\n")
    stream.write("set(MITCAD_OIDN_MODULES\n")
    for module in oidn_modules:
        stream.write('  "${MITCAD_OIDN_ROOT}/lib/%s"\n' % module)
    stream.write(")\n")
    for name, value in pins.items():
        stream.write("set(MITCAD_RENDER_%s %s)\n" % (name, json.dumps(value)))
    stream.write("set(MITCAD_CYCLES_ARCHITECTURES arm64)\n")
    stream.write("set(MITCAD_CYCLES_DEPLOYMENT_TARGET 14.4)\n")
    for name, values in (("DEFINITIONS", defines), ("OPTIONS", options)):
        stream.write("set(MITCAD_CYCLES_%s\n" % name)
        for value in values:
            stream.write("  %s\n" % json.dumps(value))
        stream.write(")\n")
    stream.write("set(MITCAD_CYCLES_DEVICES CPU METAL)\nset(MITCAD_CYCLES_KERNELS)\n")
    stream.write('set(MITCAD_CYCLES_RUNTIME "${MITCAD_CYCLES_ROOT}/runtime")\n')
    # Pinned upstream src/device/CMakeLists.txt and src/cmake/macros.cmake.
    stream.write("set(MITCAD_CYCLES_APPLE_FRAMEWORKS Metal Foundation IOKit Carbon Accelerate)\n")
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
touch "$CYCLES_DONE"
echo "Done: Mitcad's configure takes the renderer's libraries from $PREFIX (MITCAD_RENDER_DEPS)"

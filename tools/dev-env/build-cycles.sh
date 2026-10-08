#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Fetches and builds the Cycles render worker's libraries that vcpkg has no
# port for (MITCAD_RENDER, docs/rendering.md): Cycles itself (Blender's path
# tracer, Apache-2.0) and Open Image Denoise (Apache-2.0, built with Intel's
# ISPC compiler, BSD-3-Clause, which is only a build tool). Everything else
# (Embree, TBB, OpenImageIO, OpenColorIO, OpenEXR, pugixml, zstd, cgltf)
# comes from vcpkg through the manifest's "render" feature.
#
# Usage: tools/dev-env/build-cycles.sh [options] [prefix]
#
# Without options Cycles has its CPU device only. GPU devices (mitcad#50,
# docs/rendering.md "Devices"), each needing its vendor's SDK at build time:
#   --cuda            NVIDIA CUDA: the device and its kernels (cubins and
#                     PTX for --cuda-arch). The toolkit is MITCAD_CUDA_TOOLKIT
#                     (a folder with bin/nvcc), else the pinned compiler
#                     components (versions.sh) fetched into the prefix. The
#                     host compiler is MITCAD_CUDA_HOST_COMPILER, else a g++
#                     the toolkit supports (system, g++-14, g++-13), else
#                     Ubuntu's or Debian's g++-14 unpacked into the prefix
#                     (apt-get download, no root).
#   --cuda-arch LIST  the CUDA architectures (default CUDA_ARCHITECTURES)
#   --hip             AMD HIP: the device and its kernels (ROCm's HIP SDK,
#                     found through HIP_ROOT_DIR or hipcc on PATH)
#   --oneapi          Intel oneAPI: the device and its kernels (the DPC++
#                     compiler, SYCL_ROOT_DIR, and Level Zero)
#   --oidn-gpu        Open Image Denoise's devices for the chosen GPUs
#                     (CUDA: Turing and newer; HIP; SYCL), so that Cycles
#                     denoises on the GPU
# Apple Metal is part of Cycles' macOS build (WITH_CYCLES_DEVICE_METAL);
# this script builds for Linux only so far.
#
# The prefix (default: MITCAD_RENDER_DEPS, else ~/mitcad-render-deps) gets
#   downloads/  src/  build/       the pinned sources and their builds
#   vcpkg_installed/               the render feature's vcpkg libraries
#   install/oidn/                  Open Image Denoise (shared libraries)
#   install/cycles/                Cycles' headers, static libraries, GPU
#                                  kernels (kernels/lib) and
#                                  mitcad-cycles.cmake for Mitcad's build
#   cuda-<version>/, gcc-<n>/      with --cuda, when fetched
# Configure Mitcad with -DMITCAD_RENDER=ON -DMITCAD_RENDER_DEPS=<prefix>.
#
# Linux only for now. Pins: tools/dev-env/versions.sh. Re-running it is
# cheap: finished steps are skipped, and Cycles and Open Image Denoise are
# built again when the devices change (delete the prefix's build/ and
# install/ folders, by their literal paths, to build again).
#
# Environment: MITCAD_RENDER_JOBS (parallel compile jobs, default 12; the
# Cycles kernels need about 2 GB each), VCPKG_ROOT (default ~/vcpkg).
#
# Never delete a folder named by a variable here: rm -rf only on literal
# paths, or with "${VAR:?}" so that an empty variable stops the script.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$SCRIPT_DIR/../.." && pwd)
# shellcheck source=tools/dev-env/versions.sh
. "$SCRIPT_DIR/versions.sh"

if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != x86_64 ]; then
  echo "build-cycles.sh builds for x86-64 Linux only (so far)." >&2
  exit 1
fi

WITH_CUDA=0
CUDA_ARCH=$CUDA_ARCHITECTURES
WITH_HIP=0
WITH_ONEAPI=0
OIDN_GPU=0
PREFIX_ARG=
while [ $# -gt 0 ]; do
  case $1 in
    --cuda) WITH_CUDA=1 ;;
    --cuda-arch) CUDA_ARCH=${2:?--cuda-arch needs a list}; shift ;;
    --hip) WITH_HIP=1 ;;
    --oneapi) WITH_ONEAPI=1 ;;
    --oidn-gpu) OIDN_GPU=1 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed -e '/^set -euo/d' -e 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "Unknown option $1 (see --help)" >&2; exit 2 ;;
    *) PREFIX_ARG=$1 ;;
  esac
  shift
done

PREFIX=${PREFIX_ARG:-${MITCAD_RENDER_DEPS:-$HOME/mitcad-render-deps}}
JOBS=${MITCAD_RENDER_JOBS:-12}
VCPKG_ROOT=${VCPKG_ROOT:-$HOME/vcpkg}
TRIPLET=x64-linux-dynamic
mkdir -p "$PREFIX"
PREFIX=$(cd "$PREFIX" && pwd)
DOWNLOADS=$PREFIX/downloads
SRC=$PREFIX/src
BUILD=$PREFIX/build
INSTALL=$PREFIX/install
VCPKG_INSTALLED=$PREFIX/vcpkg_installed
mkdir -p "$DOWNLOADS" "$SRC" "$BUILD" "$INSTALL"

# A download checked against its SHA-256 pin.
fetch() {
  local url=$1 file=$2 sha256=$3
  if [ ! -f "$DOWNLOADS/$file" ]; then
    echo "   downloading $url"
    curl --proto '=https' --tlsv1.2 -fsSL -o "$DOWNLOADS/$file.part" "$url"
    mv "$DOWNLOADS/$file.part" "$DOWNLOADS/$file"
  fi
  if ! echo "$sha256  $DOWNLOADS/$file" | sha256sum --check --status; then
    echo "$file does not match its SHA-256 pin $sha256" >&2
    exit 1
  fi
}

echo "== vcpkg: the manifest's render feature ($TRIPLET)"
# The same manifest and baseline as Mitcad's own build, so the libraries
# Cycles is compiled against are the ones Mitcad links.
"$VCPKG_ROOT/vcpkg" install --x-manifest-root="$REPO" --x-feature=render \
  --x-install-root="$VCPKG_INSTALLED" --triplet "$TRIPLET" --no-print-usage > "$PREFIX/vcpkg.log" 2>&1 || {
  tail -40 "$PREFIX/vcpkg.log" >&2
  exit 1
}
DEPS=$VCPKG_INSTALLED/$TRIPLET

# The CUDA toolkit (--cuda): MITCAD_CUDA_TOOLKIT, else the pinned compiler
# components from NVIDIA's redistributable archives, unpacked into the
# prefix (no installer, no root).
CUDA_TOOLKIT=
CUDA_HOST_CXX=
CUDA_EXTRA_FLAGS=
cuda_toolkit() {
  if [ -n "${MITCAD_CUDA_TOOLKIT:-}" ]; then
    CUDA_TOOLKIT=$MITCAD_CUDA_TOOLKIT
  else
    CUDA_TOOLKIT=$PREFIX/cuda-$CUDA_VERSION
    if [ ! -f "$CUDA_TOOLKIT/.done" ]; then
      local redist=https://developer.download.nvidia.com/compute/cuda/redist
      local part version sha file
      mkdir -p "$CUDA_TOOLKIT"
      for part in nvcc:$CUDA_NVCC_VERSION:$CUDA_NVCC_SHA256 cudart:$CUDA_CUDART_VERSION:$CUDA_CUDART_SHA256 \
        cccl:$CUDA_CCCL_VERSION:$CUDA_CCCL_SHA256; do
        IFS=: read -r part version sha <<< "$part"
        file=cuda_$part-linux-x86_64-$version-archive.tar.xz
        fetch "$redist/cuda_$part/linux-x86_64/$file" "$file" "$sha"
        tar -xJf "$DOWNLOADS/$file" -C "$CUDA_TOOLKIT" --strip-components=1
      done
      # nvcc links from lib64, as the installer lays the toolkit out.
      ln -sfn lib "$CUDA_TOOLKIT/lib64"
      touch "$CUDA_TOOLKIT/.done"
    fi
  fi
  if [ ! -x "$CUDA_TOOLKIT/bin/nvcc" ]; then
    echo "No CUDA compiler $CUDA_TOOLKIT/bin/nvcc" >&2
    exit 1
  fi
}

# A host compiler nvcc supports (CUDA 12: GCC up to CUDA_MAX_GCC).
cuda_host_compiler() {
  local candidate major
  if [ -n "${MITCAD_CUDA_HOST_COMPILER:-}" ]; then
    CUDA_HOST_CXX=$MITCAD_CUDA_HOST_COMPILER
  else
    for candidate in g++ "g++-$CUDA_MAX_GCC" "g++-$((CUDA_MAX_GCC - 1))"; do
      command -v "$candidate" > /dev/null || continue
      major=$("$candidate" -dumpversion | cut -d. -f1)
      if [ "$major" -le "$CUDA_MAX_GCC" ]; then
        CUDA_HOST_CXX=$(command -v "$candidate")
        break
      fi
    done
  fi
  if [ -z "$CUDA_HOST_CXX" ]; then
    # The distribution's own packages of a supported GCC, unpacked into the
    # prefix (apt checks them against its signed package lists).
    local gcc_dir=$PREFIX/gcc-$CUDA_MAX_GCC v=$CUDA_MAX_GCC
    if [ ! -f "$gcc_dir/.done" ]; then
      if ! command -v apt-get > /dev/null; then
        echo "--cuda needs a g++ of version $CUDA_MAX_GCC or older: set MITCAD_CUDA_HOST_COMPILER" >&2
        exit 1
      fi
      mkdir -p "$DOWNLOADS/gcc-$v" "$gcc_dir"
      (cd "$DOWNLOADS/gcc-$v" && apt-get download "gcc-$v" "g++-$v" "cpp-$v" "cpp-$v-x86-64-linux-gnu" \
        "gcc-$v-x86-64-linux-gnu" "g++-$v-x86-64-linux-gnu" "gcc-$v-base" "libgcc-$v-dev" \
        "libstdc++-$v-dev" > "$PREFIX/gcc-download.log")
      for deb in "$DOWNLOADS/gcc-$v"/*.deb; do
        dpkg -x "$deb" "$gcc_dir"
      done
      touch "$gcc_dir/.done"
    fi
    CUDA_HOST_CXX=$gcc_dir/usr/bin/x86_64-linux-gnu-g++-$v
  fi
  # glibc 2.41 and newer declare sinpi, cospi and rsqrt as C23 functions
  # that CUDA 12's math headers declare differently; without _GNU_SOURCE in
  # the host compiler's pass glibc leaves them out (the kernels are device
  # code and use none of them).
  local glibc
  glibc=$(getconf GNU_LIBC_VERSION | awk '{print $2}')
  if [ "$(printf '%s\n2.41\n' "$glibc" | sort -V | head -1)" = 2.41 ]; then
    CUDA_EXTRA_FLAGS="-Xcompiler=-U_GNU_SOURCE"
  fi
  echo "   CUDA $("$CUDA_TOOLKIT/bin/nvcc" --version | sed -n 's/.*release \([0-9.]*\).*/\1/p') in $CUDA_TOOLKIT," \
    "host compiler $CUDA_HOST_CXX ($("$CUDA_HOST_CXX" -dumpfullversion)), glibc $glibc"
}

if [ "$WITH_CUDA" = 1 ]; then
  echo "== CUDA toolkit (build tool)"
  cuda_toolkit
  cuda_host_compiler
fi
# What the devices are built with; a change builds Cycles and Open Image
# Denoise again.
DEVICES=cpu
[ "$WITH_CUDA" = 1 ] && DEVICES="$DEVICES cuda($CUDA_ARCH)"
[ "$WITH_HIP" = 1 ] && DEVICES="$DEVICES hip"
[ "$WITH_ONEAPI" = 1 ] && DEVICES="$DEVICES oneapi"
DEVICES_STAMP=$(printf '%s' "$DEVICES" | sha256sum | cut -c1-12)
echo "== Devices: $DEVICES$([ "$OIDN_GPU" = 1 ] && echo ", Open Image Denoise on the GPUs")"

echo "== ISPC $ISPC_VERSION (build tool)"
ISPC_DIR=$SRC/ispc-v$ISPC_VERSION-linux
fetch "https://github.com/ispc/ispc/releases/download/v$ISPC_VERSION/ispc-v$ISPC_VERSION-linux.tar.gz" \
  "ispc-v$ISPC_VERSION-linux.tar.gz" "$ISPC_LINUX_SHA256"
if [ ! -x "$ISPC_DIR/bin/ispc" ]; then
  tar -xzf "$DOWNLOADS/ispc-v$ISPC_VERSION-linux.tar.gz" -C "$SRC"
fi

echo "== Open Image Denoise $OIDN_VERSION"
OIDN_SRC=$SRC/oidn-$OIDN_VERSION
fetch "https://github.com/RenderKit/oidn/releases/download/v$OIDN_VERSION/oidn-$OIDN_VERSION.src.tar.gz" \
  "oidn-$OIDN_VERSION.src.tar.gz" "$OIDN_SRC_SHA256"
if [ ! -f "$OIDN_SRC/CMakeLists.txt" ]; then
  tar -xzf "$DOWNLOADS/oidn-$OIDN_VERSION.src.tar.gz" -C "$SRC"
fi
# Open Image Denoise's GPU devices load their driver at run time; the CUDA
# one uses the driver API (no CUDA runtime linked) and runs on Turing and
# newer GPUs.
OIDN_GPU_ARGS=(-DOIDN_DEVICE_CUDA=OFF -DOIDN_DEVICE_HIP=OFF -DOIDN_DEVICE_SYCL=OFF)
OIDN_STAMP=$OIDN_VERSION
if [ "$OIDN_GPU" = 1 ]; then
  OIDN_STAMP=$OIDN_VERSION-gpu-$DEVICES_STAMP
  if [ "$WITH_CUDA" = 1 ]; then
    # Its kernels are for Turing and newer (sm_75 and up), which CUDA 13
    # compiles too: MITCAD_OIDN_CUDA_TOOLKIT may name another toolkit than
    # Cycles' (with the host compiler that toolkit takes by default).
    OIDN_GPU_ARGS[0]=-DOIDN_DEVICE_CUDA=ON
    if [ -n "${MITCAD_OIDN_CUDA_TOOLKIT:-}" ]; then
      OIDN_GPU_ARGS+=(-DCUDAToolkit_ROOT="$MITCAD_OIDN_CUDA_TOOLKIT")
    elif [ -n "$CUDA_EXTRA_FLAGS" ]; then
      # Its .cu files have host code that needs glibc's GNU extensions, so
      # the kernels' way around CUDA 12's math headers does not work.
      echo "--oidn-gpu: Open Image Denoise's CUDA device does not build with CUDA 12 on glibc 2.41 or" \
        "newer; set MITCAD_OIDN_CUDA_TOOLKIT to a CUDA 13 toolkit" >&2
      exit 1
    else
      OIDN_GPU_ARGS+=(-DCUDAToolkit_ROOT="$CUDA_TOOLKIT")
      # The device is an external project of its own: the host compiler
      # reaches it through CMake's environment variable.
      export CUDAHOSTCXX=$CUDA_HOST_CXX
    fi
  fi
  [ "$WITH_HIP" = 1 ] && OIDN_GPU_ARGS[1]=-DOIDN_DEVICE_HIP=ON
  [ "$WITH_ONEAPI" = 1 ] && OIDN_GPU_ARGS[2]=-DOIDN_DEVICE_SYCL=ON
fi
if [ ! -f "$INSTALL/oidn/.done-$OIDN_STAMP" ]; then
  # The CPU device (and with --oidn-gpu the GPUs' devices), the ray tracing
  # filter's weights only (the lightmap filter is for baking); no example or
  # test programs.
  rm -f "$INSTALL"/oidn/.done-*
  cmake -S "$OIDN_SRC" -B "$BUILD/oidn" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$INSTALL/oidn" \
    -DCMAKE_PREFIX_PATH="$DEPS" \
    -DISPC_EXECUTABLE="$ISPC_DIR/bin/ispc" \
    -DOIDN_DEVICE_CPU=ON "${OIDN_GPU_ARGS[@]}" -DOIDN_DEVICE_METAL=OFF \
    -DOIDN_FILTER_RT=ON -DOIDN_FILTER_RTLIGHTMAP=OFF \
    -DOIDN_APPS=OFF -DOIDN_INSTALL_DEPENDENCIES=OFF \
    -DCMAKE_INSTALL_RPATH="$DEPS/lib" > "$PREFIX/oidn-configure.log"
  cmake --build "$BUILD/oidn" --parallel "$JOBS"
  cmake --install "$BUILD/oidn" > /dev/null
  touch "$INSTALL/oidn/.done-$OIDN_STAMP"
fi

echo "== Cycles $CYCLES_VERSION"
CYCLES_SRC=$SRC/cycles-$CYCLES_VERSION
if [ ! -d "$CYCLES_SRC/.git" ]; then
  git clone --quiet --depth 1 --branch "v$CYCLES_VERSION" https://projects.blender.org/blender/cycles.git \
    "$CYCLES_SRC"
fi
if [ "$(git -C "$CYCLES_SRC" rev-parse HEAD)" != "$CYCLES_COMMIT" ]; then
  echo "Cycles v$CYCLES_VERSION is not at the pinned commit $CYCLES_COMMIT" >&2
  exit 1
fi
CYCLES_DONE=$INSTALL/cycles/.done-$CYCLES_VERSION
[ "$DEVICES" = cpu ] || CYCLES_DONE=$CYCLES_DONE-$DEVICES_STAMP
# The GPU devices asked for (off by default; never OptiX, whose licence
# does not fit open source: README, FAQ). Cycles loads the drivers at
# run time (cuew, hipew; Level Zero through SYCL), so a GPU build runs on
# machines without them, with the CPU only.
CYCLES_GPU_ARGS=(-DWITH_CYCLES_DEVICE_CUDA=OFF -DWITH_CYCLES_DEVICE_OPTIX=OFF -DWITH_CYCLES_DEVICE_HIP=OFF
  -DWITH_CYCLES_DEVICE_HIPRT=OFF -DWITH_CYCLES_DEVICE_METAL=OFF -DWITH_CYCLES_DEVICE_ONEAPI=OFF)
if [ "$WITH_CUDA" = 1 ]; then
  CYCLES_GPU_ARGS+=(-DWITH_CYCLES_DEVICE_CUDA=ON -DWITH_CYCLES_CUDA_BINARIES=ON -DWITH_CUDA_DYNLOAD=ON
    -DCYCLES_CUDA_BINARIES_ARCH="$CUDA_ARCH" -DCUDA_TOOLKIT_ROOT_DIR="$CUDA_TOOLKIT"
    -DCUDA_HOST_COMPILER="$CUDA_HOST_CXX")
  if [ -n "$CUDA_EXTRA_FLAGS" ]; then
    CYCLES_GPU_ARGS+=(-DCUDA_NVCC_FLAGS="$CUDA_EXTRA_FLAGS")
  fi
fi
if [ "$WITH_HIP" = 1 ]; then
  CYCLES_GPU_ARGS+=(-DWITH_CYCLES_DEVICE_HIP=ON -DWITH_CYCLES_HIP_BINARIES=ON -DWITH_HIP_DYNLOAD=ON)
fi
if [ "$WITH_ONEAPI" = 1 ]; then
  CYCLES_GPU_ARGS+=(-DWITH_CYCLES_DEVICE_ONEAPI=ON -DWITH_CYCLES_ONEAPI_BINARIES=ON)
fi
if [ ! -f "$CYCLES_DONE" ]; then
  rm -f "$INSTALL"/cycles/.done-*
  # The CPU device with Embree and Open Image Denoise, and the GPU devices
  # asked for; no OSL, USD, Alembic, OpenVDB or OpenSubdiv, no precompiled
  # libraries (vcpkg's instead). The standalone program is built too: it
  # renders Cycles' XML scenes, a check of the build without Mitcad.
  # Later options win, so the GPU options follow the defaults.
  cmake -S "$CYCLES_SRC" -B "$BUILD/cycles" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_PREFIX_PATH="$DEPS;$INSTALL/oidn" \
    -DCMAKE_INSTALL_PREFIX="$BUILD/cycles/install" \
    -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
    -DWITH_LIBS_PRECOMPILED=OFF \
    -DWITH_CYCLES_EMBREE=ON -DWITH_CYCLES_OPENIMAGEDENOISE=ON \
    -DWITH_CYCLES_ALEMBIC=OFF -DWITH_CYCLES_OPENSUBDIV=OFF -DWITH_CYCLES_OPENVDB=OFF \
    -DWITH_CYCLES_NANOVDB=OFF -DWITH_CYCLES_OSL=OFF -DWITH_CYCLES_USD=OFF \
    -DWITH_CYCLES_HYDRA_RENDER_DELEGATE=OFF -DWITH_CYCLES_LOGGING=OFF \
    -DWITH_CYCLES_PATH_GUIDING=OFF -DWITH_CYCLES_STANDALONE_GUI=OFF \
    "${CYCLES_GPU_ARGS[@]}" \
    -DEMBREE_ROOT_DIR="$DEPS" -DTBB_ROOT_DIR="$DEPS" -DOPENIMAGEDENOISE_ROOT_DIR="$INSTALL/oidn" \
    -DZSTD_ROOT_DIR="$DEPS" -DPUGIXML_ROOT_DIR="$DEPS" \
    -DCMAKE_INSTALL_RPATH="$DEPS/lib;$INSTALL/oidn/lib" > "$PREFIX/cycles-configure.log"
  KERNELS_STARTED=$(date +%s)
  cmake --build "$BUILD/cycles" --parallel "$JOBS"
  echo "   built in $(( $(date +%s) - KERNELS_STARTED )) s"

  # Install for Mitcad: the headers in Cycles' own layout (src/ is the
  # include root), the static libraries, and the compile definitions the
  # libraries were built with, which the headers depend on.
  rm -rf "${INSTALL:?}/cycles"
  mkdir -p "$INSTALL/cycles/include" "$INSTALL/cycles/lib"
  mkdir -p "$INSTALL/cycles/include/third_party"
  (cd "$CYCLES_SRC/src" && find . -name '*.h' -exec cp --parents {} "$INSTALL/cycles/include/" \;)
  (cd "$CYCLES_SRC/third_party" && find . -name '*.h' -exec cp --parents {} "$INSTALL/cycles/include/third_party/" \;)
  find "$BUILD/cycles" -name 'libcycles_*.a' -exec cp {} "$INSTALL/cycles/lib/" \;
  find "$BUILD/cycles" -name 'libextern_*.a' -exec cp {} "$INSTALL/cycles/lib/" \;
  cp "$CYCLES_SRC/LICENSE" "$INSTALL/cycles/LICENSE"
  # The GPU kernels, in the layout Cycles looks for them at run time
  # (<path>/lib/kernel_sm_61.cubin.zst, ...).
  mkdir -p "$INSTALL/cycles/kernels/lib"
  find "$BUILD/cycles/src/kernel" \( -name '*.cubin.zst' -o -name '*.ptx.zst' -o -name '*.fatbin.zst' \
    -o -name 'libcycles_kernel_oneapi*.so' \) -exec cp {} "$INSTALL/cycles/kernels/lib/" \;
  printf '%s\n' "$DEVICES" > "$INSTALL/cycles/devices.txt"
  touch "$CYCLES_DONE"
fi
# The licence texts of the code Cycles bundles (its SPDX lines name them),
# for the packages' notices (cmake/AppImage.cmake).
mkdir -p "$INSTALL/cycles/licenses"
cp "$CYCLES_SRC"/src/doc/license/*license*.txt "$INSTALL/cycles/licenses/"
# The drivers' loaders linked with a GPU device (Apache-2.0 with a stricter
# trademark clause).
if [ "$WITH_CUDA" = 1 ]; then
  cp "$CYCLES_SRC/third_party/cuew/LICENSE" "$INSTALL/cycles/licenses/cuew-license.txt"
fi
if [ "$WITH_HIP" = 1 ]; then
  cp "$CYCLES_SRC/third_party/hipew/LICENSE" "$INSTALL/cycles/licenses/hipew-license.txt"
fi
# What Mitcad's build needs to know of the Cycles build (cmake/Render.cmake).
python3 - "$BUILD/cycles/compile_commands.json" "$INSTALL/cycles" "$INSTALL/oidn" "$DEPS" "$DEVICES" <<'PY'
import json, os, re, shlex, sys
commands, out, oidn, deps, devices = sys.argv[1:6]
entry = next(e for e in json.load(open(commands)) if e["file"].endswith("src/session/session.cpp"))
args = shlex.split(entry["command"]) if "command" in entry else entry["arguments"]
defines = sorted({a[2:] for a in args if a.startswith("-D")})
# The instruction sets and floating point options of the host code (SSE4.2):
# inline functions in the headers use them.
options = [a for a in args if (a.startswith("-m") or a.startswith("-f")) and a != "-fPIC"]
libs = sorted(f for f in os.listdir(os.path.join(out, "lib")) if f.endswith(".a"))
with open(os.path.join(out, "mitcad-cycles.cmake"), "w") as f:
    f.write("# SPDX-License-Identifier: MIT\n")
    f.write("# Written by tools/dev-env/build-cycles.sh: the Cycles build, for cmake/Render.cmake.\n")
    f.write("set(MITCAD_CYCLES_ROOT \"%s\")\n" % out)
    f.write("set(MITCAD_OIDN_ROOT \"%s\")\n" % oidn)
    f.write("set(MITCAD_CYCLES_DEFINITIONS\n")
    for d in defines:
        f.write("  %s\n" % json.dumps(d))
    f.write(")\n")
    f.write("set(MITCAD_CYCLES_OPTIONS\n")
    for o in options:
        f.write("  %s\n" % json.dumps(o))
    f.write(")\n")
    # The devices built (CPU, CUDA, HIP, ONEAPI) and their kernels,
    # which go next to mitcad-render.
    names = [re.sub(r"\(.*", "", d).upper() for d in devices.split()]
    f.write("set(MITCAD_CYCLES_DEVICES %s)\n" % " ".join(names))
    kernels = os.path.join(out, "kernels", "lib")
    f.write("set(MITCAD_CYCLES_KERNELS\n")
    for k in sorted(os.listdir(kernels)) if os.path.isdir(kernels) else []:
        f.write("  \"${MITCAD_CYCLES_ROOT}/kernels/lib/%s\"\n" % k)
    f.write(")\n")
    f.write("set(MITCAD_CYCLES_LIBRARIES\n")
    for l in libs:
        f.write("  \"${MITCAD_CYCLES_ROOT}/lib/%s\"\n" % l)
    f.write(")\n")
PY

echo "== Check: Cycles' standalone program renders a test scene"
mkdir -p "$BUILD/check"
"$BUILD/cycles/bin/cycles" --background --quiet --samples 4 --width 64 --height 48 \
  --output "$BUILD/check/scene.png" "$CYCLES_SRC/examples/scene_cube_surface.xml"
test -s "$BUILD/check/scene.png"
echo "Done: configure Mitcad with -DMITCAD_RENDER=ON -DMITCAD_RENDER_DEPS=$PREFIX"

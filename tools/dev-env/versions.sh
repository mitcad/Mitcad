# SPDX-License-Identifier: MIT
# The tool versions pinned for the isolated build environments, sourced by
# setup-user.sh (Linux) and setup-macos.sh. setup-windows.ps1 cannot source
# this file and mirrors the shared pins by hand: keep them in step.

QT_VERSION=6.12.0
# aqtinstall 3.3.0 cannot install Qt 6.11+ on Windows; the fix (PR #1000)
# is unreleased, so a pinned commit is used on every platform.
AQT_SPEC="aqtinstall @ git+https://github.com/miurahr/aqtinstall@076e1659807d0b362a3ed684d54c2e9c775eb9c7"
CARGO_DENY_VERSION=0.20.2
# Must match "builtin-baseline" in vcpkg.json.
VCPKG_COMMIT=3aea538b2bb21a586502c67b00eb474fdd2e3098

# macOS only: CMake and Ninja come from the upstream release archives, not
# Homebrew, checked against these SHA-256 pins. CMake's is from
# cmake-<version>-SHA-256.txt next to its release files, Ninja's from the
# digest GitHub lists for the release asset.
CMAKE_VERSION=3.31.6
CMAKE_MACOS_SHA256=330b9514f5112e5ed4fb08b8b05803b776fd9b539a6ae12927d14dcc0ee2ba8d
NINJA_VERSION=1.13.2
NINJA_MAC_SHA256=c99048673aa765960a99cf10c6ddb9f1fad506099ff0a0e137ad8960a88f321b
# macOS compiler caches (build tools only). Digests published for these
# assets by the upstream release APIs, also checked against the archives:
# https://api.github.com/repos/ccache/ccache/releases/tags/v4.14.2
# https://api.github.com/repos/mozilla/sccache/releases/tags/v0.18.0
# The Darwin ccache archive is universal (includes arm64).
CCACHE_MACOS_VERSION=4.14.2
CCACHE_MACOS_SHA256=cf240009630b605b775c7c5c7b7105a3b196c79f31af322c1557969aa28003fc
SCCACHE_MACOS_VERSION=0.18.0
SCCACHE_MACOS_ARM64_SHA256=308184519b646f5125289e8515b36f6ca65a13a041923994aebe702348674e8e
# Linux x86_64, the same release (setup-user.sh; musl, a static binary):
SCCACHE_LINUX_VERSION=0.18.0
SCCACHE_LINUX_X86_64_SHA256=45f1447fbe231e3037bde351ef70677dd212216c8d62ae7ca409fecc4d6acc89
# A standalone CPython for aqtinstall (python-build-standalone), since the
# Command Line Tools' python3 is too old; the digest is GitHub's for the asset.
PYTHON_VERSION=3.12.15
PYTHON_BUILD=20261003
PYTHON_MAC_SHA256=316a463172740e71d8dca1f2730784e325f3f720941137b5d674d5801a632213
# pkgconf for vcpkg; its distfiles have no checksum files, so the pin is the
# hash Homebrew's formula records for the same archive.
PKGCONF_VERSION=3.0.7
PKGCONF_SHA256=c926ff491cbd9a331a589160811bd97ab1749b4d5198a519338f2cdfabe6940a

# QtKeychain (BSD-3-Clause), the live updates' broker credentials in the
# system's keychain (mitcad#89): the release's source archive
# (https://github.com/frankosterfeld/qtkeychain/archive/refs/tags/<version>.tar.gz),
# which cmake/Keychain.cmake fetches, checks and builds on every platform;
# the digest is of that archive.
QTKEYCHAIN_VERSION=0.17.0
QTKEYCHAIN_SHA256=3b85c3929034b0a99da777130c34d99f006fcd3a9d56564159399a33fee0e504

# The Cycles render worker (tools/dev-env/build-cycles.sh, every build):
# the sources that vcpkg has no port for. Cycles is fetched by tag and
# checked against the tag's commit; Open Image Denoise's source archive and
# ISPC's release (a build tool for Open Image Denoise, not linked) against
# the SHA-256 digests GitHub lists for the release assets.
CYCLES_VERSION=5.2.0
CYCLES_COMMIT=3b97e190c5ff1a2ed2160d879ad5bf95bea7b8ba
OIDN_VERSION=2.5.1
OIDN_SRC_SHA256=e71fd043a70f1cc80e301d1b90df6c1f536098c4dd94baa612742f6db3369c36
ISPC_VERSION=1.31.0
ISPC_LINUX_SHA256=d74089c835e10fd7e2c4b9225ced38b87d1fb53d35c7ceabd48cdf035da11b11
# build-cycles.ps1 reads this file's pins itself.
ISPC_WINDOWS_SHA256=9a18793800b91d5be7b851513672cd9a81a985a5a5dfec5611c2318e8ad4140a
# Native Apple Silicon archive; GitHub's official release asset digest:
# https://api.github.com/repos/ispc/ispc/releases/tags/v1.31.0
ISPC_MACOS_ARM64_SHA256=eac8009da38d41074d0adcf1fad4a3412fc9644a81ee5a49efeb07eac505b6ec
# Cycles' GPU devices (build-cycles.sh --cuda, mitcad#50). The CUDA
# compiler's components come from NVIDIA's redistributable archives
# (https://developer.download.nvidia.com/compute/cuda/redist/), checked
# against the SHA-256 digests of redistrib_<CUDA_VERSION>.json there; only
# a build tool, nothing of it is linked or shipped. CUDA 12 is the last
# that compiles for Maxwell, Pascal and Volta (sm_5x, sm_6x, sm_70); it
# takes host compilers up to GCC 14 (CUDA_MAX_GCC).
CUDA_VERSION=12.9.1
CUDA_NVCC_VERSION=12.9.86
CUDA_NVCC_SHA256=7a1a5b652e5ef85c82b721d10672fc9a2dbaab44e9bd3c65a69517bf53998c35
CUDA_CUDART_VERSION=12.9.79
CUDA_CUDART_SHA256=1f6ad42d4f530b24bfa35894ccf6b7209d2354f59101fd62ec4a6192a184ce99
CUDA_CCCL_VERSION=12.9.27
CUDA_CCCL_SHA256=8b1a5095669e94f2f9afd7715533314d418179e9452be61e2fde4c82a3e542aa
CUDA_MAX_GCC=14
# The same components' Windows archives (build-cycles.ps1 -Cuda, mitcad#202),
# from the same redistrib_<CUDA_VERSION>.json; the host compiler is MSVC
# (CUDA 12.9 takes Visual Studio 2017 to 2022).
CUDA_NVCC_WINDOWS_SHA256=227b109663b5e57d2718bcabb24a4ba0d9d4e52d958e327dc476f7c28691be85
CUDA_CUDART_WINDOWS_SHA256=179e9c43b0735ffe67207b3da556eb5a0c50f3047961882b7657d3b822d34ef8
CUDA_CCCL_WINDOWS_SHA256=17aaa7c6b8f94a417d8f3261780b7e34b9cbdfab7513bce86768623b06aa28b5
# The kernels: a cubin per architecture (a GPU runs the one of its major
# version with the highest minor not above its own) and PTX for compute
# 7.5, which the driver compiles for newer GPUs (sm_80, sm_90, ...).
CUDA_ARCHITECTURES="sm_50;sm_52;sm_60;sm_61;sm_70;sm_75;sm_86;sm_89;sm_120;compute_75"

# The Chromium Embedded Framework of Insert > Component from Web's web
# browser component (mitcad#1579): CEF's official binary distribution
# (https://cef-builds.spotifycdn.com/, the "minimal" one: the runtime, its
# headers, libcef_dll_wrapper's sources and its CMake files), fetched and
# unpacked by fetch-cef.sh / fetch-cef.ps1 into ~/cef (C:\dev\cef), which
# MITCAD_CEF_ROOT then names. The SHA-1 pins are the digests cef-builds
# publishes beside each archive (<archive>.sha1); while a pin is empty the
# fetch checks the archive against that published file and prints the
# digest to pin. The web browser component's package (tools/browser-package.sh,
# tools/browser-package.ps1) is rebuilt when CEF_VERSION changes.
CEF_VERSION=156.0.27+ga10dae3+chromium-156.0.8078.88
CEF_LINUX64_SHA1=1ccade9535dba5362f0c6adb05f9ae019e860af1
CEF_MACOSARM64_SHA1=fe8d1915b99c5ad6ba00c4cc13f53af1ae289ff3
CEF_WINDOWS64_SHA1=f3bac43e0e5a8ead78532bbc8f1de5c19f97d569

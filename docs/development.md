# Developing MitCad

## Isolated builds

Dependency build scripts (Cargo, vcpkg, pip) run third-party code, so
MitCad is built and tested only in isolated environments. Source and git
stay on the host; the working tree is mirrored one way into the build
environment before each build.

- Windows host: a **WSL distro** (no interop, no Windows drives) for
  Linux, a **Hyper-V VM** over SSH for Windows.
- Linux host: any Ubuntu 24.04 container or VM; `install-packages.sh` and
  `setup-user.sh` work there. A host with a newer system builds releases
  in an Ubuntu 24.04 build root
  ([Linux on a newer host](#linux-on-a-newer-host)).
- Mac host (Apple Silicon): a **macOS guest VM** over SSH for macOS
  ([macOS: the build VM](#macos-the-build-vm)).

## Linux: the WSL distro

One-time setup (`<repo>` is the checkout as seen from the distro):

```bat
wsl --install Ubuntu-24.04 --name mitcad-dev
wsl -d mitcad-dev -u root -e sh -c "cp '/mnt/c/<repo>/tools/dev-env/wsl.conf' /etc/wsl.conf"
wsl --terminate mitcad-dev
```

Then, from Git Bash:

```bash
tools/dev-env/sync-to-wsl.sh
wsl -d mitcad-dev --cd "~/src/mitcad" -- sudo tools/dev-env/install-packages.sh
wsl -d mitcad-dev --cd "~/src/mitcad" -- bash -lc "tools/dev-env/setup-user.sh"
wsl -d mitcad-dev --cd "~/src/mitcad" -- bash -lc "cmake --preset dev && cmake --build --preset dev && ctest --preset dev"
```

| Script (`tools/dev-env`) | Use |
|---|---|
| `sync-to-wsl.sh [distro] [dir]` | Mirrors tracked and non-ignored untracked files into the distro (default `mitcad-dev`, `~/src/mitcad`). Only files whose content differs are replaced and get the sync time, so switching branches rebuilds exactly what changed. Deletes removed files; keeps `build/` and `CMakeUserPresets.json`. |
| `install-packages.sh` | System packages (compilers, CMake, Ninja, ccache, rsync, X11/OpenGL headers, gdb, Xvfb, xdotool, rsvg-convert, ImageMagick; mosquitto for the live updates' tests, its service disabled; libsecret's headers for QtKeychain and GNOME's keyring for the keychain's test). Run as root. |
| `setup-user.sh` | Pinned per-user toolchain: Rust from `rust-toolchain.toml`, cargo-deny, vcpkg at `vcpkg.json`'s `builtin-baseline`, Qt via aqtinstall, the render worker's Cycles with its CUDA device and Open Image Denoise (`build-cycles.sh --cuda`, once; `MITCAD_RENDER_CUDA=0` without CUDA); writes `CMakeUserPresets.json` (`dev` = Linux debug). Under WSLg sets `GALLIUM_DRIVER=d3d12`; `MITCAD_GPU_ADAPTER=NVIDIA` picks a GPU. |
| `install-appimage-tools.sh` | AppImage tools into `~/appimage-tools` (`MITCAD_APPIMAGE_TOOLS`), see [below](#tools-downloaded-from-releases). |
| `build-root.sh` | An Ubuntu 24.04 build root on a newer Linux host, entered with bubblewrap, see [below](#linux-on-a-newer-host). |

Parallel checkouts: `sync-to-wsl.sh mitcad-dev src/mitcad-<name>` and copy
`CMakeUserPresets.json` from the first one.

Gotchas:

- Use a login shell (`bash -lc`, `bash -l -s`) so cargo is on `PATH`.
- `wsl.exe` expands `$` on the Windows side. In Git Bash set
  `MSYS_NO_PATHCONV=1` and pipe anything non-trivial as a script:
  `wsl -d mitcad-dev -- bash -l -s < script.sh`. From PowerShell use
  `cmd /c 'wsl ... < script.sh'` (a PowerShell pipe adds a BOM and `\r`).
- `/tmp` in WSL can be emptied mid-session; keep files in `~`.
- Keep `sudo` password-protected; install as root with
  `wsl -d mitcad-dev -u root ...`.

## Linux on a newer host

The Linux AppImage takes glibc and libstdc++ from the user's system, so it
needs those of the system it was built on
([Linux AppImage](#linux-appimage)). A Linux host newer than Ubuntu 24.04
builds the release in an **Ubuntu 24.04 build root**
(`tools/dev-env/build-root.sh`): a debootstrapped noble file system that
bubblewrap (`bwrap`, with unprivileged user namespaces) enters as the
host's own user, with a home folder of its own that holds the toolchain
built against the root's glibc. Builds and tests run there unchanged:
`check-all.sh`, the presets and `appimage-test.sh` see an Ubuntu 24.04
system.

```bash
# As root, once (again later to update the root's packages):
tools/dev-env/build-root.sh create <root>
# As the user, once: Rust, vcpkg, Qt, the render worker's libraries (with
# CUDA) and the AppImage tools into <home>; CMakeUserPresets.json in
# <home>/src/mitcad
tools/dev-env/build-root.sh setup --bind <data folder> <root> <home> <checkout>
# Any command in the root (default: a login shell)
tools/dev-env/build-root.sh run --bind <data folder> <root> <home> bash -lc 'cmake --preset dev && ...'
```

`run` mounts the root's `/usr`, `/etc` and `/var` read-only and binds
`<home>`, each `--bind` folder (at the same path), `/tmp` (shared, so
Xvfb display numbers do not clash with the host's), the user's runtime
folder (so `check-all.sh` gets its memory-capped `systemd-run --user`
scope) and the network. The test runner's history (`<home>/.mitcad`, the
time and memory of each test, [tools/testrun](../tools/testrun/README.md))
is the user's own `~/.mitcad`, shared both ways: a new root schedules from
what the host's runs learnt, its runs add to it, and neither side falls
back to an older copy. The environment is cleared but for the `MITCAD_*`
variables (corpora, test slots) and the build parallelism
(`CMAKE_BUILD_PARALLEL_LEVEL`, `CARGO_BUILD_JOBS`,
`VCPKG_MAX_CONCURRENCY`); a `MITCAD_*` path outside the bound folders is
reported, since the root cannot see it. The user's `/etc/passwd` entry
names `<home>` as the home folder. Copy the checkout's
`CMakeUserPresets.json` from `<home>/src/mitcad`, not from the host,
whose render prefix and toolchain are the host's.

`build-root.sh create --plain <root>` makes a second root with only what an
Ubuntu 24.04 desktop has for running MitCad (glibc, libstdc++, X11/xcb,
Mesa, GLVND's `libOpenGL`, fontconfig, D-Bus, GLib) plus Xvfb and
xdotool: a clean 24.04 to start an AppImage on once, on a hidden display.

Memory: the root's GCC 13 and `ld` need more than a newer toolchain
(about 2.5 GB per AVX-512 compile of a render library and per link of a
debug test executable). Under a memory cap, lower `VCPKG_MAX_CONCURRENCY`
and `MITCAD_RENDER_JOBS` for `setup`, and `CMAKE_BUILD_PARALLEL_LEVEL`
(or a link job pool, `-DCMAKE_JOB_POOLS=link=1
-DCMAKE_JOB_POOL_LINK=link`) for a fresh build.

## Windows: the build VM

A Hyper-V VM with Visual Studio 2022+ (C++ desktop tools) and OpenSSH:

```bash
tools/dev-env/sync-to-vm.sh <user>@<vm-host>
ssh -i <key> <user>@<vm-host> "powershell -ExecutionPolicy Bypass -File C:\dev\src\mitcad\tools\dev-env\setup-windows.ps1"
ssh -i <key> <user>@<vm-host> "cd /d C:\dev\src\mitcad && tools\dev-env\msvc.cmd cmake --preset dev -DMITCAD_MESA_DIR=C:/dev/mesa/bin && tools\dev-env\msvc.cmd cmake --build --preset dev && tools\dev-env\msvc.cmd ctest --preset dev"
```

| Script | Use |
|---|---|
| `sync-to-vm.sh [user@host] [dir]` | Like `sync-to-wsl.sh`, over SSH. Defaults: `MITCAD_VM`, `MITCAD_VM_DIR` (`C:/dev/src/mitcad`), `MITCAD_VM_KEY` (`~/.ssh/mitcad_vm`). |
| `setup-windows.ps1 [-Repo <dir>]` | Installs Git, CMake, Ninja, Rustup, Python, NSIS (winget), ccache, the pinned Rust toolchain, cargo-deny, vcpkg and Qt under `C:\dev`, the render worker's libraries (`build-cycles.ps1 -Cuda`, once; `MITCAD_RENDER_CUDA=0` without CUDA); writes `CMakeUserPresets.json` (`dev` = Windows release). Run as administrator. |
| `setup-ccache-windows.ps1 [-Dir] [-MaxSize]` | ccache into `C:\dev\ccache`, on the machine `PATH`, cache limit 4G (a build with the render worker; 2G was cleaned over and over). |
| `setup-mesa-windows.ps1 [-Out] [-Vcpkg] [-Jobs]` | Builds Mesa's llvmpipe (~20 min) into `<Out>\bin` (default `C:\dev\mesa`). Needed in a VM without a GPU (Windows offers only OpenGL 1.1). |
| `msvc.cmd <command>` | Runs a command in the VS x64 environment, rebuilding `PATH` from the registry first (SSH sessions keep a stale one). |

`-DMITCAD_MESA_DIR=<dir>/bin` copies Mesa's DLLs next to `mitcad.exe`
(development only, never in a release) and enables the Windows UI tests.
A second checkout in the VM needs its own directory and preset file.

## macOS: the build VM

An arm64 macOS guest on Apple's Virtualization.framework, e.g. with
[lume](https://github.com/trycua/lume) (MIT) or UTM; nothing is built on
the host:

```bash
brew install lume
lume config telemetry disable
lume create mitcad-mac --cpu 8 --memory 16 --disk-size 150 \
    --display 2880x1800 --ipsw latest --unattended tahoe
lume run mitcad-mac --detach --display none --vnc disabled
lume get mitcad-mac                      # the guest's IP address
```

No shared folders or clipboard; turn on Remote Login and install the Xcode
Command Line Tools with the macOS 27 SDK or newer, including the Metal SDK
headers and frameworks. The deployment target remains macOS 14.4; newer
AppKit APIs use runtime availability checks. The
CPU and Metal dependency builds use those tools. Open Image Denoise's
Metal device additionally needs Apple's offline `metal` and `metallib`
tools, supplied by Xcode or a separately installed Metal toolchain. Check
`xcrun --sdk macosx metal --version` in the build environment; the dependency
builder stops with setup instructions if the tools are missing. Cycles
compiles its own Metal kernels through the system's runtime compiler. Metal rendering still
needs a device and driver that can compile and execute those kernels;
see [rendering.md](rendering.md#macos). The UI tests need a user
logged in at the console (the unattended setup logs in `lume`) and a
screen of at least 2880 x 1800, or macOS shrinks their 1280 x 800 window.
Without a GPU, OpenGL runs on Apple's software renderer: enough for the
tests, but slow.

```bash
export MITCAD_MAC=<user>@<guest>
tools/dev-env/sync-to-mac.sh
ssh -i ~/.ssh/mitcad_mac $MITCAD_MAC "~/src/mitcad/tools/dev-env/setup-macos.sh"
ssh -i ~/.ssh/mitcad_mac $MITCAD_MAC "cd ~/src/mitcad && cmake --preset dev && cmake --build --preset dev && ctest --preset dev"
```

| Script | Use |
|---|---|
| `sync-to-mac.sh [user@host] [dir]` | Like `sync-to-vm.sh`: stages the archive, compares contents and replaces only changed files with the sync time; unchanged files keep their times, and deleted files disappear. Keeps `build/` and `CMakeUserPresets.json`; no `._*` files or extended attributes. Defaults: `MITCAD_MAC`, `MITCAD_MAC_DIR` (`~/src/mitcad`), `MITCAD_MAC_KEY` (`~/.ssh/mitcad_mac`). |
| `setup-macos.sh [--compiler-caches-only]` | CMake, Ninja, ccache (universal) and sccache (arm64) release archives into `~/.local` (not Homebrew), the pinned Rust toolchain, cargo-deny, vcpkg at `vcpkg.json`'s baseline, Qt via aqtinstall (`~/Qt/<version>/macos`), the render worker's libraries (`build-cycles-macos.sh`, once); writes `CMakeUserPresets.json` (`dev` = `macos-debug`). Downloads are checked against the SHA-256 pins in `versions.sh`; an empty pin stops it unless `MITCAD_SKIP_HASH=1`. The optional flag installs only the two compiler caches, leaving other dependencies, shell files and presets alone. |

`tools/dev-env/versions.sh` holds the pins `setup-user.sh` and
`setup-macos.sh` share; the Windows script mirrors them by hand. The
presets `macos-debug` and `macos-release` build for arm64 with deployment
target 14.4 (the Qt 6.12 minimum) and the overlay triplet
`triplets/arm64-osx-dynamic.cmake` (OCCT as dylibs, the same deployment
target for vcpkg's libraries).

The Darwin arm64 target of AppleClang 21.0.0 and Rust 1.99.0 (LLVM 23.1.1)
already selects `apple-m1`. The former's driver command and the latter's
target specification were inspected with a macOS 14.4 deployment target;
Rust also enables `dotprod`, `fp16` and `lse`. An absent explicit `-mcpu`
or `target-cpu` flag therefore does not imply generic AArch64 code in
this toolchain. OCCT and the renderer use the same AppleClang arm64
default. Adding the same flag would not change that CPU baseline and
does not justify rebuilding the dependency prefix. Never use `native`
for shipped binaries; recheck the compiler defaults when changing tools.

`MITCAD_MACOS_THIN_LTO` defaults to `ON` for arm64-only Mac builds and
affects their Release and RelWithDebInfo configurations. CMake IPO uses
ThinLTO for C++; explicit release-only compile/link options cover
Objective-C++, since CMake 3.31 does not emit IPO flags for its objects.
The option requires a successful C++ IPO link probe and both languages'
ThinLTO support; an unsupported compiler stops configuration instead of
silently dropping LTO. Set `MITCAD_MACOS_THIN_LTO=OFF` to disable it.
Debug, Rust profiles, Linux and Windows remain unchanged. Neither vcpkg
nor the Cycles/OIDN build inputs change, so the existing compatible
dependency prefix remains usable. Inspect generated compile commands,
including Objective-C++, when changing this configuration.

A controlled release comparison on the physical M1 Max with the toolchain
above used one synthetic 17-feature design and the CPU renderer's built-in
640 x 360, 32-sample scene. Five alternating CLI/render observations per
configuration gave these medians:

| Workload | Baseline | ThinLTO candidate |
|---|---:|---:|
| CLI recompute | 551.9 ms | 551.8 ms |
| CLI process wall time | 0.71 s | 0.71 s |
| CPU render, including denoising | 1.804 s | 1.758 s |
| Render-worker process wall time | 1.93 s | 1.89 s |

The candidate included an experimental Rust release profile with ThinLTO
and four codegen units. It did not improve this geometry-dominated CLI
fixture, so that Rust change was rejected. The render worker does not link
Rust: its 2.55% render-time improvement supports retaining C++/Objective-C++
ThinLTO independently. This is a narrow CPU result, not a general CAD or
Metal speedup. Repeated C++ and Objective-C++ compiler commands hit ccache.
The combined experimental rebuild took 518.85 s plus 11.17 s for the
explicit Objective-C++ correction; those include the discarded Rust
experiment and are not a clean build-time comparison of the retained setting.
The candidate geometry CTest passed in 194.27 s. Application and packaging
acceptance are separate checks; this measurement alone does not establish
them.

Final validation of the retained C++/Objective-C++ setting used a
warning-free native arm64 Qt Release-configuration application build with
the original Rust profile restored. Its render-worker SHA-256 stayed
identical to the measured candidate, confirming that the renderer gain
does not depend on the rejected Rust experiment. The geometry binary
also stayed identical to the one that passed the geometry CTest.
Application unit tests, CLI rendering, the 36-case IOSurface shader test,
SPDX checks and the complete Qt modelling/version-history workflow passed.
Required physical-Metal/OIDN preview, orbit, zoom, picking, resize and
shutdown passed without fallback or capability skips. Generated Debug
commands contain no LTO flags and no Cargo profile override; Qt remains
the default frontend. This scoped validation does not claim a new signed
release or package acceptance.

Compiler caches persist outside checkouts and build folders. ccache uses
`~/Library/Caches/ccache` by default (`ccache --show-config` shows any user
override); CMake enables its launcher for both C++ and Objective-C++,
including the macOS sources built with `-fobjc-arc`, unless that language
already has a launcher. `MITCAD_COMPILER_CACHE=OFF` disables these automatic
launchers. Linking, bundle assembly and hardened-runtime signing still run
normally; no cache executable is installed into the application bundle.

`MITCAD_RUST_COMPILER_CACHE` defaults to `ON` on macOS and `OFF` elsewhere.
When sccache is found, both Corrosion's Rust targets and the Cargo test-build
fixtures (debug and release, plus doctests) use it through `RUSTC_WRAPPER`,
with `CARGO_INCREMENTAL=0` and the cache in
`~/Library/Caches/Mozilla.sccache`. A missing tool is reported at configure;
a wrapper/server failure fails the build, even if the shell sets
`SCCACHE_IGNORE_SERVER_IO_ERROR=1`. A different explicit `RUSTC_WRAPPER`
requires disabling the option. Direct `cargo` commands retain the shell's
configuration. After changing a running sccache server's cache directory,
stop it once with `sccache --stop-server` before the next build.

Use `ccache --show-stats` and `sccache --show-stats` to inspect cache use.
sccache caches eligible library compilations; binaries, test harnesses,
procedural macros and other compilations that call the linker still run.
The pinned sccache's Rust keys also include the working directory and
Cargo's manifest paths: workspace crates in a different checkout may miss
even when registry dependencies hit. Its `SCCACHE_BASEDIRS` setting does
not normalize those Rust keys. Cache hits across checkouts therefore need
actual validation and are not guaranteed for every crate; see upstream's
[Rust limitations](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Rust.md)
and [Rust key implementation](https://github.com/mozilla/sccache/blob/v0.18.0/src/compiler/rust.rs).

`MITCAD_CARGO_TARGET_DIR` (a path; empty by default) keeps cargo's builds
outside the build folder, so that a new build folder reuses the Rust
crates built before while its C++ tree starts clean (release builds:
a fresh build folder, a persistent cargo folder). Empty, Corrosion builds
the library in `<build>/cargo` and the crates' test fixtures in
`<build>/core/cargo-test`. Set, they go to separate subfolders:

| Folder | What |
|---|---|
| `<dir>/lib` | the library build (Corrosion: `mitcad_ffi`, `mitcad-release`) |
| `<dir>/test-debug` | `core.cargo_build` and `core.doctests` |
| `<dir>/test-release` | `core.cargo_build_release` and `core.doctests_release` |

Corrosion always gives cargo a `--target-dir` inside the build folder, so
configure makes `<build>/cargo` a link to `<dir>/lib`: a symbolic link, or
a directory junction on Windows (no privilege needed). Delete such a build
folder with a tool that does not follow links (`rmdir /s /q` on Windows,
not Windows PowerShell 5.1's `Remove-Item -Recurse`, which descends into
junctions). Configure refuses a real `<build>/cargo` folder left by a build
without the variable, needs a single-configuration generator (the Ninja
presets), and removes the link again when the variable is cleared. The
lists of test binaries stay in `<build>/core/cargo-test`. Builds sharing
the folder wait for each other on cargo's lock. cargo's fingerprints and
Corrosion's folder name (`<dir>/lib/core_<hash>`, a hash of the manifest's
path) depend on the checkout's path: reuse needs the same source path, or
everything builds again beside the old builds. The test builds' folders
are swept after each build (below); the library's folder only grows, and
its owner trims it (e.g. deletes it when it passes a size limit or when
`rust-toolchain.toml` changes).

### The crates' test builds: sweep, jobs, debug information

cargo keeps every unit it has built in its target folder: each change of a
dependency's version, of the selected packages' features, of flags or
environment, of the toolchain or of the checkout's path builds the units
again under new hashes (`<name>-<hash>` in `deps/`, `.fingerprint/`,
`build/`) beside the old ones, and the test folder grew to tens of GB
(mitcad#796). Three things keep `<build>/core/cargo-test` (or
`<dir>/test-debug`, `<dir>/test-release`) near the size of one build:

- **Sweep.** After a successful `core.cargo_build` /
  `core.cargo_build_release`, `core/cargo-tests.cmake` reads the units the
  build named in its JSON messages (every `compiler-artifact`, built or
  fresh, and `build-script-executed`) and removes from that profile's
  folder (`debug/` or `release/` under `CARGO_TARGET_DIR`) what they do not
  name: `deps/` files and folders `<name>-<hash>.*` of other hashes,
  `.fingerprint/` and `build/` folders of other hashes, and the
  `incremental/` sessions of crates no longer built (and a crate's oldest
  sessions beyond its number of units). It logs the count and size by
  kind. Programs' files and fingerprints stay (cargo's messages name a
  program only by its copy outside `deps/`), as does anything written after
  the build started. Nothing the build used is removed, so the next build
  of the same sources compiles nothing. `MITCAD_CARGO_SWEEP=0` in the
  environment turns the sweep off; do that when several checkouts share
  one `MITCAD_CARGO_TARGET_DIR`, as each build would otherwise remove the
  others' units.
- **Jobs by memory.** Unless `CARGO_BUILD_JOBS` is set, the test builds
  run one cargo job per 2.5 GiB of the memory available when the build
  starts (at least one, at most the logical cores;
  `MITCAD_CARGO_JOB_MIB` changes the 2.5 GiB): a rustc job of these builds
  (a large crate, or a test binary's link) takes up to about 2.5 GiB, and
  a job per core ran a Windows VM of 10-19 GB out of memory once the
  crates were split. On Linux a memory limit of the build's cgroup
  (check-all's slice, mitcad-testrun's scopes) counts as well.
- **Fixed incremental setting.** `CARGO_INCREMENTAL` is part of the units'
  hashes, so one inherited from the shell built every MitCad crate a second
  time. The test builds and doctests set it themselves: on for debug, off
  for release (cargo's defaults), off for debug too with
  `-DMITCAD_CARGO_TEST_INCREMENTAL=OFF` (smaller folder, slower rebuilds of
  a changed crate) or with the Rust compiler cache. A `CARGO_INCREMENTAL`
  in the shell no longer affects them. Other variables that change the
  hashes, `RUSTFLAGS` and `CARGO_PROFILE_*` / `CARGO_BUILD_*` among them,
  are still taken from the environment: keep them the same between runs.
- **Debug information.** `[profile.test]` in `core/Cargo.toml` keeps line
  tables only (`debug = "line-tables-only"`): panics and backtraces still
  show files and lines, while the test binaries, their `.pdb` files and
  their links shrink. The application's builds keep full debug information
  for the crash reports. `CARGO_PROFILE_TEST_DEBUG=true` builds the tests
  with full debug information for a debugger (as a second build; the
  sweep removes it after the next normal build).

### macOS allocator experiments

The pinned OCCT 8.0.1 defaults to `USE_MMGR_TYPE=NATIVE`. In this build,
`Standard::Allocate` calls `calloc`, and `Standard::AllocateOptimal` calls
`malloc`; `MMGT_OPT` does **not** select an allocator. Its environment
variable is read only when OCCT is compiled with `USE_MMGR_TYPE=FLEXIBLE`.
That configuration maps `MMGT_OPT=0` to the system allocator, `1` to OCCT's
pool and `2` to the scalable allocator. The last option requires `USE_TBB`:
without it, OCCT's scalable-allocation calls fall back to the system
allocator. Inspect the actual build cache and
[`Standard.cxx`](https://github.com/Open-Cascade-SAS/OCCT/blob/V8_0_1/src/FoundationClasses/TKernel/Standard/Standard.cxx)
before comparing settings. A changed environment variable alone is not
evidence that the allocator changed.

Keep experimental libraries in a separate prefix. Verify the loaded
`TKernel` and `Standard::GetAllocatorType()` after the first OCCT allocation.
A process-wide malloc replacement is a different experiment: inspect
`malloc_zone_from_ptr`/`malloc_get_zone_name` on ordinary and OCCT allocations
as well as the loaded libraries. For mimalloc, also check heap membership
with `mi_is_in_heap_region`. Its upstream
[macOS override instructions](https://github.com/microsoft/mimalloc/blob/v3.5.3/readme.md#dynamic-override-on-macos)
use a shared library; loading it successfully alone does not prove that
allocations reached it.

Use a fixed optimized CLI and one synthetic design, for example the saved
`tools/cli/tests/perf_join_pattern.json` fixture. Run each candidate in a
fresh process with `info <design> --timings`, without a result store. Report
the feature recompute times separately from process wall time and peak
resident memory (`/usr/bin/time -l` on macOS reports the latter in bytes).
The CLI's `--timings` report does not measure viewport meshing or display.
`MITCAD_TEST_ONLY` selects `guarded` geometry tests by a name substring;
`test_geometry --part <name>` runs one part of the suite (`--list` names
them; each is the ctest `geometry.suite.<name>`, mitcad#832). The long
thread and sweep groups are numbered parts of about equal time
(`thread.1`-`thread.3`, `sweeps.1`-`sweeps.5`, mitcad#870); a test of a
second or more prints its time, for moving a test between them.
Legacy groups that call their cases directly still run, so inspect the
test entry point before treating a filtered invocation as one isolated
case. Keep workload, compiler flags, thread
limits and successful assertions equal across candidates. No allocator is
adopted merely from these setup checks; timings, memory and allocation
failure handling decide whether a replacement is worth shipping.

The bounded comparison on 24 October 2026 used a physical M1 Max
(10 CPU cores, 32 GiB), macOS 27.0.1 (26A434), AppleClang 21.0.0 and
the optimized 0.26.0 CLI/geometry binaries at `378aec0c`. Each row is the
median of five fresh processes; the candidate order rotated between
repetitions and no build ran concurrently. The design was the synthetic
90-pin join fixture above. The geometry workload was `test_geometry` with
`MITCAD_TEST_ONLY=test_the_check_where_changed_gives_the_whole_checks_answer`:
the perforated-plate case from `test_large_bodies.cpp` plus the legacy
groups that do not use `guarded` (996 input-checked operations in total).
Its time and RSS include those groups, not just the plate. RSS is the median of
each process's maximum resident size, in MiB.

| Allocator | Recompute ms | CLI wall s | CLI RSS MiB | Geometry wall s | Geometry RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Shipped native/system | 547.9 | 0.71 | 75.38 | 7.02 | 185.48 |
| Native + mimalloc 3.5.3 override | 526.0 | 0.70 | 77.98 | 6.74 | 190.34 |
| FLEXIBLE, system (`0`) | 562.0 | 0.73 | 77.41 | 7.07 | 185.41 |
| FLEXIBLE, OCCT pool (`1`) | 855.9 | 1.04 | 91.47 | 11.04 | 217.27 |
| FLEXIBLE, TBB (`2`) | 553.2 | 0.72 | 79.84 | 7.06 | 182.59 |

The separate FLEXIBLE TKernel used the same patched OCCT 8.0.1 source,
arm64, deployment target 14.4 and `-O3`, with TBB 2023.1.0 enabled.
Only that uninstalled TKernel was substituted; the three FLEXIBLE rows
share the identical binary. Enabling TBB also changes OCCT's parallel
runtime, so comparisons with the shipped non-TBB binary are not solely
allocator comparisons. Loaded-library and allocator-type probes verified
all three selections. The mimalloc override was pinned to the existing
vcpkg baseline and its source hash; probes of ordinary, aligned and OCCT
allocations verified the mimalloc zone and heap membership in the actual
CLI process. Diagnostic probes were absent from the timed runs.

The pinned TBB malloc proxy loaded and registered a `tbbmalloc` zone on
this OS, but ordinary and OCCT allocations still used `DefaultMallocZone`.
That ineffective process-wide override is excluded from the table; it is
different from the verified FLEXIBLE/TBB row.

Decision: retain the system allocator. The pool is slower and larger;
TBB brings no speed gain here. Mimalloc improves feature recompute and
the geometry case by about 4%, but CLI wall time changes by only 0.01 s
and median RSS increases in both workloads. The geometry maximum RSS
across the five runs rises from 187.95 to 200.28 MiB. This narrow benefit
does not justify adding a process-wide shipped allocator and its
packaging/failure-handling obligations. All 50 timed workload invocations
completed successfully. No dependency, allocator, out-of-memory policy
or package signature was changed. These are synthetic workload results,
not a corpus, viewport-meshing or rendering speedup claim.

## macOS: bundle and native UI

- Configure `cmake --preset dev -DMITCAD_MACOS_NATIVE_UI=ON` to build
  the AppKit frontend. It starts by default in that build; `--ui=qt`
  runs the retained frontend and `--ui=native` explicitly selects AppKit.
  The native sources are confined to `app/platform/macos/` and compiled
  with ARC. Shared command/session changes also build without this option.
- `app.ui-macos-native` drives real native menu items, fields, buttons,
  history and command search in process, verifies model volumes and saves
  a composed screenshot. The older `app.ui-macos` and
  `app.ui-macos-workflow` explicitly use `--ui=qt` to cover retained
  workflows. Run GUI tests sequentially in the isolated VM. See
  [native-macos-ui.md](native-macos-ui.md) for rollout and manual checks.
- The app is a `MACOSX_BUNDLE` (`mitcad.app`) with
  `app/packaging/macos/Info.plist.in`. `MITCAD_BUNDLE_ID` (default
  `fi.nocodo.mitcad`) is a placeholder: fix it before the first release,
  since preferences and signatures are keyed on it. `Mitcad.icns` is built
  from `app/packaging/icon/mitcad.svg` by `mitcad-iconset` and `iconutil`.
- `cmake --install <build> --prefix <dir>` deploys Qt with `macdeployqt`,
  copies the vcpkg dylibs into `Contents/Frameworks` and the licence texts
  into `Contents/Resources/licenses`.
  It also installs the render worker `mitcad-render` (every build has it)
  in `Contents/MacOS`, its dependencies and Open Image Denoise's
  runtime-loaded CPU/BNNS and Metal modules in `Contents/Frameworks`, and their licence
  texts. The complete Cycles Metal runtime source tree is installed in
  `Contents/Resources/cycles/source`. `setup-macos.sh` prepares the pinned
  CPU and Metal rendering and denoising dependencies using
  `tools/dev-env/build-cycles-macos.sh` in the guest;
  see [rendering.md](rendering.md#macos).
- `tools/package-macos.sh [build dir]` installs, signs ad hoc, checks and
  makes a `.dmg`. Developer ID signing (`MITCAD_SIGN_IDENTITY`) and
  notarisation (`MITCAD_NOTARY_PROFILE`) need an Apple Developer account.
- `tools/check-bundle-macos.sh <Mitcad.app>`: only system and bundled
  libraries linked, relative rpaths, arm64 everywhere, and `Info.plist`,
  icon, licence texts, the complete Metal source manifest and the
  `libqmacstyle` style plug-in present
  (without it Qt falls back to its own cross-platform style).
- `app/platform/macos/*.mm` is Objective-C++ with ARC, linked with AppKit,
  as warning-free as C++ (`mitcad_warnings`); AppKit calls newer than 14.4
  are guarded with `@available`.
- `--chrome=docked|floating` and `--no-glass` try both layouts; the log's
  start-up lines say which was chosen. In the VM, look with the test
  driver (`MITCAD_TEST_INPUT`, `TestDriver.hpp`): `screenshot` and
  `screenshot-dialog` grab widgets without the card windows or glass;
  `screenshot-screen` saves the app's own windows as the window server
  shows them (no Screen Recording permission; software GL may be blank
  there, so glass is checked on hardware:
  [checklist](macos-trackpad-checklist.md)). `screencapture` and
  `osascript` do not work over SSH. Check both appearances
  (`defaults write -g AppleInterfaceStyle Dark`, `defaults delete -g
  AppleInterfaceStyle`) and restore light.

## Tools downloaded from releases

Build tools that are neither Ubuntu nor winget packages are downloaded
from a pinned GitHub release and checked against GitHub's published
SHA-256 digest. None is linked into MitCad; only the AppImage runtime is
shipped.

| Tool | Script, folder | Release asset | SHA-256 |
|---|---|---|---|
| ccache 4.14.1 for Windows | `setup-ccache-windows.ps1`, `C:\dev\ccache` | https://github.com/ccache/ccache/releases/download/v4.14.1/ccache-4.14.1-windows-x86_64.zip | `6219f3865ca59aec41ee4b678df171d5d35855ecb2b6dbbbd20690b3a68af7b4` |
| ccache 4.14.2 for macOS (universal) | `setup-macos.sh`, `~/.local/bin` | https://github.com/ccache/ccache/releases/download/v4.14.2/ccache-4.14.2-darwin.tar.gz | `cf240009630b605b775c7c5c7b7105a3b196c79f31af322c1557969aa28003fc` |
| sccache 0.18.0 for Apple Silicon | `setup-macos.sh`, `~/.local/bin` | https://github.com/mozilla/sccache/releases/download/v0.18.0/sccache-v0.18.0-aarch64-apple-darwin.tar.gz | `308184519b646f5125289e8515b36f6ca65a13a041923994aebe702348674e8e` |
| linuxdeploy 1-alpha-20251107-1 | `install-appimage-tools.sh`, `~/appimage-tools` | https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage | `c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d` |
| linuxdeploy-plugin-qt, continuous build of 22 August 2026 | `install-appimage-tools.sh`, `~/appimage-tools` | https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage | `cfc1055b2b9dbc08412b579f20990b7b41a17b61beaa5847dc9477c96c9e9617` |
| appimagetool 1.9.1 | `install-appimage-tools.sh`, `~/appimage-tools` | https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage | `ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0` |
| ISPC 1.31.0 (compiles Open Image Denoise) | `build-cycles.sh`, `<prefix>/src` | https://github.com/ispc/ispc/releases/download/v1.31.0/ispc-v1.31.0-linux.tar.gz | `d74089c835e10fd7e2c4b9225ced38b87d1fb53d35c7ceabd48cdf035da11b11` |
| ISPC 1.31.0 for Apple Silicon (compiles Open Image Denoise) | `build-cycles-macos.sh`, `<prefix>/src` | https://github.com/ispc/ispc/releases/download/v1.31.0/ispc-v1.31.0-macOS.arm64.tar.gz | `eac8009da38d41074d0adcf1fad4a3412fc9644a81ee5a49efeb07eac505b6ec` |

`build-cycles.sh --cuda` (mitcad#50; `setup-user.sh` passes it unless
`MITCAD_RENDER_CUDA=0`) fetches the CUDA compiler's
components (`cuda_nvcc` 12.9.86, `cuda_cudart` 12.9.79, `cuda_cccl`
12.9.27 of CUDA 12.9.1) from
https://developer.download.nvidia.com/compute/cuda/redist/ and checks
them against the SHA-256 digests NVIDIA lists in `redistrib_12.9.1.json`
there (pins in `versions.sh`); when no GCC of version 14 or older is
installed, it unpacks the distribution's `g++-14` packages
(`apt-get download`). `build-cycles.ps1 -Cuda` (mitcad#202;
`setup-windows.ps1` likewise) fetches the same components' Windows
archives (`CUDA_*_WINDOWS_SHA256`, from the same JSON) and uses MSVC as
the host compiler. All are build tools only; see
[rendering.md](rendering.md#building).

QtKeychain (BSD-3-Clause; the live updates' credentials in the system's
keychain, mitcad#89) is built by CMake on every platform from its release's
source archive (`cmake/Keychain.cmake`, FetchContent as for Corrosion),
pinned in `versions.sh` (`QTKEYCHAIN_VERSION` 0.17.0, `QTKEYCHAIN_SHA256` of
https://github.com/frankosterfeld/qtkeychain/archive/refs/tags/0.17.0.tar.gz),
as a static library against the build's Qt: the packages carry no library
of its own, only its licence text. On Linux it needs Qt's D-Bus module and
libsecret's headers (`libsecret-1-dev`); libsecret itself is loaded at run
time when the system has it, so MitCad starts without it.

`build-cycles.sh` also builds the render worker's libraries from pinned
sources (`versions.sh`): Open Image Denoise 2.5.1's source release
(SHA-256 `e71fd043…c36`) and Cycles at tag v5.2.0 (commit `3b97e190…`);
see [rendering.md](rendering.md#building). Those are linked; ISPC is not.
On Windows `build-cycles.ps1` does the same with MSVC and reads the pins
of `versions.sh`; it takes ISPC's Windows release (SHA-256
`9a187938…140a`, `ISPC_WINDOWS_SHA256`); see
[rendering.md](rendering.md#windows).

linuxdeploy-plugin-qt publishes digests only for its continuous build, so
the check fails whenever it is rebuilt; update the digest in the script.
The tools are extracted (`--appimage-extract`, no FUSE). The AppImage
runtime is taken from the start of appimagetool's own AppImage (with its
update-information and signature sections cleared), so packing downloads
nothing.

## Windows installer

Built in the VM from a release build with CPack
([cmake/Packaging.cmake](../cmake/Packaging.cmake)):

```bat
tools\dev-env\msvc.cmd cmake --build --preset dev --target package
powershell -ExecutionPolicy Bypass -File tools\installer-windows-test.ps1 -MesaDir C:\dev\mesa\bin
```

`package` writes `mitcad-<version>-windows-x64.exe` (NSIS 3) and the same
files as a `.zip`. The installer script is our own,
[packaging/windows/NSIS.template.in](../packaging/windows/NSIS.template.in)
(CPack finds it through `CPACK_MODULE_PATH`). It runs as the user
(`RequestExecutionLevel user`) and installs either

- **just for me** (the default): into `%LOCALAPPDATA%\Programs\Mitcad`,
  with the Start menu shortcut, the `.mitcad` file type
  ([filetype.nsh](../packaging/windows/filetype.nsh)) and the uninstall
  entry under `HKCU`; no administrator rights, so no UAC prompt;
- **for all users:** into `Program Files\Mitcad`, registered under `HKLM`
  (32-bit view, as earlier releases). Only this choice asks for elevation:
  the installer starts itself again elevated (`runas`) with `/AllUsers
  /Elevated`.

The page *Choose Users* after the licence offers the choice; it starts on
*For all users* only when MitCad is installed for all users and not for
the current user.
An earlier version in the same scope is uninstalled first (its own
uninstaller, silently, in place). Silent installs: `/S` installs just for
the current user, `/S /AllUsers` for all users (`/CurrentUser` says the
default explicitly), and `/D=<dir>` (last, without quotes) sets the folder
in both. A silent install whose `/D` is the folder of the installation
for all users, which is what the updater of an earlier release runs,
stays for all users. The installer writes the scope into
`<dir>\installation.ini` (`[Installation]`, `Scope=user` or `all`), which
the uninstaller and the [automatic updates](updates.md) read; an
installation without it is an earlier release's, which was for all users.
The uninstaller removes the files and the keys of its own scope only;
for all users it elevates as the installer does. Exit codes: 0 done, 1
cancelled, 2 aborted (also when the earlier version could not be
uninstalled), 740 elevated but still without administrator rights, 1223
elevation refused.

Contents: `bin` (executables, Qt, vcpkg DLLs,
VC++ runtime, `qt.conf`), `plugins` (via `windeployqt`), licences
(`LICENSE`, `THIRD-PARTY-NOTICES.txt`, `licenses\` with the font and every
vcpkg port's `copyright`), uninstaller. Never included: Mesa, Qt's
software OpenGL, PDBs. The render worker
`mitcad-render.exe` is installed with its DLLs (Open Image Denoise's too, its
CUDA device with `-Cuda`),
its CUDA kernels in `bin\cycles\lib` (a render prefix built with `-Cuda`)
and the licences of Cycles (with cuew's) and Open Image Denoise
([rendering.md](rendering.md#windows)).

`tools/installer-windows-test.ps1` (ctest `app.installer-windows` once
the installer exists) checks that the installer asks for no elevation,
then in each scope (`-Scope both|user|all`) installs silently into a
folder of the test's own and checks the scope's registration (uninstall
entry, `installation.ini`, file type, Start menu shortcut, nothing in the
other scope). Just for the current user it checks the files, runs
`mitcad-cli`, `mitcad.exe --version` and the render worker (a test render,
`--list-devices`; the build's GPU kernels installed) with a system-only `PATH`,
checks that the installed `mitcad-updater.exe` does not run an installer
whose SHA-256 is not the given `--sha256`, or without one (mitcad#603), and,
with `-MesaDir`, renders a screenshot; for all users it reinstalls with
only `/D=` as an earlier release's updater does. With `-MesaDir` it tests
the automatic update in both scopes ([updates.md](updates.md#tests);
`-NoUpdate` skips it) in a build configured with
`-DMITCAD_UPDATE_TEST_HOOKS=ON`; a build without it (the `dev` preset that
inherits `windows-release`) is checked to ignore the update tests'
variables ([updates.md](updates.md#the-test-hooks)). Then it uninstalls and checks that no key of that
scope is left. It changes the `.mitcad` file type under `HKCU` and `HKLM`
and replaces an installation of either scope: run it only in the VM.
Session 0 cannot show a UAC prompt, so the test runs with administrator
rights and the elevation itself is not exercised.

Signing a release: [updates.md](updates.md#signing-a-release).

Icons: edit `app/branding/mitcad.svg` (64 px and up) and
`mitcad-small.svg` (below 64 px), then run `tools/make-icons.sh` in the
distro; the PNGs and `mitcad.ico` it writes are committed.

Not done yet: Authenticode signing (SmartScreen warns) and bundling the
licence texts of compiled-in Rust crates.

## Linux AppImage

`Mitcad-<version>-x86_64.AppImage`, built in the distro:

```bash
tools/dev-env/install-appimage-tools.sh            # once
cmake --build --preset dev --target appimage
```

The target ([cmake/AppImage.cmake](../cmake/AppImage.cmake),
[packaging/linux/make-appimage.sh](../packaging/linux/make-appimage.sh))
installs into `build/dev/appimage/AppDir` (executables, desktop file,
icons, licences incl. `packaging/linux/THIRD-PARTY-NOTICES.txt`), then:

- linuxdeploy copies needed libraries into `usr/lib` and gives every
  executable in `usr/bin` and every library an `$ORIGIN` RUNPATH, so
  none of them loads a system's Qt; its Qt plugin adds the X11/GLX platform, image formats, SVG
  icons and TLS backends. glibc, libstdc++, X11/xcb, OpenGL, fontconfig
  and FreeType stay the system's.
- OpenSSL comes as a pair of one build (mitcad#954): the render worker
  links vcpkg's `libcrypto.so.3` (minizip-ng's), so `libssl.so.3` from
  the same folder goes in too, and Qt's OpenSSL TLS backend, which loads
  both by name at run time, finds them in `usr/lib` (libQt6Core's
  `$ORIGIN` RUNPATH) before the system's. With libcrypto alone it took the
  system's libssl on top of the bundled libcrypto, two OpenSSL builds in
  one process, and on some distributions no TLS at all. The bundled
  libcrypto's `ENGINESDIR` and `MODULESDIR`, vcpkg's install folder on the
  build machine (they follow the install prefix), are rewritten to
  `/nonexistent/…`, so no other OpenSSL's modules load into it;
  `OPENSSLDIR` stays `/etc/ssl` (the system's `openssl.cnf`), and Qt reads
  the system's CA certificates itself (Fedora's
  `/etc/pki/tls/certs/ca-bundle.crt` among them). The header of
  `make-appimage.sh` gives the reasons.
- The render worker `mitcad-render` goes in
  with its libraries (and Open Image Denoise's CPU device module) and
  their licences ([rendering.md](rendering.md#appimage)).
- ELF files are stripped.
- appimagetool packs it with the pinned runtime. No AppImage update
  information and no GPG signature: MitCad's own manifest signature is
  what counts.

Release AppImages come from a `linux-release`-based preset, built on
**Ubuntu 24.04** (a 24.04 distro, VM or container: glibc 2.39), the
oldest supported system. The AppImage needs the glibc of the system it
was built on, so a release built on a newer distribution does not start
on 24.04 or the distributions based on it (mitcad#789); a newer Linux
host builds it in an Ubuntu 24.04 build root
([Linux on a newer host](#linux-on-a-newer-host)). Without FUSE run
it with `--appimage-extract-and-run`.

At each start the AppImage writes its desktop file (`Exec` = the
AppImage, `StartupWMClass=Mitcad`) and icons into `$XDG_DATA_HOME` if they
differ, so docks show the icon
([app/framework/DesktopEntry.cpp](../app/framework/DesktopEntry.cpp)). No
GTK is bundled, so it uses the `xdgdesktopportal` platform theme unless
`QT_QPA_PLATFORMTHEME` is set.

`tools/appimage-test.sh` (ctest `app.appimage` once the AppImage exists)
checks that every ELF resolves its libraries inside the AppImage or from
system folders (an ELF on which `ldd` fails fails the test), then two
static checks of seconds: no ELF needs a `GLIBC_`, `GLIBCXX_` or
`CXXABI_` symbol version newer than `GLIBC_FLOOR` (2.39),
`GLIBCXX_FLOOR` (3.4.33) and `CXXABI_FLOOR` (1.3.15): Ubuntu 24.04's
glibc and the libstdc++ of its `libstdc++6` package (GCC 14's), and every ELF that links Qt finds each `libQt6*` inside the AppImage,
through its own `$ORIGIN` RUNPATH and in `ldd`'s view without
`LD_LIBRARY_PATH`, never the system's Qt; it names the ELF that fails.
`libssl.so.3` and `libcrypto.so.3` must be bundled both or neither, and
no file may name a library folder of the build machine (vcpkg's install
trees, the render libraries' prefix, the built executables' RUNPATHs).
Then it runs the AppImage with an empty environment, renders with its
render worker when it has one, and updates it to itself from a locally
signed manifest, over the bundled OpenSSL: the log line
`Update check: TLS OpenSSL <version> …` names the bundled libcrypto's
version, and the running MitCad has loaded libssl and libcrypto from the
AppImage only. That last step needs a build with the update test hooks
(`-DMITCAD_UPDATE_TEST_HOOKS=ON`, on in the debug presets); a release's
AppImage, without them, is checked to ignore the update tests' variables
instead ([updates.md](updates.md#the-test-hooks)).

## Tests

**Tolerance:** every geometric comparison in a test (lengths, positions,
distances, areas and volumes by their length scale) allows at least
0.000 001 mm (1e-6) unless the test says why it needs a tighter one. A
deviation within that, on any platform, is fixed by relaxing the test to
it; a larger one is a fault to fix or a case to agree on, not a reason to
relax silently. UI tests are not relaxed for one platform: the interface
differs by platform, and a failure there is a fault.

Run in the isolated environments:

- `ctest --preset dev`: geometry, bridge, model, solver, import,
  `mitcad-cli` scripts (`tools/cli/tests`), SPDX headers, dependency
  licences. `core.vcs.*` and `cli.remote` use a bare repository in a temp
  folder and a fake `ssh`; no network; skipped without git. `cli.library`
  makes a small parts library of fasteners with
  `tools/libraries/make-fastener-library.py` in the build tree, then one
  of every size with the smallest and largest rows inserted (needs git
  and Python 3; skipped without them); `cli.fastener-tables` checks that
  generator's dimension tables (sizes, pitches, length series, each
  standard's rows, the earlier versions' rows kept; Python 3 only);
  `cli.profile-library` likewise one
  of BUILD parts with `tools/libraries/make-profile-library.py` from
  sections it makes; `cli.library-previews` renders two fasteners'
  previews with both generators and the render worker (skipped where it
  cannot render).
- The crates' tests (`core.model`, `core.vcs.*`, ...): the fixtures
  `core.cargo_build` and `core.cargo_build_release` build every crate's
  test binaries in one cargo run (`cargo test --no-run`, one lock on the
  target directory, the crates compiled in parallel), and the tests run
  the binaries directly, at the same time (`core/cargo-tests.cmake`;
  on Windows each run has `TMP` and `TEMP` in a folder of its own under
  `%LOCALAPPDATA%\mitcad-tests`, removed when it passes, as Storage Sense
  deletes the empty folders of git repositories just made in the
  system's temporary folder);
  `core.doctests` and `core.doctests_release` run the documentation tests.
  The fixtures limit cargo's jobs by the memory available and sweep the
  stale units from their target folder after the build (see
  [the crates' test builds](#the-crates-test-builds-sweep-jobs-debug-information)).
  A large crate's tests run in parts by module
  (`mitcad_cargo_test(... PARTS <part> <filter>...)` in
  `core/CMakeLists.txt`): `core.vcs.remote`, `.sync`, `.library` and
  `.rest` (everything else, so a new module is tested before it gets a
  part), and `core.vcs.parts` checks with the binaries' `--list` that the
  parts hold each test exactly once.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` in
  `core/`.
- `tools/check-dependencies.sh`: `cargo deny` with `core/deny.toml`,
  including RustSec advisories, and the licence texts of the native
  libraries the build fetches from source and links (QtKeychain's
  BSD-3-Clause, in `build/dev/_deps`; `MITCAD_BUILD_DIR` for another
  build).
- Performance: `UI_APP=build/rel/app/mitcad tools/perf-measure.sh
  design.mitcad d3 21 20` on a release build;
  `mitcad-cli info design.mitcad --timings [--result-store <dir>
  --diagnostics]`. `cli.perf_recompute`, `cli.perf_join_recompute` and
  `cli.perf_holes_recompute` guard against large slowdowns, and
  `cli.perf_holes_mesh` the display meshing of planes with many holes
  (`mitcad-cli mesh-compare` with each face's time in `--report`).
  `mitcad-cli run script.json --timings` prints each step's time
  ("Timing steps[i] <command>: <ms> ms"); `cli.perf_sketch_pattern`
  runs it on a sketch with a 10 x 10 circle pattern
  (`tools/cli/tests/perf_sketch_pattern_{5,10,20}.json`, mitcad#670), and
  `UI_APP=build/rel/app/mitcad UI_PATTERN_SIZE=10
  tools/ui-sketch-pattern-speed-test.sh` times a window selection and
  drags of that sketch in the application (`UI_SCALE=2` for a device
  pixel ratio of 2) and checks that each drag step reads the sketch from
  the model once (the "sketch query" lines, mitcad#679). The regions of
  400 disjoint circles are guarded in the model's tests (`sketch::regions_tests`, mitcad#678); `cargo test
  --release -p mitcad-model regions_timings -- --ignored --nocapture`
  prints the times of 25, 100 and 400 circles.
  `mitcad-cli info design.mitcad --names` prints the names of every
  body's faces and edges, to compare the topology two builds (or one
  build with and without `MITCAD_NO_FAR_FEATURES=1`) give a design.
- Display meshing (mitcad#644): `mitcad-cli mesh-compare design
  --report report.json [--a variant] [--b variant]` meshes every body of
  a design (opened, or imported as the application imports it) twice as
  the view's final tier does, each time with one variant of the planes'
  triangulation (`default`: the default insertion, the final tier before
  mitcad#644; `delabella`: Delabella unchecked, the interactive tier;
  `checked`: Delabella checked, the final tier now; (a) `default` and (b)
  `delabella` unless given), and compares them face by face
  (triangles, deviation, area against the face's own, folds, faces left
  without a triangulation, time); `mitcad-cli mesh-compare --summary
  <folder>...` totals the reports of many designs
  ([performance.md](performance.md#display-meshing-planes-through-delabella-mitcad644)).
- `tools/repo-growth.sh <work dir> design.mitcad...` measures git
  repository growth of the design file formats over 50 saves.
- Live updates (mitcad#89): `core.vcs` runs the MQTT client's scenarios
  against a broker inside the test and against mosquitto when it is
  installed (`install-packages.sh`; `setup-macos.sh` with Homebrew,
  `setup-windows.ps1` with winget; skipped without it), each test with its
  own broker on free ports; the TLS test makes its certificate authority
  with `openssl` (skipped without it). Fuzz targets of the packet decoder
  and the message readers (cargo-fuzz, nightly Rust; by hand, not in the
  check script): in `core/vcs/fuzz`, `cargo +nightly fuzz run
  mqtt_packets` and `mqtt_messages` (its `Cargo.toml` has the steps); the
  normal tests run a seeded random sample of the same input. The
  application's side is `tools/ui-live-test.sh` (skipped without
  mosquitto): the trust question, Live and Live offline, two instances
  (who has a design open, a version arriving at once), Preferences'
  switch and Forget, signing in to a TLS broker of the test's own
  (`openssl`, `MITCAD_TEST_LIVE_CA`) without a keychain and, when GNOME's
  keyring and `dbus-daemon` are installed, with one in a D-Bus session of
  the test's own.

### The test runner (mitcad-testrun)

`mitcad-testrun` (`tools/testrun`, mitcad#790; built with the project
beside its other programs: `build\dev\mitcad-testrun.exe` on Windows,
`build/dev/core/mitcad-testrun` on Linux and macOS) runs a build tree's ctests, each as
its own `ctest -R ^name$`, measures each (wall and CPU time, peak memory of
the process tree, the kind of result) and keeps the machine's profiles and
failure scores in `~/.mitcad/test-profiles.json` (`MITCAD_TEST_PROFILES`,
`--profiles`), outside the build trees and mirrors so that they survive
fresh builds. On Linux `tools/check-all.sh` runs its ctests and its corpus
group through it ([the check script](#the-check-script)); macOS merge and
release checks use `tools/test-macos.sh`. The Windows release script takes
it into use on its machine.

- `--measure` (the default): a fixed number of tests at once (`-j`, by
  default the cores / 2), recording only.
- `--schedule`: a memory budget (85 % of the memory) as the hard limit
  with a floor of real available memory (1.5 GiB), a CPU gate (no start
  above 90 % load), longest first with complementary pairing, a priority
  lane for the tests with the highest failure scores, timing-sensitive
  tests only on a calm machine, a hard memory limit per test of at least
  4 GiB and twice its history's peak (none for RUN_SERIAL tests or a
  widely spread history; one rerun alone without a limit after it), a
  disk floor. Tests
  with no history borrow the median of their fixture or label siblings,
  else a guess by kind (UI 1 GiB, corpus 2 GiB, others 512 MiB), and
  while most tests have no history at most the cores / 2 run at once.
- `--changed FILE`: the changed files, a path a line as `git diff
  --name-only <base>` prints them; the tests related to them (the files
  and folders their commands name, the areas their scripts declare in
  `# check-all sources:` lines, a changed crate's tests, a test program's
  own sources) join the priority lane with a failure's raised place.
- **UI entries at once** (the ctests `app.ui-*` or labelled `ui`) are
  learnt from the same runs, without a measuring run: each UI entry's run
  records how many other UI entries ran beside it. A failure beside others
  that passes alone is a contention failure (+2 to the failure score, not
  +10): it halves that test's room (the UI entries it may have beside it);
  a run in which two or more UI entries fail so lowers the machine's cap
  (per platform, in the profile file) by one (mitcad#872: one test that
  is sensitive to load holds back only itself); two clean runs that
  reached the cap raise it by one. A machine
  opts in by giving the bound, `--ui-max` or `MITCAD_UI_JOBS`: then the
  scheduler holds UI entries to the cap (which starts at the bound
  and never goes above it) and to their rooms, and reruns a UI entry that
  failed beside others alone after them, so `MITCAD_UI_JOBS` is the
  ceiling, no longer the number at once. Without a bound the runs only
  record and learn (within 4); UI entries are scheduled as before. For
  the Linux UI tests that `tools/check-all.sh` runs itself,
  `mitcad-testrun --ui-cap` prints the cap to run at, `--ui-rooms` the
  tests' rooms, which it keeps as the scheduler does, and `--record-ui
  FILE` takes the runs back (name, exit status, seconds, UI entries beside
  it).
- `--dry-run` lists the tests in their starting order with predictions.

The budget's basis is `MITCAD_TEST_MEMORY` when set, else the larger of
the physical memory (on Linux the lowest memory limit on the runner's
cgroup path where lower) and, on Windows, the commit limit. **A VM with
dynamic memory** (the Hyper-V Windows VM) reports only its current
assignment as physical memory: run it with `--memory-budget 24G` (a size
is used as given) or `MITCAD_TEST_MEMORY` set to the VM's maximum memory.
**On bare metal** `--memory-budget free` (mitcad#872; `tools/check-all.sh`
gives it there) follows the free memory instead: a test starts while the
memory available exceeds a reserve (`--memory-reserve`, default the larger
of 4G and a tenth of the memory) by its prediction and the running tests'
growth still to come, read again before each start, at most 85 % of the
basis above; the end report gives the budget's range over the run.

macOS merge checks and the release gate use `tools/test-macos.sh`:

```sh
tools/test-macos.sh --build build/macos-release -R '^(app.unit|app.ui-macos)$'
# First take-up: profile the checks already needed, at serial parallelism.
tools/test-macos.sh --build build/macos-release --measure -R '^tools.testrun$'
```

The default is `--schedule`, at most four tests, an 85% memory budget and
90% CPU gate. Aqua UI tests (`app.ui-*`) are timing-sensitive from the
start: at most one, below 70% load. `--measure` defaults to one test at a
time. Selections and other runner arguments pass through; there is no
second test list. The machine profile file remains outside the build
(default `~/.mitcad/test-profiles.json`). macOS records rusage CPU time
and the largest process's peak RSS, without a hard per-test memory limit;
it does not claim a simultaneous process-tree memory peak.

Windows (the VM, from the build's folder, `ctest` on `PATH` through
`tools\dev-env\msvc.cmd`): a Job Object per test.
`tools\dev-env\msvc.cmd build\dev\mitcad-testrun.exe --build build\dev
--schedule`. Linux: a systemd user scope per test where the user's systemd
runs (cgroup v2: the working set, `memory.max`), else rusage, the scopes in
the slice the runner itself runs in and the budget a share of the lowest
memory limit on its cgroup path;
`build/dev/core/mitcad-testrun --build build/dev --schedule`. macOS:
rusage. Failures are printed at once and appended to
`<build>/testrun/status.txt`; the end report (`report.txt`: totals,
resource use, the critical path, failures by kind, profiles that changed
much) and `results.tsv` go to the same folder. Details, the profile
file's format and what is left: [tools/testrun/README.md](../tools/testrun/README.md).

### Environment variables and options for tests

| Variable | Effect |
|---|---|
| `MITCAD_LOG_PICKS=1`, `MITCAD_LOG_VOLUMES=1`, `MITCAD_LOG_TIMING=1` | Log pick positions, body volumes after changes, step durations. |
| `MITCAD_MESH_TRACE=1` | Log the view's display meshings (faces kept, reused, meshed, BRepMesh's phases, the slowest faces, planes the final tier triangulated again after Delabella, mitcad#644) and, on standard error, any meshing a presentation does on the UI thread (mitcad#635; also `test_preview_meshing --bench`). |
| `MITCAD_SETTINGS_DIR` | Settings as an INI file in this folder. |
| `MITCAD_AUTOSAVE_DIR`, `MITCAD_TEST_AUTOSAVE_SECONDS` | Recovery folder and autosave interval. |
| `MITCAD_TEST_RECOMPUTE_DELAY_MS=n[,job=m...]` | Every feature computed in a job takes n ms longer (`400,undo=800`), to exercise the progress dialog. |
| `MITCAD_COMPUTE_INLINE=1` | Jobs on the UI thread, for a debugger. |
| `MITCAD_TEST_OCCT_CRASH=<operation>` | That kernel operation (e.g. `fillet`, or Section Analysis' `clip` and `section`) crashes like an access violation in OCCT; also in `mitcad-cli`. |
| `MITCAD_TEST_OCCT_OUT_OF_MEMORY=<operation>` | An allocation in that kernel operation (e.g. `extrude`) fails with `std::bad_alloc`: the operation fails "out of memory" (mitcad#80). |
| `MITCAD_TEST_OCCT_ALLOCATION_FAILS=<operation>` | An allocation of OCCT's collections fails in that kernel operation (e.g. `extrude`) or in the healing of a body built from B-rep data (`heal`), and is caught there as OCCT's checker catches its failures: only OCCT's count of failed allocations tells, and the operation or the body's build fails "out of memory" (mitcad#132). |
| `MITCAD_DRAWING_TRACE=1` | Print a line per drawing view projected, sheet drawn and view added on standard error, with the phases of the hidden-line removal (`compare` and `parts` also time the other algorithm and each body alone; [performance](performance.md#drawing-views-mitcad962), mitcad#962). |
| `MITCAD_CHECK_INPUTS=1` | Report operations that modify their input shapes ([architecture](architecture.md#geometry-kernel)). |
| `MITCAD_NO_FAR_FEATURES=1` | Booleans on perforated bodies work on the whole body and cut a pattern's copies at once ([architecture](architecture.md#geometry-kernel)), for comparisons. |
| `MITCAD_NO_PATTERN_RUNS=1` | Patterns of features join or cut each feature's copies on their own instead of a run of features in one boolean ([architecture](architecture.md#geometry-kernel), mitcad#855), for comparisons. |
| `MITCAD_NO_NEAR_COPIES=1` | A mirror's `combine` joins every image with its body whole, also a near copy of it ([commands.md](../core/model-api/src/commands.md#mirror)), for comparisons. |
| `MITCAD_RESULT_STORE` | Result store folder; `off` disables it. |
| `MITCAD_RESULT_STORE_MIN_MS` | Store results that took at least this long (default 100); quicker ones only when cheaper to restore than to evaluate (mitcad#882). |
| `MITCAD_STORE_QUICK=0` | Store no quicker results than `MITCAD_RESULT_STORE_MIN_MS` (also in `mitcad-cli`), for comparisons. |
| `MITCAD_DISPLAY_MESH_WRITER=0` | An open stores its display meshes in its own job instead of after the open on the writer's thread (mitcad#880). |
| `MITCAD_MESH_DURING_RECOMPUTE=0` | An open meshes its bodies for display only after its recompute, not each body once no later feature changes it (mitcad#881). |
| `MITCAD_PROJECTS_DIR` | Where New Project creates projects instead of Documents/Mitcad. |
| `MITCAD_GIT` | git program for remotes (Preferences sets it for the app). |
| `MITCAD_LIBRARIES_DIR` | Where fetched component libraries are kept (default: `libraries` in the user's local data folder). |
| `MITCAD_TEXTURES_DIR` | The texture cache of Edit Appearances' texture sources: thumbnails and fetched maps (default: `textures` in the user's local data folder; mitcad#315). |
| `MITCAD_TEXTURE_FIXTURES` | The online texture sources' requests answered by files of this folder (`<folder>/<host>/<path>`, the query left out; a missing file is HTTP 404) instead of the network: UI tests (`tools/ui-appearance-polyhaven-test.sh`, mitcad#313). |
| `MITCAD_TEST_REMOTE_RETRY_SECONDS` | First push retry delay instead of a minute. |
| `MITCAD_TEST_REMOTE_CHANGE_CHECK_SECONDS` | Age of the last fetch after which the first change refetches (default two minutes). |
| `MITCAD_TEST_LIVE_RETRY_MS` | The live updates' first delay before connecting again to a broker (default one second, doubling up to five minutes; with this, up to 20 times it). |
| `MITCAD_MOSQUITTO` | The mosquitto program for the live updates' tests, when it is not on `PATH` or in the usual places. |
| `MITCAD_TEST_LIVE_CA` | A PEM file of certificate authorities the app's live updates trust besides the system's (the test broker's, `tools/ui-live-test.sh`). |
| `MITCAD_NO_UPDATE_CHECK=1` | No update checks; update test variables, read only by a build with `MITCAD_UPDATE_TEST_HOOKS`: [updates.md](updates.md#the-test-hooks). |
| `MITCAD_SKETCH_FIRST_DIMENSION_SCALE=0` | Preferences › Sketch › *Scale the sketch to its first dimension* is off until it is saved (mitcad#860); the UI test libraries set it, so that their drawn sketches keep their sizes. |
| `MITCAD_TEST_LOG_URLS=1` | Addresses the app would open in the browser (a report's issue form) are logged, not opened; the UI test library sets it. |
| `MITCAD_LOG_TYPED_TEXT=1` | Text fields are logged as they change: `File dialog <title>: <text>` (Qt's file dialog's file name), `Dialog field <title>: <text>` and `Panel field <input>: <text>` (typed into another dialog's field or a command panel's input; never a password); the UI test libraries set it (`app/framework/TypedTextLog.hpp`). |
| `MITCAD_TEST_CRASH=app\|app-exception\|model-worker\|import-worker\|render-worker` | That process (or the model's worker thread) crashes as an access violation would (`app-exception`: the application throws an exception nothing catches, mitcad#773), for the crash reports' tests ([app/COMMANDS.md](../app/COMMANDS.md#feedback-and-error-reports)). |
| `MITCAD_TEST_PARSER_PANIC=<file name or path>` | The reader of that file panics as a parser bug would (mitcad#922; `core/model-base/src/reader_guard.rs`): in the application's guarded readers (`dxf_info`, `svg_info`, the DXF and SVG inserts, the print settings' read) the panic becomes their error; in the import process's listings (`--list-references`, `--list-links`) it ends that process (`core.bridge`, `tools/ui-import-listing-test.sh`). |
| `MITCAD_CRASH_DIR` | Where crash reports go instead of `crashes` in the local app data (the app sets it for its workers, with `MITCAD_CRASH_PARENT`). |
| `MITCAD_CORE_DUMPS=1` | Linux: `mitcad-cli`, `mitcad_run_parallel` and the corpus tests may leave core dumps (see *Core dumps* below). |
| `MITCAD_RENDER_SAMPLES`, `MITCAD_RENDER_TEST_SCENE=1`, `MITCAD_RENDER_WORKER` | The rendered view's samples per pixel; the fixed test scene instead of the document; another render worker than `mitcad-render` next to the app ([rendering.md](rendering.md)). |
| `MITCAD_CARGO_SWEEP=0`, `MITCAD_CARGO_JOB_MIB` | The crates' test builds (`core.cargo_build*`): no sweep of stale units after the build; MiB of available memory per cargo job instead of 2560 ([the crates' test builds](#the-crates-test-builds-sweep-jobs-debug-information)). |

App options for tests: `--demo` (a ready block), `--screenshot <png>`,
`--no-recovery`, `--no-native-dialogs` (Qt's file dialogs, so paths can
be typed), `--open <file>`, `--set d3=35`.

### Linux UI tests

`tools/ui-*-test.sh` start the app on a hidden Xvfb display and drive it
with real input (xdotool), checking the app's log and screenshots
([tools/ui-test-lib.sh](../tools/ui-test-lib.sh)). Each test takes its own
display, so they run in parallel. `MITCAD_UI_VISIBLE=1` shows a run on the
current display, `UI_APP` picks another build. Never start the app
visibly on a desktop in use.

- **No fixed sleeps.** `ui_sync` sends Pause; the app (started with
  `MITCAD_TEST_SYNC=1`) answers `Sync <n>` once preceding input and
  delayed logs are handled
  ([app/COMMANDS.md](../app/COMMANDS.md#logs-that-ui-tests-read),
  `app/framework/TestSync.hpp`). Fixed pauses only where a test checks
  that nothing happens, lets a long operation start before cancelling,
  paces drags, or avoids a double-click (`ui_apart`).
- **Typed text is confirmed.** A path or a value typed into a dialog goes
  through `ui_type_path "<title>" <path>` (focus the dialog, select all,
  type, Enter) or `ui_type_field "<title>" <text>` / `ui_type_text <text>`
  (the dialog `ui_focus_dialog` focused last), a command panel's input
  through `ui_type_in`: they wait until the app has logged exactly the
  typed text for the field (`MITCAD_LOG_TYPED_TEXT`), and type once more
  if a character was lost or the field had not taken the keyboard
  (mitcad#102).
- `ui_step` waits until no job runs (`ui_wait_idle`), since the window
  takes no input while the progress dialog shows; a test that cancels a
  job sends Esc without a step.
- Isolation: each test has its own `XDG_CONFIG_HOME`, `XDG_DATA_HOME`
  (autosave folder) and git config (`GIT_CONFIG_GLOBAL`,
  `GIT_CONFIG_NOSYSTEM`). Tests start the app with `--no-recovery`;
  `UI_RECOVERY=1` re-enables the recovery prompt.
- The layout: `ui_start_app` starts the app Docked (`--chrome=docked`),
  which the tests were written for, although Floating is the default
  (mitcad#429), unless the test's arguments choose a layout (`--chrome…`)
  or `UI_DEFAULT_CHROME=1` leaves it to the preference and the default
  (`ui-chrome-layout-test.sh`, `ui-browser-rows-test.sh`). Tests that
  start the app themselves pass `--chrome=docked` too.
- The result store is off (`MITCAD_RESULT_STORE=off`) because stored
  results would change how many features later steps compute; tests of it
  set `MITCAD_RESULT_STORE=<dir>` and `MITCAD_RESULT_STORE_MIN_MS=0`.
- No network: update checks are off (`MITCAD_NO_UPDATE_CHECK=1`);
  `ui-update-test.sh` uses its own local HTTPS server.
- Xvfb runs with `-noreset` so a restarted app finds the display at once.
  The screen is 1600 x 1000 logical pixels; a test of a tall dialog sets
  `UI_SCREEN_HEIGHT` before `ui_start_display`.
- The app runs without gdb: gdb loads the debug build's symbols (about
  1 GB) at every start, and the app's crash handler logs a crash's signal
  and stack itself. `UI_GDB=1` starts it under gdb for gdb's backtrace.
- Logged places are clicked as logged, so log a place a click really
  takes: a check box stretched over its row takes clicks only on its box
  and label, so its logged place is its box, not the row's middle; a tab
  bar squeezed by its neighbours hides tabs under its scroll arrows.
  Log places again when the layout or the camera moves them, and wait for
  the settled place after a change of layout (a `settled` helper that
  sees the last place stay across a sync), not the first line logged.
- Leave a mode by its own control when Esc does several things in turn:
  in a drawing Esc ends the tool, then clears the selection, then closes
  the drawing, so the tests close it with Close Drawing.

### Windows UI tests

ctest `app.ui-windows` and `app.ui-windows-workflow` (configured with
`MITCAD_MESA_DIR`), with their own `MITCAD_SETTINGS_DIR` and
`MITCAD_AUTOSAVE_DIR`. An SSH session runs in session 0 without an input
desktop: `SendInput` and UI Automation do not work, so the tests post
window messages, and popups (command search, menus) are unavailable, so
commands go through shortcuts. As on Linux, a step waits for the app's
answer to the sync key (`MITCAD_TEST_SYNC`; the library posts Pause), and
a key's modifiers stay in the keyboard state until the app logs that it
took the key (`MITCAD_LOG_KEYS`): while a model job runs, Qt keeps the
input for later. As on Linux, `Ui-StartApp` starts the app Docked unless
the arguments choose a layout or `-DefaultChrome` leaves it to the
preference and the default (Floating, mitcad#429). See
[tools/ui-windows-lib.ps1](../tools/ui-windows-lib.ps1).

Not a test: `tools/ui-windows-window-shots.ps1 -App <mitcad.exe> -Out <dir>`
saves screenshots of the whole main window in the Docked chrome on the light
and the dark theme (the demo block, a feature selected, Extrude's panel
open), through the test driver in the same session, for comparing the
window's look before and after a change. Likewise
`tools/ui-windows-print-settings-shots.ps1 -App <mitcad.exe> -Out <dir>`
saves screenshots of the print settings UI (mitcad#655): 3D Print after
Import from slicer…, Import Details, the profile editors, Materials,
Property Sets' print settings and an update bar, on data it makes with
mitcad-cli.

Not a test either: `tools/ui-windows-place-shots.ps1 -App <mitcad.exe> -Out <dir>`
saves screenshots of placement by picked geometry (mitcad#656): Box and Hole
on a face with their in-view dimensions and value boxes, a pulley on a shaft
and its Check Belt, a cylinder at a sketch point, Gear Pair and Belt Drive
with their candidate lists, an earlier version's gear opened for editing,
and Gear Pair in the Docked chrome, on data it makes with mitcad-cli.

### macOS UI tests

ctest `app.ui-macos` (`cmake/MacUiTests.cmake`, option
`MITCAD_MACOS_UI_TESTS`) runs the bundle's binary with `--demo`, `--open`
and `--screenshot` and checks the screenshots with
`tools/ui-image-stats.py --shot`, as `app.ui-windows` does. The app renders
offscreen and nothing is clicked, so no Accessibility or Screen Recording
permission is needed, but a window server is: without one the test exits
with 77 and is skipped (`MITCAD_UI_REQUIRE_GUI=1` fails instead). Settings,
recovery, projects and thumbnails go to a temp folder, and
`QT_FORCE_STDERR_LOGGING=1` brings Qt's log to the test's logs.
`MITCAD_UI_LLDB=1` runs the app under `lldb --batch` for a crash's stack.
`app.ui-macos-workflow` runs step scripts (`tools/ui-scripts/*.mitcad-ui`)
through the in-process test driver (`MITCAD_TEST_INPUT`), so it works over
SSH. Helpers: [tools/ui-macos-lib.sh](../tools/ui-macos-lib.sh) (bash 3.2,
BSD userland).

Before `focus-widget` attempts focus, its diagnostics record Qt's active
popup, active window, target and lookup window. Every matching named field
reports visibility, enabled state, geometry, its actual window, GlassCard
host and parent chain. This distinguishes an open popup or a hidden
duplicate from a command field that the presenter has not shown. The
diagnostics do not dismiss popups, change visibility or bypass the existing
visible/enabled checks.

`qt-choice` sends real pointer move, press and release events to a populated
combo popup. Each phase resolves the persistent item's current visual
rectangle and screen point again: selecting a motion type can resize its
floating command card during the press. The log records each phase's
rectangle, screen point, popup visibility and current caption. The driver
then requires the original popup's actual closure within six seconds before
advancing. It logs the resulting caption without assuming that an action
such as New Printer keeps its temporary menu label. It never selects the model item or
hides the popup directly; a dialog opened by the selected action remains
available to subsequent script steps.

`app.ui-macos-print-occurrences` and its native counterpart exercise
3MF occurrence selection and Send in both appearances (mitcad#856, #857): one
selected placement, two placements independently put on their print bed
faces, and a selected parent component sent as assembled by its button or
turned together onto a picked green face. Escape ends only the question,
sends nothing and leaves the panel open; a second Escape cancels it.
Nothing is exported or launched before the face choice is answered. A local
recording program stands in for the external slicer. The checks read its actual 3MF
ZIP/XML, placements, normals and build objects, verify visible status text
and unchanged model/undo state, and retain a capture per case. No external
GUI or network is opened.

All `ui_run_app` launches share the per-user kernel advisory lock at
`~/.mitcad/macos-ui.lock`, across checkouts and independent CTest runs.
The reserved shell descriptor 9 and the actual application share ownership;
no PID file, stale-lock removal or environment opt-out is involved. The
lock is acquired before the per-application `UI_TIMEOUT` clock starts and
released after that application is reaped. An interrupted waiter is
cleaned up; if its shell is killed, a still-running application retains
the lock until it exits. The application remains `UI_PID`, so existing
cancellation and crash handling keep targeting the real child.
Coordinate **whole GUI queues one at a time**: this per-application lock
is a backstop, and an outer CTest deadline still includes time queued for
it. Existing test deadlines are unchanged. Frozen older release checkouts
without this helper must finish their GUI queue before another starts.
`repo.macos-ui-lock` exercises the actual helper with processes that open
no windows: cross-process exclusion, queue time outside `UI_TIMEOUT`,
direct child identity, nonzero exit, interrupted waiting, a killed shell
with a live child, and application timeout cleanup. A real `TestDriver: FAIL`
line gives a lingering application up to one second to exit within its
existing deadline, then stops that
owned application and reports the assertion while preserving its actual
exit code and log. Script traces quoting that text do not trigger cleanup;
explicit LLDB runs retain their debugger lifecycle. The process regression
also covers this failure cleanup, trace exclusion and subsequent lock use.

The native slicer and table/configuration parity workflows also exercise
an actual `NSOpenPanel`. macOS hosts that chooser's editor in an XPC
service, so these workflows use the driver's `native-system-key` and
`native-system-type` steps for its Go to Folder field and Open action.
Those steps post real WindowServer keyboard/Unicode events and require
**Accessibility / event-posting permission for the Mitcad test app** in
System Settings, Privacy & Security. A missing permission fails clearly;
it is never treated as a successful import or replaced with a model API
call. An explicitly authorized local run can set
`MITCAD_TEST_REQUEST_EVENT_ACCESS=1` to request permission from Mitcad;
ordinary runs only preflight the existing grant. These input steps require
`MITCAD_TEST_INPUT`, an actual native file panel, and this app as the
active/frontmost key-window owner. A changed target stops the input.
`native-file-state [label]` records the panel's public selection and focus
state alongside the event-access and target-frame diagnostics.
The ordinary in-process UI and screenshot checks retain their existing
permission requirements.

`app.ui-macos-dialogs` checks the default Qt command search and parameter
sheet with real keyboard input in both light and dark appearance. It saves
focused captures, checks filtering and native shortcut text, and exercises
parameter creation, inline editing, validation and deletion. The colour
scheme request applies only to the test process. Set
`MITCAD_UI_REQUIRE_RETINA=1` to require a backing scale of at least 2 for
Retina acceptance; ordinary runs also support a 1x display.

### .f3d corpus tests

Real `.f3d` designs are never committed. These tests read local data and
are skipped without it:

- `MITCAD_F3D_CORPUS` (default `~/f3d-corpus`): `.f3d`/`.f3z` files,
  searched recursively. ctests `f3d.corpus_occt`, `f3d.corpus_meshes`,
  `f3d.corpus_import`, `f3d.corpus_timeline` (debug builds sample every
  n-th file), `core/f3d/tests/corpus.rs`, `design_corpus.rs`, and the
  `.f3d` part of `ui-import-test.sh` (`MITCAD_F3D_UI_PART`, else the
  smallest part).
- `MITCAD_F3D_DESIGN_REF` (default `~/f3d-design-ref`): an independent
  decoder's JSON per design, compared field by field in
  `design_corpus.rs`.
- `MITCAD_F3D_MODELS` (default `~/f3d-models`): reference models
  `<id>/<id>.f3d` with external dumps `<id>/<id>.json`
  ([SCHEMA.md](../core/import/SCHEMA.md)). ctest `f3d.models_loft` and
  `core/f3d/tests/design_models.rs`.

Full corpus runs need a release build: `test_f3d_import --corpus [dir]
[--reports DIR] [--time-limit S]` ([core/import/README.md](../core/import/README.md))
and `test_brep_import --corpus` ([core/f3d/CORPUS_REPORT.md](../core/f3d/CORPUS_REPORT.md)).

The corpus runs (`test_f3d_import --corpus` and `--models`,
`test_brep_import --corpus`, `test_exchange --corpus`, `freecad.corpus`,
`ipt.corpus`)
import each file in a child process of its own, several at once, the
largest files first ([core/tests/parallel_runs.hpp](../core/tests/parallel_runs.hpp)).
OCCT's global state stays per file, and a crash, a hang or too much memory
ends only that file, which is reported and counted as failed. The lines
come in the files' order and the totals are those of a run in one
process, so outputs compare line by line (apart from the times, and a line
on standard error that sums up the children).

| Option | Environment | Default |
|---|---|---|
| `--jobs N` | `MITCAD_CORPUS_JOBS` | the cores / 4, at most the memory available / the limit; `1`: every file in the one process, as before |
| `--memory SIZE` | `MITCAD_CORPUS_MEMORY` | `4G` per child: what it may commit (`RLIMIT_DATA` on Linux, inherited by its children; a job object's process memory limit on Windows; not enforced on macOS); `none` |
| `--file-timeout S` | `MITCAD_CORPUS_TIMEOUT` | `3600` s per child; `0`: none |

A child that cannot allocate fails its file, or hangs in a thread that
could not allocate until its time is up: keep the limit well above what a
file needs (the line on standard error gives the largest peak).

#### Core dumps

On Linux, `mitcad-cli` (every command), `mitcad_run_parallel` and the
corpus tests that start themselves per file (`test_f3d_import`,
`test_brep_import`, `test_exchange`) make themselves non-dumpable at
start (`prctl(PR_SET_DUMPABLE, 0)`), unless `MITCAD_CORE_DUMPS=1`: a
child that runs out of its memory limit aborts (a Rust allocation that
fails ends the process with `SIGABRT`), and WSL captures core dumps
through a pipe in `core_pattern`, which `ulimit -c 0` does not stop. Each
dump was the size of the process (gigabytes), was written to the host,
and took the WSL service down with every distribution in it. The flag is
per process and `execve` sets it again, so each program clears it itself;
the crash and its report are as before. A process that is not dumpable
cannot be attached to with `gdb -p` either: start it under `gdb`, or set
`MITCAD_CORE_DUMPS=1` (only where core dumps go to a file).

### .ipt corpus

Real `.ipt` part files are never committed. `MITCAD_IPT_CORPUS` (a
`PATH`-like list of folders) points at them; without it the tests skip:
the ctest `ipt.corpus` (`tools/cli/ipt-corpus.cmake`: every file imported
with `mitcad-cli import-ipt`, compared with a STEP file of the same part
next to it, `<name>.stp`, within 1e-6 or the limit the folder's
`references.tsv` gives the file), `core/ipt/tests/corpus.rs` and a step
of `ui-import-test.sh` (the smallest file). Files run as the other corpus
tests do (the options above). See
[core/import/docs/ipt.md](../core/import/docs/ipt.md#ipt-import).

### .dwg corpus

Real `.dwg` drawings are never committed. `MITCAD_DWG_CORPUS` points at a
folder; every `index.tsv` under it lists drawings (a header row naming the
columns: `file`, relative to the index, and data sheet values to check:
`length_mm`, `chord_distance_mm`, `tube_diameter_mm`, `tube_wall_mm`,
`mass_kg`, `density_kg_m3`, `volume_mm3`, `solids`, `components`; other
columns are notes). The ctest `dwg.corpus` (`tools/cli/dwg-corpus.cmake`)
imports each with `mitcad-cli import-dwg --check`: every body built and
valid, every 3D entity taken, the bodies within the drawing's extents,
and the row's values (see
[core/dwg/README.md](../core/dwg/README.md)); it skips without the
variable. Each drawing is imported in a child process of its own, several
at once, as the other corpus tests do (mitcad#875; the options above).

### FreeCAD reference models and corpus

FreeCAD documents are never committed (they contain B-rep files, and
FreeCAD's examples are LGPL). Reference data is made with FreeCAD itself,
in the distro only:

- `tools/freecad-export/install-freecad.sh`: FreeCAD 0.21.2, 1.0.2 and
  1.1.4 AppImages, checksum-verified, extracted to `~/freecad/<version>/`.
- `tools/freecad-export/run_all.sh [--no-examples] [version...]` runs
  FreeCAD headless (`xvfb-run`): each script in
  `tools/freecad-export/models/` is saved as
  `~/fcstd-models/<version>/<id>.FCStd` with a dump `<id>.json`
  (`dump.py`: objects, enumerations, placements, stored shapes'
  properties, sketches as FreeCAD reads them, expression values). Model
  families: `sketch_*`, `pd_*`, `part_*`, `expr_*`. Script directives:
  `# Mitcad expects: no fallback` (the corpus test fails if any feature
  falls back) and `# Mitcad change: Width = 60 mm` with a FreeCAD-made
  variant `<id>_changed.FCStd` (`variant()` in `_prelude.py`). FreeCAD's
  bundled examples go to `~/fcstd-examples/<version>/`. `enums.py` and
  `merge_enums.py` produce `~/fcstd-models/enums.json`, the source of
  `core/freecad/data/enums.json`.
- `MITCAD_FCSTD_CORPUS` (a `PATH`-like list): ctest `freecad.corpus`
  (`tools/cli/fcstd-corpus.cmake`) imports every file with `mitcad-cli
  import-fcstd`, compares with the dump (`--reference`) and replays
  variants (`--set`), each file in a child process of its own
  (`mitcad_run_parallel`, the options above through the environment;
  `MITCAD_CORPUS_JOBS=1` checks them in the script's process).
  `core/freecad/tests/sketch_corpus.rs` compares every
  sketch with FreeCAD's reading. Example:
  `MITCAD_FCSTD_CORPUS=~/fcstd-models:~/fcstd-examples ctest -R freecad`.
- `MITCAD_GEOMETRY_CASES` (a folder): geometry kernel cases made of
  bodies cut from reported models, which are not committed, each in a
  folder of its own as B-rep files. `geometry/tests/test_boolean.cpp`
  reads `caliper-pad028/body.brep` and `pad.brep` (a join that loses
  volume, mitcad#964); skipped with a message without the variable.

### SolidCraft designs

SolidCraft designs are never committed except the import's own test
designs, written by hand (`core/solidcraft/tests/designs`, with their
expected values worked out from SolidCraft's definitions; SolidCraft is
not built or run for them). `MITCAD_SOLIDCRAFT_CORPUS` (a `PATH`-like
list of folders) points at real ones: ctest `solidcraft.corpus`
(`tools/cli/solidcraft-corpus.cmake`) imports every `.solidcraft` with
`mitcad-cli import-solidcraft` and checks one with `<name>.expected.json`
next to it (SolidCraft's `solidcraft-cli eval` of it) against it; a folder's
`known.txt` (file name, a tab, the reason) lists files allowed to fail
their check. `core/solidcraft/tests/corpus.rs` reads every file. Skipped
without the variable.

### Designs of earlier versions

`MITCAD_DESIGN_CORPUS` (a `PATH`-like list) names folders of MitCad
projects saved by earlier versions (each design with its project's
`.mitcad/brep` store), never committed. ctest `designs.corpus`
(`tools/cli/designs-corpus.cmake`) opens every `.mitcad` with `mitcad-cli
info` and fails when a sketch of one fails, such as a linked projection
whose source changed shape since (mitcad#249); skipped without the corpus.

### 3MF print settings corpus

Real slicer projects are never committed. `MITCAD_3MF_CORPUS` (a
`PATH`-like list of folders) points at them; each folder's `index.tsv`
lists its projects (a header row naming the columns, then a row per
project; projects named by their content id). The ctest `3mf.corpus`
(`core/model-tests/print/src/print_files_corpus.rs`, a test ignored in
`core.model.print` and run with `--ignored`) imports each project's print
settings into a
fresh design as Import from slicer... does (`import_print_profile`),
checks the row's columns where it gives them: `bytes`, `dialect`,
`printers`, `processes` and `materials` (how many came in), `printer`
and `process` (their names) and `problems` (the values not read as the
import's summary lists them, separated by ` | `; empty or `none` for
none), then exports a body of each material in the same dialect and
reads the export back: every value read into MitCad's keys must come
back alike (numbers within 1e-6). Other columns are notes. Skipped
without the variable. Example:
`MITCAD_3MF_CORPUS=~/3mf-corpus ctest --preset dev -R 3mf.corpus`.

### USDZ files

USDZ export (mitcad#123, [commands.md](../core/model-api/src/commands.md#usdz))
writes the binary USD layer with MitCad's own code; the tests read it back
with `mitcad-usdz`'s reader (the package's 64-byte alignment, the layer's
header and metadata, prims, points, indices, normals, materials, subsets,
texture coordinates and transforms). The format was checked once with
OpenUSD's own validators when the export was written (mitcad#123); no USD
library is needed to build or test.

## Before a commit

For development and merges to main the check is scoped and fast; an
occasional miss is accepted:

1. On Linux: `tools/check-all.sh` in its default mode (add `--fresh`
   when CMake files changed), e.g. in the distro after `sync-to-wsl.sh`:
   `wsl -d mitcad-dev --cd "~/src/mitcad" -- bash -lc tools/check-all.sh`.
   It runs only the ctests, UI tests and corpus the change affects.
2. Windows or macOS only when the change touches that platform's own
   code (`app/platform/`, `Q_OS_*` branches, packaging, its scripts):
   a warning-free build and `ctest` there.
3. Commit in steps that build and pass the tests.

Before a release build, the full check: `tools/check-all.sh --full` on
Linux, and warning-free builds with `ctest` in the Windows VM (with
`MITCAD_MESA_DIR`) and the macOS VM (with `app.ui-macos`).

### The check script

`tools/check-all.sh [--full] [--ui-jobs N] [--ui auto|all] [--ctest auto|all] [--ctest-jobs N] [--testrun auto|off] [--corpus auto|always|never] [--corpus-tests N] [--corpus-jobs N] [--memory SIZE|auto] [--overlap auto|off] [--fresh] [--only STEP,...]`

It checks at two depths:

- **By default** (development, and before a merge to `main` on a Linux
  machine; a Windows or macOS machine checks on its own platform) the check is
  scoped to the change and fast: a test runs only when what it depends on
  differs from every run in which it passed on the machine, a few long
  ctests that seldom catch anything are left out, and a UI test depends
  only on the areas it exercises (below). Now and then a fault slips
  through to the full check; that is accepted.
- **`--full`** (or `--release`), the check before a release build, runs
  everything: every ctest (`--ctest all`, with the release builds'
  doctests and `app.appimage`), the corpus (`--corpus always`) and every
  UI test (`--ui all`). Release builds are checked only this way.

| Step | |
|---|---|
| configure, build | Compiler warnings fail the check (an incremental build only shows those of what it compiled). |
| fmt, clippy, deps | Beside the ctest step (`--overlap auto`, below). clippy's target dir is `build/dev/clippy`, kept across syncs, or, where `build/dev/cargo` links into a persistent cargo folder (`MITCAD_CARGO_TARGET_DIR`, as in the release builds, whose build folder starts fresh), `clippy/` beside it there, so that a release does not check every crate from nothing. |
| ctest | The tests whose sources changed (below), without the corpus group, through the test runner (below); `cli.perf_*` run serially. The Linux UI ctests (`app.ui-linux-*`, each on a hidden X server of its own) run beside the others, within the runner's UI entries at once (mitcad#832). The geometry suite runs as one ctest per part (`geometry.suite.<part>`). |
| corpus | `f3d.corpus*`, `f3d.models_loft`, `freecad.corpus`, `ipt.corpus`, several at once (`--corpus-tests`), each importing several files at once (`--corpus-jobs`, `MITCAD_CORPUS_JOBS`). By default the test runner gives each corpus test its files at once from its profile (`mitcad-testrun --corpus-children`, mitcad#832; [tools/testrun/README.md](../tools/testrun/README.md#corpus-tests)): every run records each file's time and peak, and the next starts the longest files first and shares the machine's cores and the run's memory budget among the corpus tests by their work, a long test more children and a short one fewer, each child's memory limit twice its test's largest file's peak, never below `MITCAD_CORPUS_MEMORY` (1536M). A test without that history yet, every test with `--corpus-jobs`, and ctest -j without the runner take the fixed plan: all their children together fit the run's memory limit less 1G at `MITCAD_CORPUS_MEMORY` each, and the cores of a test slot; about the square root of that many tests run at once (12G, 32 cores and three slots: two tests, three files each). Each child runs within 300 s (`MITCAD_CORPUS_TIMEOUT`); `f3d.corpus_designs` checks its designs one after another in one process since 0.32.0 (at once, under the full check's load in the Linux release's build root, an item ran out of its item time), and a debug build's imports try an item from its own start, the route tried first, three times as long (`--own-start-time-scale`, mitcad#847); the runner then plans it by its own history, not by children (mitcad#872). With the budget from the free memory (`--memory auto` on bare metal, below) a child beside others starts only while the memory available exceeds the reserve by the child's file's last peak (`MITCAD_CORPUS_RESERVE`). The corpus step runs beside the UI tests (`--overlap auto`). Every corpus test is a row of the table. `auto` runs them only when `CORPUS_SOURCES` or the corpus files differ from every passing run on the machine (`~/.cache/mitcad/corpus-passed`). `CORPUS_SOURCES`: the readers and the import, the bridge's kernel and import files (`core/ffi/src/kernel`, `*_import.rs`, `ipt_history.rs`, `exchange.rs`, `memory.rs`, `core/cpp`; not `lib.rs` or other families' files), the geometry, `mitcad-cli`'s imports (`tools/cli/import.cpp`, not `main.cpp`), the build files, the OCCT port, and the resolved dependencies of the import's crates (`cargo tree` of `CORPUS_CRATES`, instead of `Cargo.toml` and `Cargo.lock`, so a crate added for something else does not count). Changes in `core/model` alone don't trigger it: use `--corpus always` when they may affect imports. |
| `ui-<name>` | The `tools/ui-*-test.sh` whose sources changed (below), `--ui-jobs` at a time (below), longest first, 30 min limit each (`UI_TIMEOUT`); with the test runner the machine's learnt cap within `MITCAD_UI_JOBS`, and each test's learnt room (`mitcad-testrun --ui-rooms`: the most other UI tests it may have beside it after it failed beside more and passed alone) is kept: such a test waits until few enough run, while the others go on (mitcad#872). The tests' waits for a log line (`ui_expect_log`, `ui_expect_new`) do not count the time the application computes (a job logged `Computing ... started` and not its end), up to 60 s more, so that a preview that takes longer under load is not a failure. `ui-compute` runs alone at the end (flaky under load), after the corpus step that ran beside the others. A test that fails among the others without crashing runs once more alone after them: a pass shows as `ok-again` and counts, so load flakiness stays visible without failing the check; a second failure, or a crash, fails it (mitcad#657). |

ctests by what changed (`--ctest auto`, the default; mitcad#134): each
ctest has a fingerprint of what it depends on, and is skipped (an
indented `ctest:<name>` row, `skipped`) when it passed with that
fingerprint on the machine before (`~/.cache/mitcad/ctest-passed/<name>`,
shared by the checkouts). `tools/ctest-fingerprints.py` finds the
dependencies from the build itself, not from a list kept by hand: ctest's
own listing (`--show-only=json-v1`), CMake's file API (check-all asks for
it before configuring; the targets, their folders, sources and
dependencies) and `cargo metadata` (the crates' dependencies). A test's
fingerprint covers:

- its registration (command, properties), without the checkout's paths,
  so that the checkouts share the passes;
- the files its command names: a cli test's JSON script or design, a
  `cmake -P` script, a Python or shell script and the files beside it;
- each program its command runs or names (the test executable,
  `mitcad-cli` as `-DCLI=...`): the CMake targets it is built from,
  through their dependencies. For each target its folder (less the
  folders of other CMake directories and of Rust crates, and less the
  files tests name, so that a cli test's JSON counts only for that test),
  its listed sources, and for a crate Corrosion builds (`mitcad_ffi`,
  `mitcad-release`) that crate's Rust closure; with them the build files
  (the top-level CMake files, `cmake/`, `triplets/`, `third_party/` with
  the OCCT port, `vcpkg.json`, `rust-toolchain.toml`), the compiler and
  the build's options;
- for a crate's tests (`core.vcs.*`, `core.model`, ...; the fixtures
  `core.cargo_build*`; the doctests) the Rust closure of the package: the
  workspace crates it depends on (with its dev-dependencies), the files
  they `include_str!` (so `core.mcp` follows `commands.md`), the external
  crates with their versions and features, `core/Cargo.toml` and
  `rust-toolchain.toml`;
- `core.dependencies` (cargo-deny): the manifests, `Cargo.lock` and
  `deny.toml`; `repo.spdx`: every file but documents;
- the fingerprints of the fixtures it requires (`cli.export` follows
  `cli.exchange`'s), but not of the fixtures that build the crates' test
  binaries.

A test whose program none of this recognises depends on the whole tree,
as does every test when the file API's reply is missing. Markdown
documents count only through an `include_str!`. So a documentation change
runs no ctest, one in `core/vcs` the vcs tests and what links the bridge
(`cli.*`, `core.bridge`, ...), one in `geometry/` the geometry tests and
what links it, and one in a cli test's JSON only that test (and the tests
whose fixtures it sets up). `FULL_ONLY` tests (`app.appimage`,
`core.doctests_release`) run only with `--ctest all`. A test named in
`--only` (`--only ctest:core.model`) always runs; `--only ctest:<name>`
without `ctest` runs only the tests named. ctest still brings in the
fixtures a test needs (`core.cargo_build` before a crate's test), and
the tests run in one ctest call, in parallel. `check-logs/ctest-plan.tsv`
lists each test's decision; `tools/ctest-fingerprints.py plan ...
--explain <test>` prints what a fingerprint is made of, and
`tools/check-all.sh --dry-run` lists the ctests, the corpus and the UI
tests that would run for the working tree, building and running nothing.

UI tests by what changed (`--ui auto`, the default): a UI test is skipped
(a `skipped` row) when its fingerprint is one it passed with on the
machine, kept in `~/.cache/mitcad/ui-passed/<name>` and shared by the
checkouts as the corpus's are. The fingerprint is of the files' contents
(Markdown documents left out, here and in the corpus's, since no UI test
or corpus test reads them):

- the areas the test exercises, which it declares in its header,
  `# check-all sources: app/sketch core/solver tools/cli` (several such
  lines add up), which must exist: the parts of `app/` it drives
  (`app/sketch`, `app/commands`, `app/files`, `app/render`, ...), the
  crates and bridge files of its family (`core/vcs`, the import's crates),
  `tools/cli` when it makes its designs with `mitcad-cli`;
- the build files (`UI_BASE`: `third_party/`, the top-level CMake files
  and `cmake/`, `triplets/`, `rust-toolchain.toml`, `vcpkg.json`) and the
  build's options (`MITCAD_*` switches such as `MITCAD_COMPILER_CACHE`,
  the build type);
- its script and the test library (`UI_LIBRARY`: `tools/ui-test-lib.sh`,
  `ui-image-stats.py`, `xwd2png.py`);
- the main window's layout as the built application lays it out:
  `tools/ui-layout.sh` starts MitCad with the demo design on a hidden Xvfb
  display once per check (also in a dry run, with the last build) and
  takes its `Window layout:` line, the window's size and minimum size and
  where the menu bar, the toolbars, the docks, the status bar and the 3D
  view are (kept as `check-logs/ui-layout.txt`). Every UI test clicks and
  takes screenshots in that window, so a change that moves or resizes any
  of it reruns every UI test, whichever files it was in (mitcad#412: a row
  added to the sketch palette made the window 9 px taller, and
  `ui-report`, which declares only `app/report`, was skipped until main
  failed). When the layout cannot be read, no UI test is skipped and no
  pass is kept;
- for the smoke set (`UI_SMOKE`: `ui-smoke`, `ui-workflow`, `ui-command`,
  `ui-file`) also everything the application is built from (`UI_COMMON`:
  `app/`, `geometry/`, `core/`).

So a change in `app/sketch` reruns the sketch tests and the smoke set, one
in `app/MainWindow.cpp`, `geometry/` or `core/model` only the smoke set
(the full check runs the rest), and one only in `core/vcs` the tests that
declare it and the smoke set; a change anywhere that alters the window's
layout reruns them all. A test that comes to exercise another area
must declare it, else changes there skip it until the full check. The
rule for what to declare: the areas whose behaviour the test checks (the
dialogs, panels and commands it drives, the crates behind them, the tools
that make its designs); what every test shares, the window and its
layout, is covered by the layout line, so a test does not declare
`app/MainWindow.cpp` only because it runs in the window. A panel or page
that is longer than its dock scrolls (command panels, the docked sketch
palette) rather than raising the window's minimum size, so adding rows to
it leaves the layout, and the other tests, as they were.
`--ui all` runs every UI test; a test named in `--only` (`--only
ui-sketch`) always runs. Passes of the ctests, the UI tests and the corpus
are kept only from runs that built (the build step ran), so a run on an
older build does not count for newer sources.

Results: `build/dev/check-timings.tsv` (plus ctest tests ≥ 10 s as
`ctest:<name>`, every corpus test, and the ctests skipped), history in `build/dev/check-timings-history.tsv` (the same rows with each run's start, so a corpus test's time can be compared between runs), logs in
`build/dev/check-logs/<step>.log`. Exit 0 only if all steps pass.
`--only ui-compute` or `--only build,ctest` reruns single steps. A UI
test that fails keeps the application's whole log there too
(`ui-<name>.app.log`, `MITCAD_UI_LOG_DIR`), and a rerun of a failed UI
test first renames its logs to `ui-<name>.failed.log` and
`ui-<name>.failed.app.log`, so the failure's logs stay for a look. A
UI test that crashed runs once more under gdb (`UI_GDB=1`), its log
`ui-<name>.gdb.log` and the app's in `ui-<name>.gdb/`; the test stays
failed whatever that run gives. UI tests run `--ui-jobs` at once, by
default the test runner's learnt cap within `MITCAD_UI_JOBS`, else 4
(each takes about 0.5 G without gdb), each within its learnt room.

The ctest and corpus steps run through the test runner
([mitcad-testrun](#the-test-runner-mitcad-testrun), `--schedule`) when it
is built (`build/dev/core/mitcad-testrun`; `--testrun off` or
`MITCAD_TESTRUN=off` turns it off): each test in a systemd scope of its
own, as many at once as the memory budget and the CPU gate allow (the
corpus group at most `--corpus-tests` at once), the longest first and the
tests that failed lately early. Its output is the step's log, followed by
the output of each test that did not pass (lines marked `| `) and its
results in ctest's form (result lines, the summary, `The following tests
FAILED:`), which the table's `ctest:<name>` rows, the passes kept for
`--ctest auto` and the release scripts read as before; its own report
(`report.txt`: resource use, the critical path, failures by kind),
`results.tsv`, `status.txt` and each test's log are in
`build/dev/testrun/<step>/`, the machine's profiles in
`~/.mitcad/test-profiles.json`. When the runner is not built or does not
finish (it cannot list the tests, it stops), the step runs as
`ctest -j` in a test slot instead (its output kept as
`check-logs/<step>.testrun.log`).

The UI tests, and the ctest and corpus steps without the runner, hold one
of two machine-wide slots (`flock` on `MITCAD_TEST_LOCK`, default
`~/.mitcad-test.lock` and `.2`; `MITCAD_TEST_SLOTS`), so at most two checks
test concurrently; the runner needs none, as it starts a test only within
the memory left. Each run has a memory limit (`--memory`,
`MITCAD_CHECK_MEMORY`, `none` to disable): the run goes into
a systemd slice of its own with that limit (no swap), made beneath the
slice the check was started in, so that a job slice's ceiling above it
holds too, and the runner's test scopes go into the same slice, so that
the limit covers the tests as well and the runner's budget is a share of
it (85 %). The slice goes when the run ends or is stopped. Without
`busctl` the run is a scope with the limit in the caller's slice, as
before.

The memory, by default (`--memory auto`, mitcad#872), depends on where
the check runs, and the run's first lines say which applied:

- **On bare metal** the budget follows the free memory: the runner runs
  with `--memory-budget free`, and starts a test only while the memory
  available (`MemAvailable`, or less where a slice above it, the job slice
  among them, is nearer its limit) exceeds a reserve for the desktop and
  the other jobs by what the test is predicted to take and the running
  tests are still predicted to grow into, read again before each start;
  the corpus tests' children do the same (`MITCAD_CORPUS_RESERVE`). So a
  check backs off while another job (an agent's build) takes memory and
  uses the room when it ends; the tests' own hard limits stay as the
  safety net. The reserve is `MITCAD_CHECK_RESERVE`, else the larger of
  4G and a tenth of the memory; the run's slice is limited to the memory
  less the reserve.
- **In a virtual machine** (`systemd-detect-virt --vm`, WSL, or the
  hypervisor flag in `/proc/cpuinfo`), whose free memory can read low
  until it asks for more (dynamic memory, ballooning), it is a fixed 12G
  as before.
- A size given with `--memory` or `MITCAD_CHECK_MEMORY` is used as given
  anywhere (the old behaviour).

The steps overlap (`--overlap auto`, `MITCAD_CHECK_OVERLAP`; mitcad#872)
when the test runner runs the tests: the lint steps run beside the ctest
step, and the corpus step beside the UI tests (before `ui-compute` and the
UI tests that run again alone, the run waits for it; a `wait for <group>`
row gives that wait). Each group runs in the background, its output
printed when it ends (`check-logs/<group>.out`) and its rows added to the
table. In 0.33.0's Linux release check the corpus step (8 min, 1.6 tests
at once, 18 % of the CPU) and the UI tests (10.5 min) ran one after the
other; beside each other they take the longer of the two. `--overlap off`
runs one step after another. Build and lint use half the cores unless
`CMAKE_BUILD_PARALLEL_LEVEL` / `CARGO_BUILD_JOBS` are set.

### Faster builds

- The Linux `dev` preset builds MitCad as Debug but links OCCT's release
  libraries (`MITCAD_OCCT_RELEASE_IN_DEBUG`, default on; not on Windows,
  where debug and release runtimes don't mix). Set it `OFF` to step into
  OCCT. Cargo's dev profile optimises dependencies at `opt-level = 2` and
  MitCad crates at 1 (`core/Cargo.toml`).
- C++ and macOS Objective-C++ go through ccache when installed (`MITCAD_COMPILER_CACHE`, default
  on). Paths are hashed relative to the checkout, so side-by-side
  checkouts share the cache. MSVC uses `/Z7` (CMP0141) because ccache
  can't cache shared PDBs. Rust can use sccache with
  `MITCAD_RUST_COMPILER_CACHE` (default on for macOS, off elsewhere),
  disabling incremental compilation only while that cache is active;
  see the [macOS cache setup and limitations](#macos-the-build-vm).
- OCCT comes from the overlay port in `third_party/vcpkg-ports` (patched:
  speed-ups of booleans and the checker, and the fillet's edge overflow,
  mitcad#121, which changes results; see its README), so vcpkg builds it
  from source, debug and release, on
  the first configure of a machine and after every change to the port:
  about 15 min in the WSL distro and 19 min in the Windows VM (both at
  once, vcpkg limited to 12 jobs with `VCPKG_MAX_CONCURRENCY`), with build
  trees of about 5 GB in the distro and 14 GB in the VM. They go to
  `$VCPKG_ROOT/buildtrees` and `packages` unless
  `-DVCPKG_INSTALL_OPTIONS=--x-buildtrees-root=<dir>;--x-packages-root=<dir>`
  moves them (in the VM, under the checkout's `build` folder, which the
  sync keeps), and can be deleted afterwards: every later configure, in
  any checkout, restores OCCT from vcpkg's binary cache (an archive of
  0.3 GB on Linux, 0.55 GB on Windows). macOS builds it the same way.

## Code conventions

- Code, comments, UI texts, docs and commit messages in English.
- C++17, warning-free with MSVC `/W4` and GCC `-Wall -Wextra -Wpedantic`,
  no compiler-specific constructs, no Windows path assumptions; builds and
  runs on Windows, Linux and macOS.
- Every source file starts with an SPDX line (`// SPDX-License-Identifier:
  MIT` or `# ...`), checked by `tools/check-spdx.sh`.
- Narrow strings (`std::string`, `char*`, paths) are UTF-8 on every
  platform. On Windows a new executable gets the UTF-8 code page with
  `mitcad_utf8_code_page(<target>)` (`cmake/Utf8CodePage.cmake`), so that
  its `argv` and file names are UTF-8 too.
- Match the surrounding style and comment density.
- Terms (project, design, component, occurrence, body) and the names of
  controls in user-facing text follow [naming](naming.md).
- A tooltip says in one short line what the control is or does: a
  command's (`CommandDef::tooltip`, the ribbon and the menus) at most 80
  characters, an input's or any other control's at most 100. No usage
  instructions, no parenthesised asides, no lists of exceptions; a short
  shortcut hint ("Place timber members (Enter places)") is allowed and
  counts. The detail goes into the [user guide](user-guide.md). The ctest
  `repo.tooltips` (`tools/tooltip-check`) checks the texts in `app/`.
- Metric defaults everywhere ([architecture](architecture.md#units)).
- A dialog that holds its form in a scroll area uses `FittingScrollArea`
  and sizes itself with `fitWindowToContents`
  (`app/framework/FitToScreen.hpp`): `QScrollArea`'s own size hint stops
  at 24 text lines and `QWidget::adjustSize` at two thirds of the screen,
  so the dialog would scroll although the screen has room. The scroll bar
  is for a screen too low for the form.
- Vector arithmetic on `[f64; 3]` comes from `core/vec3` (`mitcad_vec3::{add, sub, dot, cross, norm, unit, …}`), never from a private copy in a module; where a caller must keep a cut-off or a division by the length it has always had, use the named variant (`unit_above`, `unit_div`, `unit_div_above`). The same holds for `[f64; 2]` with `mitcad_vec3::vec2` (`perp`, `lerp` and a scalar `cross` besides; the 2D length is `hypot`). The order of the floating-point operations there is part of the contract.
- Rust first for new code of our own: new logic we write ourselves goes
  in Rust (`core/`), on what the `Kernel` already gives (meshes, faces,
  edges, distances), rather than as new C++ code. Mesh analyses,
  classification, arithmetic and bookkeeping over shapes belong in Rust.
  What OCCT already provides (exact B-rep construction, booleans,
  fillets and blend surfaces, sweeps, exact intersections) is called
  through the C++ facade, not reimplemented in Rust; existing C++ is not
  rewritten just because it is C++. A port patch to OCCT
  (`third_party/vcpkg-ports/opencascade`) only when nothing else works.
- New geometry operation: a `Kernel` method with a default
  `Err(Unsupported)` in `core/model-base/src/kernel.rs`, grouped by family;
  bridges per family (`core/ffi/src/kernel/<family>.rs`,
  `core/cpp/bridge/<family>.{hpp,cpp}`, `geometry/src/<family>.cpp`).
  The trait and the data it takes and gives live below the model's core
  in `mitcad-model-base` (mitcad#777): a type a `Kernel` method uses goes
  there too (the model re-exports it at its usual path), and its methods
  that need the core stay in the model as an extension trait. The mock
  kernel's side goes into `core/model-testkit/src/lib.rs`.
- New plain data or helpers that need nothing of the model's core (ids,
  names, specifications, value types): into `core/model-base`, re-exported
  by the model; the rest into `core/model`.
- Sketch code (2D geometry, constraints, the solve, regions, projections,
  offsets, patterns, texts, SVG, the sketch edits and the sketch
  definition `SketchDef`): into the crate `core/sketch` (`mitcad-sketch`,
  below the model's core, mitcad#780), which the model re-exports as
  `mitcad_model::sketch`; what needs the document (the sketch feature's
  references, checks against the timeline and evaluation) into
  `core/model/src/features/sketch.rs`. A type the sketch needs from the
  core moves down into `core/model-base`. A sketch command goes into the
  api's `sketch.rs`.
- New feature: a module in `core/model/src/features/`, registered in
  `features/mod.rs`, documented in `core/model-api/src/commands.md`. Its
  `map_params` comes from `map_params!` (`features/mapping.rs`) when every
  value maps under its field's name; the fields are listed in the order
  the values are visited ([Adding to the
  API](../core/model-api/src/commands.md#adding-to-the-api)). UI
  commands: [app/COMMANDS.md](../app/COMMANDS.md).
- New command or query: a variant of a `named_enum!` enum
  (`core/model-api/src/dispatch.rs`) with its name on the wire; a family
  with an enum of its own is one row of `Document::command_families`.
  The api is a crate above the model (`mitcad-model-api`), so its methods
  on `Document` are extension traits: a file's methods go into its
  `<File>Api` (public), `<File>CrateOps`/`<File>Ops` (the crate's) or
  `<File>Local` (the file's own) trait; model items it needs are `pub`,
  and those that are not the model's API are re-exported by the model's
  `internal` module (`mitcad_model::internal::<module>`, mitcad#811),
  where the api names them; their definitions stay `#[doc(hidden)]` or
  in a private module. Every split crate does the same: what another
  Mitcad crate needs of it but is not its API is under its `internal`
  module, so that its root and documented modules show its API alone.
- BUILD: the planning of the BUILD commands (placing, connecting, timber,
  frames, junctions, the cut list) goes into `core/model-build`
  (`mitcad-model-build`, above the model, below the api), so that a
  planning edit does not recompile the model; its methods on `Document`
  are extension traits (`<Block>Build` public, `<Block>CrateOps` the
  crate's; `mitcad_model_build::prelude`). What the design file keeps of
  BUILD (sources, records, the libraries' `build` blocks) and what the
  features use stays in `core/model/src/build/`; the BUILD tests go
  into `core/model-tests/build-*`.
- Design comparison (`diff`): `core/model-diff` (`mitcad-model-diff`),
  above the model; model items it reads that are not the model's API
  come from `mitcad_model::internal`; its tests go into
  `core/model-tests/files` (`diff_tests.rs`).
- Drawings: what a design file keeps of a drawing (a new stored field and
  its check) goes into the model's `core/model/src/drawing/` (`def.rs`,
  `annotate_def.rs`, `tables_def.rs`, `template_def.rs`); its layout on
  the sheet and the writers into the drawing crate `core/model-drawing`
  (`mitcad-model-drawing`, above the model, re-exporting the model's
  drawing types under the same paths); a drawing command into the api's
  `drawing*.rs`.
- New tests: a test that drives the model through the JSON api goes into
  the test crate of its area under `core/model-tests/` (a small crate per
  area, ctest `core.model.<area>`; [README](../core/model-tests/README.md)),
  so that adding it recompiles only that crate; a new area is a new crate
  there, a member in `core/Cargo.toml` and a `mitcad_cargo_test` in
  `core/CMakeLists.txt`. Tests of the model's own code that do not need the
  api stay in `core/model` (`core.model`), the api's unit tests in its
  files (`core.model_api`), the base's in its files (`core.model_base`),
  the drawing layout's in its files (`core.model_drawing`), the sketch's
  in its files (`core.sketch`).
  Shared fixtures: the model's `testing` (the mock kernel, the crate
  `mitcad-model-testkit`) and `test_support` (the block document, feature
  definitions, face and edge names) under its `testing` feature, and
  BUILD's test library in `core/model-tests/support`.

## Licence policy

- MitCad's code is MIT with an SPDX identifier in every file.
- Third-party components are used under their most permissive offered
  licence (an MIT/Apache crate as MIT, Qt as LGPL-3.0).
- Not allowed: GPL-2.0-only, non-commercial or otherwise restricted
  licences, statically linked LGPL. LGPL components (OCCT, Qt) are linked
  dynamically.
- Accepted exception: the AppImage runtime (type2-runtime, MIT) is a
  separate program statically linked with libfuse (LGPL-2.1), shipped
  unchanged except for its cleared update information;
  `packaging/linux/THIRD-PARTY-NOTICES.txt` names it with the sources.
- Copied or adapted code keeps its licence and headers in `third_party/`.
  OCCT is built with patches of MitCad's (under OCCT's licence, as
  `SPDX-License-Identifier` lines in them say) through the vcpkg overlay
  port `third_party/vcpkg-ports/opencascade`, which `vcpkg.json` names, so
  every platform builds the same OCCT, still as shared libraries; the
  patches are published with the source, the third-party notices name them,
  and [its README](../third_party/vcpkg-ports/README.md) gives their origin
  and licences.
- The annotations' technical lettering (mitcad#902, an ISO 3098 type B
  style font, `third_party/fonts/iso-lettering`, its README gives the
  name and the source), is offered
  under GPL-3.0, GPL-2.0 or LGPL-3.0, each with a font exception, and used
  under LGPL-3.0 with the font exception: shipped unchanged as a file of
  its own (beside the executables, `share/mitcad/fonts` in the AppImage,
  `Resources/fonts` in a macOS bundle) and loaded at run time, as LGPL
  components are linked dynamically; never embedded in a program. The
  packages carry its licence text and the third-party notices name it.
- If GPL components are ever included, the distribution becomes
  GPL-3.0-or-later; keep such components behind replaceable interfaces.
- Rust dependencies are checked by cargo-deny (`core/deny.toml`).
- ccache (GPL-3.0-or-later) and sccache (MPL-2.0) are development tools
  invoked as separate processes. Neither is linked into or shipped with
  MitCad, and neither adds a Cargo or native library dependency.
- Native libraries built from source and linked statically: QtKeychain
  (BSD-3-Clause, `cmake/Keychain.cmake`); its licence text goes into the
  packages and the third-party notices name it. `tools/check-dependencies.sh`
  checks the fetched source's licence text. libsecret (LGPL-2.1), which it
  uses on Linux, is loaded at run time from the system, not linked or
  shipped.
- GPU SDKs (the render worker's GPU devices, mitcad#50,
  [rendering.md](rendering.md#licences)) are build tools only: nothing of
  NVIDIA's CUDA toolkit (CUDA EULA), AMD's HIP SDK or Intel's oneAPI
  compilers is linked statically or shipped; the GPU drivers are loaded
  at run time (Cycles' cuew and hipew, Apache-2.0), and the kernels
  shipped are Cycles' code compiled by them (the AppImage's
  `usr/lib/mitcad/cycles/lib`, the Windows installer's `bin\cycles\lib`;
  the notices name cuew). Open Image Denoise's CUDA device (Windows,
  `build-cycles.ps1 -Cuda`) contains CUTLASS (BSD-3-Clause) and CCCL
  header code and calls the driver through its own runtime; it links only
  the driver's import library. NVIDIA's OptiX SDK headers
  (a proprietary licence: binary-only, NVIDIA hardware only, recipients
  bound to its terms) would be compiled in, so MitCad does not use OptiX
  (README, FAQ).
- HEIF images of image references (mitcad#850): libheif and its HEVC
  decoder libde265 (GNU LGPL version 3 or later, as their sources say;
  vcpkg's metadata names LGPL-3.0-only) from vcpkg, decode only. The
  manifest's default feature `heif` asks for `libheif` with
  `default-features: false`: its default feature `hevc` is the x265
  encoder (GPL-2.0-or-later), and no other encoder feature is turned on,
  so the dependencies are libde265 and vcpkg's build helpers. Both are
  shared libraries of the dynamic triplets, linked by `mitcad` (libde265
  through libheif); `app/CMakeLists.txt` stops on a static libheif or
  libde265, and `tools/check-dependencies.sh` on a static archive of
  either or an x265 or x264 in the build's vcpkg tree. They stay separate
  files a user can replace with interface-compatible builds (the
  AppImage's `usr/lib/libheif.so*` and `libde265.so*`, the installer's
  `heif.dll` and `libde265.dll`, the bundle's `Contents/Frameworks`,
  where replacing a library breaks the bundle's signature as for Qt and
  OCCT). Their licence texts (their COPYING files, with the LGPL and the
  GPL it builds on, which `check-dependencies.sh` looks for; libde265's
  with `win32cond.c`'s licence for the Windows build) go into the packages
  with the other vcpkg ports' copyright files,
  and the third-party notices name the versions and the corresponding
  sources: the upstream release tags, built unmodified but for the build
  patches of vcpkg's ports at `vcpkg.json`'s `builtin-baseline`. They are
  in a build only with `-DMITCAD_HEIF=ON`: `MITCAD_HEIF` is off by
  default, releases included, and such a build neither builds nor ships
  them and reads no HEIF image. The HEVC
  test data of `app/tests/heif_image.cpp` was made with ffmpeg's libx265
  (GPL) by `tools/heif-test-fixture.py`, a separate program run by hand;
  nothing of it is linked or shipped.
- This is not a legal review; licence combinations are checked before a
  release.

## MCP validation

The headless server is implemented in `core/mcp` (stdio protocol and
catalog), `core/ffi/src/mcp.rs` (native CAD tools and workspace paths),
and `tools/cli/mcp.cpp` (command-line entry). Live GUI access adds
`app/mcp/LiveServer.*` (its listening socket from
`app/mcp/LoopbackListener.*`), the shared private endpoint discovery in
`app/mcp/LocalEndpoint.*`, `app/MainWindowMcp.cpp`, the application's own
tools in `app/MainWindowMcpUi.cpp` (mitcad#265) and the CLI's
`tools/cli/mcp-connect.*` stdio bridge. Which commands and queries MCP
allows follows their class in `core/model-api/src/classes.rs`; its unit
tests (in `core.model_api`) fail for a command or query without one. The CLI dispatches MCP
before configuring global component libraries, and suppresses OCCT's
stdout message printers. User setup and modeling examples:
[mcp.md](mcp.md).

In an approved isolated build environment, after mirroring the source
and building with the usual preset, run:

```bash
ctest --preset dev -R '^(cli\.mcp|core\.mcp)$' --output-on-failure
# In the isolated macOS VM with a logged-in window server:
ctest --preset dev -R '^app\.ui-macos-mcp$' --output-on-failure
# In the Windows VM, configured with MITCAD_MESA_DIR (session 0):
ctest --preset dev -R '^app\.ui-windows-mcp$' --output-on-failure
# In the Linux environment, on Xvfb:
tools/check-all.sh --only ui-mcp
```

`core.mcp` uses the Cargo build fixture that CTest includes automatically.
`cli.mcp` is registered when Python 3 is available and runs
`tools/cli/mcp-test.py` against the built native executable. Its subprocess
reads and shutdowns have timeouts, and it creates temporary workspaces
under the build directory, cleaning them after each test. It checks
native box geometry, file round trips and STEP export in addition to
protocol and path refusals. On Windows the test receives Qt's DLL
directory on `PATH`, like the other CLI tests (`mitcad-cli render` loads
Qt).

`app.ui-macos-mcp` runs the actual application and CLI with a private
`MITCAD_MCP_DISCOVERY_DIR`, settings and recovery directories under the
build tree. The in-process TestDriver verifies model volumes, native
history and viewport screenshots after live MCP edits. The Python harness
checks authentication, stale and insecure records, ambiguous instances,
notification and file-operation refusals, geometry errors with applied
mutations, undo, full-width integer IDs, malformed/oversized frames,
reconnect, EOF retention and GUI New invalidating old handles. It runs
serially with the other Mac GUI tests and needs no Accessibility permission.

The live debugging flow (mitcad#265,
[tools/ui-mcp-live.py](../tools/ui-mcp-live.py)) runs in `app.ui-macos-mcp`
and in the Linux UI test `ui-mcp` (`tools/ui-mcp-test.sh`) against a design
whose sketch holds a projection that no longer follows: queries and
`sketch.add_line` with the sketch open, `ui_state`, `ui_select`,
`ui_screenshot`, `ui_command` (Clean Up Stale Projections, Finish Sketch),
`document_save` and `ui_log`. `app.ui-windows-mcp`
(`tools/ui-windows-mcp-test.ps1`) drives the same flow from PowerShell
through `mitcad-cli.exe mcp --connect` in the Windows VM's session 0.
The Linux UI test `ui-mcp-files-camera` (`tools/ui-mcp-files-camera-test.sh`)
drives the bridge from the shell (jq, two FIFOs) for Save As and Open into
the folders for designs and for the camera (mitcad#402, mitcad#405).

Follow the focused checks with `tools/check-all.sh` in the Linux isolated
environment and the documented Windows and macOS builds with CTest.
Builds, tests, formatters, package installation and runtime checks belong
in those isolated environments, never on the host. Commit messages must
identify any platforms whose checks were not run.

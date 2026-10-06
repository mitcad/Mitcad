# Mitcad

Mitcad is an open-source parametric desktop CAD application for Windows,
Linux and macOS (Apple Silicon, macOS 14.4 or newer), licensed under the
[MIT licence](LICENSE).

- **Parametric modelling with a history:** sketches with constraints and
  dimensions, features on a timeline, and parameters with expressions.
  Change a dimension or edit a feature and the model is rebuilt.
- **Visual workflow:** a tabbed toolbar, command panels with live preview
  and drag handles, a model browser, a timeline and command search.
- **Support for multiple CAD formats:** FreeCAD and `.f3d` designs open
  with their history; STEP, IGES, BRep, STL, OBJ, DXF and 3MF import and
  export ([the table below](#support-for-multiple-cad-formats)).
- **3D printing:** bodies straight to a slicer (Bambu Studio, OrcaSlicer,
  PrusaSlicer, UltiMaker Cura).

How to use it: [docs/user-guide.md](docs/user-guide.md).

## Why

Mitcad was started because the world still lacked a CAD application that
is easy to use, imports a wide range of file formats and runs on Linux as
well. It is free to use, change and share under the MIT licence.

## Support for multiple CAD formats

| Format | Import | Export | Design history |
|---|---|---|---|
| Mitcad project (`.mitcad`) | Open, Insert Component | Save | Yes |
| FreeCAD (`.FCStd`) | Open, Import | – | Features, sketches and parameters |
| `.f3d`, `.f3z` | Open, Import | – | Features, sketches and parameters |
| STEP (`.step`, `.stp`) | Bodies | Bodies (AP214 or AP242) | – |
| IGES (`.iges`, `.igs`) | Bodies | Bodies | – |
| BRep (`.brep`, `.brp`) | Bodies | Bodies | – |
| STL | Meshes | Meshes | – |
| OBJ | Meshes | Meshes | – |
| DXF | Into a sketch | From a sketch | – |
| 3MF | – | For slicers | – |

If a file does not import as it should, please
[open an issue](https://github.com/mitcad/Mitcad/issues).

## Status

Mitcad is under active development. Version 0.1.0 is the first release,
with a Windows installer and a Linux AppImage; on macOS a script makes a
disk image. Part modelling, sketching, assemblies, analysis, the formats
above and version history of projects work.

Known gaps: direct modelling without a history, and some loft
combinations.

## Updates

Mitcad checks once a day for a signed release and offers to install it
(Windows installer, per-user AppImage); checks can be turned off by the
user or an administrator ([docs/updates.md](docs/updates.md)).

## Building

Mitcad is C++17 (Qt 6 Widgets, Open CASCADE Technology) and Rust (the
model, the solver and the file import), built with CMake and Ninja;
Corrosion builds the Rust crates and CXX bridges the two languages.
Versions used in development: Qt 6.12 LTS, OCCT 8.0.1 (built by vcpkg),
the Rust toolchain pinned in [rust-toolchain.toml](rust-toolchain.toml).

The configure presets in [CMakePresets.json](CMakePresets.json)
(`linux-debug`, `linux-release`, `macos-debug`, `macos-release`,
`windows-debug`, `windows-release`) take
vcpkg and Qt from the environment variables `VCPKG_ROOT` and
`QT_ROOT_DIR`. The setup scripts below write a local
`CMakeUserPresets.json` (not under version control) with a `dev` preset
that sets them.

Build and test in an isolated environment (dependency build scripts run
third-party code): [docs/development.md](docs/development.md).

### Linux

Requirements: GCC 13+ or Clang, CMake 3.25+, Ninja, Rust, vcpkg and Qt
6.8+. On Ubuntu 24.04 the scripts install everything with pinned
versions:

```bash
sudo tools/dev-env/install-packages.sh
tools/dev-env/setup-user.sh
cmake --preset dev && cmake --build --preset dev && ctest --preset dev
```

The first configure builds OCCT with vcpkg (about 10 minutes). The
application is `build/dev/app/mitcad`; use a release build
(`linux-release`) for performance measurements. Packaging as an
AppImage: [docs/development.md](docs/development.md#linux-appimage).

### Windows

Requirements: Visual Studio 2022 or newer with the C++ desktop tools.
`setup-windows.ps1` installs Git, CMake, Ninja, ccache, Rust, Python, NSIS,
cargo-deny, vcpkg and Qt (run it as an administrator) and writes
`CMakeUserPresets.json`; `msvc.cmd` runs a command in the MSVC
environment:

```bat
powershell -ExecutionPolicy Bypass -File tools\dev-env\setup-windows.ps1 -Repo <checkout>
tools\dev-env\msvc.cmd cmake --preset dev
tools\dev-env\msvc.cmd cmake --build --preset dev
tools\dev-env\msvc.cmd ctest --preset dev
```

The application is `build\dev\mitcad.exe` (with Qt's `bin` on `PATH`)
and needs OpenGL 2.1+. Without a GPU, build Mesa's llvmpipe with
`tools/dev-env/setup-mesa-windows.ps1` and configure with
`-DMITCAD_MESA_DIR=<dir>/bin` ([docs/development.md](docs/development.md#windows-the-build-vm)).

### macOS

Requirements: Apple Silicon, macOS 14.4 or newer, the Xcode Command Line
Tools and OpenGL 4.1 core (deprecated by Apple but present). In the build
VM, `setup-macos.sh` installs CMake, Ninja, Rust, cargo-deny, vcpkg and Qt
in the user's home directory and writes `CMakeUserPresets.json`:

```bash
tools/dev-env/sync-to-mac.sh
ssh <user>@<guest> "~/src/mitcad/tools/dev-env/setup-macos.sh"
ssh <user>@<guest> "cd ~/src/mitcad && cmake --preset dev && cmake --build --preset dev && ctest --preset dev"
```

The application is `build/dev/app/mitcad.app`. Packaging as a disk image:
[docs/development.md](docs/development.md#macos-bundle-and-native-ui).

## Tests

```bash
tools/check-all.sh
```

runs the warning-free build, `cargo fmt`/`clippy`, dependency checks,
`ctest`, the corpus tests when relevant and the UI tests (on a hidden Xvfb
display). Details, the Windows and macOS tests and the corpus setup:
[docs/development.md](docs/development.md).

## Repository layout

| Directory | Contents |
|---|---|
| `core/model` | Rust model: document, timeline, features, sketches, naming, recompute, project file, JSON API ([commands.md](core/model/src/api/commands.md)) |
| `core/solver` | 2D sketch constraint solver |
| `core/f3d`, `core/freecad`, `core/import`, `core/zip` | `.f3d` and FreeCAD readers and the history import |
| `core/dxf`, `core/3mf` | DXF and 3MF |
| `core/vcs`, `core/update` | Version history in git; automatic updates |
| `core/ffi`, `core/cpp` | CXX bridges between Rust and C++ |
| `geometry` | C++ facade over OCCT |
| `app` | Qt application ([app/COMMANDS.md](app/COMMANDS.md)) |
| `tools` | `mitcad-cli`, UI tests, dev environment scripts, FreeCAD export macros, macOS packaging |
| `docs` | [Architecture](docs/architecture.md), [development](docs/development.md), [user guide](docs/user-guide.md), [updates](docs/updates.md) |
| `third_party` | Third-party files with their own licences |

## AI agent policy

Pull requests and issues made entirely by an AI agent are welcome, with
these conditions:

- An agent that writes code or issues uses at least Claude Opus 5.5 or
  GPT-6.1 Sol, with medium reasoning effort. High reasoning effort is
  preferred. Lighter models and lower effort levels are not accepted
  for this work.
- Before writing an issue, the agent finds the code the issue concerns
  and refers to it in the issue (files, functions, the current
  behaviour). This confirms that it has understood the context and the
  target correctly; an issue written without looking at the code is not
  accepted.

## Licence

MIT ([LICENSE](LICENSE)). Qt and OCCT are LGPL and linked dynamically;
licence policy: [docs/development.md](docs/development.md#licence-policy).

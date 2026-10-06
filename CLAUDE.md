@private/CLAUDE.md

# Mitcad – instructions for Claude

The import above is the maintainers' optional private instructions
(`private/` is git-ignored; without it the import does nothing).

Mitcad is an MIT-licensed parametric desktop CAD application (Rust model,
C++ OCCT facade, Qt UI). Read [docs/architecture.md](docs/architecture.md)
and [docs/development.md](docs/development.md) before larger changes.

## Rules

- Everything in English: code, comments, UI texts, docs, commit messages.
- Follow the [code conventions](docs/development.md#code-conventions)
  (SPDX line in every new file, C++17 warning-free on MSVC `/W4` and GCC
  `-Wall -Wextra -Wpedantic` and AppleClang, Windows, Linux and macOS,
  metric defaults) and the
  [licence policy](docs/development.md#licence-policy).
- Never commit real `.f3d` designs, FreeCAD documents or other
  third-party models; corpus tests read them from `MITCAD_F3D_CORPUS` and
  `MITCAD_FCSTD_CORPUS`.
- Build and test only in the isolated environments (WSL distro, Windows
  VM, macOS VM), never on the host; mirror the tree with
  `tools/dev-env/sync-to-*.sh`. Use a login shell in the distro
  (`bash -lc`, `bash -l -s`) and keep intermediate files out of `/tmp`.
- Never start the application visibly on the desktop. Linux UI tests run
  on a hidden Xvfb display (`tools/ui-*-test.sh`), Windows UI tests are
  the ctests `app.ui-windows*`, macOS ones `app.ui-macos*` in the macOS
  VM.
- Before a commit: `tools/check-all.sh` in the distro
  ([the check script](docs/development.md#the-check-script)), the
  Windows build with `ctest` and the macOS build with `ctest`.

## Key design constraints

- `core/model` has no OCCT dependency: geometry goes through the `Kernel`
  trait (`core/model/src/kernel.rs`); model tests use a mock kernel.
- The app talks to the model only through JSON commands and queries
  ([commands.md](core/model/src/api/commands.md)); UI commands are
  declarative definitions ([app/COMMANDS.md](app/COMMANDS.md)).
- Platform code: few `Q_OS_MACOS` branches (GL format, menus, shortcuts)
  and AppKit only in `app/platform/macos/`, behind
  `app/platform/MacChrome.hpp` (inline stubs elsewhere). The layout is
  chosen at run time (`chromeStyle()`: Docked on Windows and Linux,
  Floating on macOS); Docked must keep working everywhere.
- Face and edge references are topological names (`F2:side(c1[c4,c2])`,
  `E{…|…}`) carried through OCCT's history.
- Geometry operations must not modify their input shapes.

## Shared registration points

Parallel changes often add to the same lists. Keep each addition one
contiguous block, appended at the end:

- `core/model/src/features/mod.rs`: `pub mod`, `pub use` and one line in
  `feature_types!`.
- `core/model/src/kernel.rs`: new `Kernel` methods with a default
  `Err(Unsupported)`, grouped under a comment naming the family.
- `core/ffi/src/kernel/mod.rs`, `core/CMakeLists.txt` (`FILES` and
  `target_sources` of `mitcad_bridge`), `geometry/CMakeLists.txt`: one
  block per family; new families in new files.
- `core/model/src/api/commands.md` and README sections: a section of
  your own.
- `tools/dev-env/versions.sh`: tool pins shared by `setup-user.sh` and
  `setup-macos.sh` (`setup-windows.ps1` mirrors them by hand).

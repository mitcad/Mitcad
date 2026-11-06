@private/CLAUDE.md

# MitCad – instructions for Claude

The import above is the maintainers' optional private instructions
(`private/` is git-ignored; without it the import does nothing).

MitCad is an MIT-licensed parametric desktop CAD application (Rust model,
C++ OCCT facade, Qt UI). Read [docs/architecture.md](docs/architecture.md)
and [docs/development.md](docs/development.md) before larger changes.

## Rules

- Everything in English: code, comments, UI texts, docs, commit messages.
- Follow the [code conventions](docs/development.md#code-conventions)
  (SPDX line in every new file, C++17 warning-free on MSVC `/W4` and GCC
  `-Wall -Wextra -Wpedantic` and AppleClang, Windows, Linux and macOS,
  metric defaults) and the
  [licence policy](docs/development.md#licence-policy).
- Use the terms of [docs/naming.md](docs/naming.md): MitCad, not
  Mitcad; component, not part; user-facing text names controls as the UI shows them (the
  Assemble group, not ASSEMBLE).
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
- Before a commit or a merge into dev, features or main (the branches:
  [docs/development.md](docs/development.md#branches)): a scoped check on the machine's
  own platform only, never on another platform's environment: on a Linux
  machine `tools/check-all.sh` in its default, scoped mode
  ([the check script](docs/development.md#the-check-script)), on a
  Windows machine the warning-free build and the affected ctests in its
  Windows environment, on a Mac its build and the affected ctests. An
  occasional miss is accepted: every platform is checked in full by the
  release build (`check-all.sh --full`, the Windows build with the full
  `ctest`, and on macOS `tools/release-check-macos.sh`: build, no AppKit,
  the full `ctest`, the package).

## Key design constraints

- `core/model` has no OCCT dependency: geometry goes through the `Kernel`
  trait (`core/model-base/src/kernel.rs`); model tests use a mock kernel.
- Rust first for new code of our own: logic we write ourselves goes in
  Rust rather than new C++ when Rust does it well. What OCCT already
  does (fillets and blends, booleans, sweeps, exact intersections) is
  called, not reimplemented in Rust, and existing C++ is not rewritten
  just because it is C++
  ([code conventions](docs/development.md#code-conventions)).
- The app talks to the model only through JSON commands and queries
  ([commands.md](core/model-api/src/commands.md)); UI commands are
  declarative definitions ([app/COMMANDS.md](app/COMMANDS.md)).
- Platform code: macOS uses the shared Qt UI only (owner 6.11.2026,
  [macos-shared-qt.md](docs/macos-shared-qt.md)): the AppKit frontend
  (`MITCAD_MACOS_NATIVE_UI`) is not built until native UI work resumes.
  `#ifdef Q_OS_MACOS` is for behaviour the system or keyboard needs (the GL
  context, the Metal render service, menu roles, the Settings window,
  system keys, Backspace, trackpad), not for presentation; the 0.48 macOS
  presentation is under `MITCAD_MAC_UI` (built only with
  `MITCAD_MACOS_SHARED_QT_UI=OFF`, for comparison). New UI is shared Qt on
  every platform. AppKit code stays in `app/platform/macos/`, behind the
  public interfaces in `app/platform/` (`MacChrome` and the rest, with
  stubs). The Qt layout is chosen at run time (`chromeStyle()`: Floating by
  default everywhere since mitcad#429, Docked by the preference or
  `--chrome=docked`); Docked must keep working everywhere.
- Face and edge references are topological names (`F2:side(c1[c4,c2])`,
  `E{…|…}`) carried through OCCT's history.
- Geometry operations must not modify their input shapes.

## Shared registration points

Parallel changes often add to the same lists. Keep each addition one
contiguous block, appended at the end:

- `core/model/src/features/mod.rs`: `pub mod`, `pub use` and one line in
  `feature_types!`.
- `core/model-base/src/kernel.rs`: new `Kernel` methods with a default
  `Err(Unsupported)`, grouped under a comment naming the family.
- `core/ffi/src/kernel/mod.rs`, `core/CMakeLists.txt` (`FILES` and
  `target_sources` of `mitcad_bridge`), `geometry/CMakeLists.txt`: one
  block per family; new families in new files.
- `core/model-api/src/commands.md` and README sections: a section of
  your own.
- `tools/dev-env/versions.sh`: tool pins shared by `setup-user.sh` and
  `setup-macos.sh` (`setup-windows.ps1` mirrors them by hand).

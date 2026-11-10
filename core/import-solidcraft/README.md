# mitcad-import-solidcraft

The `.solidcraft` import (mitcad#450): a design read by
[`mitcad-solidcraft`](../solidcraft/README.md) and built again as MitCad
features through the model's commands, generic over the model's `Kernel`
(`mitcad-ffi` runs it with OCCT: `core/ffi/src/solidcraft_import.rs`).
The file keeps no bodies to check against, so every feature is evaluated
by MitCad. The crate docs (`src/lib.rs`) describe what comes in how;
[docs/solidcraft.md](docs/solidcraft.md) holds the notes.

It has an importer of its own, not the `.f3d` dump replay's, and depends
only on `mitcad-import-base` (vector helpers, the timeline notes' finish),
not on the replay (mitcad#785). `mitcad-import` re-exports it as
`mitcad_import::solidcraft`, so its callers keep that path.

A `.sc` file (SolidCraft's extension now, which other programs use too,
mitcad#1622) is imported only when the reader finds SolidCraft's format
key in it; any other is refused with "Not a SolidCraft design: …"
(`mitcad_solidcraft::check_design`, re-exported here with `check_file`
and `has_design_extension`).

Tests: `cargo test -p mitcad-import-solidcraft` (ctest
`core.import-solidcraft`), on the reader's test designs
(`core/solidcraft/tests/designs`) with the mock kernel; `tests/sc_extension.rs`
imports them copied to `.sc`. The corpus
(`MITCAD_SOLIDCRAFT_CORPUS`) runs through `mitcad-cli`
(`solidcraft.corpus`).

# mitcad-import-ifc

The IFC import (mitcad#273): an IFC file's building model, read by
[`mitcad-ifc`](../ifc/README.md), as a MitCad design, generic over the
model's `Kernel` (`mitcad-ffi` runs it with OCCT: `core/ffi/src/ifc_import.rs`,
the `import_ifc` command, `mitcad-cli import-ifc`).

| Module | What it holds |
|---|---|
| `lib.rs` | `import_ifc` and `IfcOptions`: the spatial structure as components, each element's shape in a command of its own, openings, appearances, occurrences of a type's representation maps. |
| `build` | Representation items as features (sketches, extrusions, revolutions, sweeps, pipes, lofts, cuts, combines); the `.rvt` import (`mitcad-import-rvt`) builds its elements with it too. |
| `brep` | Breps as base features, face sets as mesh bodies; mesh checks shared with the `.rvt` import. |
| `sketch`, `transform` | Profiles and paths as sketches; items moved, scaled and mirrored by a transform. |
| `properties`, `report` | Elements' property sets (mitcad#496); the report, with the quantity checks against the file's element quantities. |

A product (with its openings), or a representation map's component,
expands at most 10 000 items from its mapped items, nested maps included;
past that its shape is one "too many nested items" note (mitcad#930).

It depends on `mitcad-import-base` (the import's `Progress`, its memory
guard) and on no other import.

Tests: `cargo test -p mitcad-import-ifc` (ctest `core.import-ifc`). The
IFC corpus (`MITCAD_IFC_CORPUS`) is read by the reader's tests and the CLI
corpus runs, not here.

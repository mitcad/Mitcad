# mitcad-import-fcstd

The FreeCAD document import (`.FCStd`, read by
[`mitcad-freecad`](../freecad/README.md)), generic over the model's
`Kernel` (`mitcad-ffi` runs it with OCCT: `core/ffi/src/fcstd_import.rs`,
the `import_fcstd` command, `mitcad-cli import-fcstd`). The crate docs
(`src/lib.rs`) describe what comes in how; [docs/freecad.md](docs/freecad.md)
holds the notes, stage by stage, and the corpus test (`freecad.corpus`).

| Module | What it holds |
|---|---|
| `lib.rs` | `import_fcstd` and `FcstdOptions`: the document's structure (components, occurrences, links) with the bodies FreeCAD stored (stage 1). |
| `sketch`, `sketches`, `profiles` | Sketches with their constraints and dimensions (stage 2), face profiles and chosen parts of sketches. |
| `history`, `binders`, `features`, `part`, `primitives`, `sweep_inputs`, `fixed_paths`, `elements`, `reference` | The history of PartDesign Bodies and the Part workbench's results, each feature checked against its stored shape (stage 3). |
| `params` | Spreadsheets, VarSets, named constraints and bound expressions as parameters (stage 4). |
| `report`, `notes` | The report, and the timeline notes made from it (mitcad#170). |
| `appearances` | FreeCAD's body and face colours as document appearances on the bodies made, stored or rebuilt (mitcad#793). |

It depends on `mitcad-import-base` (vector helpers, planar faces, the
candidate core, free names, notes' texts, the sketch helpers and inactive
constraints), not on the `.f3d` replay.

Tests: `cargo test -p mitcad-import-fcstd` (ctest `core.import-fcstd`), on
documents built in memory with the mock kernel. The FreeCAD corpus
(`MITCAD_FCSTD_CORPUS`) is read by the reader's tests and the CLI corpus
runs.

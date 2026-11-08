# mitcad-import-base

What MitCad's imports share (mitcad#775): the `.f3d` and `.ipt` replay
(`mitcad-import`), IFC (`mitcad-import-ifc`), `.rvt` (`mitcad-import-rvt`)
and FreeCAD documents (`mitcad-import-fcstd`) depend on it; it depends on
none of them and on no file reader.

| Module | What it holds |
|---|---|
| `progress` | `Progress`: how far an import has come, for the watchdog and the memory guard of `mitcad-ffi`; the import running on a thread (`tick`, `current_progress`, `abandoned`, `Ticking`). Its `thread_local!` exists only here, so the geometry kernel's cancel check (`mitcad-ffi`, `kernel/cancel.rs`) sees every import's progress. |
| `geom` | Vector helpers on `[f64; 3]` and the dump's units (cm to mm). |
| `body_scope` | The bodies that belong to one replay: a receiving document's own bodies are neither references nor replaced. |
| `planes` | The planar faces of the bodies at the marker, and the planar faces and edges kept by body version while an import runs (`KeptPlanes`). |
| `scaffold` | The `$k` placeholders of a candidate's definitions, the core of a candidate, and the end of an import (its timeline notes and undo step). |
| `memory_guard` | `ImportMemoryGuard`: what the memory guard's measurements (`mitcad-ffi` measures the process against its limits) tell an import's `Progress`: tight at 70 %, low at 85 % or, from 50 % on, when growth kept up over two measurements would get there within 0.1 s, recovered below 70 %; measured every 0.02 s with the look-ahead of the guard of model commands (`mitcad_model::monitor::memory_guard`, mitcad#1505). Tested with fake measurements. |
| `names`, `notes`, `sketch` | A free MitCad name for a file's parameter name, the timeline notes' texts (a base feature standing in for items, an item imported partly, a note as a sentence; mitcad#170), and the sketch definitions' driven-dimension references and inactive constraints and dimensions (mitcad#721). |

`DEFAULT_GEOMETRIC_TOLERANCE` (0.01 mm, mitcad#224) is the default of the
imports' geometric tolerance.

`mitcad-import` re-exports what it exported before (`Progress`, `tick`,
`current_progress`, `abandoned`, `Ticking`, `Helper`,
`DEFAULT_GEOMETRIC_TOLERANCE`, `geom`), so its users keep their paths.

Tests: `cargo test -p mitcad-import-base` (ctest `core.import-base`).

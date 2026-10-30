# MitCad architecture

Three layers: a Rust model that knows the document, timeline and features
but no geometry; a thin C++ facade over Open CASCADE Technology (OCCT)
that does the geometry; and a Qt application that talks to the model with
JSON commands.

Sketch drawing tools attach their live value editors to dimension
previews built from temporary geometry. The overlay uses the placed
dimension renderer for their extension lines, arrows and sectors; typed
values continue to reach the model through the existing sketch commands.

## Layers

| Part | Language | Responsibility |
|---|---|---|
| `core/model` (`mitcad-model`) | Rust | Document, timeline, features, parameters and expressions, units, sketches, topological naming, cached recompute, undo, design file, design comparison. No OCCT. Its `testing` feature gives other crates' tests the mock kernel (`testing`, the crate `mitcad-model-testkit`) and the shared fixtures (`test_support`). |
| `core/solver` (`mitcad-solver`) | Rust | 2D constraint solver and the joint solver of assemblies behind a narrow API ([README](../core/solver/README.md)). |
| `core/vec3` (`mitcad-vec3`) | Rust | The `[f64; 3]` helpers (`add`, `sub`, `dot`, `cross`, `norm`, the unit-vector variants) that the model, the solver and the readers share, and the same for `[f64; 2]` in its `vec2` module; no dependencies, no vector type. |
| `core/f3d` (`mitcad-f3d`) | Rust | The `.f3d` reader's facade: re-exports the crates below under their old paths (`mitcad_f3d::design::ir`, `mitcad_f3d::asm`, ...), reads a document's body blobs, and has the test references, `mitcad-f3d-inspect` and the corpus tests. |
| `core/dump` (`mitcad-dump`) | Rust | The design dump IR of [SCHEMA.md](../core/import/SCHEMA.md) as Rust types, with the items' file states; written by the `.f3d` and `.ipt` readers, replayed by the import ([README](../core/dump/README.md)). |
| `core/asm` (`mitcad-asm`) | Rust | The ASM B-rep data of `.f3d` body blobs, `.ipt` B-rep records and `.dwg` solids (binary and text form, the history), a neutral B-rep model in millimetres with checks, and the conversion to it ([README](../core/asm/README.md)). |
| `core/f3d-names` (`mitcad-f3d-names`) | Rust | The names that faces and bodies of `.f3d` body blobs carry, and the faces, edges and vertices of a history state that feature inputs name by them ([README](../core/f3d-names/README.md)). |
| `core/f3d-file` (`mitcad-f3d-file`) | Rust | The document inside an `.f3d` file's zip container (segments, body blobs, `.f3z` packages), its display meshes and its appearance asset packages ([README](../core/f3d-file/README.md)). |
| `core/f3d-design` (`mitcad-f3d-design`) | Rust | The design segment of `.f3d` files (timeline, parameters, sketches, features, components) built into the dump IR from the decoded streams, with the feature inputs found in the bodies' ASM history; re-exports `mitcad-f3d-decode`'s modules under their old paths ([README](../core/f3d-design/README.md)). |
| `core/f3d-decode` (`mitcad-f3d-decode`) | Rust | The streams, object records, sketches, recipes and stored curves of the `.f3d` design segment decoded, below the feature building of `mitcad-f3d-design` (mitcad#779; [README](../core/f3d-decode/README.md)). |
| `core/expr` (`mitcad-expr`) | Rust | Parameter expressions with units: lexer, parser, evaluation, formatting, units and dimensions, parameter limits, the parameter table and the `.f3d` expression checks ([README](../core/expr/README.md)). No dependencies; the model re-exports it as `mitcad_model::expr`. |
| `core/print-keys` (`mitcad-print-keys`) | Rust | The print settings' key map: MitCad's printer, process and filament keys with their types, units, form places and, per slicer dialect, the slicer's key and value conversion ([README](../core/print-keys/README.md)). Re-exported as `mitcad_model::print_keys`. |
| `core/model-api` (`mitcad-model-api`) | Rust | The JSON command, query and preview API of a document, with its reference [commands.md](../core/model-api/src/commands.md) ([README](../core/model-api/README.md)). Above the model (mitcad#777): the document's api methods are extension traits (`DocumentApi`, `prelude`). |
| `core/model-build` (`mitcad-model-build`) | Rust | The BUILD tab's planning: placing and editing parts, connectors, panels, timber members, joinery, spacing sets, frames, fasteners, the junctions and the cut list ([README](../core/model-build/README.md)). Above the model, below the api (mitcad#777); what the design file keeps of BUILD stays in `mitcad_model::build` and is re-exported. The document's BUILD methods are extension traits (`prelude`). |
| `core/model-diff` (`mitcad-model-diff`) | Rust | Comparison of two designs or versions in the model's terms, for the version history ([README](../core/model-diff/README.md)). Above the model (mitcad#777); used by `vcs` and `ffi`. |
| `core/model-tests/<area>` (`mitcad-model-tests-<area>`) | Rust | The model's tests through the JSON api, a small crate per area (sketch, features, parts, assembly, joints, drawing, print, BUILD, the imports' tests that need the api, ...; ctest `core.model.<area>`), so that a new test recompiles only its own crate ([README](../core/model-tests/README.md)); `support` holds BUILD's test library. |
| `core/model-base` (`mitcad-model-base`) | Rust | Below the model's core (mitcad#777): the `Kernel` trait with the data it takes and gives (exchange options and meshes, drawing views, sheet metal charts, named segments), and the base data that needs nothing of the core: ids, topological names, profiles, datums, transforms, sweep and surface specifications, parameters, properties, configurations, appearances, render settings, the recompute monitor, fingerprints, hashes ([README](../core/model-base/README.md)). The model re-exports every module at its old path (`mitcad_model::kernel`, `mitcad_model::topo`, ...). |
| `core/model-testkit` (`mitcad-model-testkit`) | Rust | The mock geometry kernel of the model's tests (`MockKernel`, `MockShape`) on `mitcad-model-base` alone, so that an edit in it rebuilds only test binaries ([README](../core/model-testkit/README.md)). The model's own tests use it as a dev-dependency; its `testing` feature re-exports it as `mitcad_model::testing`. |
| `core/model-drawing` (`mitcad-model-drawing`) | Rust | The drawing layout and writers above the model (mitcad#777): sheets laid out with their views, dimensions, annotations, hole tables, parts lists, frame and title block from a sheet template, written as PDF, SVG, DXF and DWG; the built-in templates and the template libraries; the projection cache (`DrawingCache`, in the document's side cache). What a design file keeps of a drawing stays in the model (`core/model/src/drawing`: `def.rs`, `annotate_def.rs`, `tables_def.rs`, `template_def.rs`), which this crate re-exports under the same paths ([README](../core/model-drawing/README.md)). |
| `core/sketch` (`mitcad-sketch`) | Rust | The general sketch below the model's core (mitcad#780): 2D geometry, entities, constraints and dimensions, the solve, profile regions, projections, offsets, patterns, equal spacing, texts, fonts, QR codes, SVG and DXF drawings, the sketch edits (`SketchEdit`) and the sketch feature's definition (`def.rs`: `SketchDef`, `SketchOutput`), on `mitcad-model-base`, `mitcad-expr`, `mitcad-solver` and `mitcad-dxf` ([README](../core/sketch/README.md)). The model re-exports it as `mitcad_model::sketch` and the definition as `mitcad_model::features::sketch`, where the feature's references, checks against the timeline and evaluation stay. |
| `core/import` (`mitcad-import`) | Rust | Replays `.f3d` dumps (also those `core/ipt` makes of `.ipt` parts) as MitCad commands, checked against the file's stored results ([README](../core/import/README.md)): the replay's core (`mitcad-import-replay`) with its families (`mitcad-import-joints`, `mitcad-import-sheet`, `mitcad-import-primitives`, `mitcad-import-appearances`) put together, and the replay's tests (mitcad#785); `.solidcraft` designs through `mitcad-import-solidcraft`. Above `core/model`, not the api (mitcad#778): it asks the model for sheet states, joint frames and constraint rows directly, and runs the JSON commands a cut and paste needs through `Options::commands`, which `core/ffi` fills with the api's `command` (`mitcad_import::Commands`). |
| `core/import-base` (`mitcad-import-base`) | Rust | What the imports share: the import's `Progress` (one per process, for the watchdog, the memory guard and the kernel's cancel check), vector helpers, the bodies of one replay, planar faces, the candidate core, the notes' texts, the sketch helpers ([README](../core/import-base/README.md)). No reader. |
| `core/import-ifc` (`mitcad-import-ifc`) | Rust | The IFC import: the building model as components, features and bodies ([README](../core/import-ifc/README.md)). |
| `core/import-rvt` (`mitcad-import-rvt`) | Rust | The `.rvt` import: building elements as features and bodies, one component per level, with the IFC import's builder ([README](../core/import-rvt/README.md)). |
| `core/import-fcstd` (`mitcad-import-fcstd`) | Rust | The FreeCAD import: structure, stored bodies, sketches, history and parameters, checked against FreeCAD's per-feature shapes ([README](../core/import-fcstd/README.md)). |
| `core/import-solidcraft` (`mitcad-import-solidcraft`) | Rust | The `.solidcraft` import: designs with their history built again as MitCad features, with an importer of its own (mitcad#785; `mitcad-import` re-exports it as `solidcraft`) ([README](../core/import-solidcraft/README.md)). |
| `core/import-replay` (`mitcad-import-replay`) | Rust | The core of the `.f3d` dump replay: `Importer`'s state, the timeline walk, references, the history check, fallbacks and the report (mitcad#785; `mitcad-import` re-exports its public paths). Reaches the families above it through its `Families` trait ([README](../core/import-replay/README.md)). |
| `core/import-joints` (`mitcad-import-joints`) | Rust | The replay's joints family above its core: joints, joint origins, captured positions, ground items and assembly relationships, as extension traits on the core's `Importer` (mitcad#785; [README](../core/import-joints/README.md)). |
| `core/import-sheet` (`mitcad-import-sheet`) | Rust | The replay's sheet-metal family above its core: sheet items as native sheet features, roundings and chamfers as sheet corners (mitcad#785; [README](../core/import-sheet/README.md)). |
| `core/import-primitives` (`mitcad-import-primitives`) | Rust | The replay's primitives family above its core: boxes, cylinders, spheres, tori and coils placed on a plane (mitcad#785; [README](../core/import-primitives/README.md)). |
| `core/import-appearances` (`mitcad-import-appearances`) | Rust | The replay's appearances family above its core: the file's appearances and physical materials on the replay's bodies, faces and occurrences, as an extension trait on the core's `Importer`; the report's types stay in the core (mitcad#791; [README](../core/import-appearances/README.md)). |
| `core/freecad` (`mitcad-freecad`) | Rust | `.FCStd` reader: typed objects and properties, placements, links, sketches, spreadsheets, expressions. No geometry. |
| `core/zip` (`mitcad-zip`) | Rust | Zip container for `.f3d` and `.FCStd` (stored, deflate, zstd; a writer for tests). An entry inflates to at most 256 MiB unless the caller opts in to up to 4 GiB (body blobs, `.f3z` documents, large meshes), and never to more than 1032 bytes per compressed byte (mitcad#923). |
| `core/ipt` (`mitcad-ipt`) | Rust | `.ipt` part files: compound file container (MS-CFB), property sets (MS-OLEPS), the segment database, the B-rep record converted by `core/asm`, the meshes of mesh features, the definitions segment (parameters, sketches, features) as the import's dump IR, and `.iam` assemblies (referenced files, occurrences, placements; [README](../core/ipt/README.md)). No geometry. |
| `core/ifc` (`mitcad-ifc`) | Rust | IFC files (IFC2X3, IFC4, IFC4X3; `.ifczip`): an own ISO 10303-21 reader, units, the spatial structure, products and their placements, relationships, styles, and the shapes' representation items decoded (profiles, swept solids, booleans, breps, face sets, mapped items; [README](../core/ifc/README.md)). No geometry. |
| `core/solidcraft` (`mitcad-solidcraft`) | Rust | `.solidcraft` designs: the JSON document or its container, upgraded from any file format to one by SolidCraft's own steps (`third_party/solidcraft`), typed sketches and feature kinds, expressions in MitCad's language. No geometry. |
| `core/dxf` | Rust | DXF read and write. |
| `core/dwg` (`mitcad-dwg`) | Rust | `.dwg` drawings, R13 to R2018: the containers and their compression, header variables, classes, the object map, objects, the data storage; the 3D content (B-rep records parsed by `core/asm`, meshes, blocks and inserts) and the plan of components; the writer of 2D drawings in R2000 for the drawing export (mitcad#728) ([README](../core/dwg/README.md)). No geometry. |
| `core/3mf` (`mitcad-3mf`) | Rust | 3MF writer (one object of parts) and a checking reader for tests. |
| `core/usdz` (`mitcad-usdz`) | Rust | USDZ writer: the binary USD layer (crate format 0.8.0, LZ4 and USD's integer coding written here, no USD library) of a scene of meshes, `Xform`s and `UsdPreviewSurface` materials, packed with its textures 64-byte aligned; a checking reader for tests (mitcad#123). |
| `core/vcs` (`mitcad-vcs`) | Rust | Version history of projects in git (gitoxide, only here); remotes through the system's git; component libraries and community indexes in git repositories ([libraries.md](libraries.md)). |
| `core/update` (`mitcad-update`) | Rust | Release manifest, signatures, version selection; the `mitcad-release` tool. No network ([updates.md](updates.md)). |
| `core/mcp` (`mitcad-mcp`) | Rust | MCP lifecycle, JSON-RPC envelopes, tool schemas, the command-reference resource and `model_reference`, the reference in sections (mitcad#714); headless stdio and live application profiles. No geometry dependency ([mcp.md](mcp.md)). |
| `core/ffi` (`mitcad-ffi-bridges`), `core/cpp` | Rust, C++ | CXX bridges: the `Kernel` on OCCT, the document API for C++. An rlib (mitcad#792), so that Cargo starts it on its dependencies' metadata. |
| `core/ffi-staticlib` (`mitcad-ffi`) | Rust | The static library the application links (CMake target `mitcad_ffi`): `mitcad-ffi-bridges` with every Rust library it uses, no code of its own ([README](../core/ffi-staticlib/README.md)). |
| `geometry` (`mitcad_geometry`) | C++ | OCCT facade: profiles, features, booleans, name transfer, queries, file formats, ASM bodies, analyses. No Qt. |
| `tools/cli` (`mitcad-cli`) | C++ | Headless open/import, recompute, properties, export; kernel and import tests run through it. Also `render` (the final render through the render worker), which links Qt Core and Gui for the application's scene and job code. |
| `app` | C++/Qt, Objective-C++/AppKit | Shared document and command orchestration; Qt desktop UI and native macOS UI. |
| `tools/freecad-export` | Python, Bash | FreeCAD macros that make the FreeCAD import's reference models and dumps. |

Dependencies point one way: `app` → `core/ffi` → `core/model-api` → `core/model-build`, `core/model-drawing` → `core/model` (with
`model-diff`, `solver`, `vec3`, `f3d` (over `f3d-design`, `f3d-decode`, `f3d-file`, `f3d-names`, `asm`, `dump`), `import`, `freecad`, `zip`, `3mf`, `usdz`, `vcs`, `ipt`, `ifc`, `solidcraft`, `dwg`, `expr`, `print-keys`, `import-base`, `import-ifc`, `import-rvt`, `import-fcstd`, `model-base`, `sketch`; `update`
stands alone) and `core/ffi` → `geometry` → OCCT. The model reaches geometry only
through the `Kernel` trait, so model tests run on a mock kernel.

## Terms

Each term has one meaning in the code, the UI and the docs:

| Term | Meaning | In the code |
|---|---|---|
| **Design** | One `.mitcad` file (a part or an assembly) | `Document::load_design`, `save_design` (bridge), `is_design_file`, `FileKind::Design` |
| **Project** | A folder that is one git repository and holds designs and folders; Local or Cloud | `Project`, `ProjectRepo`, `open_project`, `find_project`, `create_project` |
| **Folder** | A folder inside a project | relative paths with `/` |
| **Version** | A recorded state of a design: one commit | `commit`, `history`, `load_version` |
| **Owner** | A user or an organisation on a git service (the first part of a Cloud address) | `CloudAddress::owner` |
| **Projects panel** | The panel of projects, folders and designs; the **Browser** is the design's feature tree | |
| **Projects folder** | The default parent of new projects (Documents/Mitcad) | `projectsDirectory()` (`framework/AppSettings`) |

JSON command and field names and the file formats' keys stay as they
are; the autosave metadata's `project` key names the snapshot's design
file, and the format `"project"` of `save_design` means version 3, a
design in a project.

## Identifiers

- **Feature:** `FeatureUid(u64)`, stored as `F<n>`. Timeline order is a
  separate list; an index is not an id.
- **Sketch entity:** `EntityUid(u32)` within its sketch: `c<n>` curves,
  `p<n>` points, `t<n>` texts (one number space); constraints and
  dimensions `k<n>`.
- **Body:** `BodyUid` = creating feature + ordinal (`F7.b0`). The display
  name (`Body1`) is separate.
- **Component / occurrence:** `C<n>` (root `C0`) and `O<n>`; an
  occurrence is named `<component>:<n>`.
- **Parameter:** referenced by a persistent `ParamId`, so it can be
  renamed. A model parameter's `owner` is the feature whose dimension it
  is and is deleted with it.

## The model

- `Document`: parameters, timeline (`Vec<FeatureUid>` + features), the
  timeline marker (roll-back point), components and occurrences, settings.
- **Tables** (mitcad#667, `core/model/src/tables.rs`,
  [table-lookups.md](table-lookups.md)): named tables of constant cells
  beside the parameters, which expressions read with `Table.Column` and
  `XLOOKUP`, `MATCH`, `INDEX`. The parameter table evaluates with the
  document's tables (`TableSource`); a table edit re-evaluates the
  parameters that use it in one atomic step, so features recompute only
  where a value changed.
- `FeatureEntry { uid, name, suppressed, component, def: FeatureDef }`.
  Each `FeatureDef` variant has its own module in
  `core/model/src/features/` giving dependencies, validation and
  evaluation.
- **Body state:** each feature maps the previous `BodyUid → Shape` state
  to a new one (New Body, Join, Cut, Intersect). A failed feature is
  skipped. State is per component; a feature sees only its component's
  bodies.
- **Recompute cache:** keyed by the feature's definition, the values of
  the parameters it uses and the versions of its inputs; reads of other
  features' results are recorded in the key (`Read::Output`,
  `Read::Datum`). A result's version is a 128-bit fingerprint of uid,
  definition and inputs (`core/model-base/src/fingerprint.rs`), stable across
  documents and processes.
- **Appending to a long timeline** (mitcad#841): a command's recompute
  goes on after the document's last results when the new state begins
  with the same feature entries and nothing they read of the state
  changed (parameter values, join tolerance, library sources, split
  memory, the assembly; no joints): it takes their results as they are
  and goes through only the features after them
  (`recompute::recompute_after`), with the result a recompute from the
  start gives. An importer's batches of features (`FeatureBatch`) thus
  cost what they add, not the timeline before them. Features are looked
  up by uid through an index (`core/model/src/feature_index.rs`) that
  checks every position it gives against the timeline, so it needs no
  upkeep when the timeline changes.
- **Result store** (`core/model/src/store.rs`): costly results are also
  kept on disk under the same key, so reopening a design or an older
  version reuses them. Shapes are stored with their face names and the
  edge and vertex names derived from them, which a shape read back takes
  instead of deriving them again
  (`geometry/include/mitcad/geometry/persist.hpp`); results of another
  build are ignored; sketches and construction features are not stored
  (base features are, also those that only bring in bodies: a stored body
  keeps the edge and vertex names that bringing it in derives,
  mitcad#887). An import's worker hands every result a store takes over
  to the window in a store of the import's own (mitcad#413). A result
  found keeps the shapes of the bodies it sets packed until something may
  read them (mitcad#858): the body states hold a stand-in under the stored
  version, an evaluation unpacks those it reads as it reads them
  (mitcad#886: the others stay packed), a joint unpacks them first, the
  bodies left at the end are unpacked, and
  a state the next restored result replaces is never unpacked (its result
  unpacks itself if the timeline's history is read later). What is
  unpacked together is unpacked on the machine's threads, a result's
  several shapes too (mitcad#887). The store also
  keeps the shown bodies' display triangulations under each body's
  version and the meshing options (see the display meshes below). The memory
  cache has a byte budget (the app gives a quarter
  of RAM) and evicts least recently used results not held by the current
  document. The `cache` query reports both caches.
- **Sheet material state:** `sheet_rules` publishes resolved versioned fabrication
  rules; `sheet_face` constructs exact named planar material domains through
  `Kernel::sheet_shape`. A body shape and its sheet delta are applied atomically,
  with final component ownership and matching shape generation. Rule and sheet
  reads participate in cache keys, including missing inputs and failed-operation
  blockers. Arbitrary body replacements invalidate existing material semantics;
  rigid moves transform them. The result store persists paired shape/domain
  outputs under a sheet protocol version. Native files store definitions and
  settled panel provenance; Paste New copies external rule dependencies and
  their parameter closure. `sheet_unfold`/`sheet_refold` (and `sheet_cut` and
  `sheet_corner_relief`, which cut the developed material) develop and refold
  chosen bends by traversing the labelled panel/bend graph from a fixed panel
  in the model (`core/model/src/sheet/folded/develop.rs`); charts keep their
  material coordinates and only their embeddings change, so the kernel only
  reconstructs. A cut that disconnects the material publishes every piece as
  a body with its own sheet state (the first in geometric order keeps the id,
  the others are the cutting feature's bodies). A Flat Pattern is a document
  item (`flat_patterns`, saved in the design file), evaluated on demand at
  the marker from the body's sheet state with its own cache; it is never a
  body. Copies of components copy the items of their bodies, and design
  diffs list them. See
  [sheet-metal.md](sheet-metal.md).
- **Cancellation:** a recompute reports progress to a `RecomputeMonitor`
  and stops between features when cancelled. Commands, undo, redo and
  previews compute the new state before committing it, so a cancelled one
  changes nothing. Inside a feature, long OCCT operations (booleans,
  fillets, sweeps, lofts, offsets, splits, sewing) poll the monitor
  through a progress range (`Kernel::interruptible`,
  `geometry/include/mitcad/geometry/cancel.hpp`) and throw `Cancelled`.
  A monitor can be part of others (`RecomputeMonitor::within`): cancelled
  with any of them or at its deadline. The `.f3d` import tries each
  definition under one, with the user's stop, the hang watchdog's
  give-up of the try, its memory guard and the item's time as its reasons
  to stop (mitcad#69, mitcad#71, mitcad#80), and the definition's share
  of that time, at which it gives way to the next ones and waits for what
  is left (mitcad#78). Without a monitor (`mitcad-cli`) nothing stops
  early.
- **Parallel evaluation:** `Document::fork` copies a document for another
  thread (definition, cached results and last recompute shared; a kernel
  of its own from `Kernel::fork`, None where a kernel cannot work on
  another thread), and `Document::adopt_results` takes a fork's results
  into the document's cache. The `.f3d` import evaluates the definitions
  ranked after the one it tries on such forks, with `threads` workers,
  and takes their results in rank order, so the import is the one of a
  single thread (mitcad#95, `core/import/docs/f3d-performance.md`, *Definitions
  evaluated in parallel*). Its kernels must be `ImportKernel`s: `Send`,
  with shapes that threads may share (`Send + Sync`). Within the same
  `threads` (`Progress::helper`) the import reads the file's bodies
  on several threads, heals the large bodies of each history's first
  state, and of the first states the replay asks for, while the rest is
  read (mitcad#245, mitcad#676), and builds the history's
  bodies ahead of the replay (mitcad#103, *Work beside the replay*); the
  timeline items
  themselves are replayed one after another. Every `Kernel` is `Send`
  with `Send + Sync` shapes, since forks go to other threads.
- **Components evaluated in parallel** (mitcad#842,
  `core/model/src/recompute/ahead.rs`): with recompute workers
  (`Document::set_recompute_workers`, 1 by default; the environment
  variable `MITCAD_RECOMPUTE_WORKERS`, which the ctests
  `core.model.<area>.workers` set), the recompute cuts the timeline into
  windows that end at barriers: joints and the other features that move
  or make occurrences, sketches with driven dimensions and Bound Length
  (they write parameters), and features that change other components'
  bodies. In a window each component's features run ahead on a worker
  with a kernel of its own (`Kernel::fork`, `Kernel::worker_thread`);
  a feature that reads other components (links, or reads of their
  bodies, placements or results in its last result) is left to the
  recompute. The recompute itself goes through the timeline in order as
  with one thread and takes a worker's result only where it would take a
  cached one (the same definition, every recorded read holding where the
  recompute is, the definition's parameter values the same) and the
  evaluation succeeded; otherwise it evaluates the feature and cancels
  the rest of that worker's job. Results, versions, topological names,
  statuses, warnings, the memory cache and the monitor's progress are
  therefore those of one thread; a stop cancels every worker and keeps
  nothing they returned. Results not yet taken count against the memory
  cache's budget, and the workers are stopped (and waited for) before the
  cache drops results for its budget, since the cache keeps what a worker
  still holds.
- **Memory:** an allocation that fails ends the process (Rust's cannot be
  caught; OCCT's `Standard_OutOfMemory` and `std::bad_alloc` inside a
  kernel operation become its error "<operation>: out of memory",
  `KernelError::OUT_OF_MEMORY`). The `.f3d` import therefore measures the
  process against its tightest limit (`core/ffi/src/memory.rs`: an
  address-space or data limit, a Windows job's limit or the commit left,
  the memory the system has left) and gives up definitions before an
  allocation can fail (`core/import/docs/f3d-performance.md`, *Memory*). OCCT's own
  allocator throws instead of returning null (a patch of the port,
  mitcad#132) and counts its failures per process and per thread, the
  jobs of OCCT's thread pool counted for the thread that ran them
  (`geometry::failed_allocations`, `failed_allocations_in_thread`):
  OCCT's checker and healing catch failures inside and go on, so an
  operation, the checks and the build of a body from B-rep data compare
  their thread's count before and after and fail "out of memory" when it
  moved, and the import's memory guard takes a moved process count as
  low memory.
- **Hangs:** a kernel call cannot be interrupted, so the `.f3d` import's
  watchdog gives up a try that makes no progress for its hang limit and
  runs the import again. The import ticks it between kernel calls, also
  in its own long loops (building and measuring a history state's bodies,
  finding faces and edges) and as OCCT's algorithms report progress (the
  healing of a body built from the file, and the interruptible ones above:
  `CancelSource::progressed`), so only a kernel call that does not return
  counts (mitcad#82). A try given up keeps its thread until the call
  returns; a process that ends meanwhile ends with `std::_Exit` once its
  output is written (`abandoned_imports()`: `mitcad-cli`, the import
  worker, the application), as the shared libraries' static destructors
  would tear down OCCT's state under that thread.
- **Undo:** the definition state (parameters, features, marker, body
  attributes, assembly, visibility, named views, analyses) is cheap to
  clone and pushed on a stack; several commands can merge into one step
  (`merge_undo`). Each state has a revision number, so `mark_saved` and
  the `document` query's `modified` work without comparing files.
- **Parameters:** name, expression (`d1 * 2 + 5 mm`), unit, comment,
  kind (model or user), evaluated in dependency order; cycles and unit
  mismatches are errors. Internal units are mm and radians. Input accepts
  a decimal comma (`core/expr/src/lib.rs`); stored expressions
  always use points. The language is a superset of `.f3d` expressions
  with the functions and syntax of spreadsheet formulas (mitcad#663):
  case-insensitive function names, `=` comparing inside parentheses,
  `let`, `value(x; "mm")`; `let` names are `NodeKind::Local`, never
  parameter references.
  Printed fit suggestions (`print_fits.rs`) are a table of ordinary user
  parameters, created only by explicit feature-field references during
  definition-state resolution. They share the feature's transaction and
  undo step. The UI reads their names from the model; expression hints
  and generator reports evaluate prospective defaults on parameter copies,
  while previews use the usual private definition-state copy.
  Limits (mitcad#665, `core/expr/src/limits.rs`): a parameter's optional `min`,
  `max` (expressions that join the dependency graph) and `step` clamp and
  snap its value, or refuse it when `strict`; the table keeps the result
  before them and which limit took effect.
  Text parameters (mitcad#664): the unit `Unit::TEXT` (`Text`) gives a
  parameter a text instead of a number. An expression's value is
  `expr::Value` (a `Quantity` or a text); the evaluator's numeric path
  refuses a text and the text path a number, so the two never mix.
  `Lookup::lookup_text` gives text parameters to expressions. A sketch
  text follows one through the sketch's `text_values` (slot
  `texts[t5].text`, a `ParamId` like any slot, so renames, ownership and
  dependencies are the usual ones); the recompute records a text
  parameter's read as a hash of its text (`Parameters::read_bits`).
- **Construction geometry:** planes, axes and points are features that
  make a datum, not bodies. Features refer to geometry through `GeomRef`
  (origin datum, construction feature, or a body face/edge/vertex) and to
  paths through `PathRef`.
- **Body attributes** (visibility, material, appearance; steel by
  default) live in the definition state.
- **Materials** (mitcad#640, `core/model/src/material.rs`,
  `document/materials.rs`): objects with a density, an appearance and an
  optional filament (`Print Filament` values), `based_on`, a version and
  an origin. The built-in ones keep the old ids (`steel`, `pla`, ...); a
  design keeps materials of its own in the definition state and the
  design file (one with a built-in's id takes its place), and default
  materials of components. A body is made of its own material, else the
  nearest component's default up the occurrence path, else the root's,
  else steel. Setting a body's material sets its appearance too, unless
  the body's was set by hand.
- **Print settings key map** (mitcad#640, `core/print-keys`):
  one Rust table of MitCad's printer, process and filament keys, each
  with its type, unit, choices, plain name and, per slicer dialect (the
  Bambu Studio family, PrusaSlicer, Cura), the slicer's key and the
  conversion of values; built from the slicers' option definitions by
  meaning (the module documentation names them). Export and import of
  print settings convert through it. Each key also has its place in the
  settings form (mitcad#655: a category, a group, advanced or common), in
  the table's display order, so the application keeps no list of its own.
- **Print profiles** (mitcad#640, `core/model/src/print_profile.rs`,
  `document/print_settings.rs`): the design keeps printer and process
  profiles in its definition state and file; components, occurrences and
  bodies choose one and override values in the `Print Printer` and
  `Print Process` property sets, typed by the key map. What a body prints
  with is resolved root, components down the occurrence path, occurrence,
  body (nearest wins; a level's own values before its profile's), with
  the material's filament (`print_settings`, `build_volume`).
- **Print libraries** (`core/model/src/print_library.rs`,
  `document/print_library.rs`): materials and profiles are JSON items of
  the base library (the parts library's manifest lists them; read through
  the library resolver, `LinkResolver::print_library`), the user's folder
  and the project's folder; the design keeps complete copies with their
  origin and takes a newer version only on request.
  `document/library_items.rs` (mitcad#655) lists the choosable items where
  they live (`library_items`: built-in, base, user, project, the design's
  own), edits an item in its library file with the design's copy taking
  the same change in one undo step, and copies a library's item in within
  the undo step that chooses it.
- **Slicers' files** (`core/model/src/print_files/`, `core/3mf`): the
  import reads a slicer's settings (Bambu Studio family JSON, PrusaSlicer
  INI, Cura stacks; presets and bundles) into profiles and materials, never
  geometry; the 3MF export writes each body's effective settings in a
  slicer's dialect (`core/3mf` carries more metadata, per-object
  metadata and files; `read_package` reads them without the geometry,
  within a budget checked against the declared sizes before reading: at
  most 4096 entries and a 256 MiB model part, other files over 64 MiB
  left out and 256 MiB for the files read in all, mitcad#927).
- **Appearances** (mitcad#46, `core/model-base/src/appearance.rs`): physically
  based parameter sets (base colour, metalness, roughness, specular,
  transmission, IOR, coat, emission, opacity, and a texture for the base
  colour: an image file or one embedded in the design file, mitcad#53,
  with normal, roughness, metalness and occlusion maps and the set's
  source, mitcad#313; an embedded image is kept once per design by its
  SHA-256, shared in memory and in the file).
  The library is built into the model; a document's own appearances are
  in the definition state and the design file, changed by undoable
  commands that recompute nothing. Bodies refer to one by id, and so do
  single faces by topological name (a body attribute; the face's
  appearance overrides the body's and follows the face as other face
  references do). The shaded view uses the display colour (the base
  colour) of bodies and faces, the rendered view every parameter and the
  texture with all its maps (mitcad#314). View > Component Colors
  temporarily overrides body and face colours in the shaded and wireframe
  views with a stable colour per owning component (shared by its
  occurrences). This user preference does not
  change appearances, exports or the document. Enabling it leaves the
  rendered view; entering the rendered view turns component colours off.
- **Render settings** (mitcad#47, `core/model-base/src/render_settings.rs`):
  how the rendered view lights and shows the design, per document: the
  environment (a built-in studio or outdoor setup, or an HDR image), the
  background, the ground and the film (exposure, view transform), and
  the final render's output (mitcad#48: size, samples, time limit,
  denoising, transparency, file format), as sections of fields with
  defaults, so that more sections can be added and older files open;
  and the user's lights (mitcad#54: point, spot, area and sun lights,
  placed in the design or relative to the camera), a list changed by
  commands of their own and compared light by light. They are in the definition
  state and the design file (left out while they are the defaults),
  changed by undoable commands that recompute nothing, and compared field
  by field in the comparison of versions.
- **Analyses** (`core/model/src/document/analyses.rs`): section analyses
  kept in the definition state with a name, a plane reference, an offset,
  a flip and a light bulb; their commands are undo steps that recompute
  nothing. The plane is resolved at the marker when asked for (the
  `analyses` query), so a face's section follows the model by its name.
  One is shown at a time, so one plane cuts the bodies; the app cuts the
  view where the shown one says (`MainWindow::followAnalyses`), except
  while Section Analysis' panel previews its own. Print analyses
  (mitcad#620) are kept the same way with their bodies, a bed face and
  their settings; their overhangs, thin walls and estimate are worked out
  in Rust (`core/model/src/print_analysis.rs`, the `print_*` queries) on
  the bodies' `Kernel::triangle_mesh` and `physical_properties`, with a
  bounding volume hierarchy of their own for the wall rays, no kernel
  method of their own; the results are kept in memory by the bodies'
  shape versions and the settings.
- **Image references** (mitcad#782;
  `core/model/src/document/image_references.rs`,
  `core/model-base/src/image_header.rs`, `api/image_references.rs`):
  pictures kept in the definition state with a name, a plane reference
  resolved at the marker as an analysis' is, a centre, angle and flip in
  the plane's frame, the size of a pixel, an opacity, a light bulb and
  the image file's bytes (an `ImageData`, saved base64 in the design
  file). The model reads only the files' headers (size and resolution;
  a HEIF file's size as shown, after its rotation and crop, mitcad#850);
  the app decodes the pictures (`view/ImageFile.*`: Qt's readers, HEIF
  through libheif and libde265, shared libraries from vcpkg in builds
  with `MITCAD_HEIF`) and draws them as textured quads by the
  corners the `image_references` query gives
  (`MainWindowImageReferences.cpp`, `view/ImageReferenceView.cpp`).
  Their commands are undo steps that recompute nothing.
- **Annotations** (mitcad#702, grown from the measurement objects of
  mitcad#649; `core/model/src/document/annotations.rs`,
  `api/measurements.rs`, `api/annotations.rs`, `iso286.rs`): dimensions,
  leader notes, datum labels and feature control frames kept in the
  definition state with a name, a colour, a light bulb, a group, a kind
  (distance, angle, offset, axis or centre distance, radius, diameter;
  the leaders note, datum and frame: a letter, an ISO 1101 tolerance whose
  rules Rust checks, a frame under a dimension), one or two picks as the
  `measure` query's selections (topological names with their
  occurrences), the
  plane they lie in (a direction or a picked plane), the dimension line's
  and the text's places relative to the picks, a format and a tolerance
  (ISO 286 fit codes computed in Rust); their commands are undo steps
  that recompute nothing. Each is measured at the marker whenever the
  `annotations` query asks (the kernel's measures and
  `api::measure_axes`), and laid out in Rust in its plane: extension and
  dimension lines, arcs, arrows, the text's place and direction, the
  value in words ISO 129 style; a pick that is gone gives an error
  instead, and the annotation stays. Since mitcad#894 the texts' height is
  the design's, in millimetres (set, or Auto from the ISO 3098 series by
  the size of what is annotated), a new annotation's colour is `auto`
  (the background tone's annotation colour; the old rotating defaults read
  as Auto), and a length's automatic dimension line lies outside its
  bodies' extent in its plane, stacked outside parallel lines, all worked
  out in Rust from the picks' bodies' boxes and planar faces kept with
  the measured picks. The app projects the layout, keeps
  the text readable within its plane (ISO 129-1), sizes the text and
  arrowheads by the model's height on screen and paints it with QPainter
  (`app/view/MeasurementDrawing`, `MeasurementOverlay`, which also takes
  clicks and drags on the texts and lines), in snapshots rendered
  offscreen at a larger size (`OcctViewer::snapshot`, OCCT's
  `V3d_View::ToPixMap`; all, or one group) and over Render Image's image;
  the Annotations window lists and edits them (`app/view/AnnotationsWindow`).
  Since mitcad#902 hole callouts (`api/hole_callouts.rs`) are annotations
  whose text the model writes from the hole's feature with the exported
  drawing's engine (`compose::hole_callout`, in ISO 129-1's symbols);
  leaders report which end of their text the leader meets and end in a
  dot on a face; a plane's extension line starts at the picked face's
  own outline (`MeasuredPicks::outlines`); default places are in text
  heights. The app paints the reference look: the bundled technical
  lettering (an ISO 3098 type B style font, a file loaded at run time, LGPL-3.0 with the font
  exception), crisp lines on the pixel grid, arrowheads in the
  annotation's plane, the text between a floor and a ceiling on screen,
  selection and hover as a recolouring only, Auto's ink chosen over what
  lies behind (the view's frame grabbed once the camera rests,
  `MeasurementBackdrop`; lines and terminators piece by piece, text and
  frames one ink each; no casing or halo), and
  pictures in the tone of their own background.
- **Drawings in the design** (mitcad#707, `core/model/src/drawing/def.rs`,
  `document/drawings.rs`, `api/drawings.rs`): drawings kept in the
  definition state in the root's and components' Drawings folders, each
  sheets of views (base, projected, section, detail; breaks on any) with
  their places, scales, styles and subjects, and nothing computed. Their
  commands are undo steps that recompute nothing; the design file and the
  comparison of versions keep and list them; a component's drawings are
  copied with it (`DocState::copy_drawings` in `copy_component_mapped`) and
  go with it. The `drawing` query works a sheet out at the marker: each
  base view's subject in the drawing's component's coordinates, projected
  by `Kernel::project_view` (no kernel method of their own) and kept in
  the document's projection cache (`mitcad_model_drawing::ProjectionCache`, by shape
  versions, placements and the view's spec); details and breaks are cut
  in Rust in the view's plane (`core/model-drawing/src/clip.rs`), shaded views from
  `Kernel::triangle_mesh`; the sheet is drawn by the export's
  `compose::render` (`core/model-drawing`). A view's place is where the component's
  origin lies on the sheet, so views in line share a coordinate. The
  application paints the query's groups in a sheet view of its own
  ([app/COMMANDS.md](../app/COMMANDS.md), "Drawings"). Annotations on
  views (phase 2, stored in `core/model/src/drawing/annotate_def.rs`,
  laid out by `core/model-drawing/src/annotate.rs`,
  `api/drawing_annotations.rs`)
  keep picks of model topology (a body, its occurrence, an edge or a round
  face by its topological name): a click is matched in Rust against the
  bodies' named edges and faces as the view projects them
  (`Kernel::edge_geometries`, `Kernel::face_geometries`, kept per shape
  version in the same cache), and each pick is found again by its name
  whenever the sheet is drawn, so the annotation follows the model or is
  marked broken. Their values are written by the model annotations'
  engine (`api/annotations.rs`, mitcad#702) and laid out on the sheet in
  paper millimetres relative to their picks. A view sees what it shows
  (mitcad#717): a section what lies behind its plane, and the planar faces
  its plane cuts (their cut edges, matched in Rust against the projection's
  cut faces); a detail and a broken view the parts of edges they show; of
  picks as near, the one the projection draws visible, then the one nearest
  the eye; a pictorial view measures aligned dimensions in 3D and refuses
  foreshortened ones. Tables (phase 3, `drawing/tables_def.rs`,
  `core/model-drawing/src/tables.rs`,
  `api/drawing_tables.rs`) are annotations of a view too: a hole table
  (`Kernel::round_holes` of the bodies it shows, seen end on) and a parts
  list (the view's occurrences counted per component, the `parts_list`
  rows' and properties' words) are worked out whenever the sheet is drawn
  and drawn in Rust; balloons take their numbers from the sheet's parts
  lists. Sheet templates (phase 4, `drawing/template_def.rs`,
  `core/model-drawing/src/template.rs`,
  `api/drawing_templates.rs`) are data: a frame, a title block's lines
  and fields with names in braces, a revision table; built in (JSON files
  compiled in), or files in the user's and the project's library folders
  (beside the materials and print profiles), read and written by the
  model; a sheet keeps a copy of its template, and its fields are filled
  in from the design (properties, materials, the mass from the bodies'
  volumes) whenever it is drawn. A drawing's revisions are rows of the
  drawing itself. A cut sheet (mitcad#853, `drawing/cut_sheet_def.rs`,
  `core/model-drawing/src/cut_sheet.rs`, `api/cut_sheets.rs`) is a
  drawing of its own kind: its parts are bodies seen along a planar
  face's normal at 1:1, their outlines the bodies' silhouettes worked out
  in Rust from the kernel's hidden-line projection (the regions it
  bounds classified by segment crossings through the body), written as
  DXF and SVG for cutting; B-spline edges come through the projection
  as B-splines (`ViewCurve::Spline`) and are written as the curves they
  are (`core/model-drawing/src/bspline.rs`).
- **Parameters as CSV** (mitcad#666, `core/model/src/parameters_csv.rs`,
  `document/parameters_csv.rs`, `api/parameters_csv.rs`): MitCad's
  parameter table and the `.f3d` application's parameter table CSV are
  read and written in Rust; the queries and the command carry the CSV
  text, and the app only reads and writes the file (Change Parameters'
  Import and Export). An import works on a copy of the definition state
  row by row in dependency order, as the `.f3d` import does, and is one
  undo step; its preview runs the same rows on a copy that is dropped.
- **Components** (`core/model/src/assembly.rs`): structure is in the
  definition state, not on the timeline, though features can create
  components. Occurrences belong to a component, so sub-assemblies are
  shared and world placements are paths from the root. One timeline;
  occurrence moves on it are placement changes. A feature refers only to
  geometry of its own component, except joints (below) and a sketch's
  plane and projections, which may name another component's geometry
  through an `OccurrenceLink` (mitcad#100, `core/model/src/links.rs`):
  the occurrence paths from the root to both components, resolved at the
  sketch's point of the timeline (`placement(target)⁻¹ ·
  placement(source)`; the cache keys on those placements and the other
  component's bodies, `Read::Placement`, `Read::BodyIn`). The `.f3d` import
  makes them for sketches on other components' faces and for linked
  sketch curves that follow one edge of the replay (mitcad#225,
  `core/import-replay/src/linked.rs`). A linked
  external component is read-only; its bodies are stored in a base
  feature refreshed on open. Since a feature's component is only its
  `FeatureEntry::component`, features made in the root can later move
  into a new component placed at the identity (mitcad#171,
  `document/move_features.rs`): their uids and topological names stay,
  the occurrence paths that reach their geometry (links, joint origins)
  take the new occurrence, and which features must go together follows
  from what the last recompute read and changed. Bodies alone move into
  an existing component at a point of the timeline by a `body_transfer`
  feature of the destination (mitcad#465, `features/body_transfer.rs`):
  the same body ids, converted once into the destination's coordinates
  (by the placements there along an `OccurrenceLink`, or a fixed rigid
  transform an import keeps), owned by the destination from then on
  (`FeatureOutput::owners`).
- **Joints** (mitcad#55, `core/model/src/joints.rs`, phases 1 to 3):
  timeline features of a component (`joint`, `as_built_joint`,
  `joint_origin`, `rigid_group`) that connect its occurrences. The one
  cross-component reference: a joint origin is an occurrence path from
  the joint's component plus a `GeomRef` resolved in the component that
  path ends in (that component's bodies, sketches and construction
  geometry at that point of the timeline) and moved into the joint's
  component by the path's placements. A joint's evaluation only reads its
  values (cached); recompute then applies it after the cache, like
  `place` applies placement changes, because it reads other components'
  bodies and the placements so far: it resolves its frames and the joint
  solver (`mitcad_solver::RigidSystem`, `core/solver/src/rigid.rs`, on
  the sketch solver's iteration, factorization and rank analysis) solves
  it with the joints before it that share moving occurrences, a body per
  unit (an occurrence with what its rigid groups join to it; grounded
  ones fixed), moving them as little as possible. The units that moved
  get `PlacementChange::Set`s of the joint feature; the occurrences'
  stored transforms stay the starting placements. Contradicting joints
  fail the feature. Motions are driven by a stored position (else the
  rest value) or free within limits; degrees of freedom are the rank of
  the joint equations; a drag query and command move occurrences while
  the joints hold. A joint moves only top-level occurrences of its
  component (a sub-assembly as a whole). Commands and queries:
  [commands.md](../core/model-api/src/commands.md) ("Joints"). In the
  application (phase 3) the ASSEMBLE group's panels add and edit them, a
  preview reports the placements it changes so that panels show the
  moved bodies, components are dragged in the view through the drag
  query, and joints are animated as previews of driven positions
  ([app/COMMANDS.md](../app/COMMANDS.md), "Joints"). The `.f3d` import
  (phase 4, `core/import-joints/src/joints.rs`) maps the file's joints to these
  features (*Imports with history* below).
- **Assembly constraints** (mitcad#148, `core/model/src/constraints.rs`,
  `features/constraint_set.rs`): a `constraint_set` feature's rows (align,
  angle, center, tangent, insert, axis insert) each hold geometry of one
  occurrence against another's, referenced as joint origins, with only
  some of a joint's equations (`mitcad_solver::RigidConstraint`, partial
  equations in frame b's coordinates, optionally with a value between
  limits). Recompute treats the rows like joints (`joints::apply`): they
  are `ActiveJoint`s with their parts, solved in the same runs, rank
  analysis, degrees of freedom and drags; conflicts name rows
  (`Constraint1 row 2 (insert)`). The application's Constraint command
  edits the rows in one panel, the type inferred from the picks by the
  `constraint_row` query ([commands.md](../core/model-api/src/commands.md),
  "Assembly constraints").
- **Motion studies and contact sets** (mitcad#454, `core/model/src/motion.rs`,
  `contact.rs`, `document/motion.rs`): definition state beside the
  timeline, saved in the design file. A study keys joints' free motions
  along steps; a contact set names occurrences whose bodies stop each
  other. Motions that should stop at contact (Drive Joint, drags,
  `play_study` and the previews of a study's playback and of Animate
  Joint's frames) are sampled from where they start, each sample the
  state with the joints driven there and recomputed (`sweep_joints`; a
  drag runs the joint solver's drag instead), checked pair by pair
  (boxes first, then `Kernel::overlap_volumes`, OCCT's fuzzy boolean
  common of the placed bodies, `geometry/src/motion_contact.cpp`), and
  the step before the first contact bisected; recompute itself never
  checks contact. The application plays a study in a panel of previews
  (`app/motion/MotionStudyDialog`), the document unchanged until Keep
  ([commands.md](../core/model-api/src/commands.md), "Motion studies and
  contact sets"). The `.solidcraft` import maps the file's onto them.
- **Motion links** (mitcad#466, `features/motion_link.rs`,
  `core/model/src/motion_links.rs`): `motion_link` timeline features that
  make one joint's free motion follow another's: a ratio, gears, a rack
  and pinion, a screw (sizes as values or radii of picked round geometry,
  found again on every recompute) or a cam (a table of the follower's
  values over a turn, worked out from the profile, the follower and its
  tip; the contact geometry adapted from SolidCraft,
  `third_party/solidcraft/cam.rs`). Recompute applies a link like a joint
  (`joints::apply`): its joints and every joint, constraint and link
  sharing moving occurrences with them are solved together, the joint
  solver holding one coupling row per link (`mitcad_solver::RigidCoupling`,
  linear or a periodic table); the rank, drags and the contact-stopped
  motions of motion studies take the links in, so linked joints move
  together. A follower whose driver is driven lets go of its own
  position; Drive Joint and studies take the positions of linked motions
  away, so the last driven one drives ([commands.md](../core/model-api/src/commands.md),
  "Motion links"). The application adds them with ASSEMBLE › Motion Link
  and lists them in each component's Motion Links folder; the
  `.solidcraft` import maps the file's.
- **Design policy** (mitcad#220, `core/model/src/policy.rs`,
  `document/policy.rs`): the root and each component have nine local
  settings (sketches, features, components; sketch attachment to and from
  other components, and of a component's first sketch, mitcad#293;
  references to and from them), in the definition state
  and the design file. The general defaults (the Hybrid preset,
  mitcad#421) allow everything; the Part and Assembly presets are the
  stricter choices. A new design (`Document::new_design`) has the root and
  an empty, active Component1 (named as any new component, mitcad#711),
  both with the general defaults, as have
  `Document::new`, new components and owners without a stored policy (a
  stored one keeps its values; the file format is unchanged). Every change a JSON
  command or preview makes goes through one check before it is computed
  (`apply_with`, `apply_move`, previews): the next state's violations are
  compared with the current ones, counted per owner, setting, object and
  other owner, and a change that adds one is refused with the reason.
  Existing violations stay and are reported (the `policy` query); a policy
  change modifies no content. Importers that call the model directly are
  not checked. Later move and reparent operations get the check by building
  their next state through `apply`/`apply_move`
  ([commands.md](../core/model-api/src/commands.md), "Design policy").
  The one redirection: a new component (New Component, a feature's
  new-component operation) that an owner's policy refuses is placed in the
  nearest owner above that takes components, at the same place, and the
  reply says so (`Document::component_parent_beside`, mitcad#292).
- **Plastic part features** (mitcad#459, `core/model/src/features/plastic.rs`):
  Boss, Snap Fit and Rest are built in the model from the kernel's
  extrusions, booleans and fillets; Lip and Groove from the band along a
  rim face's loop (`Kernel::rim_band`, `geometry/src/plastic.cpp`). A
  plastic rule is a timeline feature that changes no body; the features
  that name it read its parameters and its result (`Read::Output`), so
  an edit of the rule computes them again
  ([commands.md](../core/model-api/src/commands.md), "Plastic part
  features").
- **Gears** (mitcad#472, `core/model/src/features/gear.rs`): the teeth
  are worked out in the model (`gear_profile.rs`: involute flanks fitted
  as B-splines, the generating rack's root fillet and undercut, ring
  spaces, racks, Tredgold bevel sections) and handed to the kernel as
  loops of named curves: `Kernel::gear_sweep` sweeps them straight
  (a prism) or along a screw motion in steps (helical, herringbone; faces
  built from each curve's poles moved along the motion and sewn,
  `geometry/src/gear.cpp`), `Kernel::gear_cone` rules a bevel gear's
  section to its apex within its blank of revolution. Bores, hubs and
  grooves are further sweeps joined or cut. A gear meshed with another
  reads that gear's definition and parameters (and its result,
  `Read::Output`) to place itself; its result keeps its frame as the
  mount `gear`, which `link_gears` reads to couple the gears' joints with
  a motion link ([commands.md](../core/model-api/src/commands.md),
  "Gears").
- **Pulleys** (mitcad#562, `core/model/src/features/pulley.rs`): the
  grooves of the timing belt profiles are worked out in the model
  (`pulley_profile.rs`: lines and arcs of a fixed groove per profile) and
  swept by `Kernel::gear_sweep` as a gear's teeth are; flanges, hub, bore
  and set screws are further sweeps joined or cut. A pulley with
  `belt_with` reads the other pulley's definition and parameters (and its
  result, `Read::Output`) to place itself; the belt's length is a derived
  value of the `pulley_values` query. Its result keeps its frame as the
  mount `bore` ([commands.md](../core/model-api/src/commands.md),
  "Pulleys").
- **Printable bearings** (mitcad#577,
  `core/model/src/features/printable_bearing.rs`): no kernel method of
  their own. The model works out the bearing's section (one function of
  the distance from the pitch circle bounds both races, so they mirror
  each other), builds the races and the first element as regions in code
  revolved by `Kernel::revolve` (an airsoft ball a sphere of
  `Kernel::primitive`), places the other elements by `transform_shape`
  with instance names, and cuts the cage's pockets and a raceway in a
  picked body with booleans. The printability checks are part of the
  section's layout, so the panel's `bearing_values` and the evaluation
  give the same errors. A feature may name the component its
  new-component operation makes (`FeatureInfo::component_name`, used by
  the document when it makes the component); an airsoft bearing's balls
  are purchased rows of `parts_list` (`api/bearings.rs`), not components
  ([commands.md](../core/model-api/src/commands.md), "Printable
  bearings"). The seat on a shaft or in a hole is the shared seat
  resolver's (mitcad#578).
- **Seats on shafts** (mitcad#578, `core/model/src/features/seat.rs`): a
  pulley, a gear with a bore and the printable bearing take a `seat`
  instead of a plane: one resolver turns a circular edge or a cylindrical
  face (`Kernel::edge_geometry`, `Kernel::face_cylinder`; another
  component's body read in its component and moved by the seat's
  occurrence link, as a sweep's `body_links`) into the end measured from,
  the axis and the diameter, which the part's frame and its plain bore
  follow. The app's panels place a part on a shaft through Place On
  (`app/commands/PlaceInputs.hpp`, mitcad#656): `at.axis` resolved as a
  seat, `seat` with its link for another component's shaft.
- **Placement by picked geometry** (mitcad#656,
  `core/model/src/features/place_at.rs`): pulleys, gears, bearings, coils,
  primitives and holes take `at`, one placement of three forms (on an
  axis: a seat, or an axis of its own; at a point with its sketch's or a
  plane's normal; on a face at offsets from two edges), kept as references
  and resolved into a frame by one resolver at each recompute, so the part
  follows what it was placed by. A gear placed so and meshed is only
  turned by its mesh. `features/drives.rs` works out Gear Pair (modules
  and tooth counts that fit two axes: exact, by profile shifts, by a helix
  angle) and Belt Drive (tooth counts, catalogue belts and their tension
  travel); `api/drives.rs` serves them as queries and makes both parts in
  one undo step. The app's shared input is `app/commands/PlaceInputs.hpp`;
  Gear Pair and Belt Drive are `app/commands/DriveCommands.cpp`. The
  panels show distances as dimensions in the view, a value box on each
  handle (`Manipulator::valueBox`).
- **Extrude's automatic operation** (mitcad#289, mitcad#575,
  `core/model/src/document/auto_operation.rs`): a preview asked for it
  evaluates the extrusion as a new body, tries its tool against the
  visible bodies of its owning component (boxes first, then
  `Kernel::tool_contacts`, Join's contact test) and reports Join, Cut or
  New Body with the bodies. Added previews consider bodies at the marker;
  edited previews compute a temporary state strictly before the edited
  feature, excluding its result and all later geometry regardless of the
  marker. The report and retained geometry remain the supplied definition's
  ordinary preview, including any Cut scope, without changing the document
  or its undo history. The app decides when to take the answer into the
  panel ([commands.md](../core/model-api/src/commands.md),
  "Extrude's automatic operation"). Contact preparation (mitcad#638) retains
  geometry-only face bounds, edge counts and eligible planes on the immutable
  `Shape`, keyed also by contact tolerance. A changed BRep, orientation or location has a
  fresh index; display mesh quality does not affect it. Each query still
  evaluates its current tool and the model's current visible participants;
  no distance, operation or participant answer is retained. For planar
  faces with many edges, up to four tool vertices can prove contact using
  plane distance and OCCT's trimmed-face classification. A point in a hole,
  on a trim boundary or away from that plane uses the existing checks.
- **Extrude target conversion** (mitcad#570): only Cut activates its
  `cut_scope`; other solid operations may retain that scope as dormant
  editor data, with no geometry or dependency effect. A new boss stores
  `initialize_on_cut: true` in that scope to distinguish automatic Join
  participants from explicit local body choices. Its editor initializes
  targets and snapshots visibility when the user chooses Cut. Previously
  chosen scopes retain exclusions through operation changes and file
  round trips. Legacy Cuts and explicit local participants remain local;
  opening an editor never expands targets.
- **Extrude of a body face** (mitcad#535, `features/extrude.rs`): an
  extrusion's profile may be a planar face of a body (`profile_face`)
  instead of sketch regions: the face is found by its topological name on
  every recompute and `Kernel::extrude_face` sweeps a copy of it, holes
  kept, along its outward normal (`geometry/src/face_extrude.cpp`); the
  side faces are named after the face's edges, the caps after the face
  ([commands.md](../core/model-api/src/commands.md), "extrude"). A
  revolution takes one the same way (mitcad#539, `features/revolve.rs`,
  `Kernel::revolve_face`, `geometry/src/face_revolve.cpp`; the FreeCAD
  import's face profiles), by angles only.
- **BUILD** (mitcad#375, `core/model/src/build/` for what the design
  file keeps, the planning in the crate `core/model-build/`): the library parts the
  BUILD tab places are kept once as the design's **library sources**
  (`library_sources`: the library record, the design's text, inside a
  single file or in a project's store, and the manifest's `build`
  block). A placed part is a component whose first body is a
  `derived_part` (`features/derived_part.rs`): the source's design
  evaluated inside the feature with its row and values (a recompute of
  its own, with the document's kernel), its visible bodies copied and
  renamed `<F>:part(<source face>)` (`Kernel::rename_faces`), its ends
  cut at their long points (the plane through the section's outer edge
  on the long side: the length is the outer length) by booleans with a
  box (`<F>:end_a`, `<F>:end_b`), and its `mount:<name>` joint origins
  published as frames that joints and other features name
  (`{"mount": "F7:mount(end_b)"}`,
  `Resolver::mount`). A `bound_length` measures a ray from one part's
  frame to another's face or frame and sets its reference parameter as
  driven dimensions do, before the derived part that takes it as its
  length. The rules that place, mitre, connect, size panels and list cuts
  plan everything against the document and make it in one undo step;
  components record what BUILD made (`build`). They work in the **BUILD
  host** (mitcad#394, `Document::build_host`): the active component when
  its policy takes components, else the nearest one up that does (in a
  new design Component1 itself, mitcad#421), so the design policy never
  refuses where a part goes; bound lengths count as assembly features
  there, as joints do. Everything else treats
  them as ordinary components, features, joints and parameters
  ([commands.md](../core/model-api/src/commands.md), "BUILD: profile
  assemblies").
- **Timber frames** (mitcad#479, `core/model-build/src/frames.rs`): a
  wall, floor or roof frame is a set of ordinary BUILD members (grounded
  members by two points, with plumb end cuts and `birdsmouth` cuts in
  their derived parts) whose definition the lead member's record keeps
  (`framing`; each member's `frame` and `member`, its place in the
  frame). Laying a frame out is geometry from definitions only: a wall
  reads the walls it butts against and those that butt against it, a
  roof its two walls; an edit plans the frame and the frames that depend
  on it, keeps the members whose place stays (`replace_member`), makes
  and removes the others in one `apply`, drives the fasteners through
  `build_connect` and merges it all into one undo step (`merge_undo`)
  ([commands.md](../core/model-api/src/commands.md), "BUILD: timber
  frames"). Since mitcad#522 each frame is a component of its own in the
  BUILD host (`framing.component`), its definition in that component's
  coordinates: its members are made there, and its fasteners are driven
  with that component as the BUILD host (`Document::set_build_host_override`)
  so their joints are inside it; frames saved before stay flat. The
  junction search reads a timber member's material as its box clipped by
  its end cuts (mitcad#521, `build/junctions.rs`), so cut ends meet sides
  by their cut faces and boxes overlapping where no material is make no
  junction. A wall's plate longer than its timber's longest is laid out
  as pieces over the studs (mitcad#536, `Splicing` in `frames.rs`): the
  studs are planned first, the plates' joints chosen over them (a backing
  stud added where none is in reach), and the pieces are ordinary members.
- **Timber fastener seating** (mitcad#512, `build/wood.rs`): the library's
  `wood.seat` selects a head-top or bearing frame. Legacy axis-aligned
  wood screws and nails infer the head-top plane from evaluated body
  bounds; pan/hex/washer heads keep the bearing frame. A joint override
  moves only the fastener's own frame, leaving the member's entry frame
  fixed. Swap recomputes it from the replacement's row. Boards are not
  machined for the intentional overlap; `build_parts.seating` reports
  physical head-top and tip distances for inspection and UI acceptance.
- **Boundary Fill** (mitcad#457, `features/boundary_fill.rs`): a
  `boundary_fill` feature fills the cells its tool bodies enclose, each
  picked by a point inside it (a cell: inside every tool that holds the
  point, outside the others). `Kernel::boundary_cells` divides space by
  the tools' faces once (OCCT's `BOPAlgo_CellsBuilder`,
  `geometry/src/boundary_fill.cpp`) and gives each point's cell with the
  tools that hold it; the cells become new bodies or one tool that joins,
  cuts or intersects participants as an extrusion's does
  ([commands.md](../core/model-api/src/commands.md), "Boundary Fill").
- **Surface modelling** (mitcad#455, beside Patch, Stitch and Thicken of
  mitcad#456): surface (sheet) bodies are ordinary bodies whose shape is
  open shells; `Kernel::body_kind` tells them from solids. Extrude,
  Revolve, Sweep and Loft make them with `surface` set
  (`features/surface_sweeps.rs`): the walls of profiles are the solid
  feature's result without its caps (`Kernel::open_faces`), so the solid
  feature's options and names apply, and sketch curves are swept by the
  kernel's sheet methods (`geometry/src/surface_sweeps.cpp`). The
  families of surface features that change surface bodies follow the
  same pattern: a feature module, a block of `Kernel` methods with a
  default `Unsupported`, and a geometry file of their own
  (`surface_offsets.cpp`, `surface_edits.cpp`, `surface_fills.cpp`;
  [commands.md](../core/model-api/src/commands.md), "Surface
  modelling"); the application has them in the SOLID tab's SURFACE group.
  Free edges (of one face only) have no names: features take them as
  openings of the body, narrowed by faces and points
  (`features/sheet_edges.rs`).
- **Properties** (mitcad#496, `core/model-base/src/properties.rs`,
  `document/properties.rs`): typed key/value properties in named sets on
  components (the definition's), occurrences (their own values over their
  component's) and bodies, with the design's own types (allowed values,
  unit, default). The `PropertyStore` is part of the definition state
  (each owner's sets shared between undo steps until changed) and the
  design file (`property_types`, `properties`); its commands are undo
  steps that recompute nothing. Values keep canonical units and their
  source (`user`, an import, `library`, `computed`); computed values
  (volume, area, mass, a BUILD member's length) and a library part's data
  are worked out at query time and never stored, so a recompute never
  changes the file. Importers add theirs with `Document::import_properties`
  (one step, merged into the import's); copies of components take theirs
  along ([commands.md](../core/model-api/src/commands.md), "Properties").
- **Reorient Design** (mitcad#502, `core/model/src/reorient.rs`,
  `document/reorient.rs`): the whole design turned by quarter turns as a
  rewrite of its definition, every component but linked ones in its own
  coordinates. A turn is a signed permutation of the axes, so nothing is
  rounded. References and paths turn wherever a definition holds them
  through their JSON form (`geom_ref::to_turned_value`: an origin datum
  becomes the one the turn takes it onto, else the fixed plane or axis
  there), positions and directions outside references per feature type,
  sketches on origin planes get their own frame, sketches whose plane's
  frame follows the model axes get one from the turned design's
  evaluation, base features a Move, occurrences `R · T · R⁻¹`. The
  command computes the result and refuses it unless every body and
  occurrence is where the turn takes it
  ([commands.md](../core/model-api/src/commands.md), "Reorient Design").

### Units

The default is metric wherever a unit is chosen: document (mm), import and
export units, thread and hole tables (ISO first), materials and mass
properties, grid and settings. Inches only by choice. An imported file's
own units are kept.

## Geometry kernel

The `Kernel` trait (`core/model-base/src/kernel.rs`) is grouped by feature
family. New methods default to `Err(Unsupported)`, so the mock kernel
implements only what tests use. Bridges are split per family
(`core/ffi/src/kernel/<family>.rs`, `geometry/src/<family>.cpp`) and share
the `Shape` type.

- Inputs are plain structures: profile regions as loops of named segments
  (`Line`, `Arc`, `Circle`, `Ellipse`, `EllipseArc`, `BSpline`) plus a
  `SketchFrame`. Results are `Shape` handles whose faces carry names.
- Query results are kernel-independent descriptions: a planar face's
  origin is the projection of the model origin; a surface of revolution's
  axis points along its largest component.
- Volumes and areas are integrated per face, span by span with fixed
  Gauss rules (`geometry/analysis/src/face_integral.cpp`, mitcad#140). The
  integral over the face's domain in the parameter space becomes one along
  its boundary curves (Green's theorem, as in OCCT's `BRepGProp`) of an
  integral across the surface. Both are split where the integrand is not
  smooth or a piece would be too long for a fixed rule: across the surface
  at its knots (B-spline surfaces, the curves of extrusions and
  revolutions, the basis of offsets) and every eighth of a turn of an angle
  parameter; along a curve at its knots, where it crosses a knot line of
  the surface, and the same angle steps. Each piece gets Gauss points exact
  for a polynomial piece's second moments (8 for cubics), more for
  rational and offset geometry, and rational spans are taken in quarters
  (an area's square root has branch points near them). The number of
  points depends on the geometry only, so a face always measures the
  same; faces of every kind come within 1e-9 of their exact values in the
  tests. OCCT's rules do not follow spans: the fixed one takes at most 61
  points across a face and along a boundary curve (a hole bounded by a
  cubic of 132 spans, an intersection curve stored in a file, took 141 mm²
  off its face, mitcad#139), and the adaptive one never refines across the
  face (its error estimate there starts at zero and is never set) and
  drops one span of a boundary curve of more than 1953 spans (a stored
  thread's crest bounded by helices of 2574 spans lost 1.4 % of its area).
  Where a boundary curve ends short of the vertex it shares with the next
  one, a gap the vertex's tolerance bridges in a valid shape, the loop in
  the parameter space is closed by a straight segment (mitcad#325: a
  fillet's blend face whose curve stopped 0.36 mm short of a vertex of
  0.74 mm tolerance measured 4.38 mm² instead of 6.64, and the body 3.5 mm³
  off the file's). Only FreeCAD's reference check measures with OCCT's fixed rule
  (`fixed_point_properties`), as FreeCAD does. `Shape` caches its measured properties and bounding box
  behind a lock. Operations may read the same input shapes on several
  threads at once (the `.f3d` import's workers, mitcad#95): they never
  modify their inputs, and those whose OCCT algorithms write into their
  inputs' sub-shapes work on copies (below). A shape must still not be
  triangulated while another thread computes with it (OCCT stores the
  triangulation in its faces).
- Boolean results get coplanar faces and collinear edges merged, unless
  the merge breaks a result OCCT's checker accepted.
- Local booleans (`geometry/src/local_boolean.hpp`, modelled threads):
  OCCT's booleans rebuild the whole shell of a solid whose faces they
  split, and its checker visits every face, both quadratic in the faces.
  A small tool on a large body is fused or cut with only the body's faces
  whose boxes reach the tool's; the pieces are classified by the faces
  around their section edges (else by a point), the shell is the far
  faces with the kept pieces, and the checker sees only the faces that
  changed. Anything unusual (several solids or shells, a result that
  falls apart, a piece that cannot be classified) runs the ordinary
  boolean. Several tools, each one solid and with boxes apart from each
  other's, go into one local boolean (`local_boolean_all`), else each in
  turn. A modelled thread of several faces (mitcad#843,
  `geometry/src/thread_options.hpp`) reads every face from the body it is
  given; the tools of the faces whose reach (a box round all their tools
  can change) meets no other face's are built at once on OCCT's thread
  pool, each job an operation of its own under the caller's cancel source
  and from shapes of its own, and applied stage by stage (bands,
  chamfers, beyond the crest, grooves), a stage's tools in one local
  boolean; faces whose reaches meet follow one by one, each read from the
  body the one before left. `MITCAD_THREAD_SEQUENTIAL=1` threads every
  face so, `MITCAD_THREAD_PARALLEL=0` builds the tools on the calling
  thread, `MITCAD_THREAD_MERGE=0` applies them one by one; with
  `MITCAD_BOOLEAN_TRACE` each thread prints `thread-trace:` lines (per
  face its tools', booleans' and checks' time, per thread its phases).
- Booleans on perforated bodies (`geometry/src/far_features.hpp`,
  mitcad#72): OCCT's boolean, the face merge after it and its checker go
  through every wire of a face they touch, some once per wire or per pair
  of wires, so a small cut into a plate with thousands of holes took
  minutes. A single target's holes away from the tool (far faces bounded
  by whole inner wires of other faces, such as the wall of a hole) are
  taken out first; the boolean, the merge and the checker run on the rest
  (a closed solid with those holes filled), and the holes go back into
  the faces that took their faces' place. Holes the merge could change
  stay (a neighbour in the same surface, a vertex between two edges along
  the same faces), unless the body is itself a merged boolean result
  (`Shape::unified`). A cut with many solids apart from each other (a
  pattern's copies) runs region by region, about 300 solids at a time,
  each cut seeing only the holes near it; when a face of the target splits
  into pieces or the target falls apart, it is one cut after all, so that
  the pieces' numbers are those of one cut. Results are the same faces,
  edges, names and volumes as without (`geometry/tests/
  test_far_features.cpp`); `MITCAD_NO_FAR_FEATURES=1` turns both off for
  comparisons.
- The material a near copy of a body lacks
  (`geometry/include/mitcad/geometry/removed.hpp`, mitcad#85; the import's
  history-based hole guess: a replayed body minus the body stored for its
  next state). Where the faces of two bodies nearly coincide, OCCT's
  boolean walks along every such pair of free-form faces, seconds each,
  and its result is often wrong. The cut runs only in boxes around the
  faces where the two differ by more than a slack (new faces with a point
  farther than that from the other body's faces; faces gone with every
  point that far): both bodies are cut down to the boxes first. A piece
  that reaches the side of its box grows the boxes; boxes taking half the
  body's run the whole cut.
- The join of a body with a near copy (same header, mitcad#88; a
  mirror's `combine` of a nearly symmetric body with its image): where at
  most half the faces of each differ from the other's by more than 0.1 mm,
  only what the copy adds there is joined to the body (the copy minus the
  body in boxes around those faces, as above, slivers between
  near-coincident faces left out); no such face, the body itself. Other
  copies are joined whole.
- Joins with several bodies near the tool first find the bodies it
  touches (`touches` in `geometry/src/boolean.cpp`) and fuse only those.
- A union of several bodies (`unite`, a `combine` join with several
  tools; mitcad#400) joins the bodies whose faces are all elementary
  (planes, cylinders, cones, spheres, tori) first and the bodies with
  free-form faces then with their union, its faces merged. OCCT's boolean
  intersects every pair of faces that meet: a spiral stair's slabs and
  treads, whose rims a sketch's lines split into hundreds of faces on one
  cylinder, met its helical stringers lying on that cylinder piece by
  piece (5 664 face pairs for seven slabs, eight treads and two stringers
  in `geometry/tests/test_helical_joins.cpp`, 309 joined first). OCCT's
  face merge builds a ring of cylinder pieces cut through from end to end
  (the stringer's band along the rim, the treads rising on it) as one face
  without a seam (port patch 0680); before, it built such rings with
  seams, the result was invalid, and the union was computed again
  without the merge, keeping every piece. The `interference` query's
  booleans of pairs of bodies run on all cores, as the model's do (such a
  slab against the stringer took 2 s on one).
- Large bodies (mitcad#181): OCCT's checker runs on OCCT's thread pool
  (its jobs count their failed allocations as the calling thread's,
  mitcad#132), where it checks whole bodies (operation results, the
  bodies built from files); on one thread checking a stored body of
  23 000 faces took 37 s. A cut whose tool passes a body by (no face of
  it modified or removed) is not checked: its result is the body as it
  was, which the caller leaves alone (before, such a body that the
  checker rejected, a stored body invalid already, failed the whole cut).
  Booleans do not check their solids for being inverted
  (`SetCheckInverted(false)`: the check built a point classifier of all
  of a solid's faces on one thread, every operation). The `.f3d` import
  heals a stored body of 256 faces or more only where it differs from
  its build in the history state before (`brep::Healed`; that base is
  fixed by the file, not by the order of the builds, mitcad#243;
  `core/import/docs/f3d-performance.md`). A shape of 64 faces or more finds
  its faces by a reference through an index of its face names, made the
  first time it is asked (`Shape::find_faces`); before, each lookup read
  every face's names, and finding the faces of a large body one by one
  was quadratic in its faces.
  Measuring the distance of a tool face to a face with thousands of holes
  classifies points against all of its edges for every pair, so when the
  target's faces near a tool solid have 1,000 edges or more, up to 32 of
  the solid's vertices are classified first: one inside the target, or on
  its boundary within 1e-6 (measured), shows the touch; otherwise the
  faces are measured as before, so the answer is the same (mitcad#77).
- Patterns leave out of a cut (and of Adjust's join, which skips copies
  that reach no body) the groups of copies whose boxes stay more than
  0.1 mm from the boxes of all participants, before the copies are
  united: they change no body, and a large pattern of holes over a round
  plate had most of its copies off the plate. Copies whose boxes come
  that near each other are kept or left out together.
- A pattern of several features (mitcad#855,
  `Copying::together` in `core/model/src/features/pattern.rs`) applies
  a run of consecutive features that all join, or all cut, the same one
  body in one boolean: each feature's copies united as before, the
  features' tools as one compound. The cost of a join or cut follows the
  target, not the tool (a 6-face tool into a body of 3 000 faces took
  1.5 s), and a pattern of a dozen join features made a dozen such
  joins. The run is taken only where it is the same as the features one
  after another: the boxes of the copies of different features stay more
  than 0.1 mm apart, so each feature acts on a part of the body the
  others do not touch; it made no body; every feature's copies have
  faces in the result; and a face of the body split into pieces (named
  `#k` in their order) has one feature's copies near its box only, so the
  pieces are numbered as that feature alone numbers them. Otherwise the
  body is restored and the rest of the pattern goes one feature by one,
  so a pattern wastes at most one boolean. The last condition is foreseen
  from boxes before the boolean: a run ends before a feature whose copies
  come near a face of the body (`Kernel::face_boxes`) that another
  feature's copies come near too, unless all those copies stand inside
  the face (a flat face whose box holds them across it: bosses and holes
  on one face). `MITCAD_BOOLEAN_TRACE` says for each run why it was not
  taken or kept.
  `MITCAD_NO_PATTERN_RUNS=1` turns it off, for comparisons. A cut run
  counts as many passes over the body as its features (the split pieces'
  memory, mitcad#408).
- A pattern's optimized compute option with fillets or chamfers among the
  features copies the faces they made instead of rounding at each copy:
  `Kernel::feature_changes` gives the material the features added and
  removed (`geometry/src/face_copy.cpp`, on `removed_material`), the
  copies are moved and joined or cut in one operation each, and
  `Kernel::supported_copies` cuts off the material of a copy over a part
  of its seam that no face of the body lies under (a rounding that fails
  on the 146th of 1,078 copies is never worked out again there).
- Transforms check their input once (`Shape::checked_valid`) instead of
  every copy: a rigid motion, mirror or uniform scale keeps a valid shape
  valid, and checking each of a pattern's copies was four fifths of their
  time. A copy that only renames faces (a tool a pattern rebuilt in
  place) shares the input's B-rep.
- Checks where a boolean changed the body (mitcad#121, `geometry/src/
  util.hpp`, `is_valid_where_changed`): a shape the checker found valid
  says so (`Shape::checked_valid`: boolean results, roundings and
  chamfers, the bodies built from files). A boolean whose targets are all
  such gives the checker only the result's faces it does not share with
  them and the faces that meet those at a vertex, and counts over the
  whole result that each shell is connected, holds no face twice and has
  one face on each side of each edge running along it in opposite
  directions. A face the result shares, and whatever only such faces use,
  is as it was checked (the non-destructive boolean leaves its inputs'
  sub-shapes alone, and the face merge changes only what the faces it
  merges use), so the answer is the checker's on the whole shape. Where
  the result is not solids of shells, any of that does not hold,
  or most faces changed, the whole shape is checked; a solid of several
  shells (a body with a void) is also checked as a solid, how its shells
  lie in each other (`BRepCheck_Solid`, mitcad#855). A solid of a shape
  found valid is valid (`geometry::solids`). On bodies of 5 000
  faces the whole check took seconds after every join and cut.
- A shape keeps its faces' kinds and areas once measured
  (`Shape::face_measures`, `face_infos` measures them on all cores), and
  its optimal bounding box is its faces' boxes found on all cores (the
  same box as OCCT's `BRepBndLib::AddOptimal`): the `.f3d` import asked
  for the faces of the same large body dozens of times an extrusion.
- A shape also keeps its edges' middles and curves once worked out
  (`edge_middles`, `edge_geometries`) and a box that holds each face
  (`face_boxes`, `Kernel::face_boxes`, on all cores; faces of offset,
  swept and other surfaces get unbounded boxes): the `.f3d` import's
  lookups of a face through a point pass over the faces whose box lies
  farther than the tolerance, instead of measuring the point's distance
  from each of a body's 23 000 faces (mitcad#119, [performance.md](performance.md)).
- **Operations must not modify their inputs.** Several OCCT algorithms
  write into input sub-shapes (p-curves, tolerances), which made a feature
  give a different B-rep depending on what ran before. Merged booleans,
  fillets, chamfers, shells, lofts from faces and shape healing therefore
  work on copies (`detail::InputCopy`, `detail::working_copy` in
  `geometry/src/history.hpp`). `MITCAD_CHECK_INPUTS=1` fingerprints inputs
  before and after every operation and reports changes on stderr
  (`geometry/include/mitcad/geometry/input_check.hpp`); geometry tests run
  with it.
- OCCT crashes become errors where possible (`catch_occt_crashes`; on
  Linux and macOS needs `OCC_CONVERT_SIGNALS`, defined in the root
  `CMakeLists.txt`; on macOS a fault is SIGSEGV or SIGBUS).
  Handlers are installed on the app's model worker thread, on its section
  worker's thread (mitcad#438), on its UI thread at start (`app/main.cpp`),
  in `mitcad-cli`'s `main` and on the import's threads. The analyses the app asks for run as operations too
  (mitcad#392): the analysis bridge (`core/cpp/bridge/analysis.cpp`) runs
  section, clip, measure, distance, interference and physical properties
  through `geometry::run_operation` (`guard.hpp`, the public form of
  `run`), so a crash in Section Analysis' booleans is an error: the body
  is shown whole and the panel shows the error. Section Analysis clips and
  sections the bodies shown together on OCCT's thread pool, each body its
  own operation there (`analysis_clip_all`, `analysis_section_all`,
  mitcad#438), and skips the booleans of bodies the plane does not reach
  (from their kept boxes); the calling thread's cancel source
  (`geometry::cancel_source`) reaches each task, and the tasks not started
  once it is set fail as cancelled. For Section Analysis off the UI
  thread, `analysis_detach_all` (`Kernel::detach_all`) detaches the bodies
  first, on the thread that owns them: a copy that shares no sub-shape
  with a body the plane may reach (`geometry::independent_copy`, with the
  same names, its box, its mesh faces' triangles), and of a body the plane
  passes only its kept box. The crash
  that was reported came from mesh bodies: OCCT's booleans need surfaces
  and read a null handle on a face without one, so the analysis sections
  and clips mesh bodies on their triangles instead
  (`geometry/analysis/src/mesh_section.cpp`).
  `MITCAD_TEST_OCCT_CRASH=<operation>` forces a crash (`cli.occt_crash`,
  `cli.occt_crash_section`, `geometry.guard_operation`,
  `tools/ui-section-crash-test.sh` with `clip` and `section`).
  On Linux and macOS OCCT's handlers are the process's; a crash outside
  an operation that catches it (`Standard_ErrorHandler::IsInTryBlock`)
  goes to the handler installed before them, the application's crash
  report (below), instead of OCCT's "no catch was found" exit
  (`geometry/src/guard.cpp`, `geometry.guard_*`). On Windows OCCT's
  handlers (the unhandled exception filter, the C runtime's signals) turn
  a fault into an exception only on a thread running an operation
  (`run` in `geometry/src/util.hpp`) and on OCCT's own threads while one
  runs; any other fault goes to the filter installed before them, the
  crash report, with the stack from the fault.
- C++ exceptions never cross the bridge (mitcad#937). Every
  `#[cxx::bridge]` with an `extern "C++"` block includes
  `core/cpp/bridge/trycatch.hpp` (`include!("bridge/trycatch.hpp")`; a
  new bridge does the same), which overrides cxx's
  `rust::behavior::trycatch`: the shim of each C++ function returning
  `Result` catches `Standard_Failure` (its message, or "OCCT exception
  <type>" without one), `std::exception` and anything else ("unknown C++
  exception"), and Rust gets an `Err` that becomes `KernelError::Failed`.
  cxx's own `trycatch` catches only `std::exception`, so an exception of
  another type, thrown by a bridge function that calls OCCT or the
  geometry's helpers outside `run_operation`, would have reached the
  shim's `noexcept` and ended the process. OCCT 8.0.1's `Standard_Failure`
  derives from `std::exception`, but the boundary does not rely on it.
  `core.bridge` (`test_exceptions_across_the_bridge`, through
  `core/ffi/src/kernel/exception_probe.rs`) throws each kind.
- Where OCCT cannot impose what a feature asks, the geometry builds the
  surfaces itself (`geometry/src/skin.hpp`: interpolation with end
  derivatives, Gordon surfaces). Lofts with end conditions or rails
  (`loft_skin.cpp`), asymmetric and curvature-continuous fillets
  (`blend.cpp`) and setback/blend corners (`corner.cpp`) are built so;
  free lofts stay OCCT's through-sections.
- Export: 3MF and STL are written by the model from
  `Kernel::triangle_mesh` (one closed, outward-facing mesh per body,
  placed by its occurrences). STEP, IGES, OBJ and BRep go through
  `Kernel::write_file` with each body's placements (`ExportBody::placements`;
  STEP as an assembly, the others as moved copies). USDZ is written by
  the model from `Kernel::face_meshes` (each face its own vertices with
  the surface's normals, in the order `Kernel::faces` lists them, so
  face appearances find their triangles): a mesh per placement under
  `Xform`s of the occurrences, materials from the appearances
  (`core/model/src/exchange/usdz.rs`, `core/usdz`). Drawings (mitcad#471,
  `export_drawing`): `Kernel::project_view` gives a view's edges with
  hidden lines removed (OCCT's `HLRBRep_Algo`, lines and circles exact,
  other curves as polylines; `HLRBRep_PolyAlgo` on a triangulated copy
  when asked or when the exact one fails, for mesh bodies, and for a
  drawing's views of bodies with free-form faces, where the exact one
  took half a minute a view (`ViewSpec::quick`, mitcad#962; its segments
  fitted back into lines and arcs, `docs/performance.md`) and a
  section's cut faces (Section Analysis' `analysis::clip` and
  `analysis::section`), `Kernel::round_holes` the holes (coaxial
  cylinders with the material outside, a counterbore or countersink
  beside them); the drawing crate (`core/model-drawing`, no geometry library)
  lays the sheet out (views by the projection method, sizes, callouts,
  the hole table with the `threads` query's threads, the title block)
  and writes PDF and SVG itself and DXF through `core/dxf`. An export
  lays out a temporary drawing and draws it as a drawing kept in the
  design is drawn (below, *Drawings in the design*).
- A mesh body (STL, OBJ) is a face with a triangulation and no surface.
  `geometry::transform_shape` moves a copy of the triangulation, and the
  validity check skips such faces. Surface operations do not take one
  (mitcad#399): the features refuse a mesh body they name, with its
  name, and leave mesh bodies out of all bodies
  (`core/model/src/features/surface_bodies.rs`, through
  `Kernel::body_kind`); the geometry facade refuses faces without
  surfaces before OCCT's booleans, dressups and face operations get them
  (`detail::require_surfaces`, `geometry/src/util.cpp`); and the app's
  inputs of those features do not offer them (`InputDef::withoutMeshes`,
  the pick item's geometry `mesh`). Convert Mesh (mitcad#462,
  `features/mesh_convert.rs`) rebuilds a mesh body of one profile
  extruded straight as a solid: the section is found in Rust from
  `Kernel::triangle_mesh` (adapted from SolidCraft,
  `third_party/solidcraft/mesh_convert.rs`) and built by `Kernel::extrude`,
  so the faces are named as an extrusion's; no kernel function of its own.

## Topological naming

References to faces and edges are names derived from how faces came to
be; edges and vertices are named by their faces. Face names travel through
OCCT's history (`Modified`/`Generated`), and edge names are rebuilt from
neighbouring faces, so a reference survives a rebuild with new dimensions.
Grammar: `core/model-base/src/topo.rs`. C++ treats names as strings, carries
them through operations and derives edge names.

**Face name:** `<feature uid>:<role>[(<key>)]`, e.g. `F3:side(c1[c4,c2])`.

| Feature | Roles |
|---|---|
| Extrude, revolve, sweep, loft | `side(<segment>)`, `start(<region>)`, `end(<region>)`; a parallel sweep turning back `turn(<region>)` |
| Thin extrude | `outer(<segment>)`, `inner(<segment>)`, `mid(<region>)`; along curves (mitcad#463) `outer(<curve>)`, `inner(<curve>)`, `tip0(<curve>)`, `tip1(<curve>)`, caps `start(r{<curves>})`, `end(r{<curves>})` |
| Extrude of a body face (mitcad#535) | `side(<edge of the face>)`, `start(<face>)`, `end(<face>)` |
| Fillet, chamfer | `fillet(<edge>)`, `chamfer(<edge>)`, `corner(<vertex>)` |
| Hole | `hole<i>.wall`, `.tip`, `.cbore_wall`, `.cbore_floor`, `.csink`, `.top`; thread `thread(<face>)` |
| Shell | `offset(<face>)`, `offset_cap(<removed face>)` |
| Replace face, split | `replace(<face>)`, `split`, `split(<tool face>)` |
| Pattern, mirror, copy | `inst<i>(<original face name>)`; a pattern of a pattern nests: `F6:inst1(F5:inst2(F4:side(c1)))`; a fillet or chamfer among the objects, repeated on the copies' edges (mitcad#105): `fillet(<copy's edge>)`, `chamfer(…)`, `corner(…)` |
| Pipe, coil | `side<i>`, `inner<i>`, `start`, `end` |
| Primitives, bounding solid | `top`, `bottom`, `side<i>` |
| Imported body | `import(<i>)` |
| Boundary fill | none of its own: a cell's faces keep the names of the tool faces they are pieces of |

- **Segment:** the part of a sketch curve in a profile,
  `<curve>[<start>,<end>]`, bounded by the curves it ends on in the
  curve's direction: `c3[c2,c4]`. Several curves at one point: `c2+c5`;
  free end: `-`; whole circle: `c5`.
- **Region:** sorted set of the outer loop's segments,
  `r{c1[c4,c2],c2[c1,c3],…}`. A key that no longer exists is traced on
  the current curves (`sketch::regions::Layout`, mitcad#407): its segments
  as stretches of their curves between the same neighbours; when they close
  a loop, the regions inside it are its pieces (a region an edit split stays
  selected whole); else the region with the most curves in common. The main
  piece keeps the old key, and the feature warns.
- **Renamed faces** (mitcad#272, `core/model/src/features/renamed.rs`):
  a sketch edit that gives a segment another neighbour renames its side
  face. A face reference that finds no face takes the face of the same
  feature and role whose key has the most curves in common (a side face:
  the same own curve, then the most end curves in place; names in keys
  alike), only when that face is unique: a tie fails with the candidates.
  An edge finds its two faces so and takes the edges between them (one
  with `#k`, all without); a vertex its faces. The reference keeps its
  stored name and the feature warns (`… found as … after a change in
  Sketch1`). Every feature that resolves a face, edge or vertex on a body
  goes through this one resolver; face appearances use the same likeness
  but take every tied face.
- A face may carry several names. When a boolean splits a face, pieces get
  `#k` in geometric order; a reference without `#k` means all pieces.
  The order (`piece_order`, `geometry/src/util.hpp`) compares the pieces'
  centres of mass in X, Y and Z, then the lower corners of their boxes,
  their areas and the upper corners, each only where two pieces differ by
  more than 1e-4 mm: pieces within that of each other in one component
  are ordered by the next. So pieces lying symmetrically (alike in X,
  apart in Y) are not numbered by the nanometres of noise an upstream
  change leaves in X (mitcad#895: a chamfer lost its edge when a frame's
  two start pieces swapped `#0` and `#1`). Pieces whose deciding
  components differ by more than 1e-4 mm keep the order of the rounded
  centres used before, so existing designs keep their numbers. A
  reference that matches several faces lists them in this order too.
  A face a fillet or chamfer consumes, where the blend runs along the
  face's far edge (a face as wide as the blend or narrower, mitcad#121),
  gives its names to the blend's face, so its edges keep their names.
- A modelled thread numbers its pieces (a crest, two flanks and a root a
  turn) by their place along its axis, comparing centres and areas only
  where they differ by more than 1e-4 mm, so that rounding noise, which
  differs between platforms, decides no number (mitcad#490). The numbers
  start at the pieces' count; the pieces whose number of the earlier
  order (centres of mass in X, Y, Z) held on every platform keep that name
  too, and the old names of the other pieces find no face.
- **Edge:** `E{<face A>|<face B>}` (sorted), `#k` when the same faces meet
  more than once; a seam has the same face twice. **Vertex:** the set of
  faces meeting at it, `#k` likewise. Sorting is byte-wise in both
  languages. The `#k` of edges and vertices follow the same tie-tolerant
  order as a face's pieces (`edge_order`, `vertex_order`,
  `geometry/src/util.hpp`, mitcad#896): an edge by the point in its middle
  in X, Y and Z, then its centre of mass, its length and its box; a vertex
  by its point. Each component tells two apart only where they differ by
  more than 1e-4 mm, so two symmetric edges between the same two faces
  (alike in X, apart in Z) are not numbered by nanometres of noise in X;
  where the deciding components differ by more, the order is the earlier
  one of the rounded keys, so existing references keep their edges and
  vertices. A reference without `#k` lists them in this order.
- Splitting a body: the first time, the first piece in geometric order
  keeps the body id and the others get ids of the splitting feature. The
  feature remembers each piece's volume and centre of mass (saved with it
  as `split_pieces`), and later evaluations give the ids by matching the
  pieces with the remembered ones, so an upstream edit that reorders or
  changes the pieces keeps later features on the same pieces; a body that
  becomes another piece warns where it is named, and a piece no longer
  made is passed by (remove body) or fails its readers, its id given to no
  other piece (mitcad#408, `core/model/src/features/pieces.rs`). The memory
  is a cache read of the feature (`Read::Pieces`) that fits both the memory
  evaluated with and the one left, so taking it into the document costs no
  second evaluation.
- Imports resolve the source file's references geometrically against the
  replayed body and store MitCad names.

## Sketch and solver

A sketch definition (`SketchDef`, `core/sketch/src/def.rs`; the sketch code
is the crate `core/sketch`, mitcad#780) is stored; its evaluation
(`core/model/src/features/sketch.rs`) is not.

- **Plane:** origin plane, construction plane or planar face, with an
  optional frame relative to it (imports keep the source's exact frame),
  and an optional placement after that frame: an offset along the
  normal, a move, a rotation and a flip, each value a parameter of the
  sketch (`sketch/placement.rs`; Redefine Sketch Plane moves a sketch to
  another plane and places it there, `sketch.redefine_plane`,
  mitcad#787).
- **Entities:** point, line, circle, arc (centre/start/end, CCW), ellipse,
  elliptical arc, spline (fit or control points, degree, weights, knots; a
  fit point spline may be closed: periodic, C2 through its first point,
  mitcad#783), text, projections (optionally linked). Flags `construction` and `fixed`.
  Points are entities and curves refer to them, so a shared point is a
  real connection.
- **Constraints:** coincident, horizontal, vertical, parallel,
  perpendicular, tangent, equal, fix, midpoint, concentric, collinear,
  symmetric, smooth; a tangent or smooth may name the point (an end of the
  curves) it applies at (mitcad#783). **Dimensions:** distance (aligned, horizontal,
  vertical), length, angle, radius, diameter, arc length. A driving
  dimension is bound to a parameter (`d<n>`); a driven one only measures,
  into a read-only reference parameter that features after the sketch
  can use (mitcad#231). A definition may name a user parameter as a
  driven dimension's reference, which the sketch then takes over: the
  importers map a source file's parameter of a driven dimension so
  (mitcad#250). A constraint or dimension can be **inactive**: kept and
  shown, left out of the solve until it is activated; the importers make
  those the source file's own geometry does not meet inactive instead of
  dropping the sketch's constraints (mitcad#721).
- **Stored positions** are the last solution and the next solve's start,
  so the same definition always gives the same geometry.
- **Implied constraints:** a new constraint the others imply in full is
  refused; one they imply only in part replaces the older geometric
  constraints it makes redundant (never a dimension, a fixed entity or a
  constraint on projected geometry), in one undo step (mitcad#287,
  `SketchEdit::drop_implied`).

Evaluation: resolve the frame; compute projections (linked ones follow
their sources by topological name); solve from the stored positions;
report DOF, fully defined entities, conflicting and redundant
constraints (a conflict is an error, the last geometry is still shown);
cut profile regions from the planar arrangement of non-construction
curves (half-edge structure, leftmost turn; islands are separate regions;
overlapping pieces and curves on each other, such as a circle drawn twice,
count once).

A profile feature's `profiles` are a set of these regions: the area they
select is the odd/even composition of the selected regions, which never
overlap, so each region once whatever the order or repetition of the list
(`extrude::selected_regions`, shared by preview and evaluation; references
that find the same region count once). Toggling a region is the
application's: a click on a selected region takes its entry out
(mitcad#199; [commands.md, Profile
selections](../core/model-api/src/commands.md#profile-selections)).

Solid mode's Autoselect groups come from the model's `autoselect_profiles`
query (`document/autoselect.rs`, mitcad#193). Only the final unused sketch
at the end of the timeline qualifies. Projected, drawn and hybrid groups
classify every boundary segment, including holes, by stored provenance:
`Entity::reference` covers unlinked projections too, independently of the
solver's `fixed` flag. A projected rectangle around drawn circles has a
hybrid surrounding region and separate drawn circle interiors. Region
keys come from the current evaluation, so edits and undo cannot leave
cached candidates. The application owns per-item toggles and hover
previews, and passes normal profile preselection to commands.

A linked projection that moved is solved in steps from where the stored
definition has it, so geometry constrained to it keeps its side (as the
solver's continuation does for a dimension change). The stored definition
keeps the positions of the last edit; the evaluation's moved entities
(`SketchOutput::moved`) are what the `sketch` query shows and what the
next edit starts from (and then stores).
Each entity of a linked projection follows the fresh curve of its kind most
like it (by points and sizes; in the kernel's order when that is as
good), so a projection of some of a face's edges, or of edges listed in
another order, follows; one that finds none, and a projection whose
source is gone, keeps its last geometry and is a warning on the sketch
(`SketchOutput::follow`, mitcad#232). Projecting leaves out curves the
same as another of the source; Clean Up Projections
(`sketch/cleanup.rs`) removes repeats across projections and lines on
another on request. The evaluation also lists, per projection that did not
follow, its stale curves (`ProjectionFollow::stale`: matching none of the
source's current curves); Clean Up Stale Projections (`sketch/stale.rs`,
mitcad#252) removes those nothing uses, trims stale lines to the regions
later features use (only when those keep their names and areas) and
unlinks the projections that still cannot follow; it never links one
again by geometry, as following would move its curves.
Two kinds of linked record share this machinery (mitcad#389): section
curves (`Projection::section`, `sketch.project` with `section`), where
the sketch's plane cuts a face, computed by the kernel
(`Kernel::section_curves`: each face cut by itself, B-spline pieces that
meet end to end joined into one), with the plane moved into the source
component's coordinates by the link; Intersect (mitcad#736) adds an
edge's crossing points with the plane (`Kernel::section_points`) and a
whole body's section, one section record per face its pieces lie on
(`sketch::project::section_groups`); and a projection of a chain of edges
(`Projection::chain`, made by the `.f3d` import for a curve the model
splits into pieces, a thread's helix of an edge per turn), the edges'
curves joined end to end into one B-spline in the model
(`sketch::project::join_curves`) before they are projected.

Editing operations (trim, extend, offset, mirror, patterns, project,
move/copy, fillet, chamfer, slots, polygons) are model commands producing
entities and constraints, shared by the UI and the imports.

- **Text** (`sketch/text.rs`): glyph outlines from `Kernel::font_glyphs`
  (OCCT font manager; bundled Droid Sans as fallback), laid out in Rust;
  each outline is a region curve `t<n>.g<k>.c<j>`. A text may carry the
  outlines a file stores for it (`glyphs`, mitcad#416), laid out instead
  of the fallback font's while its font is missing and its string and
  font are unchanged. Font files and shaping (mitcad#473,
  `sketch/fonts.rs`): a text's font may come from a file the model reads
  itself (skrifa; WOFF and WOFF2 decoded by wuff), its outlines stored
  with the text. Only a file chosen in the session is read again
  (`FontFile::chosen`, not saved, mitcad#929): a path that came with a
  design needs the stored outlines, so a design file never makes the
  model read a file it names; text in a font file, and right-to-left or joined
  scripts in an installed font (its file through `Kernel::font_source`),
  is shaped by harfrust in runs of one direction (unicode-bidi), the
  C++ side giving only the installed fonts' outlines and files; every
  line is laid out in its visual order. QR codes (`sketch/code.rs`) are
  texts whose `code` turns their content into module contours.
- **SVG drawings** (`sketch/svg.rs`, mitcad#473): read with roxmltree and
  svgtypes into exact sketch geometry (lines, Bézier splines, circular or
  elliptical arcs), texts as sketch texts, the regions a drawing fills
  found by its fill rules after the insert.
- **Patterns** (`sketch/pattern.rs`): copies are tied to originals by
  solver constraints (`Translated`, `Rotated`, `TurnedDirection`,
  `EqualSize`), so they add no DOF and keep ids when the count changes.
- **Offsets** (`sketch/offset.rs`): lines, arcs and circles are offset
  exactly with constraints. Chains with ellipses or splines are derived
  after each solve as a C1 cubic B-spline within 1e-4 mm; they are not
  solver unknowns, and dragging them moves the source chain.
- **Equal spacing** (`sketch/spacing.rs`, mitcad#221): one constraint on
  independent objects' anchors (points, circle and arc centres, line
  midpoints) in a stored order; the solver's `EqualSpacing` makes each
  successive interval (x, y, along a line, or a distance) equal to the
  next, and an optional `spacing` dimension (`Spacing`, on the first
  interval) drives them all. No copies, no alignment, no sizes bound.
- **Copy and paste** (`sketch/clipboard.rs`, mitcad#788): a fragment is
  plain data (entities, texts, constraints and dimensions with the
  source's ids, values with the expressions and parameter names they
  use, and the ids that stood for the origin and axes), so it goes
  between sketches and designs through the clipboard; a paste maps every
  id anew and the origin and axes to the target's own.

The solver works on its own ids; the sketch model maps entity ids to them.

## Application

Formula assistance lives in the Rust expression layer: its existing tokenizer
and AST provide partial-call signatures, source ranges, lexical `LET` context
and sub-expression evaluation through the read-only `formula_help` query.
The Qt `FormulaEditor` shares this query with value fields and returns ordinary
formula text and normal parameter-limit edits; it stores no additional design
state. Both the fx browser and Change Parameters open the same editor.

The app uses the model only through JSON commands and queries
([commands.md](../core/model-api/src/commands.md)); geometry comes back as
`SharedPtr<Shape>` handles, so new commands need no bridge changes.
A `preview` computes on a copy and leaves results in the cache, so OK does
not recompute.

- **Commands** ([app/COMMANDS.md](../app/COMMANDS.md)): a UI command is a
  declarative definition; the framework builds the panel, selections,
  preview and add/edit. Editing rolls back to before the feature,
  previews there, and on OK runs `edit_feature` as one undo step.
  A feature's preview draws the design as the view shows it with the
  command's result (`app/framework/PreviewBodies.hpp`, mitcad#266): shown
  bodies keep their occurrences' placements, what the command makes comes
  at the placements the report's `instances` give, and a body the view
  hides stays hidden. A body the command consumes as a tool stays drawn,
  see-through and pickable, where other occurrences show it while its
  input is active (`InputDef::toolOverlays`, mitcad#561).
- **Viewport picks** (`app/PickContext.*`): nearby edges and vertices
  precede the face or edge they bound. Selection overlays resolve to the
  original body's local topology and exact occurrence before this ranking,
  so selected faces leave their edges available while remaining pickable
  for removal. While a command's preview shows the body changed, an
  overlay hit no farther than the preview's first hit ranks before the
  preview's items (`setPickCovers`), so a face of the same name on the
  preview does not take the click meant for the selected face or its
  edges (mitcad#672). Opposing edges of a narrow face win only where one is
  clearly nearer the cursor; hover and clicks use the same ordering. The
  pick places UI tests click (`OcctViewer::pickPoints`) are the camera's
  projection rounded to the nearest logical pixel, probed at exactly that
  pixel.
- **Sketch mode** (`app/sketch/`): the app keeps the sketch in the form of
  the `sketch` query and does snapping, inference and hit testing itself;
  every change is a `sketch.*` command, one undo step per user action.
  Tools use a custom cursor (`app/framework/Cursors`). Body edges and
  vertices in the sketch plane, which the Dimension tool picks and the
  Point tool snaps to, come from the model's `plane_geometry` query (the
  coplanarity test is the model's, on the kernel's curves); they are
  projected, linked, only by the action that makes something from them
  (mitcad#256, mitcad#258). The same query with `out_of_plane` also gives
  the other edges and vertices, as what they project to along the plane's
  normal and with where they are, so that the Dimension tool picks them
  where the view shows them and projects them the same way (mitcad#866). A component's sketch is edited where one of
  its occurrences shows it (mitcad#505): the `sketch` query's frame is in
  the component's coordinates, and sketch mode places it in the design by
  that occurrence once (`SketchHost::sketchPlacement`): the plane link's
  target for a plane of another component, else the occurrence its face
  was picked at, else the component's first. The view's sketch plane, Look
  At, the grid, Hide Above Sketch and the sketch's curves and profiles all
  use that one placement; outside sketch mode a component's sketch is
  shown at its component's first occurrence. During a geometry drag, the
  edited sketch's filled profiles are removed from display and picking;
  curves and points stay live. Profile queries and face rebuilding wait
  until release or interruption, including a drag the solver refused.
  Dimension-text drags keep their profiles.
- **Navigation settings** (`app/view/Navigation.hpp`): stable preset ids
  share a canonical input table with Custom bindings. Reserved selection
  drags and the orientation cube remain separate. The viewer applies
  orbit acceleration through OCCT, scales pan deltas with fractional
  carry, and scales wheel/drag zoom; trackpad scrolling has its own
  multiplier. Settings and the visual guide use the same binding table.
- **3D mice** (mitcad#951): `app/platform/SpaceMouseDevice.hpp` is the
  small backend interface (six axes in the view's frame, button events,
  hot plug) with the pure parsing of the devices' reports; one backend per
  platform, none with a vendor SDK: Raw Input on Windows
  (`platform/windows/SpaceMouseWindows.cpp`), spacenavd's own socket on
  Linux and the BSDs (`platform/unix/SpaceMouseSpnav.cpp`), IOKit's HID
  Manager on macOS (`platform/macos/SpaceMouseMac.cpp`). A missing device
  or daemon is no error. `view/SpaceMouseController` maps the axes to the
  camera's rates (`view/SpaceMouseNavigation.hpp`: speed, dead zone,
  reversed axes, locks, dominant axis; pan and zoom only in sketch mode)
  and runs the buttons' view commands through their actions;
  `OcctViewer` moves the camera by the rates at each frame in the 3D
  mouse's Navigation mode (mitcad#978: Object and Camera about the mouse
  orbit's centre, `AIS_ViewController::GravityPoint`, the one opposite to
  the other; Fly about the eye in perspective) and with its own Lock
  horizon, not the mouse's Constrained orbit. The settings are part of
  `NavigationSettings`.
- **Timeline inspection** (`app/browser/FeaturePresentation.hpp`): hover
  previews are independent of selection and exclude failed, suppressed
  and rolled-back features. A nonpickable dashed boundary on the Topmost
  layer locates enclosed geometry alongside the normal shaded highlight.
  Tooltips and feature cards share command labels, loaded dimension text
  and component names; they do not expose internal identifiers.
- **Browser, timeline, view** (`app/browser/`, `app/view/`): the model is
  read once per change; the view keeps AIS objects that did not change.
  Visibility is the model's: an unset sketch light bulb follows "hidden
  while a feature uses it" (`document/browser.rs`). The browser lists each
  feature under the sketches it uses (the `timeline` query's `sketches`,
  from the same references), and its selection and the timeline's follow
  each other (mitcad#177). The light bulbs of bodies, sketches,
  construction geometry, components and their folders work in every mode;
  during a command they run outside its undo range (a cancelled command
  leaves them, a committed one is an undo step of its own after them;
  `CommandSession::runVisibility`, mitcad#295). Change Parameters works
  the same way while a panel is open (`CommandSession::runParameter`,
  mitcad#564): the panel's values are evaluated again and its preview
  computed again, and an edited feature's own parameters stay the
  panel's; in sketch mode a change is a sketch action's undo step and the
  sketch is read again. Create Sketch shows the
  origin planes for its pick in every design, behind the model's faces
  for picking, without touching the Origin folder's light bulb
  (mitcad#844, replacing mitcad#294). A command shows
  the sketches it uses for its time, as if their light bulbs were on (an
  edited feature's `sketches`, a new command's starting profiles; a view
  state of the window, `view/CommandSketches.hpp`, never a model command:
  mitcad#546).
  In the native macOS Browser, Return/Enter activates the selected row
  before document confirmation; a row's text editor keeps Return while
  renaming. Command-W dismisses command and timeline search popovers,
  preserving the document and restoring timeline search's saved selection.
- **Background computation** (`app/framework/ModelWorker`): the model runs
  on one long-lived worker thread (256 MB stack, OCCT crash handlers) as
  jobs, one at a time; opening, every model command, undo/redo and every
  panel preview is a job. The document belongs to one thread at a time.
  The view keeps drawing what it built earlier but must not read or
  triangulate shapes while a job runs (OCCT writes flags and
  triangulations into shared sub-shapes). Explicit Qt commands stay synchronous:
  50 ms blocked, then input queued, after 400 ms a modal progress dialog
  whose Cancel goes to the job's `RecomputeMonitor` (`JobControl` in the
  bridge). The wait cursor is a guard of the wait
  (`app/framework/WaitCursor`, mitcad#268): restored once, by the dialog
  or at the end, logged; one left on is cleared when the window is idle.
  Hover, clicks and candidate-point enumeration all suspend while a job
  owns the document; candidate probes also call model-backed command filters.
  Queries run on the UI thread between jobs; reads during a job
  are refused (`ModelBusy`), and deferred work waits for idle
  (`MainWindow::whenIdle`). What the UI must know at once during a job it
  takes from what it showed last instead: the length unit a value box
  gives a typed number when Enter closes it while a preview computes
  (`MainWindow::lengthUnit`, from the snapshot, mitcad#888), whether the
  design is modified (the title's mark). A job's wait runs the timers (a
  synchronous command's event loop, an asynchronous preview's), so a call
  put off "until after this event" that reads or changes the model goes
  through `afterEventWhenIdle` (`app/framework/IdleCall.hpp`): at once
  when no job runs, else at the job's end; a change of the edit lock waits
  so too. `MITCAD_COMPUTE_INLINE=1` runs jobs on the UI thread for
  debugging. Feature previews use asynchronous jobs on both
  frontends (mitcad#634), without modal progress. The first pending input
  starts a bounded 150 ms timer; further input does not restart it. While
  a job owns the document, numeric edits replace one pending action per
  field, evaluated only on idle. Structural edits form ordering barriers
  and invalidate/cancel old work. A completed numeric preview may display
  intermediate geometry, but only the latest input's result may update
  operation/target controls or enable OK. Pending values drain before the
  next job, without another debounce delay; a value of the same field
  taken at once (the model idle before the drain) empties them, so none
  puts an older value back (mitcad#636); release requests an immediate
  preview and OK waits for the latest exact result. `MITCAD_LOG_TIMING=1`
  also logs numeric queue replacement/drain counts and solid installation
  during a drag. `ManipulatorOverlay` keeps a separate continuous pointer
  value for local drawing (mitcad#637). Its snapped value alone emits a
  command input change; the marked snap position and field remain the
  committed value. An unchanged snap interval updates only the overlay,
  including while the worker is busy. A form refresh preserves pointer
  feedback for the same handle geometry; a changed placement/kind or a
  removed handle invalidates its grab. Opt-in feedback counters distinguish
  delivered moves, semantic changes, visual updates and overlay paints.
  The jobs that change what the view draws
  (opening, model commands but a sketch's own, previews) end with the
  view's meshes of the shown bodies, the previewed ones and the tool
  (`app/framework/DisplayMeshes`, mitcad#398): made on the worker as the
  view makes them, so that the view only finds and keeps them, and a
  design of large curved faces keeps the window (and live MCP, which then
  answers busy) responsive; the job's Cancel stops that meshing only, and
  the view meshes what is left. A face whose triangulation does not end
  does not hold the Cancel (mitcad#846): with a job's stop, the faces are
  triangulated on the meshing's own detached threads, each on a private
  copy of its face whose triangulation is moved into the face when it
  ends; once stopped, the meshing waits `io::kMeshAbandonSeconds` (2 s)
  for the faces under way and then abandons the rest, whose results are
  discarded whenever they end (`MeshReport::abandoned`). The bodies are
  then drawn with the faces meshed by then, and the view never meshes
  them itself. Such a face's thread counts in `io::abandoned_faces()`
  until it ends; a process that quits meanwhile (the application once its
  window and session are gone, `mitcad-cli`, `mitcad-geomtool`) ends with
  `std::_Exit`, as for the imports' given-up tries (mitcad#936,
  `app/framework/ProcessEnd`). A face whose triangulation throws, whatever
  the exception, is left failed and the others are meshed; an allocation
  that failed in one makes the meshing throw "triangulation: out of
  memory" (mitcad#936). All the bodies of a job are meshed
  together in one compound (`io::mesh_all_in_place`, mitcad#438), so that
  OCCT meshes the faces of all of them in parallel on every core (one body
  at a time, a design of many small bodies used a core or two) and a face
  that bodies share is meshed once; should that throw, the bodies are
  meshed one by one and one OCCT fails on is left to the view. Two tiers
  (mitcad#635): a preview's bodies are drawn through stand-ins, copies
  sharing no face, edge or vertex with them (`geometry::independent_copy`),
  meshed in the interactive tier (`previewMeshOptions`: the view's
  deviation, planes through OCCT's Delabella base triangulation, about ten
  times faster on planes with many holes for the same nodes); the bodies
  themselves keep no triangulation until a job meshes them in the final
  tier: the command that commits the preview, or a refinement job (no
  model change, no undo step) once a drag ends and the inputs rest, and
  opening a design (`viewMeshOptions`: planes by the default insertion;
  mitcad#644's checked Delabella, `checkedPlaneMeshOptions`, where a plane
  Delabella leaves without a triangulation or whose triangles' area is off
  its boundary's is triangulated again by the default, was the final tier
  until a design hung in it, mitcad#846). So a
  committed body never keeps a preview's meshes, and no job writes into a
  shape the view draws. Both tiers first take the triangulations of the
  faces a command or preview left untouched from the bodies as shown
  (`io::reuse_triangulations`: the same surface, curves, parameter curves,
  ranges and end points; nothing cached between jobs), so only the changed
  faces are meshed. Opening a design with the result store
  (`meshOpenedForDisplay`, mitcad#858) first gives each shown body whose
  shape's version has display triangulations stored with the final
  tier's options its faces' triangulations and edge polygons back
  (`io::restore_triangulations`, every face checked against the stored
  one before anything is written), so a warm open meshes no face; the
  bodies meshed instead go to the store once meshed to the end
  (`io::triangulation_bytes`). A changed body, other options or another
  build of the program find none (`MITCAD_DISPLAY_MESH_STORE=0`: never).
  The open does not wait for that write (mitcad#880; a large design's
  600 MB took 11 s of its first open): its job only takes the bodies'
  triangulations and edge polygons by their handles
  (`io::capture_triangulations`, which later meshings, changes and the
  document's close leave as they were) with the bodies' versions
  (`Document::display_mesh_write`), and the window's writer
  (`DisplayMeshWrites`) turns them into bytes and stores them on a thread
  of its own after the open, a body at a time, each file whole or not at
  all. The design opened again waits for what is still queued for it;
  clearing the store or quitting drops what is queued (the next open
  meshes and stores those bodies again). `MITCAD_DISPLAY_MESH_WRITER=0`
  stores them in the open's job as before.
  A cold open meshes bodies while its recompute still goes on (mitcad#881,
  `RecomputeMeshing`; a large design meshed 23 s after a 12 s recompute
  that used one core). The open's recompute puts each shown body into a
  feed once no later feature can change it (`Document::feed_final_bodies`,
  `core/model/src/recompute/final_bodies.rs`): every feature is classified
  by its definition as changing no body (sketches, construction geometry,
  joints and the other features that only place occurrences or write
  parameters), the bodies it names (participants, a fillet's or chamfer's
  body, a combine's target and tools, the bodies of a move, transfer or
  pattern, what the features a pattern repeats change), every body of its
  component (an operation on all bodies, any other feature) or any body (a
  cut across components, a feature that made a component); a body is final
  after the last feature that may change it, and bodies a feature makes
  are new. A body whose display meshes the store keeps is left to the
  store. A thread of the open meshes what comes in batches, each body
  through a copy that shares no sub-shape with it
  (`geometry::independent_copy`; the body may still be read by later
  features, which a triangulation in its faces would change), and hands
  the copies' triangulations over by their handles
  (`io::capture_triangulations`). After the recompute no batch begins; the
  open meshes the other bodies while the last batch goes on, and the model
  thread alone then gives each body whose shape is still the one meshed
  its copy's triangulations and edge polygons
  (`io::restore_triangulations` of a capture, checked face by face). A body
  changed after all, a batch cut short and the bodies not taken are meshed
  as before; the triangles are the same. The open's summary line counts
  the bodies meshed during the recompute, the time and the part after it.
  `MITCAD_MESH_DURING_RECOMPUTE=0` turns it off.
  A body meshed to the end by a job or given its stored meshes is drawn
  with them as they are: its drawers do not mesh (`drawerMeshes`,
  mitcad#878), where they ran OCCT's mesher over a body again on the UI
  thread whenever a face's deviation was coarser than theirs (a large
  face's bound, a large cylinder's), only to keep every face.
  `MITCAD_MESH_TRACE=1` logs each meshing's faces, phases
  and slowest faces. While a handle is dragged, Extrude also shows a
  provisional tool at once (mitcad#636; `framework/DragProxy`,
  `framework/CommandSessionProxy.cpp`, `CommandDef::dragProxy`,
  `commands/ExtrudeDragProxy`): when an exact preview is installed, its
  tool's interactive-tier triangulation is copied on the UI thread into an
  owned, immutable `ProxyMesh` (the job that made it is over and nothing
  writes into a stand-in), with the inputs it was computed from and the
  tool's placements in the design (the report's `tool_instances`, which
  also place the exact tool now). Each dragged value then changes a copy
  of that mesh on the UI thread: Extrude stretches it along the dragged
  arrow about its start (one side), its middle (symmetric) or the profile
  plane (two sides, one side only), or moves it for the start offset; only
  for distances without a taper, from the profile plane or an offset, not
  wrapped, and only while every other input is as the exact preview had
  it. The arrow's direction is taken into the tool's coordinates through
  each placement and must give the length the values say, or nothing is
  shown. `OcctViewer::setProvisional` draws it see-through amber with
  dashed edges, unpickable, hiding the exact tool meanwhile; no model
  query, no job and no worker-owned shape is involved, and the bounded
  scheduler of mitcad#634 computes the exact previews as before. An exact
  preview of the value dragged to replaces the proxy; a structural change,
  a typed value, a failed, cancelled or cleared preview, OK, Cancel and
  the session's end remove it. The automatic operation and the targets
  still follow the exact previews only. `MITCAD_LOG_TIMING=1` logs each
  update; every drag logs its inputs, proxy updates, exact previews and
  frames drawn (`OcctViewer::framesDrawn`) apart.
  Move/Copy's handles drive a provisional motion instead (mitcad#838;
  `CommandDef::dragMotion`, `commands/MoveDragMotion`): when an exact
  preview is asked for (the model idle), the command works out what its
  motion needs of the model (Free Move's pivot, the placement of the
  component the move works in) and keeps it with the preview as a function
  of the inputs. Rotate's axis and the points of Point to Point and Point
  to Position are taken from the exact preview itself: the model reports
  what `MoveSpec::resolve` worked the motion out of (the preview's
  `motion`), and `DragMotion::resolve` makes the function of the inputs
  from it, so a drag turns about the very axis the result turns about.
  Rotate's ring runs about that reported axis too (`previewReported`
  keeps it with the Axis pick it is of; `keptRotateAxis`): the model takes
  a circle's axis in its canonical direction, which may be against the
  circle's own, so the ring's sense, the proxy and the result agree.
  Move Face shares the ring (`rotateRingAxis`) and keeps its preview's
  reported axis the same way (mitcad#854: `move_face` resolves its motion
  through `MoveSpec::resolve` and reports `motion`, also when the preview
  fails); it has no provisional drag motion, since its body is not moved
  rigidly. Each dragged value then draws the exact preview's bodies
  where it puts them: a body the move moved or copied at its placement
  times the change, a body of an occurrence it places with the change
  taken into the design before its placement (`draggedPlacement`). The
  view keeps a body that is only placed elsewhere and moves its drawn
  objects (`OcctViewer::placeBody`, `AIS_InteractiveContext::SetLocation`),
  so a drag costs no model query, no presentation and no meshing. While
  the handle is held and the bodies follow, the exact preview waits until
  the pointer rests for 250 ms or the button goes up; it then replaces the
  provisional placements and stays what OK commits. The exact result reuses
  the meshes too: a rigid motion of a Move or an Align places the body as a
  location on its own B-rep (`geometry::move_shape`, `Kernel::move_shape`)
  instead of a transformed copy, so its faces keep their surfaces and
  triangulations and both tiers find them meshed (the input is not changed;
  it shares its sub-shapes with the result). Mirrors, scales and mesh
  bodies are copied as before.
- **macOS computation scheduling** (`app/platform/macos/MacWork.mm`,
  through `MacChrome.hpp`, mitcad#556): model and section workers set
  `QOS_CLASS_USER_INITIATED` on entering their thread loops. Before either
  worker starts, the application explicitly sets that class on every
  thread of OCCT's default pool through a launcher job, so display meshing
  and parallel sections use it too. This starts the existing pool earlier,
  without changing its size or the calling UI thread's class; it does not
  depend on pthread QoS inheritance. Autosave writes, texture-folder scans
  and result-store garbage collection use `QOS_CLASS_UTILITY` for each
  task, restoring the shared thread's class when the task returns or
  throws. Result-store writes remain synchronous model work, with the
  model worker's class.
  Each model job and section round holds an `NSProcessInfo` activity;
  rendered-view and final-render clients hold one while their render
  session runs, and the windowless render worker holds its own until its
  renderer is destroyed. RAII and process-exit/error handlers end them on
  completion, cancellation and worker failure. The activity option is
  `NSActivityUserInitiatedAllowingIdleSystemSleep`: it prevents App Nap
  while permitting normal system idle sleep. `NSActivityLatencyCritical`
  is deliberately omitted because it requests high-priority I/O intended
  for audio/video recording, without evidence rendering needs it. Native
  tokens and scheduling calls stay in `platform/macos/`; other platforms
  use inline no-ops. Qt remains the default macOS UI.
- **Section Analysis off the UI thread** (`app/framework/SectionWorker`,
  `app/MainWindowSection.cpp`, mitcad#438): the clips of the bodies shown,
  their meshes for the view and the panel's area and length run as rounds
  on a thread of the section worker's own, not as model jobs: the model
  prepares what a round needs while the document is idle
  (`Document::prepare_clips`, `prepare_query`: the bodies' shapes, the
  planes in their coordinates) and the round works it out without the
  document, so the window stays responsive (no progress dialog, input
  taken, the view drawing what it showed) and the result is shown on the
  UI thread when it is ready. One round runs at a time: a newer plane
  stops the running round through the geometry's cancel and replaces those
  waiting, so no stale result is shown and nothing piles up; the
  measurement waits for the view's round. A round never reads a sub-shape
  of a body the UI thread draws: what it works on is detached when it is
  prepared (`Kernel::detach_all`: copies that share no face, edge or
  vertex with the bodies the plane may reach, and of the others only the
  box they keep), so the view's meshes, a clip the UI thread makes itself
  (Hide Above Sketch, a body the kept clips lack) and a model job, which
  write into the bodies' sub-shapes, may run beside a round. A model job
  still tells the rounds to stop (`DocumentSession::setBeforeJob`, not
  waiting), as their result may be stale after it, and they are done
  again once the window is idle. The clips of the plane last clipped are kept (by
  body, shape and placement), so the bodies shown again at that plane are
  not clipped again. The panel's measurement is an inspection's
  `laterQuery`, answered through `CommandHost::measureLater` (at once, as a
  query, for other hosts).
- **Document owner** (`app/framework/DocumentSession`): owns the Rust
  document, model worker, active command, selection, mode and file/recovery
  state. `MainWindow` retains reference aliases for existing workflows.
  Queries are allowed only while idle; a cached snapshot remains available
  during computation. The native command presenter uses asynchronous jobs
  and callbacks on the UI thread. Input changes invalidate pending previews;
  cancel waits for the worker, restores the native command checkpoint's
  definition, saved state and exact undo/redo history, then releases it
  before the session finishes. Legacy workflows keep their existing
  synchronous wait presentation through an injected handler.
- **Assistant setup** (`app/mcp/ConnectAssistantDialog`): one discoverable
  dialog configures local clients without owning a model or changing undo.
  A Rust TOML planner preserves unrelated config and comments; supported CLI
  registration runs on a private copy before an atomic, backed-up update.
  MCP Bundles contain a standalone native launcher for the installed bridge.
  Initialized client labels are UI metadata, never authentication identities.
- **MCP** (`core/mcp`, `core/ffi/src/mcp.rs`, `app/mcp`): the live
  application profile exposes `document_list`, `model_query`,
  `model_command`, `model_export`, `model_reference`, `document_save`, `document_open` and the application's
  own `ui_state`, `ui_log`, `ui_screenshot`, `ui_select`, `ui_command` and
  `ui_camera` against the
  window's actual `DocumentSession` (mitcad#265, `app/MainWindowMcp.cpp`,
  `app/MainWindowMcpUi.cpp`). Save As (`document_save` with a path) and
  Open reach only the folders for designs (`app/mcp/AgentFolders`:
  Preferences › AI Assistants, `--mcp-folder`; none by default, mitcad#402), live
  mode's counterpart of headless mode's workspace. Replies that move the
  view (`ui_camera`, `ui_select`'s reveal, `view.*` commands) end the
  view's animation and any pending fit first (`OcctViewer::settleCamera`,
  mitcad#405), so their camera and the next screenshot agree. Each
  connection has a Rust `LiveSession` for protocol state and a pending request; it owns no detached document. The
  FFI prepares a validated tool and wraps its completed result. What MCP
  allows follows each command's and query's class in the model's API
  (`core/model-api/src/classes.rs`: model, file IO, remote, internal),
  not a list of names. Commands follow the ordinary window workflow and run
  on the same model worker; in an open sketch its own commands go through
  `SketchOp` as the user's would, and an agent's undo steps are labelled
  "(agent)". Queries are refused only while the model computes (a busy
  result with when to retry); a command panel, drags, dialogs and edit
  locks guard model commands, and replacing the document invalidates its
  handle. `ui_command` runs registry commands through the panel's own
  actions (`CommandSession::scriptInputs`) and closes any modal dialog it
  opens (`mcp::ModalGuard`); `ui_log` reads a ring of the application's log
  lines (`mcp/LogRing`). The agent indicator (`mcp/AgentIndicator`,
  mitcad#710), beside the project indicator, names the initialized sessions
  and can disconnect them. A listener binds only
  to IPv4 loopback on an allocated port; a private per-user discovery
  record supplies an instance ID, the process id and authentication token;
  records of processes no longer running are skipped and removed
  (`mcp::processRunning`, mitcad#437).
  `mitcad-cli mcp --connect` discovers and authenticates a running instance,
  then bridges standard MCP stdio to its JSON-lines socket without exposing
  the token on stdout. Bridge EOF leaves the GUI and unsaved document open.
  `--no-mcp` disables the listener. The separate headless `--workspace`
  mode retains its own documents and confined file tools ([mcp.md](mcp.md)).
  In both, a `model_command` is an error only when it is refused or a
  feature it added or edited fails; the model's `failed_features` (each
  failed feature, `edited`, `new`; `core/model/src/document/command_outcome.rs`)
  go to the top of the result, and `mitcad_mcp::command_outcome` decides
  for both profiles (mitcad#401). A `model_command` takes an array of
  commands (one undo step; a refused entry undoes the earlier ones) and
  answers with what it changed, compared in Rust
  (`core/mcp/src/changes.rs`: snapshots before and after, the shape
  versions of the `bodies` query, `recompute_times`); the live
  `model_export` writes into the folders for designs with the headless
  server's command and checks (`Document::mcp_export_plan`), and
  `ui_screenshot` returns an MCP image block (mitcad#689).
- **Chat panel** (`app/agent`, `core/mcp/src/chat.rs`, mitcad#947,
  [ai-chat.md](ai-chat.md)): the user's own agent through its own native
  interface, one class per agent behind `agent::ChatAgent` (Claude Code's
  print mode with stream-json, `agent::ClaudeCodeAgent`; Codex's app server
  with JSON-RPC, `agent::CodexAgent`, the bridge given as the thread's
  configuration overrides). The agent's
  only tool source is the live server of its window, through the installed
  bridge with the chat's marker (`mitcad-cli mcp --connect --chat-session`);
  the listener asks the panel (`mcp::ChatGate`, `agent::ChatController`)
  before each call of that connection and holds it, and that connection's
  next requests, while a card waits for the user. The conversation reads
  the undo depth and the document's revision around the agent's calls and
  merges a message's steps into one (`merge_undo`) unless the user changed
  the design meanwhile. The history is a JSON Lines file per design in the
  user's data folder. What does not need Qt (the agents' streams as panel
  events, command lines, the approval policy, the undo merge, the history
  format, the tool calls' group rows, Codex's whole JSON-RPC exchange) is
  Rust, reached through
  `agent_chat` requests and `ChatStream` (`feed_line`, and `control` for a
  protocol with state of its own: its events include the lines to write).
  The panel is Qt in both layouts (a `FloatingLayout::Anchor::Right` card,
  a dock); the native AppKit panel comes later.
- **Logging** (`app/mcp/LogRing`, `app/framework/DebugLog`, mitcad#269):
  one message handler for the whole run, installed by `main()` after the
  UI test driver's and passing every message on to it: a ring of the
  newest 5000 lines (live MCP's `ui_log`, the problem reports) whose sink
  is the debug log. That writes the lines to a file while it is switched
  on (Preferences' Diagnostics page, `--debug-log [FILE]`, `MITCAD_DEBUG_LOG`),
  with a header (version, build, system, graphics, command line, design,
  switches), flushed every second and at each warning, rolled over at
  20 MB, the folder kept to 10 files and 200 MB.
- **Imports** run in a separate process (`mitcad --import-worker`) so a
  kernel crash or hang only kills that process. Nothing of an imported
  file is read in the application's process (mitcad#922): the listings
  asked before the import (the documents an `.f3d`/`.f3z` design refers
  to, a building project's linked models) run in that process too
  (`--list-references`, `--list-links`), behind the progress dialog, and
  a reader's fault there fails the import with "the import process
  stopped" instead of ending the session. The parsing queries that do
  run in the application (`dxf_info`, `svg_info`, the DXF and SVG
  inserts, the print settings' read) run their reader through
  `reader_guard::catch_panic` (core/model-base), which turns a panic into
  the query's or command's error instead of an abort at the bridge. The worker hands the
  import's evaluation back in a result store of the import's own (every
  result a store takes, `hand_off_results`); the window reads the
  imported design with it, restoring those results rather than
  evaluating every feature a second time (mitcad#413). Stop keeps what was
  imported (the worker reads `stop` on stdin); Cancel kills the process.
  The `.f3d` import's one question (keep the imported bodies that match
  the file's on a retry on its stored bodies, or replace them; mitcad#283)
  goes through the job (`RecomputeMonitor::ask`): the worker prints it
  (`import-ask`), the window asks the user and writes the answer to its
  stdin. Items whose results deviate from the file's (mitcad#379) are
  asked about after the import, with the design in the window (so that a
  row can select its faces): the items the user keeps parametric make the
  window run the worker again with the same options and
  `accept_deviations`, and the new document replaces the first.
  Inside it, the `.f3d` and `.ipt` imports guard their memory themselves
  (mitcad#80, mitcad#60): low on memory, they give up definitions and
  finish with the file's bodies rather than end the process. The IFC
  import (mitcad#273) stops before its next element and reports the rest
  as not imported.
- **Rendering** (`app/render/`, in every build since mitcad#141; the
  configure stops without the prebuilt Cycles and Open Image Denoise):
  View > Rendered shows the visible bodies path traced by Cycles in the
  render worker `mitcad-render`, an executable of its own next to
  `mitcad`. Only the worker links Cycles and its libraries (Embree, Open
  Image Denoise, OpenImageIO, OpenColorIO); the application links none of
  them and offers View > Rendered when the worker is installed. Commands
  go over its stdin and stdout as JSON lines (its first line names its
  protocol version, which the application checks); frames come back
  through anonymous shared memory (a `memfd` whose file descriptor goes
  over a Unix socket; a named `QSharedMemory` on Windows), which cannot
  outlive the two processes, and become the 3D view's background, under
  OCCT's overlays, which the bodies' depth hides as in the shaded view
  (the image is a layer over the bodies, under the overlays). The worker
  follows the view's camera and size and gets scene updates written while
  the model is idle: every body by id with its mesh's content hash,
  placement and material, and only the meshes it does not hold (glTF), so
  that a change sends only what changed; and the document's render
  settings: the environment, the background's visibility, the ground and
  the user's lights as an `environment` command; the exposure, the view
  transform and a background colour are applied by the application when
  it shows a frame. Frames have two planes: the bodies' light over a
  transparent ground, and the ground's shadow catcher factors per colour,
  which multiply the background behind it (shadows, coloured reflections;
  mitcad#54). Only `CyclesRenderer.cpp` includes Cycles (C++20); the rest is
  C++17 ([rendering.md](rendering.md)). The worker renders on the CPU or
  a GPU (mitcad#50: Cycles' CUDA device, which the Linux and Windows
  releases ship, mitcad#202; Metal on macOS; HIP or oneAPI in builds
  with their SDKs): the device is a per-machine setting
  (Preferences > Display, the user's settings), sent as the worker's
  first command; a device that is not there or fails hands over to the
  CPU, which renders the same scene and says why (`RenderDevices.cpp`).
  The final render (Render Image, `mitcad-cli render`, mitcad#48)
  runs the same worker in a batch mode on a job file (scene, environment,
  camera, output settings): it writes the image file itself (PNG and
  JPEG with the view's exposure and view transform, OpenEXR as linear
  light) and reports progress and previews on stdout; the application
  stays usable and can cancel it.
- **Sheet metal** (mitcad#210, `app/commands/SheetCommands.cpp`): the SHEET
  METAL group's commands keep no sheet model of their own. A pick on a sheet
  body is a face or edge named by its material domain; the model's
  `sheet_reference` and `sheet_seam` queries turn it into the semantic
  panel, bend, boundary or corner the definition stores. The browser lists a
  sheet body's rules and Flat Pattern from `sheet_bodies`; a Flat Pattern
  shown flat replaces the body's display with its developed solid
  (`analysis_shape` `flat_pattern`), and its DXF is written by the model from
  the evaluated outline (`export_flat_pattern_dxf`, [sheet-metal.md](sheet-metal.md)).
- **Updates** (`app/update/`): see [updates.md](updates.md).
- **Web pages** (`app/web/`, mitcad#423): a design as one self-contained
  HTML file, since browsers block module scripts and `fetch()` from
  `file://`. `WebGlb` writes the visible bodies as a binary glTF file in
  the design's unit and frame (Z up, as the model; the viewer turns the
  data's `up` itself): one node per placed body (`n1`, `n2`, …; the
  component tree in the page's data names them), each body meshed once on
  a copy of its shape (`io::triangulate`, the export's tolerances), the
  appearances as metallic-roughness materials with their texture images
  inside and texture coordinates by the rendered view's projection.
  `WebPage` adds the data (units, tree, named views, Home) and inlines it,
  with the viewer's `viewer.js` and its modules as `data:` URLs of an
  import map, into the viewer's `template.html`; the viewer's files are Qt
  resources (`:/web-viewer/<repository path>`). `writePage` reads the open
  design through `CommandContext` (Export › Web page, on the UI thread
  while the model is idle); `writePageOfFile` loads a design file without
  a window (`load_design`, links followed, computed with the result
  store) and may run on any thread.
- **Texture sources** (`app/materials/`, mitcad#315): where Edit
  Appearances finds sets of texture maps. Each source is one class behind
  `TextureSource` (asynchronous categories, search, thumbnails and fetch;
  answers delivered on the GUI thread, dropped when the asking object is
  gone) and one line in `textureSourceRegistry()`; `FolderTextureSource`
  scans the folders of Preferences › Textures on a worker thread. The
  browser (`TextureBrowser`, a `QListView` grid with thumbnails asked for
  as they come into sight) sits in `AppearanceDialog`; Use turns the
  fetched files into one `create_appearance` with every image embedded
  (`textureUseCommand`), so the model never sees a source, the network or
  the cache (`textures/<source>/<asset>/<resolution>/` in the local data
  folder, `MITCAD_TEXTURES_DIR`), and a design never needs them to open.
  `PolyHavenTextureSource` (mitcad#313) is the online one: Poly Haven's
  public API, the list of textures, each texture's list of files and the
  thumbnails kept in the cache (so the service is not asked again and
  again), the chosen resolution's maps downloaded and checked. Its
  requests go through a `TextureFetcher` given to it (`TextureFetcher.hpp`):
  Qt Network in the application (HTTPS only, User-Agent
  `Mitcad/<version>`, nothing while Preferences does not allow the
  network), fixture files for UI tests (`MITCAD_TEXTURE_FIXTURES`), a fake
  with recorded answers in the unit tests, so no test uses the network.
- **Feedback and error reports** (`app/report/`, mitcad#61, mitcad#62):
  Help › Send Feedback and the reports offered after a crash or an
  internal error share one path: a `report::Report` of sections, masked
  (home folder, other absolute paths, user and host names) before the
  preview, where the user edits, leaves out or cancels; then the issue
  tracker's form, filled in through its address and opened in the
  browser. MitCad holds no token and sends nothing itself; a relay
  service could take the delivery's place later. The tracker is a build
  setting (`MITCAD_ISSUE_URL`) with a user setting over it. Crash capture
  is plain C++ (`CrashHandler.cpp`) in the application, the import worker
  and `mitcad-render`: signal handlers (Linux, macOS) or an unhandled
  exception filter (Windows), `std::terminate`, and a Rust panic hook
  that leaves the panic's message (`core/ffi/src/panic_note.rs`); each
  writes a text report (header, stack, recent actions) into the crash
  folder and lets the process die as before. The stack comes from
  `backtrace_symbols_fd` on Linux and macOS; on Windows it is walked from
  the exception's context (`StackWalk64`, on a thread of its own), so it
  starts at the faulting function, and dbghelp names the frames from the
  PDB file next to the executable where there is one (module and offset
  otherwise). On Linux the executables export MitCad's own symbols
  (`app/report/crash-symbols.list`) so that stacks name its functions. The application offers a worker's report at once (it watches
  the folder; `MITCAD_CRASH_PARENT` tells its workers' from others') and
  its own at the next start. The duplicate key is a hash of the signal and
  the top frames as module and function, with the handlers' own frames
  and addresses left out, so the same crash in another run, folder or
  machine gives the same key; an internal error's is a hash of its
  message with numbers and paths replaced.
- **3D view** (`app/OcctViewer`): an OCCT `V3d_View` in a
  `QOpenGLWidget`; OCCT renders into FBOs and needs a native drawable to
  make Qt's context current on. Linux: a GLX 4.5 compatibility context
  with a hidden helper window (a core profile makes OCCT read Qt's pbuffer
  and crash). Windows: WGL, the window of Qt's offscreen surface. macOS: a
  4.1 core context, the highest Apple offers; `macSurfaceFormat()` is the
  default `QSurfaceFormat` before `QApplication` exists, because contexts
  of different profiles cannot share there, and the drawable is the
  top-level window's `NSView`. The GL assumptions stay in the surface
  format and `occtDrawable()`, so that another backend stays local (OCCT
  has no Metal one). The orientation cube and pick widths scale with the
  device pixel ratio. The view meshes at an absolute 0.01 mm and 5°, and
  each doubly curved face within about 20 000 triangles
  (`app/view/ViewTessellation.hpp`, mitcad#398): a face that would take
  more (a sphere of metres took millions) meshes with a deviation that
  grows with its size (`geometry/io` `mesh.hpp`, `face_deviation_floor`).
  Cylinders and cones mesh at most as finely as a turn in 500 straight
  pieces (mitcad#438): their triangles are mostly their edges' points (a
  circle of 6.8 m radius took 2 600 at 0.01 mm), so those of radii over
  about half a metre, and the edges that bound them (at the smallest floor
  of their faces, planes having no say, and no more than their own
  curve's), mesh with a deviation of 2e-5 of the radius (about 700 points
  a turn, as BRepMesh cuts edges at half their deviation); smaller ones,
  and edges shared with small faces, at 0.01 mm.
  OCCT's presentations mesh through that algorithm
  (`use_bounded_presentation_meshes`, BRepMesh's default for the process);
  the model's worker makes the same meshes beforehand (`mesh_in_place`,
  for all the bodies of a job at once `mesh_all_in_place`),
  and the exports' presets bound their faces the same way
  ([commands.md](../core/model-api/src/commands.md), Mesh refinement).
- **Theme and icons** (`app/framework/Theme`, `Icons`): dark mode is read
  from the application palette (`isDarkPalette`), and the error, warning
  and accent colours and the `set*StyleSheet` helpers follow palette
  changes. The SVG icons are rendered on demand at the asked size and
  device pixel ratio. Their colours are roles (`IconRole`): each source
  `#rrggbb` stands for one role (outline, lit top, side and far faces, pale
  surface, hollow, construction, action, sketch, pencil, muted, status), and
  each role has one colour and opacity per theme, written as
  `fill-opacity`/`stroke-opacity`/`stop-opacity`; the same on every
  platform. Amber is for construction and inspection only. A disabled icon
  draws every role in the outline colour and the whole icon at 34 %.

## Window layouts and the macOS layer

The Qt idle selection card uses the model's single-item `measure` query for
up to eight selected faces, edges or bodies. Results are cached until the
selection changes or the scene refreshes, so edits and undo invalidate them.
Full topological references remain in the tooltip. Floating cards size their
active page independently of the hidden sketch palette. The palette sits in
a scroll area in both layouts: in the floating command card it keeps its
rows at their size hint and scrolls when the card (limited by the view's
height) is shorter, instead of being squeezed below its hint (mitcad#470).
The idle page (the selection's card, the Autoselect rows) scrolls the same
way in both layouts: in the Docked layout a dock's minimum height is its
content's, so content that grows with the selection or with rows would make
the main window taller, up to past the screen (mitcad#412, mitcad#560). No
dock's content may raise its minimum height with what it shows; the window
logs its layout again whenever its size changes after it shows.

With `MITCAD_MACOS_NATIVE_UI=ON`, macOS starts the AppKit frontend by
default. `--ui=qt` selects the retained Qt frontend; `--chrome` also selects
Qt. `--ui=native` requires the native build option. The native frontend keeps
the single-document workflow and the macOS 14.4 deployment target.

`NativeMacWindow` owns an `NSWindow`, configurable `NSToolbar`, model
`NSOutlineView`, inspector, and collapsible horizontal feature history with
coloured icons and an in-row marker. Timeline filtering uses the shared
component filter while preserving model indices; playback skips boundaries
inside hidden feature runs. The native toolbar presents the shared project,
live-update and edit-lock status and opens the shared project menu.
Saved toolbars receive the Project item once when upgrading an older layout;
later status changes and application restarts preserve its explicit removal.
The menu bar, application delegate, Finder open requests, document edited
indicator, Services, text editing and window actions are AppKit's. The
application delegate forwards methods it does not override to Qt's previous
delegate, preserving application-state signals, activation hover updates and
secure state restoration. Startup disables Qt's native menu bar rather than
using plugin mode, so the Cocoa integration still installs Qt's delegate.
Dock reopen runs Qt's handler before presenting the
native document; termination and file-open requests keep the shell's policy.
The saved document frame restores both its origin and size; only a window
without a saved frame starts centred with the default content size. System
controls, semantic colours, fonts and sidebar material follow the running
macOS version and accessibility settings. The toolbar changes in sketch
mode; command definitions and saved shortcuts remain shared.

The shared command registry validates shortcut proposals against commands
which can be available together, fixed application/menu keys and effective
alternate bindings. Profile-accepting model features also run in sketch
mode; other Model/Sketch pairs can share keys. A command of several
contexts can have its own key in each (mitcad#720; `shortcuts.<context>/<id>`
beside the all-context `shortcuts/<id>`); the window sets the current
context from its mode, and the actions carry that context's keys. Alternates apply only while
the proposed primary remains its default. Conflicting legacy custom
bindings remain saved for repair in Keyboard Shortcuts, but are disabled
at runtime with a diagnostic; their menu actions remain available. The
registry exposes the same effective keys for Qt actions, native dispatch,
NativeText hints and the shortcut overview.

`CommandPresenter` separates command-session actions and displayed state
from controls. `NativeForms` builds real AppKit inputs from `InputDef`;
`CommandPanel` remains the Qt presenter. Only one presenter is attached to
a command. The AppKit form lays its captions and controls out as the
shared form does and hands its OK and Cancel to the window as a footer
under the scrolling inputs; its value fields run the shared
`ParameterField` expression tools on hidden Qt proxies
(`NativeControls`, mitcad#815). Secondary dialogs and Settings use `NativeDialogs`, which binds
the existing hidden Qt presenter models and signals to AppKit controls.
Those widgets initialize their existing workflows but are never drawn.
Presenter-tree events and control/model/selection signals coalesce into one
queued native refresh; unchanged AppKit properties are left alone. A 500 ms
safety net covers Qt properties without notify signals, including model
replacement, row hiding and index-widget changes. Its model check hashes
visibility and widget identities without reading model data or allocating
description strings; nodes and selection are rebuilt only when dirty.
Sketch inline editors follow presenter/ancestor geometry events and their
host's move, resize, screen and backing-property notifications.
File and colour panels use explicit AppKit sheet hosts. File presenters
select widget-backed storage before setting their initial file and filter,
so AppKit can read them without showing a Qt chooser. Native alerts and
secondary modal sheets preserve existing synchronous result semantics;
native command previews, inspections and commits use the asynchronous model API.
Physical Properties carries ordered section and label/value rows through
`Inspection::report` into a shared `QFormLayout`. Each value is a plain,
selectable label and only body/Total section headings are emphasized. This
keeps both frontends' two-column report semantics without flattening an
HTML table or drawing interactive widgets as images.

Child windows of a presenter (context menus, message boxes) are not part of
its native structure, so they never rebuild its controls or move their
keyboard focus. A presenter can opt its outline into row drags
(`setNativeRowMoves`): it keeps the rules, the outline carries the drag. The
Projects panel uses that and gives the native window its tree keys
(`ProjectsPanel::treeKey`) and a visible row name editor
(`ProjectsPanel::setNativeEditor`, `platform/macos/NativeProjects.mm`), so
the native outline drives the same project actions (mitcad#816).

The viewport alone remains Qt/OCCT content. A standalone viewport host is
embedded as a native sibling of the AppKit panes through the public
`QWindow::fromWinId(NSView*)` and `QWindow::setParent` APIs. No private Qt
platform API or replacement geometry/rendering kernel is used. AppKit owns
the outer window and resizes the embedded drawable; general Qt overlay
buttons are suppressed. A failed native attachment is reported at startup
instead of leaving an unusable canvas. OpenGL focus, frame pacing, fullscreen,
display changes and hardware rendering require the manual checks in
[native-macos-ui.md](native-macos-ui.md).

The following layouts describe the retained Qt frontend.

`chromeStyle()` (`app/framework/ChromeStyle`) picks the layout at start-up:
**Docked** (the ribbon in a toolbar, the panels in docks) or **Floating**
(the default everywhere since mitcad#429; the Browser's Rows background by
default on Windows and Linux). Preferences › Display › Layout chooses
one for the next starts (`appearance/layout`, `layoutPreference()`; a change
offers to restart, `framework/Restart`), and `--chrome=docked|floating`
overrides both for a run (mitcad#419). `MainWindow` builds either from the
same panels, so object names and the positions the UI tests click are
the same in both. The ribbon's workspace tabs are Solid and Build
(mitcad#375: slot-profile assemblies, whose commands call only the
model's BUILD commands and queries; app/COMMANDS.md, *BUILD*), with Sketch
after them while a sketch is edited; the AppKit frontend lists BUILD's
commands in a menu.

Docked has a look of its own, `WindowStyle` (`app/framework/WindowStyle`,
mitcad#251), the same on every platform and in both themes: the docks and
the ribbon's toolbar are raised surfaces (slightly lighter than the window,
rounded, 6 px apart: the main window's separators are the gap and paint
nothing); each dock's title bar is a `PanelTitleBar` with its title in
small grey text, preserving its spelling, and float and close buttons that
show while the mouse is over it (the timeline's is a strip at its left); one accent colour, the
icons' action blue (`actionColor`), is the palette's Highlight and Accent,
the selected rows' rounded bands (28 px rows, `stylePanelTree`), the
timeline's selection and its marker's handle, and the ribbon's selected
tab's pill; the Command panel's Autoselect has a section title instead of a
group box. The colours (`windowColors`) follow the platform palette's theme
and are set again when it changes. On Windows 11 the title bar takes the
menu bar's colour, with Mica and rounded corners, and the font is Segoe UI
Variable (`app/platform/WindowsFrame.hpp`).

Floating is a full-window 3D view with the panels over it:

- `TitleBar`: one toolbar-high row under the native title bar (the content
  extends under it; empty parts move the window): Undo and Redo, the
  ribbon's tabs as a segmented control, the document name with the project
  indicator beside it and the command search. Finish Sketch stays in the
  ribbon's Finish Sketch group and the Sketch Palette.
  Under it the ribbon in
  `Presentation::Mac` (capsule groups whose name opens a menu of all their
  commands), and the update and remote notices. That is macOS's header
  above the view; on Windows and Linux (`floatingTopCards()`, mitcad#425)
  only the menu bar is above the view, and the title bar row (a `GlassCard`,
  `titleCard`), the ribbon's groups (`Ribbon::setGroupCards`: each group a
  card, the ribbon masked to them) and the notices (`noticeCard`) are rows
  of cards over the view's top (`FloatingLayout::addTopRow`).
- `GlassCard`: the rounded Browser, Command and Timeline cards and the
  status pill. The Command card shows only while it has something to say.
  There is no status bar; messages go to the pill.
- `FloatingLayout`, the central widget, owns all card geometry: cards are
  anchored to the view's corners (Browser upper left, Command upper right
  under the orientation cube, Timeline along the bottom), sized by their
  content up to a share of the view, laid out in one queued pass. A
  Command card taller than the room under the cube grows up to the top
  first, the cube moving to its left (the right inset), before it scrolls
  (mitcad#655). It gives
  the view the free margins (`OcctViewer::setOverlayInsets`) and covered
  rectangles (`setCoveredRects`): the cube and its buttons stay clear of
  cards, picks and logged click positions avoid them, and Fit, Home and the
  named views fit the model into the free area (`fitToFreeArea`). Cards
  take the mouse only inside their rounded rectangle. No card starts above
  the bottom of the header (the title bar row, the ribbon, the notices),
  whose size and place the layout follows too (mitcad#419), or above the
  last row of top cards (mitcad#425), whose cards the view gets as well
  (`OcctViewer::setTopCards`: the free area starts below them, picks are
  refused inside them only, so that the view picks between them). The
  Browser's card is as high as its rows down to the card below it, then
  scrolls (`maximumContentHeight`).

**Why the cards are windows.** With `glassActive()` (macOS 26+, Floating,
no `--no-glass`) a card's material is the system's Liquid Glass
(`NSGlassEffectView`). Qt's OpenGL content covers any native view inside the
main window, so each card is then a frameless top-level window of its own
(`GlassCard::setWindowed`), attached as a child window of the main window
(`mac::attachGlassWindow`), and the window server composes the glass over the
real 3D view. Without glass the cards are child widgets that paint their own
material. `GlassCard` and `FloatingLayout` hide the difference: card places
are slots in parent coordinates mapped to the screen and kept there when the
main window moves, resizes, minimizes or goes full screen; card windows stay
out of the Window menu, the window cycle and other Spaces; a card window
without a focused widget forwards keys to the main window's focus widget
(`GlassCard::forwardKey`); a command panel that focuses a value field
activates its card window first when another window of the same main window
has the keys (`focusField` in `CommandPanel.cpp`); logged positions are in
host window coordinates (`GlassCard::mapToHost`).

Dialogs modal to the main window are sheets on macOS (`app/framework/Dialogs`:
`prepareModal`, `sheetWarning`, `sheetInformation`, `sheetQuestion`). On
macOS Preferences is `SettingsWindow`, a pane per Preferences page whose
changes apply as they are made, and About is the standard About panel.

The default Qt frontend also uses macOS presentation for Command Search
and Change Parameters. Their platform helpers use the system font, SF
Symbols, native search fields and buttons, and palette-derived table and
selection colours. Layout dimensions are logical pixels, so the controls
and their icons follow the display's backing scale. Parameter editing and
command dispatch stay in the shared widgets; the appearance does not
change which frontend is selected.

The shared `MacDialogs.hpp` extends those conventions to library/part,
browser/project/settings and render/build dialogs (mitcad#645): native search
fields and SF Symbols, source lists, unframed sections and readable tables.
Unchecked table cells have an explicit palette-derived checkbox outline;
checked/mixed cells retain native painting, and the model/delegate keeps
mouse and Space editing. Library tiles use rounded image selection and a
separate caption highlight. Role-based `QDialogButtonBox` layouts follow the
platform's button order; explicit primary defaults prevent auxiliary actions
from taking Return. Escape follows each dialog's close/cancel path, including
stopping an active or queued final render. Icon-only controls retain their
accessible names and tooltips. Shared UI labels use the single ellipsis
character; menu/log-name normalization accepts both forms.

**The platform layer.** `app/platform/MacChrome.hpp` declares the
AppKit-only functions in `mitcad::mac`: SF Symbols as icons in the palette's
colours (`symbolIcon`), menu images hidden in the menu bar and shown in the
ribbon's menus, the hidden native title, the glass child windows, the About
panel and `captureOwnWindows` (the application's own windows as the window
server shows them, for tests). `app/platform/macos/MacChrome.mm` implements
them (Objective-C++, ARC); elsewhere they are inline stubs in the header, so
portable code calls them without `#ifdef`. `app/platform/WindowsFrame.hpp`
does the same for Windows (`mitcad::win`): the Docked window's Windows 11
frame through `DwmSetWindowAttribute` (skipped before build 22000, decided
at run time) and the Segoe UI Variable font, implemented in
`app/platform/windows/WindowsFrame.cpp`. `Q_OS_MACOS` branches in portable
files are for Qt-level behaviour only (the GL format, shortcuts).

## Design file

A design is one `.mitcad` file. **Single-file design** (format 2):
`{"format": "mitcad", "version": 2, "units", "parameters", "features":
[{"uid": "F3", "type": "extrude", ...}], "bodies", "marker"}`, plus
`components`, `occurrences`, `active_component`, `root_component`, a
feature's `component` when present, named `views` and `analyses`. Version 1 files
are converted on open. Parametric geometry is not stored; a feature that
split bodies keeps the volume and centre of each piece (`split_pieces`,
mitcad#408) so the pieces keep their ids. Imported bodies
live in base features as OCCT binary BRep, zlib + base64
(`{"format": "occt", "compression": "zlib", "size": n, "data": "…"}`,
`core/model/src/features/base.rs`); their faces are named
`<feature>:import(<j>)`. A base feature added from shapes (an import, the
`.f3d` import's fallbacks) is evaluated from those shapes the first time
(`Kernel::adopt_shape`, mitcad#244): the body its data reads as, without
reading the data back, sharing the shape's B-rep and keeping what was
measured of it that does not depend on face names. Later evaluations read
the data.

**Design in a project** (format 3): a project is a folder with the
marker `.mitcad/project.json`. Designs inside keep base-feature B-rep
data in a content-addressed store, `.mitcad/brep/<aa>/<sha256>.brep.zlib`,
referenced as
`{"format": "occt", "compression": "zlib", "size": n, "sha256": "…"}`.
Store files are written once, before the design file, so the JSON stays
small and diffable. A file is version 3 only when it has such references.
Opening resolves them from the nearest enclosing project and checks size
and hash; missing data fails its base feature with a message. The
per-user display state (Origin folder, Isolate) is kept out of versioned
files in `.mitcad/local/display/<path>.json`. Autosave and the import
process always write single-file designs (format 2).

**Size limits on reading** (mitcad#928): a design file and its project's
store come from other people too (git, cloud sync, zip), so sizes are
checked before anything is inflated (`features/base.rs`,
`build/sources.rs`). A B-rep's `size` is at most 512 MiB (`MAX_SIZE`)
and at most 64 times its zlib data plus 1 MiB (`max_inflated`); all the
B-rep data one file decodes, inside it or from the store, is at most
2 GiB (`MAX_FILE_SIZE`, a per-thread `FileBudget` while `load_state`
reads). A library source's design text in the store is inflated to at
most 512 MiB and `max_inflated` of its store file, and the sources'
texts together to at most 2 GiB; a store file that decodes to more is
"damaged" and leaves the source a reference with a warning.

**Id numbers on reading** (mitcad#929, mitcad#944): a session hands out
new ids by counting on from the largest its design holds: components
(`C<n>`), occurrences (`O<n>`) and their numbers, features (`F<n>`),
sketch points, curves and texts (`p<n>`, `c<n>`, `t<n>`) and constraints
(`k<n>`), library sources (`S<n>`), drawings (`D<n>`), their views,
annotations, cut-sheet parts and export counts. A file chooses those
numbers, so reading refuses any of them in the top `ID_RESERVE` = 2^20
numbers of its type (`core/model-base/src/ids.rs`: at most
`LARGEST_U32_ID` = 4293918719, a feature at most `LARGEST_U64_ID` =
18446744073708503039; `file/id_reserve.rs`), in the words of a malformed
id: `components[0]: invalid component id 'C4293918720' (at most
C4293918719: …)`. The id text forms themselves still refuse only the
type's largest number, so commands name the ids a session handed out.
This reserve was chosen over allocation that can fail at every counting
site: one rule on reading covers them all, and every session keeps about
a million ids of each kind, far more than it makes, so counting on never
overflows (a panic in Debug, a wrap to duplicate ids in release before).
A design that came in just below the bound and was saved with ids in the
reserve is refused on reading like any other such file.

Autosave publishes immutable snapshots named
`<session UUID>.<generation UUID>.mitcad` in the user's recovery folder.
An atomic replacement of `<session UUID>.json` (autosave metadata version
2) points to the completed snapshot. The prior snapshot remains until that
replacement succeeds, so failed or interrupted updates preserve the last
completed generation. Recovery still accepts metadata version 1 with its
fixed `<session UUID>.mitcad` snapshot. Snapshot names are validated before
recovery reads them; obsolete generations are removed after publication or
when the session is discarded.

Code: `Document::save_file` (atomic: temp file renamed over),
`load_design`, `from_json_at`, `from_json_in` with a `BlobStore`
(`core/model/src/file/project.rs`: `FsStore`, `MemoryStore`; `core/vcs`
adds `GitTreeStore`). `mitcad-cli project init` and
`convert --format v2|v3`. `project init` writes:

```
# .gitattributes
*.mitcad text eol=lf merge=binary
.mitcad/brep/** binary
# .gitignore
.mitcad/cache/
.mitcad/local/
.*.tmp
```

`eol=lf` protects files from `core.autocrlf`; `merge=binary` makes git's
own merge treat a design file changed on both sides as a whole-file
conflict instead of producing valid JSON of a broken design. Temp files
of an interrupted save are `.<name>.<process>.<n>.tmp`. On Windows the
rename is retried for up to 2 s while another program holds the file
(`file::rename_retrying`).

### Version history

`core/vcs`, gitoxide (`gix =0.88.0`, default features off: only `sha1`
and `index`; a default feature pulls in an MPL-2.0 crate). No gix types
in the crate's API. JSON commands: [commands.md](../core/model-api/src/commands.md)
("Version history").

- MitCad commits only when the project folder is the root of a git
  repository, so a user's enclosing repository is never touched.
- A version is a commit built from HEAD's tree plus the saved paths (like
  `git commit --only`), adding the B-rep files they reference and removing
  unreferenced ones. `index.lock` is held for the round and only committed
  entries change, so the user's staged changes survive and `git status`
  stays clean. No commit when nothing changed, HEAD is detached, or a
  merge/rebase is in progress.
- Store reads and version recording refuse symbolic links in the metadata
  directory, store, shard directories and files. Before adding a store file,
  recording checks its complete zlib stream and SHA-256 of the uncompressed
  data with bounded streaming memory. Project clones persist
  `core.symlinks=false` before checkout, so remote links become plain text.
- Author: the repository's own `user.name` and `user.email` (New Project
  and Project Settings write them), else git's configured user, else
  Preferences' default author. Messages end with trailers
  `Mitcad-Version` and `Mitcad-Format: 3`.
- A file's history is HEAD's first-parent chain, following content-
  preserving renames. Open, Restore and Compare follow that lineage even
  when another design previously used the current filename. Explicit
  versions outside the first-parent chain use the requested path. Older
  versions are read from the commit tree with `GitTreeStore`, no checkout.
  Restore writes the old content back as a new version; history is never
  rewritten.
- **Comparison** (the crate `core/model-diff/`): two document snapshots are
  compared in model terms without computing them (parameters by name,
  features by uid with moves, definition fields, sketch elements,
  components, bodies, views; optionally computed volumes and areas). The
  model has no git: the vcs `diff` command reads texts from trees, the
  bridge's `diff_documents` compares open documents.

In the app (`app/files/MainWindowVersions.cpp`): Save in a project with
history writes the file, then commits on the UI thread (it reads saved
files, not the document); a failed commit leaves the file saved. The
message is the undo labels since the saved state
(`changes_since_saved`), excluding display-only changes. Before
overwriting, Save compares the file's blob ids in the folder and HEAD
(`status`) with those at open/save and asks on a mismatch. A file renamed
outside MitCad (`status`'s `renamed_from`) takes its display state along
(`follow_rename`), and the next save records the rename first. Projects
are made by the core's `create_project` (below); an older project without
history gets it from `init_project_history`. Autosave never records a
version.

The Version History window (`app/files/VersionHistory.cpp`) reads history
on its own thread with its own `Project` (a project is single-threaded):
the list first, then per-version summaries. Geometry comparison runs as a
job on the model worker. Open loads a version as a new untitled design;
Restore saves or drops unsaved changes first. Each Save keeps a 256 px
preview in the user's local app data, `thumbnails/<blob id>.png`.

### Remote repositories

gix has no push or checkout, so network operations run the system's git
(`core/vcs/src/remote/cli.rs`; `MITCAD_GIT`, else `PATH`, on Windows also
Git for Windows' usual folders; 2.34+): `ls-remote`, `fetch`, `push`,
`clone`, `merge --ff-only`. TLS, SSH and credentials are git's (SSH
agent, credential helpers). git runs non-interactively (no stdin,
`GIT_TERMINAL_PROMPT=0`, `setsid` outside Windows), with `LC_ALL=C` so
failures can be classified (`errors.rs`), `GIT_ALLOW_PROTOCOL` limited to
ssh/https/http/file, and `-c core.autocrlf=false -c core.eol=lf`. Runs
are cancellable and time out after 5 minutes without output. MitCad never
handles passwords: URLs with credentials and `ext::` helpers are refused,
and credentials are hidden in output.

A repository's own git configuration is untrusted input (mitcad#920,
`core/vcs/src/remote/trust.rs`): a project folder may arrive from
anywhere with its `.git`, and git runs on open without a question (the
edit lock, the quiet fetch). Before every run, `GitCli::run` reads the
configuration of the repository git would use there (`config` of the
common git folder, `config.worktree`; no includes followed) and refuses
(class `untrusted_config`, git not started) when it sets a key that starts
a program, sends the sign-in or traffic elsewhere or moves the work tree:
`core.sshCommand`, `core.gitProxy`, `core.askPass`, `core.worktree`,
`remote.*.uploadpack/receivepack/vcs/proxy`, `credential.*helper`,
`url.*.insteadOf/pushInsteadOf`, `http.*` (but transfer sizes),
`protocol.*`, `filter.*`, diff and merge drivers, `gpg.*`,
`push.gpgSign`, `include`/`includeIf`, `init.templateDir`, git-lfs's
transfer agents. These are refused rather than overridden because their
safe value is the user's own configuration (an SSH command or credential
helper that signing in needs). What has one safe value is overridden on
every run instead: `core.hooksPath=/dev/null` (no hooks, also none from
`.git/hooks`), `core.fsmonitor` off, no submodule recursion; so a team's
`core.hooksPath` does not refuse a project. A `fetch`, `push` or
`ls-remote` addressed to a configured remote checks its `url` and
`pushurl` with `check_url` first. Projects MitCad created or cloned set
none of the refused keys. `remote_info` names the refusal
(`untrusted_config`), which the project indicator shows.

A remote in a folder (a path or `file://`) is untrusted too (mitcad#940):
git runs that repository's own git on this computer for a push
(`receive-pack`, with its `pre-receive`, `update`, `post-receive` and
`reference-transaction` hooks) and a fetch or listing (`upload-pack`),
and the `-c` overrides above do not reach it (git clears them for a local
remote). So `check_run` finds the address a network command reaches (the
named remote's `pushurl`s for a push, else its `url`s, also from the
legacy `remotes/` and `branches/` files; else the operand itself) and, for
a folder, puts `--receive-pack='git -c core.hooksPath=/dev/null -c
core.fsmonitor= receive-pack'` (`--upload-pack=…` for `fetch`,
`ls-remote`, `pull`, `clone`) right after the command, and refuses
(`untrusted_config`) when the configuration of that folder's repository
sets a refused key (`core.alternateRefsCommand`, filters or
`core.worktree` for a push that updates its work tree). Remotes on a
server get no such option: their hooks are the server's. Tests that need
a remote that refuses lock refs hide them by its configuration
(`receive.hideRefs=refs/mitcad`) instead of a hook.

- Ahead/behind counts come from local refs (`HEAD` vs.
  `refs/remotes/origin/<branch>`), so status needs no network.
- Connecting refuses an unrelated history, sets `origin` and upstream,
  commits `.gitattributes`, then pushes or fetches. Pushes are never
  forced; files over 100 MiB are held back. Clone refuses a repository
  without a project at its root.
- **Sync** (`core/vcs/src/remote/sync.rs`) fetches, then replays the
  project's unpublished versions on top of the remote's with gix, keeping
  history linear. Whole files are the unit: a path changed on both sides
  is a conflict resolved as keep mine, take theirs or save mine as a copy;
  file contents are never merged. Nothing changes until every conflict has
  a choice; the old history is kept in `refs/mitcad/sync-backup/<time>`
  (pruned after 30 days, newest 5 kept), then `git reset --keep` moves the
  branch and folder. `git gc --auto` runs afterwards since gix writes
  loose objects.
- Per-user remote state: `.mitcad/local/remote.json`.

In the app (`app/files/RemoteController.cpp`) remote work runs one task
at a time on its own thread (`RemoteTask` with its own `Project` and a
cancellable `SyncControl`), never on the UI thread or the model worker;
`remote_info` and `incoming` (no network) run on the UI thread. Saves push
immediately, retrying with exponential backoff from one minute; the remote
is fetched on open, every 10 minutes and at the first change (both per
project in Project Settings, Preferences' Cloud page giving the defaults).
Save and Restore wait for a running sync.

### Projects

Local and Cloud projects (mitcad#89, `core/vcs/src/projects/`). A project
is Local when its folder is the root of a git repository without a
remote and Cloud when the repository has one. The kind is never stored:
`inspect_folder` reads it (and what else a folder is: missing, empty, a
repository, designs, inside a project, inside another repository) without
the network or writes, so a project cloned with any git tool is Cloud.
JSON commands without a project go through the bridge's
`projects_command` with a `SyncControl`; a project's own are `Project`
commands ([commands.md](../core/model-api/src/commands.md), "Projects").

- **Made in one step** (`create_project`): the folder, the repository, the
  marker, `.gitattributes`, `.gitignore`, the author in the repository's
  own configuration and the first version; for Cloud the remote is
  checked first (nothing changes on a failure), an empty one gets the
  versions, one with files but no project is cloned and the project made
  beside its files, then a push. A failure before the first version puts
  the folder back as it was; a failed push leaves the versions waiting.
  `clone_project` with `adopt` makes a repository with files a project;
  `init_bare` makes a shared folder a remote.
- **Onto a repository's files** (`connect` with `onto_files`, Local →
  Cloud): the sync's replay (`remote/sync.rs`) puts all of the project's
  versions after the remote's, with `.gitignore` and `.gitattributes`
  merged by lines and other paths on both sides as conflicts; a replay
  that stops before it changed the project removes the remote again.
- **Settings**: the shared ones (edit locks, the MQTT broker) are typed
  fields of the marker (`core/model/src/file/settings.rs`, read
  defensively), and a change is recorded as a version; this computer's
  are in `.mitcad/local/`. The author of versions is the repository's own
  `user.name` and `user.email` first.
- **Remotes**: a branch without an upstream follows `origin`, else the only
  remote; with several and none followed nothing syncs until one is
  chosen: the window's project is Local with the remotes' names
  (`ProjectState::unfollowedRemotes`; the indicator says "No remote
  chosen"), and Project Settings asks which, `remote_follow` setting the
  branch's upstream (no network).
- **Folders and designs** (mitcad#144, `projects/listing.rs`,
  `projects/operations.rs`): `project_tree` lists a project's folders and
  designs without the network or writes, each design with its latest
  version (counted in one walk of HEAD's first-parent history, renames
  followed as `history` follows them), whether it changed since, and its
  edit lock from the local copies of the last lock poll. `create_folder`,
  `new_design`, `move` and `delete` are each one version with Save's rules
  (`ProjectRepo::commit`: only their paths, the index locked, nothing
  while HEAD is detached or a merge is in progress), and a Cloud project
  pushes it as a Save does (the application passes `push: false` and
  pushes itself). git records files, not folders, so a folder keeps an
  empty `.gitkeep`, never listed; a move or delete that would empty a
  folder's versions adds one. A move records old and new paths in one
  version, so the B-rep store stays consistent (the commit keeps what the
  new paths refer to) and the history follows the rename; the per-user
  display state and the design opened last follow too. Refusals (a target
  that is there, a path outside the project, hidden or not portable, a
  design locked by another person, a delete that would lose unrecorded
  work) change nothing and are answers' `error`s. The listing and the
  commit's walk both skip every dot-folder.
- **What a remote or a server sends is untrusted**: a check of a remote
  without a project clones it without file contents into a temporary
  repository that is removed, and names shown are cleaned of control and
  invisible characters. SSH host keys come from `ssh-keyscan` (the git
  installation's or the system's, no terminal, 20 s, 64 KiB) parsed in
  Rust: only the asked host's lines of known key types with well-formed
  keys, fingerprints computed here and compared with those GitHub, GitLab
  and Codeberg publish (built in) and with `known_hosts` (hashed names
  too). Trusting appends to `~/.ssh/known_hosts` only a key a new scan
  still gives and, for those services, one they publish.

### Projects in the application

In the app (`app/files/MainWindowProjects.cpp`) the window keeps the
**current project** (`ProjectState`: kind, root, remote; `files/Projects`)
as state of its own, independent of the open design's file. New Project,
Open Project, Open from Cloud, Save to Project and opening or saving a
design set it (`setCurrentProject`, `followFileProject`); an untitled
design keeps it. `RemoteController`, the indicator and Project Settings
use its root, never the design's path (before mitcad#89 the remote looked
for the project through the design's file and could not find a project
just made).

- Dialogs (`files/ProjectDialogs`, `files/ProjectSettings`): New Project,
  Open from Cloud, Change Address and Project Settings. Save to Project
  (mitcad#503, File menu after Save As) uses the project picker
  (`projects/ProjectPicker`) with its New Project... button, and saves
  as Save As does. Open from Project (mitcad#530, File menu after Open)
  is a file-dialog-like window on the same data (`projects/ProjectBrowser`;
  its listing and search `projects/ProjectBrowserData`): folders left,
  contents right, a path bar, a search across the projects, Read-only;
  only what `project_tree` lists, never the raw folders. New Project and Create Project Here make an empty
  project (`create_project` without a design; mitcad#145), and Open
  Project opens the design opened last (`last_design`), else the newest,
  none for an empty project. The first two and Change Address
  share `CloudSection` (`files/CloudSection`): the service, the address
  composed of owner (a user or an organisation; on GitLab also a group
  with subgroups, `group/sub`), repository and HTTPS or SSH
  (`files/CloudAddress`,
  plain Qt Core, unit-tested; the address field comes first and a typed or
  pasted address of any common form (https, `git@host:owner/repo.git`,
  `ssh://`, a browser page's address, `git clone …`) fills the fields back,
  an unknown host as `owner/repository` on a generic server; an address
  that is none is refused before the check, `repositoryAddressProblem`), and
  the check of the address while typing (after a pause), again when the
  dialog gets the focus back, on a thread of its own; its failures get
  their fixes (Copy Public Key and the SSH keys page, a credential helper's
  hint, Trust This Server's host key dialog). Folders are inspected on a
  thread too; a project is made (`create_project`) with its progress and
  Cancel in the dialog, and a cancel or a failure leaves the folder as it
  was.
- The **project indicator** (`files/ProjectIndicator`) replaces the
  status bar's version and remote labels: in the docked layout's status
  bar, in the floating layout's title bar row beside the document's name.
  It shows the project, its kind, the open design's version and the
  remote's state (`RemoteController::indicatorState`), and has the
  project's menu.
- The AppKit frontend observes the same indicator state and opens its menu
  through the native menu presenter. Native asynchronous model commands
  enforce the shared edit-lock policy; checkpoint bookkeeping remains
  available to finish inspections and cancel an in-flight edit after a lock
  is lost.
- Hooks for the edit locks and live updates (the lock controller, below):
  `ProjectIndicator::setLockText`, `setLiveText`, `setLockActions`;
  `ProjectSettingsHost::stoppingSync`, `settingsChanged`, `probeLocks`,
  `testBroker`; `MainWindow::m_openingReadOnly` while Open Read-Only
  opens a design.
- The **Projects panel** (mitcad#145, `app/projects/`) shows the projects,
  their folders and designs: project → folders → designs (mitcad#173: a
  project row is its repository's root, nothing groups the projects),
  with the sync state, versions, edit locks and changes `project_tree`
  gives (no network). `ProjectsTree::readProjects` reads
  the known projects (`ProjectsSettings` in the application settings,
  remembered by `setCurrentProject`) and the projects folder's on a
  `RemoteTask` thread, each with a `Project` of the thread's own; the
  panel reads again after Save (`updateVersionStatus`), a sync or a push,
  a change of its own and when the application gets the focus back, and
  only while it is shown. The panel only shows and asks; the window
  (`projects/MainWindowProjectsPanel.cpp`) runs `new_design`,
  `create_folder`, `move` and `delete` on the UI thread as Save records a
  version (after `RemoteController::waitForBranch`, with the project's
  author and `push: false`), then sends the current project's versions
  through `RemoteController::versionRecorded` and another Cloud project's
  with a `push` task. A move of the open design takes the window's file,
  version base and edit lock along (`openDesignMoved`, the lock through
  `LockController::designSaved`); deleting it closes it. Renames and new
  names are typed in place (`QTreeWidget` editors), drags are the tree's
  own mouse handling (no system drag and drop: nothing leaves the panel).
  In the Docked layout it is a dock split above the browser's (the view
  keeps its width); in the Floating one a card, which `FloatingLayout`
  stacks above the browser's card (upper left cards stack in the order
  added). The rows give the names the width (mitcad#529): the state as
  compact marks (`projects/ProjectRowText`), a name that does not fit
  elided in the middle by the rows' delegate, and the width dragged by the
  panel's edge and remembered (the card's grip,
  `GlassCard::setWidthAdjustable` with `FloatingLayout::setCardWidth`; the
  dock's edge). The AppKit frontend presents the shared Projects panel through
  `NativeDialogs`, in a bounded native outline above Browser. Its header
  disclosure collapses the content; View > Projects controls whole-panel
  visibility. The native bridge must preserve the panel's workflow capabilities;
  generic model binding alone does not establish keyboard/editing/move parity.
  Shown or hidden at the start (mitcad#151): the
  user's last choice (`ProjectsSettings::panelShown`, written only by a
  show or hide), else shown when `anyProjectConfigured` finds a project
  (folder entries only, nothing opened). A project row shows a display
  name (mitcad#152; the project's `local.name`, `.mitcad/local/name.json`,
  per computer and never versioned), else a Cloud project's repository
  address without the scheme (`repositoryAddressOf`:
  `github.com/owner/repository`), else the folder's name; sorted by it,
  Local and Cloud told apart by the icon and the tooltip; row keys keep
  the folder's name. Project Settings from the panel
  (the settings button for the selected row's project, mitcad#153; a
  double-click on a Cloud project) is the current
  project's `file.project_settings`, or the same dialog for another project
  without opening it (`panelProjectSettings`: its host refreshes the
  panel, pushes a recorded version in the background, and has no lock
  probe or remote work to settle, as the window runs none for it).

### Edit locks

In a Cloud project a design has one editor at a time (mitcad#89,
`core/vcs/src/remote/locks.rs`). The lock is a git ref on the remote
outside the branches, so it works with any git host that accepts such
refs; it is advisory (MitCad honours it, other git tools do not see it).

- `refs/mitcad/locks/<sha256 of the path>` points to a parentless commit
  whose tree holds `lock.json` (holder, session, times, idle time and
  poll interval, state, receipts and answers); requests are refs of
  their own, `refs/mitcad/lock-requests/<id>/<session>`, so a requester
  never races the holder.
- Every write is a compare-and-swap push (`--force-with-lease` with the
  expected commit, empty for "must not exist"): of two takers one wins,
  a takeover fails when the holder refreshed meanwhile, and a hand-over
  writes the requester as the owner in one step. A rejected push is told
  apart from a remote that refuses such refs by listing the ref again;
  `lock_probe` checks a remote once (create, list, delete).
- One `ls-remote` per poll lists the lock refs, the requests and the
  branch head; changed refs are fetched into `refs/mitcad/remote-locks/`
  and read with gix under size limits, every field checked, texts
  cleaned and numbers clamped (`locks/format.rs`; fuzz targets in
  `core/vcs/fuzz`). Branch fetches, pushes and Sync never carry lock refs.
- Staleness never compares two computers' clocks: a ref seen unchanged
  for the idle time and two polls, a request without a receipt within two
  of the holder's polls, or a receipt without an answer, all on this
  process's monotonic clock. The observations are kept per project folder
  in the process, shared by every `Project` of it, since the application
  opens one per task.
- Requests go stale the same way, so that a requester whose MitCad was
  killed is not asked about for ever: the requester's polls write a
  waiting request again every half idle time as a refresh of itself (its
  id, the commit it was first written as, stays, and with it the
  receipt, the answer and its place in the order); another session's
  request seen unchanged for the idle time and two of the requester's
  polls gets no receipt, is passed by in a hand-over and is removed by
  the next poll of whoever sees it, with a compare-and-swap deletion that
  a refresh in between defeats.

In the app (`app/files/LockController`, `files/MainWindowLocks.cpp`) one
lock controller per window keeps the open design's lock:

- Its lock commands run as `RemoteTask`s one at a time, each with a
  `Project` of its own; taking the lock when a design opens, polls, the
  refresh, requests and answers do not wait, while releasing, handing over
  and the sync before them run with a progress dialog (closing,
  quitting, the idle time). Polls come every `poll_seconds` (2 minutes
  while live updates are connected), the refresh every half idle time,
  and an idle timer restarts on activity: commands, model commands that
  ran, selections and the camera that came to rest (not frames drawn for
  a highlight under the mouse). `MITCAD_LOCK_TIME_SCALE` divides the
  timers as it speeds up the core's clock, for the UI tests. A request
  the window waits on is kept alive by its polls (they pass the poll
  interval); the holder is asked only about requests that are not stale
  and not declined, in the order served.
- The window is editable while its first take runs, and while the remote
  cannot be reached ("Edit lock not confirmed"); the lock is then taken
  when the remote answers, or the window turns read-only as for a lost
  lock. Saving, the designs' versions and Sync stay `RemoteController`'s:
  it asks the lock controller before Sync sends files someone else holds
  (`sync_plan`'s `locked` from the last listing, no network), leaves the
  newer-version notice of a design with a lock to it (a read-only window
  shows the editor's version, read with `load_version` from the fetched
  commit without touching the file; a design whose lock was just taken
  is brought up to date with a sync when the remote has a newer version
  of it, not when only other files are newer), and runs the sync before a release
  (`syncNow`).
- **Read-only windows** refuse edits where they pass rather than in each
  widget: the commands' availability (`MainWindow::isAvailable`: only
  the view's, inspections, exports, files, projects and versions), every
  model command (`MainWindow::command`, which the browser, the timeline,
  the parameters, sketches and drags all go through, throws
  `ReadOnlyError` for all but the display state, section analyses and
  exports), the design's file (`writeFile`; Save as New Version of a lost
  lock's changes is the one exception) and sketch mode and command panels.
  Unsaved changes from before the window turned read-only are kept for
  Save as Copy or Save as New Version; other changes in a read-only window
  (isolation, analyses) are never saved over the file.
- What others should learn at once goes to the live controller through
  `LiveLink` (`files/LiveLink.hpp`: locks, requests, receipts, answers;
  the open design of a Cloud project, editing or read-only, also with
  edit locks off; the versions sent are the live controller's own, from
  `RemoteController::versionsSent`); its events come back through
  `LockEvents` (`files/MainWindowLive.cpp` joins the two): a lock,
  request or version event polls at once, open events say who else has
  the design open, and a holder's offline will offers to take the lock
  after a question.

### Live updates

An optional MQTT broker tells the open projects at once about edit locks,
requests for them, pushed versions and who has a design open (mitcad#89;
commands and events: [commands.md](../core/model-api/src/commands.md#live-updates)).
It only speeds things up: a message makes the application poll git at
once, and only the lock refs on the remote grant a lock.

- **All in Rust** (`core/vcs/src/remote/mqtt.rs`, `mqtt/`), so that
  nothing a broker or another user sends is parsed by C or C++: the
  socket (`std::net`, blocking with timeouts, one thread per connection,
  no async runtime), TLS (rustls with ring as its crypto; the broker's
  certificate checked by rustls-webpki against the system's root
  certificates from rustls-native-certs, and a certificate authority file
  the user names), MQTT 3.1.1 (`packet.rs`: CONNECT with a will, user name
  and password, SUBSCRIBE, UNSUBSCRIBE, PUBLISH QoS 0 and 1 with PUBACK
  and resending, retained messages, keep-alive 30 s), reconnecting with an
  exponential back-off. Qt MQTT (GPL-3.0 or commercial) is not used.
- **One connection per broker, user and prefix** (`hub.rs`), shared by
  the open projects on it; each subscribes to `<prefix>/<project id>/#`
  (the project id is the repository's first commit) and unsubscribes when
  it closes. The connection's will marks the session `offline` at
  `<prefix>/sessions/<session>`: one will per connection is why presence
  is not under a project and why the prefix is part of the key.
- **Credentials**: the application passes the user name and password of
  the keychain for the broker it connects to; a password goes only over
  TLS (`mqtt://` with a password is refused before anything is sent;
  without one it works, with a warning).
- **Untrusted input** (`check.rs`, `messages.rs`): the decoder refuses a
  remaining length over 64 KiB before reading the packet; more than 200
  messages a second end the connection; topics only of the known forms
  under the prefix, of subscribed projects; payloads read with serde into
  typed structures of a known format and version, fields checked (ids,
  UUIDs, RFC 3339 times, paths, branch names, numbers clamped to the
  settings' bounds), text for people cleaned of control, bidirectional
  and invisible characters; at most 64 retained messages per file. A bad
  message is dropped and counted, never an error. The application shows
  names and messages as plain text only.
- **State over reconnects**: the session's own retained state (its lock
  summaries and open entries) is published again after each reconnect;
  QoS 1 messages are sent again until acknowledged; a session that goes
  offline has its open entries cleared by the sessions that follow it
  (reported once the broker acknowledged the clearing). Another session's
  open entry is given only once its presence says it is online, so that a
  newcomer never shows the entry of a session whose will is already on the
  broker; it clears such an entry instead.

In the app the `LiveHub` of the bridge (`core/ffi/src/live.rs`) takes JSON
commands on the UI thread and gives checked events through a blocking
`events(timeout_ms)` on a worker thread, as remote tasks run; it never
sees MQTT's bytes. `mitcad-cli live test` is Project Settings' Test
button.

The application's side is the live controller (`app/files/LiveController`,
wired in `files/MainWindowLive.cpp`):

- **One hub** for the application; its events are read on a thread of the
  controller's own and handled on the UI thread. The window's current
  Cloud project subscribes with what `project_settings`' `live_updates`
  says this computer uses, under its id (`project_id`: the first commit of
  HEAD's first-parent chain), and unsubscribes when another project opens,
  its live settings change, it stops syncing or Preferences turns live
  updates off. Quitting closes the hub with `wait_ms`, so that the others
  see a clean leave rather than the will.
- **Trust** (`files/LiveBrokers`): the trust question's answer per broker
  address in the application settings (`live/brokers`), Connect or Not
  Now; nothing connects before it, and another address asks again.
- **Credentials** (`files/Keychain`): QtKeychain (BSD-3-Clause, built from
  its pinned source as a static library, `cmake/Keychain.cmake`) keeps a
  broker's user name and password in the system's keychain, one entry per
  host and port; on Linux it loads libsecret at run time (not linked) or
  talks to KWallet over D-Bus, and without either the credentials are kept
  for the session. A broker that refuses shows a notice under the toolbar
  with Sign In, never blocking an open; a password goes only over TLS.
- **The rest of the window:** the indicator's `Live` / `Live offline
  (polling)`; `RemoteController::setLive` stops the remote's check timer
  while live, a `version` event of another session (or a connection back
  after a loss) checks the remote at once, and `versionsSent` (a push, a
  sync that pushed) publishes a `version`. The lock controller meets it
  through `files/LiveLink.hpp`: it publishes locks, requests and windows
  through `LiveLink`, and gets every event and the connection's state
  through `LockEvents`; while there is none, the controller publishes the
  open design (`open`) itself. Tests run against a broker in the test process and against
mosquitto (a test tool from the dev-env setup scripts, not shipped) when
it is installed; fuzz targets for the decoder and the readers are in
`core/vcs/fuzz` (nightly, by hand).

### Component libraries

Libraries of parts (mitcad#64) and the community library (mitcad#63) are
git repositories with a manifest (`mitcad-library.json`) whose components
are ordinary designs, usually with a **configuration table**
(`core/model-base/src/configurations.rs`: named rows of parameter expressions
with selectors such as Size and Length; applying a row only changes
expressions). Formats: [libraries.md](libraries.md).

- **The model knows no git.** A `LinkResolver` (`core/model/src/library.rs`)
  gives a library component's text and B-rep data at a version;
  `mitcad-vcs`'s `LibraryResolver` reads them from the cache, the bridge
  installs it for the process (`configure_libraries`), tests give a
  document one in memory. A library part is a component with a
  `LibraryRef` (library, URL, commit, tag, component, path, row, licence,
  authors, designation); linked, its bodies are a base feature as for a
  linked file (`document/library_parts.rs`), copied, it is the design's
  history in the row. The table is a top-level field of the file and of
  the definition state (undoable, compared row by row); both are
  optional, so files without them are as before.
- **Linked files stay in the project** (mitcad#929,
  `document/components.rs` `followed_link`): `update_links` reads a
  linked file only when its path, taken lexically against the design's
  folder, stays in the design's project (its folder without one);
  anything else is reported and not read, so a design file does not
  choose files outside its project for the model to open. A new link
  records a file in the project relative to the design's folder.
- **Versions per part.** A part records the commit the user chose when
  inserting it; `update_links` reads that commit, never a newer one, and
  only `update_library_parts` (an undo step) moves a part to another
  version or row. Nothing checks for newer versions on its own.
  Revision names follow Git's tag/branch naming rules, including Unicode;
  invalid names are reported as errors. Only full hexadecimal commit ids
  are shortened in missing-version messages.
- **Cache** (`core/vcs/src/library/`): bare clones in the user's data
  folder, one per URL (`<name>-<8 hex of the URL's SHA-256>.git`), made
  and updated by the system's git (`clone --bare`, `fetch` of branches
  and tags; `transfer.fsckObjects`, `gc.auto=0` so versions designs use
  stay) only on request; read with gix's isolated options (no user or
  system git configuration). A part is read from the clone of its URL,
  else from any clone that has its commit with the library's id. Library
  content is data: no hooks or filters run, manifests and files have
  size limits, paths stay inside the repository, symbolic links are
  refused.
  The application and CLI share `LibraryCache::default_root()` through the
  bridge: `Mitcad/Mitcad/libraries` in the platform's data folder. Defaults
  also read the older CLI `mitcad/libraries` cache as an extra source;
  explicit root overrides do not. Existing caches are never moved or deleted
  during configuration.
- **Community index:** a repository with `mitcad-index.json` and an entry
  per library (URL, licence, components, reviewed versions); search runs
  over the fetched indexes and libraries locally, with a licence filter;
  items without a licence are hidden unless asked for.
- **Publishing:** a library of one's own is a MitCad project folder with
  version history; the library commands write its manifest and items, and
  the version history and remote commands record and push it.
- **BUILD parts** (mitcad#375, `core/vcs/src/library/build.rs`): a
  component's `build` block (role, slot systems, length, mount frames,
  connector, panel and linear data) and its supplier fields are kept in
  the manifest as written; `library check` holds them and the design
  against the rules of [libraries.md](libraries.md#build-parts), and
  `library_build_candidates` fits connectors to a junction the model
  found (`build_junctions`) and parts to a picker's role and system. The
  model never reads a library for this: the app asks the library side for
  candidates and passes the chosen part to the BUILD commands.
- **Mount frames** (mitcad#525, `core/model/src/document/mounts.rs`,
  `core/vcs/src/library/mounts.rs`): any part's joint origins with a
  `mount` (face, shaft, hole, grid, foot; the joint a placement makes).
  The design is what counts: an insert keeps them as joint origins of the
  new component (a linked part's on fixed planes, read again with its
  bodies), and `insert_component`'s `mount` places the part by one with
  its joint, grids by their cells; the manifest's `mounts`, which
  `library_add` writes from the design and `library check` holds against
  it, tells the app which parts have frames before it reads a design.

In the app (`app/files/MainWindowLibraries.cpp`, `LibraryDialogs.cpp`)
fetches run as `RemoteTask`s on their own thread with Cancel; the other
library commands are local and run on the UI thread; inserts and updates
are model commands (jobs on the model worker). The part picker
(`app/library/`, mitcad#418) shows the library's parts as pictures for
Insert from Library and the part-choosing commands: previews are read a
few at a time while the UI is idle, only for tiles in sight; the user's
favourites are app settings, not design data; the design's library parts
come from the model's `library_usage` query. Its groups are a tree
(mitcad#527): the libraries' answers give each component its groups'
names from the top (`groups`, from the categories' `parent` and path
names), and the app merges groups of the same names across libraries
(`PartGroupTree`). A part with mount frames is placed by Place Component
(`app/commands/MountCommands.cpp`), a panel of the framework whose
preview is the model's `mount_preview`.

### Autosave

Autosave writes unsaved work as a single-file design (format 2) with a
metadata file and a session lock to a recovery folder in the user's local
app data, never into the project (format: [app/COMMANDS.md](../app/COMMANDS.md),
Files). It writes only when the revision changed. On start the app offers
sessions whose lock is stale; a recovered document keeps its file path
but is modified and has no undo history.

## Imports with history

`.f3d` designs and FreeCAD documents are replayed as MitCad features,
each step checked against what the file stores (the `.f3d` ASM history;
FreeCAD's per-feature shapes). What cannot be translated or does not match
falls back to a base feature of the stored bodies, and the replay
continues on it. What the report says stays on the timeline as import
notes in the definition state (mitcad#170, `core/model/src/document/import_notes.rs`):
the fallbacks' base features and partial items show as warnings, and an
item the file had suppressed or failed becomes a suppressed `placeholder`
feature at its place. An `.f3d` item suppressed in the file comes in as
its own feature, suppressed, where its definition translates
(mitcad#180, `core/import-replay/src/suppressed.rs`). The `.f3d` history is matched by its solids; its surface
bodies come in with the fallbacks as stored bodies, which joins and cuts
without participants then leave alone. Both run over the `Kernel` trait,
so they are testable on the mock kernel; the app runs them in the import
process.

The replay's geometry differs from the file's by the kernels' noise (an
earlier feature built a micrometre otherwise). The `.f3d` import's
geometric tolerance (`import_f3d`'s `geometric_tolerance`, 0.01 mm by
default; Preferences, General) is the distance within which the replay's
geometry is taken as the file's where no exact match is: the lookups of
decoded edges, faces and vertices look within 1e-3 mm first and then
within it, and an item's new faces must lie on the file's within it
(mitcad#224, `core/import/docs/f3d-fallbacks.md`). Fillets and chamfers with a tangent
chain follow MitCad's own chain (edges that meet end to end in the same
direction), not only the kernel's (`commands.md`, fillet).

- `.f3d`: the design streams are decoded into the dump IR
  ([SCHEMA.md](../core/import/SCHEMA.md)), which external dumps (route A)
  also use; formats in [ASM_FORMAT.md](../core/f3d/ASM_FORMAT.md) and
  [OGS_FORMAT.md](../core/f3d/OGS_FORMAT.md). The file's single timeline
  is split by component: each item goes into the component whose bodies
  it changes, else the one that owns it (an item that changes the bodies
  of several components has a feature of its own in each other one,
  replayed there, mitcad#169), and a feature that uses another
  component's sketch gets a copy moved by the occurrences' placements at
  its point of the timeline (features work only on their component's
  bodies). Occurrences take the placements the file gives them at the end
  of its timeline (where the timeline's items put them last, which the
  decoder reads per item, mitcad#81; else the last captured positions,
  else the stored transforms), and the file's captured positions come in
  as `capture_position` features at their points of the timeline;
  occurrences of components of other documents (inserted parts,
  fasteners) are occurrences of empty components during the replay, which
  the import then fills (mitcad#196, `core/ffi/src/f3d_references.rs`,
  `core/import-replay/src/references.rs`): each document the design refers to
  is imported from its own file (the reference folder next to the design,
  folders and files given, the GUID the file's manifest lists; the
  application asks for each one not found, the command line never) with
  its history and its own references, and copied into its component
  (`Document::fill_component_copy`), the captured positions inside it
  coming in as `capture_position` features of it; one undo step with the
  import. A document not found leaves its components empty, the report
  naming it. The file's joints come
  in as joint features only where they hold where the file places the
  occurrences (nothing the import adds moves an occurrence elsewhere),
  else as as-built joints of the placements; whether a joint holds is
  computed from its frames before it is added, so a joint item costs one
  recompute (mitcad#87). Rigid groups come in as rigid groups.
- FreeCAD: `core/freecad` reads the archive and XML (`roxmltree`, DTDs
  refused, nesting, expressions and B-splines bounded against hostile
  files, mitcad#932); enumerations are indices, mapped per FreeCAD version
  through `core/freecad/data/enums.json`.

How both work, their limits and corpus tests:
[core/import/README.md](../core/import/README.md) and, for FreeCAD
documents, [core/import-fcstd/docs/freecad.md](../core/import-fcstd/docs/freecad.md).
Real files are kept
outside the repository (`MITCAD_F3D_CORPUS`, `MITCAD_F3D_MODELS`,
`MITCAD_FCSTD_CORPUS`, `MITCAD_IPT_CORPUS`;
[development.md](development.md#freecad-reference-models-and-corpus)).

`.ipt` parts (mitcad#60) come in with their history: `core/ipt` decodes
the part's parameters, sketches, work planes and features from the
definitions segment into the `.f3d` import's dump IR, and `mitcad-import`
replays it against the ASM history of the B-rep record
(`core/ffi/src/ipt_history.rs`), with the same checks and fallbacks as an
`.f3d` design (`core/ffi/src/ipt_import.rs`; the `import_ipt` command), in
the import process like the others; or only the bodies stored in the file
as base features (`bodies_only`). The triangles of mesh features (kept in
the graphics segment, not the B-rep record) come in as mesh bodies, base
features after the replay; a part without bodies opens empty, with a
warning.

`.iam` assemblies (`core/ffi/src/iam_import.rs`; the `import_iam`
command, also `import_ipt` with an assembly) import each distinct part
file once into a document of its own with the `.ipt` import, and place it
as a component: its stored bodies shared as one base feature, or with
`history` its replayed design copied in (`Document::add_component_copy`).
With `history`, a part whose replay fails comes in as its stored bodies,
and the report gives each part's outcome; the referenced files are
resolved before the first part is imported, so that the worker's trace
counts the parts (`part <n> of <m>`) for the application's progress
(mitcad#507). File › Open and Import ask for the history (**Parts with
their design history**, remembered; `app/files/IamImport.*`).

IFC building models (mitcad#273) come in through `core/ifc` (the reader:
no geometry) and `core/import-ifc` (`mitcad-import-ifc`) on the `Kernel` trait
(`core/ffi/src/ifc_import.rs`; the `import_ifc` command): the spatial
structure as components (storey planes, grid axes), each element's swept
solids and booleans as features and its breps and face sets as bodies
the kernel builds from their faces and triangles (`Kernel::boundary_body`,
`Kernel::triangle_body`; the OCCT side reuses the B-rep builder of the
other imports), all of one element in one command
(`Document::add_features_with`: a building of thousands of elements is
thousands of commands, not tens of thousands), types' shapes as
components with occurrences, openings cut from their elements. The
report accounts for every product of the file.

`.dwg` drawings (mitcad#453) hold no history: `core/dwg` reads the
drawing and plans its components (each named block with 3D content a
component, each rigid insert an occurrence; anonymous blocks and scaling
or mirroring inserts copied in place), the B-rep records of its solids
are converted by `core/asm` (the text form of older drawings
through `asm::text`) and built by the imports' B-rep builder, its 3D
faces and meshes become mesh bodies, one base feature per component and
layer (`core/ffi/src/dwg_import.rs`; the `import_dwg` command), in the
import process like the others.

`.rvt` projects and `.rfa` families (mitcad#274, mitcad#275, mitcad#333): `core/rvt`
reads the container (the compound file module shared with `core/ipt` as
the `mitcad-cfb` crate), the paged gzip streams, the metadata and the
file's own serialisation schema, and walks the element records by that
schema; its `building` module reads the building elements' parameters
(location curves, joins, layers, boundary loops, sections, cut loops,
grids, links), and its `brep` module the geometry the file keeps of its
elements (winged-edge B-reps in their faces' parameters, types placed by
transforms) from the partitions' geometry chains. `core/import-rvt` (`mitcad_import_rvt`)
brings a project in through the model's kernel, one component per level
with its construction plane: walls, floors, roofs, ceilings, columns,
beams and stair runs rebuilt as features
(the IFC import's feature builder, one command per element), each checked
against the IFC export of the same project when one is given (by
GlobalId) or else its stored box and undone when it disagrees; every other
element a body: a mesh of the geometry the file keeps (triangulated in
`mitcad_import_rvt::geometry`), else the reference's body, its saved
mesh or its box (hidden unless structural); linked models (projects, IFC
files) in components of their own (`core/ffi/src/rvt_import.rs`; the
`import_rvt` command), in the import process like the others, under the
memory guard.

SolidCraft designs (`.solidcraft`, mitcad#450) come in with their history
through `core/solidcraft` (the reader: the JSON document or its container,
every file format upgraded to one by SolidCraft's own steps, adapted in
`third_party/solidcraft`; no geometry) and `mitcad-import-solidcraft`
module on the `Kernel` trait (`core/ffi/src/solidcraft_import.rs`; the
`import_solidcraft` command, also the headless MCP server's
`document_open`), in the import process like the others. The file keeps
definitions, not bodies, so every feature is MitCad's own evaluation:
parameters keep their names and expressions, sketches their constraints
and dimensions, and faces and edges are found by SolidCraft's persistent
names and pick points on the rebuilt bodies, then followed by MitCad's
topological names. Components, occurrences, joints, motion links, contact sets, motion studies and sheet
metal map to MitCad's; a feature without a counterpart, or one that fails, is a
suppressed placeholder with the reason, and the report lists it
([core/import-solidcraft/docs/solidcraft.md](../core/import-solidcraft/docs/solidcraft.md#solidcraft-import)).

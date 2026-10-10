# Mitcad architecture

Three layers: a Rust model that knows the document, timeline and features
but no geometry; a thin C++ facade over Open CASCADE Technology (OCCT)
that does the geometry; and a Qt application that talks to the model with
JSON commands.

## Layers

| Part | Language | Responsibility |
|---|---|---|
| `core/model` (`mitcad-model`) | Rust | Document, timeline, features, parameters and expressions, units, sketches, topological naming, cached recompute, undo, project file, design comparison, JSON command API. No OCCT. |
| `core/solver` (`mitcad-solver`) | Rust | 2D constraint solver and the joint solver of assemblies behind a narrow API ([README](../core/solver/README.md)). |
| `core/f3d` (`mitcad-f3d`) | Rust | `.f3d` container, ASM B-rep, design streams decoded into the dump IR. |
| `core/import` (`mitcad-import`) | Rust | Replays `.f3d` dumps and FreeCAD documents as Mitcad commands, checked against the file's stored results ([README](../core/import/README.md)). |
| `core/freecad` (`mitcad-freecad`) | Rust | `.FCStd` reader: typed objects and properties, placements, links, sketches, spreadsheets, expressions. No geometry. |
| `core/zip` (`mitcad-zip`) | Rust | Zip container for `.f3d` and `.FCStd` (stored, deflate, zstd; a writer for tests). |
| `core/ipt` (`mitcad-ipt`) | Rust | `.ipt` part files: compound file container (MS-CFB), property sets (MS-OLEPS), the segment database, the B-rep record converted by `core/f3d`'s ASM code, the meshes of mesh features, the definitions segment (parameters, sketches, features) as the import's dump IR, and `.iam` assemblies (referenced files, occurrences, placements; [README](../core/ipt/README.md)). No geometry. |
| `core/dxf` | Rust | DXF read and write. |
| `core/3mf` (`mitcad-3mf`) | Rust | 3MF writer (one object of parts) and a checking reader for tests. |
| `core/vcs` (`mitcad-vcs`) | Rust | Version history of projects in git (gitoxide, only here); remotes through the system's git; component libraries and community indexes in git repositories ([libraries.md](libraries.md)). |
| `core/update` (`mitcad-update`) | Rust | Release manifest, signatures, version selection; the `mitcad-release` tool. No network ([updates.md](updates.md)). |
| `core/ffi`, `core/cpp` | Rust, C++ | CXX bridges: the `Kernel` on OCCT, the document API for C++. |
| `geometry` (`mitcad_geometry`) | C++ | OCCT facade: profiles, features, booleans, name transfer, queries, file formats, ASM bodies, analyses. No Qt. |
| `tools/cli` (`mitcad-cli`) | C++ | Headless open/import, recompute, properties, export; kernel and import tests run through it. In builds with `MITCAD_RENDER` also `render` (the final render through the render worker), which links Qt Core and Gui for the application's scene and job code. |
| `app` | C++/Qt | The UI. |
| `tools/freecad-export` | Python, Bash | FreeCAD macros that make the FreeCAD import's reference models and dumps. |

Dependencies point one way: `app` → `core/ffi` → `core/model` (with
`solver`, `f3d`, `import`, `freecad`, `zip`, `3mf`, `vcs`, `ipt`; `update`
stands alone) and `core/ffi` → `geometry` → OCCT. The model reaches geometry only
through the `Kernel` trait, so model tests run on a mock kernel.

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
  definition and inputs (`core/model/src/fingerprint.rs`), stable across
  documents and processes.
- **Result store** (`core/model/src/store.rs`): costly results are also
  kept on disk under the same key, so reopening a design or an older
  version reuses them. Shapes are stored with their names
  (`geometry/include/mitcad/geometry/persist.hpp`); results of another
  build are ignored; sketches, construction and plain base features are
  not stored. The memory cache has a byte budget (the app gives a quarter
  of RAM) and evicts least recently used results not held by the current
  document. The `cache` query reports both caches.
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
  single thread (mitcad#95, `core/import/README.md`, *Definitions
  evaluated in parallel*). Its kernels must be `ImportKernel`s: `Send`,
  with shapes that threads may share (`Send + Sync`). Within the same
  `threads` (`Progress::helper`) the import reads the file's bodies
  on several threads and builds the history's bodies ahead of the
  replay (mitcad#103, *Work beside the replay*); the timeline items
  themselves are replayed one after another.
- **Memory:** an allocation that fails ends the process (Rust's cannot be
  caught; OCCT's `Standard_OutOfMemory` and `std::bad_alloc` inside a
  kernel operation become its error "<operation>: out of memory",
  `KernelError::OUT_OF_MEMORY`). The `.f3d` import therefore measures the
  process against its tightest limit (`core/ffi/src/memory.rs`: an
  address-space or data limit, a Windows job's limit or the commit left,
  the memory the system has left) and gives up definitions before an
  allocation can fail (`core/import/README.md`, *Memory*). OCCT's own
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
  worker), as the shared libraries' static destructors would tear down
  OCCT's state under that thread.
- **Undo:** the definition state (parameters, features, marker, body
  attributes, assembly, visibility, named views, analyses) is cheap to
  clone and pushed on a stack; several commands can merge into one step
  (`merge_undo`). Each state has a revision number, so `mark_saved` and
  the `document` query's `modified` work without comparing files.
- **Parameters:** name, expression (`d1 * 2 + 5 mm`), unit, comment,
  kind (model or user), evaluated in dependency order; cycles and unit
  mismatches are errors. Internal units are mm and radians. Input accepts
  a decimal comma (`core/model/src/expr/mod.rs`); stored expressions
  always use points.
- **Construction geometry:** planes, axes and points are features that
  make a datum, not bodies. Features refer to geometry through `GeomRef`
  (origin datum, construction feature, or a body face/edge/vertex) and to
  paths through `PathRef`.
- **Body attributes** (visibility, material, appearance; steel by
  default) live in the definition state.
- **Appearances** (mitcad#46, `core/model/src/appearance.rs`): physically
  based parameter sets (base colour, metalness, roughness, specular,
  transmission, IOR, coat, emission, opacity, and a texture for the base
  colour: an image file or one embedded in the project file, mitcad#53).
  The library is built into the model; a document's own appearances are
  in the definition state and the project file, changed by undoable
  commands that recompute nothing. Bodies refer to one by id, and so do
  single faces by topological name (a body attribute; the face's
  appearance overrides the body's and follows the face as other face
  references do). The shaded view uses the display colour (the base
  colour) of bodies and faces, the rendered view every parameter and the
  texture.
- **Render settings** (mitcad#47, `core/model/src/render_settings.rs`):
  how the rendered view lights and shows the design, per document: the
  environment (a built-in studio or outdoor setup, or an HDR image), the
  background, the ground and the film (exposure, view transform), and
  the final render's output (mitcad#48: size, samples, time limit,
  denoising, transparency, file format), as sections of fields with
  defaults, so that more sections can be added and older files open;
  and the user's lights (mitcad#54: point, spot, area and sun lights,
  placed in the design or relative to the camera), a list changed by
  commands of their own and compared light by light. They are in the definition
  state and the project file (left out while they are the defaults),
  changed by undoable commands that recompute nothing, and compared field
  by field in the comparison of versions.
- **Analyses** (`core/model/src/document/analyses.rs`): section analyses
  kept in the definition state with a name, a plane reference, an offset,
  a flip and a light bulb; their commands are undo steps that recompute
  nothing. The plane is resolved at the marker when asked for (the
  `analyses` query), so a face's section follows the model by its name.
  One is shown at a time, so one plane cuts the bodies; the app cuts the
  view where the shown one says (`MainWindow::followAnalyses`), except
  while Section Analysis' panel previews its own.
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
  component's bodies, `Read::Placement`, `Read::BodyIn`). A linked
  external component is read-only; its bodies are stored in a base
  feature refreshed on open.
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
  [commands.md](../core/model/src/api/commands.md) ("Joints"). In the
  application (phase 3) the ASSEMBLE group's panels add and edit them, a
  preview reports the placements it changes so that panels show the
  moved bodies, components are dragged in the view through the drag
  query, and joints are animated as previews of driven positions
  ([app/COMMANDS.md](../app/COMMANDS.md), "Joints"). The `.f3d` import
  (phase 4, `core/import/src/joints.rs`) maps the file's joints to these
  features (*Imports with history* below).

### Units

The default is metric wherever a unit is chosen: document (mm), import and
export units, thread and hole tables (ISO first), materials and mass
properties, grid and settings. Inches only by choice. An imported file's
own units are kept.

## Geometry kernel

The `Kernel` trait (`core/model/src/kernel.rs`) is grouped by feature
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
  Only FreeCAD's reference check measures with OCCT's fixed rule
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
  boolean.
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
- Transforms check their input once (`Shape::checked_valid`) instead of
  every copy: a rigid motion, mirror or uniform scale keeps a valid shape
  valid, and checking each of a pattern's copies was four fifths of their
  time. A copy that only renames faces (a tool a pattern rebuilt in
  place) shares the input's B-rep.
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
  Handlers are installed on the app's model worker thread, in
  `mitcad-cli`'s `main` and on the import's threads.
  `MITCAD_TEST_OCCT_CRASH=<operation>` forces a crash (`cli.occt_crash`).
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
  STEP as an assembly, the others as moved copies).
- A mesh body (STL, OBJ) is a face with a triangulation and no surface.
  `geometry::transform_shape` moves a copy of the triangulation, and the
  validity check skips such faces.

## Topological naming

References to faces and edges are names derived from how faces came to
be; edges and vertices are named by their faces. Face names travel through
OCCT's history (`Modified`/`Generated`), and edge names are rebuilt from
neighbouring faces, so a reference survives a rebuild with new dimensions.
Grammar: `core/model/src/topo.rs`. C++ treats names as strings, carries
them through operations and derives edge names.

**Face name:** `<feature uid>:<role>[(<key>)]`, e.g. `F3:side(c1[c4,c2])`.

| Feature | Roles |
|---|---|
| Extrude, revolve, sweep, loft | `side(<segment>)`, `start(<region>)`, `end(<region>)`; a parallel sweep turning back `turn(<region>)` |
| Thin extrude | `outer(<segment>)`, `inner(<segment>)`, `mid(<region>)` |
| Fillet, chamfer | `fillet(<edge>)`, `chamfer(<edge>)`, `corner(<vertex>)` |
| Hole | `hole<i>.wall`, `.tip`, `.cbore_wall`, `.cbore_floor`, `.csink`, `.top`; thread `thread(<face>)` |
| Shell | `offset(<face>)`, `offset_cap(<removed face>)` |
| Replace face, split | `replace(<face>)`, `split`, `split(<tool face>)` |
| Pattern, mirror, copy | `inst<i>(<original face name>)`; a pattern of a pattern nests: `F6:inst1(F5:inst2(F4:side(c1)))`; a fillet or chamfer among the objects, repeated on the copies' edges (mitcad#105): `fillet(<copy's edge>)`, `chamfer(…)`, `corner(…)` |
| Pipe, coil | `side<i>`, `inner<i>`, `start`, `end` |
| Primitives | `top`, `bottom`, `side<i>` |
| Imported body | `import(<i>)` |

- **Segment:** the part of a sketch curve in a profile,
  `<curve>[<start>,<end>]`, bounded by the curves it ends on in the
  curve's direction: `c3[c2,c4]`. Several curves at one point: `c2+c5`;
  free end: `-`; whole circle: `c5`.
- **Region:** sorted set of the outer loop's segments,
  `r{c1[c4,c2],c2[c1,c3],…}`. A key that no longer exists matches the
  region with the most curves in common and keeps the old key.
- A face may carry several names. When a boolean splits a face, pieces get
  `#k` in geometric order; a reference without `#k` means all pieces.
- **Edge:** `E{<face A>|<face B>}` (sorted), `#k` when the same faces meet
  more than once; a seam has the same face twice. **Vertex:** the set of
  faces meeting at it. Sorting is byte-wise in both languages.
- Splitting a body: the first piece in geometric order keeps the body id,
  the others get ids of the splitting feature.
- Imports resolve the source file's references geometrically against the
  replayed body and store Mitcad names.

## Sketch and solver

A sketch definition (`SketchDef`, `core/model/src/sketch/`) is stored; its
evaluation is not.

- **Plane:** origin plane, construction plane or planar face, with an
  optional frame relative to it (imports keep the source's exact frame).
- **Entities:** point, line, circle, arc (centre/start/end, CCW), ellipse,
  elliptical arc, spline (fit or control points, degree, weights, knots),
  text, projections (optionally linked). Flags `construction` and `fixed`.
  Points are entities and curves refer to them, so a shared point is a
  real connection.
- **Constraints:** coincident, horizontal, vertical, parallel,
  perpendicular, tangent, equal, fix, midpoint, concentric, collinear,
  symmetric, smooth. **Dimensions:** distance (aligned, horizontal,
  vertical), length, angle, radius, diameter, arc length. A driving
  dimension is bound to a parameter (`d<n>`); a driven one is display only.
- **Stored positions** are the last solution and the next solve's start,
  so the same definition always gives the same geometry.

Evaluation: resolve the frame; compute projections (linked ones follow
their sources by topological name); solve from the stored positions;
report DOF, fully defined entities, conflicting and redundant
constraints (a conflict is an error, the last geometry is still shown);
cut profile regions from the planar arrangement of non-construction
curves (half-edge structure, leftmost turn; islands are separate regions;
overlapping pieces and curves on each other, such as a circle drawn twice,
count once).

A linked projection that moved is solved in steps from where the stored
definition has it, so geometry constrained to it keeps its side (as the
solver's continuation does for a dimension change). The stored definition
keeps the positions of the last edit; the evaluation's moved entities
(`SketchOutput::moved`) are what the `sketch` query shows and what the
next edit starts from (and then stores).

Editing operations (trim, extend, offset, mirror, patterns, project,
move/copy, fillet, chamfer, slots, polygons) are model commands producing
entities and constraints, shared by the UI and the imports.

- **Text** (`sketch/text.rs`): glyph outlines from `Kernel::font_glyphs`
  (OCCT font manager; bundled Droid Sans as fallback), laid out in Rust;
  each outline is a region curve `t<n>.g<k>.c<j>`.
- **Patterns** (`sketch/pattern.rs`): copies are tied to originals by
  solver constraints (`Translated`, `Rotated`, `TurnedDirection`,
  `EqualSize`), so they add no DOF and keep ids when the count changes.
- **Offsets** (`sketch/offset.rs`): lines, arcs and circles are offset
  exactly with constraints. Chains with ellipses or splines are derived
  after each solve as a C1 cubic B-spline within 1e-4 mm; they are not
  solver unknowns, and dragging them moves the source chain.

The solver works on its own ids; the sketch model maps entity ids to them.

## Application

The app uses the model only through JSON commands and queries
([commands.md](../core/model/src/api/commands.md)); geometry comes back as
`SharedPtr<Shape>` handles, so new commands need no bridge changes.
A `preview` computes on a copy and leaves results in the cache, so OK does
not recompute.

- **Commands** ([app/COMMANDS.md](../app/COMMANDS.md)): a UI command is a
  declarative definition; the framework builds the panel, selections,
  preview and add/edit. Editing rolls back to before the feature,
  previews there, and on OK runs `edit_feature` as one undo step.
- **Sketch mode** (`app/sketch/`): the app keeps the sketch in the form of
  the `sketch` query and does snapping, inference and hit testing itself;
  every change is a `sketch.*` command, one undo step per user action.
  Tools use a custom cursor (`app/framework/Cursors`).
- **Browser, timeline, view** (`app/browser/`, `app/view/`): the model is
  read once per change; the view keeps AIS objects that did not change.
  Visibility is the model's: an unset sketch light bulb follows "hidden
  while a feature uses it" (`document/browser.rs`).
- **Background computation** (`app/framework/ModelWorker`): the model runs
  on one long-lived worker thread (256 MB stack, OCCT crash handlers) as
  jobs, one at a time; opening, every model command, undo/redo and every
  panel preview is a job. The document belongs to one thread at a time.
  The view keeps drawing what it built earlier but must not read or
  triangulate shapes while a job runs (OCCT writes flags and
  triangulations into shared sub-shapes). The caller stays synchronous:
  50 ms blocked, then input queued, after 400 ms a modal progress dialog
  whose Cancel goes to the job's `RecomputeMonitor` (`JobControl` in the
  bridge). Queries run on the UI thread between jobs; reads during a job
  are refused (`ModelBusy`), and deferred work waits for idle
  (`MainWindow::whenIdle`). `MITCAD_COMPUTE_INLINE=1` runs jobs on the UI
  thread for debugging.
- **Imports** run in a separate process (`mitcad --import-worker`) so a
  kernel crash or hang only kills that process. Stop keeps what was
  imported (the worker reads `stop` on stdin); Cancel kills the process.
  Inside it, the `.f3d` and `.ipt` imports guard their memory themselves
  (mitcad#80, mitcad#60): low on memory, they give up definitions and
  finish with the file's bodies rather than end the process.
- **Rendering** (`app/render/`, only with the CMake option `MITCAD_RENDER`):
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
  a GPU (mitcad#50: Cycles' CUDA device, and HIP, Metal or oneAPI
  in builds with their SDKs): the device is a per-machine setting
  (Preferences > Display, the user's settings), sent as the worker's
  first command; a device that is not there or fails hands over to the
  CPU, which renders the same scene and says why (`RenderDevices.cpp`).
  The final render (File > Render Image, `mitcad-cli render`, mitcad#48)
  runs the same worker in a batch mode on a job file (scene, environment,
  camera, output settings): it writes the image file itself (PNG and
  JPEG with the view's exposure and view transform, OpenEXR as linear
  light) and reports progress and previews on stdout; the application
  stays usable and can cancel it.
- **Updates** (`app/update/`): see [updates.md](updates.md).
- **Feedback and error reports** (`app/report/`, mitcad#61, mitcad#62):
  Help › Send Feedback and the reports offered after a crash or an
  internal error share one path: a `report::Report` of sections, masked
  (home folder, other absolute paths, user and host names) before the
  preview, where the user edits, leaves out or cancels; then the issue
  tracker's form, filled in through its address and opened in the
  browser. Mitcad holds no token and sends nothing itself; a relay
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
  otherwise). On Linux the executables export Mitcad's own symbols
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
  device pixel ratio.
- **Theme and icons** (`app/framework/Theme`, `Icons`): dark mode is read
  from the application palette (`isDarkPalette`), and the error, warning
  and accent colours and the `set*StyleSheet` helpers follow palette
  changes. The SVG icons are rendered on demand at the asked size and
  device pixel ratio, with a light outline on a dark palette.

## Window layouts and the macOS layer

`chromeStyle()` (`app/framework/ChromeStyle`) picks the layout at start-up:
**Docked** (Windows and Linux: the ribbon in a toolbar, the panels in docks)
or **Floating** (the default on macOS; `--chrome=docked|floating` overrides
it). `MainWindow` builds either from the same panels, so object names and
the positions the UI tests click are the same in both.

Floating is a full-window 3D view with the panels over it:

- `TitleBar`: one toolbar-high row under the native title bar (the content
  extends under it; empty parts move the window): Undo and Redo, the
  ribbon's tabs as a segmented control, the document name with the project
  indicator beside it, the command search and, in a sketch, Finish Sketch.
  Under it the ribbon in
  `Presentation::Mac` (capsule groups whose name opens a menu of all their
  commands), and the update and remote notices.
- `GlassCard`: the rounded Browser, Command and Timeline cards and the
  status pill. The Command card shows only while it has something to say.
  There is no status bar; messages go to the pill.
- `FloatingLayout`, the central widget, owns all card geometry: cards are
  anchored to the view's corners (Browser upper left, Command upper right
  under the orientation cube, Timeline along the bottom), sized by their
  content up to a share of the view, laid out in one queued pass. It gives
  the view the free margins (`OcctViewer::setOverlayInsets`) and covered
  rectangles (`setCoveredRects`): the cube and its buttons stay clear of
  cards, picks and logged click positions avoid them, and Fit, Home and the
  named views fit the model into the free area (`fitToFreeArea`). Cards
  take the mouse only inside their rounded rectangle.

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
(`GlassCard::forwardKey`); logged positions are in host window coordinates
(`GlassCard::mapToHost`).

Dialogs modal to the main window are sheets on macOS (`app/framework/Dialogs`:
`prepareModal`, `sheetWarning`, `sheetInformation`, `sheetQuestion`). On
macOS Preferences is `SettingsWindow`, a pane per Preferences page whose
changes apply as they are made, and About is the standard About panel.

**The platform layer.** `app/platform/MacChrome.hpp` declares the
AppKit-only functions in `mitcad::mac`: SF Symbols as icons in the palette's
colours (`symbolIcon`), menu images hidden in the menu bar and shown in the
ribbon's menus, the hidden native title, the glass child windows, the About
panel and `captureOwnWindows` (the application's own windows as the window
server shows them, for tests). `app/platform/macos/MacChrome.mm` implements
them (Objective-C++, ARC); elsewhere they are inline stubs in the header, so
portable code calls them without `#ifdef`. `Q_OS_MACOS` branches in portable
files are for Qt-level behaviour only (the GL format, shortcuts).

## Project file

Single-file format, version 2:
`{"format": "mitcad", "version": 2, "units", "parameters", "features":
[{"uid": "F3", "type": "extrude", ...}], "bodies", "marker"}`, plus
`components`, `occurrences`, `active_component`, `root_component`, a
feature's `component` when present, named `views` and `analyses`. Version 1 files
are converted on open. Parametric geometry is not stored. Imported bodies
live in base features as OCCT binary BRep, zlib + base64
(`{"format": "occt", "compression": "zlib", "size": n, "data": "…"}`,
`core/model/src/features/base.rs`); their faces are named
`<feature>:import(<j>)`.

**Projects** (version 3): a folder with the marker `.mitcad/project.json`.
Project files inside keep base-feature B-rep data in a content-addressed
store, `.mitcad/brep/<aa>/<sha256>.brep.zlib`, referenced as
`{"format": "occt", "compression": "zlib", "size": n, "sha256": "…"}`.
Store files are written once, before the project file, so the JSON stays
small and diffable. A file is version 3 only when it has such references.
Opening resolves them from the nearest enclosing project and checks size
and hash; missing data fails its base feature with a message. The
per-user display state (Origin folder, Isolate) is kept out of versioned
files in `.mitcad/local/display/<path>.json`. Autosave and the import
process always write single files (version 2).

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
`load_project`, `from_json_at`, `from_json_in` with a `BlobStore`
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
own merge treat a project file changed on both sides as a whole-file
conflict instead of producing valid JSON of a broken design. Temp files
of an interrupted save are `.<name>.<process>.<n>.tmp`. On Windows the
rename is retried for up to 2 s while another program holds the file
(`file::rename_retrying`).

### Version history

`core/vcs`, gitoxide (`gix =0.88.0`, default features off: only `sha1`
and `index`; a default feature pulls in an MPL-2.0 crate). No gix types
in the crate's API. JSON commands: [commands.md](../core/model/src/api/commands.md)
("Version history").

- Mitcad commits only when the project folder is the root of a git
  repository, so a user's enclosing repository is never touched.
- A version is a commit built from HEAD's tree plus the saved paths (like
  `git commit --only`), adding the B-rep files they reference and removing
  unreferenced ones. `index.lock` is held for the round and only committed
  entries change, so the user's staged changes survive and `git status`
  stays clean. No commit when nothing changed, HEAD is detached, or a
  merge/rebase is in progress.
- Author: the repository's own `user.name` and `user.email` (New Project
  and Project Settings write them), else git's configured user, else
  Preferences' default author. Messages end with trailers
  `Mitcad-Version` and `Mitcad-Format: 3`.
- A file's history is HEAD's first-parent chain, following content-
  preserving renames. Older versions are read from the commit tree with
  `GitTreeStore`, no checkout. Restore writes the old content back as a
  new version; history is never rewritten.
- **Comparison** (`core/model/src/diff/`): two document snapshots are
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
outside Mitcad (`status`'s `renamed_from`) takes its display state along
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
are cancellable and time out after 5 minutes without output. Mitcad never
handles passwords: URLs with credentials and `ext::` helpers are refused,
and credentials are hidden in output.

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
commands ([commands.md](../core/model/src/api/commands.md), "Projects").

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
Open Project, Open from Cloud, Move to a Project and opening or saving a
design set it (`setCurrentProject`, `followFileProject`); an untitled
design keeps it. `RemoteController`, the indicator and Project Settings
use its root, never the design's path (before mitcad#89 the remote looked
for the project through the design's file and could not find a project
just made).

- Dialogs (`files/ProjectDialogs`, `files/ProjectSettings`): New Project,
  Open from Cloud, the design chooser, Move to a Project's question,
  Change Address and Project Settings. The first three and Change Address
  share `CloudSection` (`files/CloudSection`): the service, the address
  composed of account, repository and HTTPS or SSH (`files/CloudAddress`,
  plain Qt Core, unit-tested; a typed address fills the fields back), and
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
- Hooks for the edit locks and live updates (the lock controller, below):
  `ProjectIndicator::setLockText`, `setLiveText`, `setLockActions`;
  `ProjectSettingsHost::stoppingSync`, `settingsChanged`, `probeLocks`,
  `testBroker`; `MainWindow::m_openingReadOnly` while Open Read-Only
  opens a design.

### Edit locks

In a Cloud project a design has one editor at a time (mitcad#89,
`core/vcs/src/remote/locks.rs`). The lock is a git ref on the remote
outside the branches, so it works with any git host that accepts such
refs; it is advisory (Mitcad honours it, other git tools do not see it).

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
- Requests go stale the same way, so that a requester whose Mitcad was
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
commands and events: [commands.md](../core/model/src/api/commands.md#live-updates)).
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
are ordinary project files, usually with a **configuration table**
(`core/model/src/configurations.rs`: named rows of parameter expressions
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
- **Versions per part.** A part records the commit the user chose when
  inserting it; `update_links` reads that commit, never a newer one, and
  only `update_library_parts` (an undo step) moves a part to another
  version or row. Nothing checks for newer versions on its own.
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
- **Community index:** a repository with `mitcad-index.json` and an entry
  per library (URL, licence, components, reviewed versions); search runs
  over the fetched indexes and libraries locally, with a licence filter;
  items without a licence are hidden unless asked for.
- **Publishing:** a library of one's own is a Mitcad project folder with
  version history; the library commands write its manifest and items, and
  the version history and remote commands record and push it.

In the app (`app/files/MainWindowLibraries.cpp`, `LibraryDialogs.cpp`)
fetches run as `RemoteTask`s on their own thread with Cancel; the other
library commands are local and run on the UI thread; inserts and updates
are model commands (jobs on the model worker).

### Autosave

Autosave writes unsaved work as a version 2 project file with a metadata
file and a session lock to a recovery folder in the user's local app data,
never into the project (format: [app/COMMANDS.md](../app/COMMANDS.md),
Files). It writes only when the revision changed. On start the app offers
sessions whose lock is stale; a recovered document keeps its file path
but is modified and has no undo history.

## Imports with history

`.f3d` designs and FreeCAD documents are replayed as Mitcad features,
each step checked against what the file stores (the `.f3d` ASM history;
FreeCAD's per-feature shapes). What cannot be translated or does not match
falls back to a base feature of the stored bodies, and the replay
continues on it. The `.f3d` history is matched by its solids; its surface
bodies come in with the fallbacks as stored bodies, which joins and cuts
without participants then leave alone. Both run over the `Kernel` trait,
so they are testable on the mock kernel; the app runs them in the import
process.

- `.f3d`: the design streams are decoded into the dump IR
  ([SCHEMA.md](../core/import/SCHEMA.md)), which external dumps (route A)
  also use; formats in [ASM_FORMAT.md](../core/f3d/ASM_FORMAT.md) and
  [OGS_FORMAT.md](../core/f3d/OGS_FORMAT.md). The file's single timeline
  is split by component: each item goes into the component whose bodies
  it changes, else the one that owns it, and a feature that uses another
  component's sketch gets a copy moved by the occurrences' placements at
  its point of the timeline (features work only on their component's
  bodies). Occurrences take the placements the file gives them at the end
  of its timeline (where the timeline's items put them last, which the
  decoder reads per item, mitcad#81; else the last captured positions,
  else the stored transforms), and the file's captured positions come in
  as `capture_position` features at their points of the timeline;
  occurrences of components of other documents (inserted parts,
  fasteners) are occurrences of empty components. The file's joints come
  in as joint features only where they hold where the file places the
  occurrences (nothing the import adds moves an occurrence elsewhere),
  else as as-built joints of the placements; whether a joint holds is
  computed from its frames before it is added, so a joint item costs one
  recompute (mitcad#87). Rigid groups come in as rigid groups.
- FreeCAD: `core/freecad` reads the archive and XML (`roxmltree`, DTDs
  refused); enumerations are indices, mapped per FreeCAD version through
  `core/freecad/data/enums.json`.

How both work, their limits and corpus tests:
[core/import/README.md](../core/import/README.md). Real files are kept
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

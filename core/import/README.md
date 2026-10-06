# mitcad-import: .f3d and FreeCAD designs with their history

`core/import` replays a design's history as Mitcad features, checks each
step against the bodies stored in the file, and brings in what it cannot
replay as the stored bodies. Two imports live here:

- **.f3d** (`src/*.rs`): the design dump IR
  (`mitcad_f3d::design::ir::Dump`, [SCHEMA.md](SCHEMA.md)), checked
  against the file's ASM history.
- **FreeCAD .FCStd** (`src/freecad/`), checked against every feature's
  stored result.

Both are generic over the [`Kernel`](../model/src/kernel.rs); the tests run
them on the model's mock kernel. The JSON commands and report fields are in
`core/model/src/api/commands.md` (*.f3d import with the timeline*,
*FreeCAD import (.FCStd)*).

## .f3d import

Entry points:

- `mitcad-cli import-f3d part.f3d [--save part.mitcad] [--report
  report.json] [--design <doc>] [--dump dump.json] [--no-verify]
  [--no-fallback] [--no-compare] [--time-limit S] [--hang-limit S]
  [--json]`; `--bodies-only` imports the bodies without the history;
- the document command `{"cmd": "import_f3d", "path": …}` and
  `Document::import_f3d_timeline(path, json)`, implemented in
  `core/ffi/src/f3d_import.rs`;
- Rust: `mitcad_import::import_design(&mut doc, &dump, &mut geometry,
  &options)`.

### Modules

| Module | |
|---|---|
| `params.rs` | User and model parameters with the file's names and expressions, in dependency order. Expressions that do not evaluate become values; names Mitcad reserves get `_1`. A model parameter is adopted by the feature or sketch that made it (`Document::adopt_parameters`). |
| `sketch.rs` | Points, lines, circles, arcs (counter-clockwise), ellipses, splines (control points, weights, full knots), texts (style, flips, multi-line frame and alignment, text along or fitted to a path), constraints and dimensions by type. Ids are renumbered into Mitcad's one id space (points first). Positions are the file's stored solution; the sketch is accepted when Mitcad's solve keeps them (1e-4 mm), else retried with driven dimensions, then without constraints (`partial`). |
| `features.rs` | Sketch planes (origin planes; imported construction planes with the exact frame as `frame`; planar faces found geometrically; else a fixed construction plane), construction planes (offset from an origin plane or a face found by distance, else fixed), extrude, revolve, fillet, chamfer. |
| `ops.rs` | Combine, mirror, circular and rectangular patterns, shell, offset faces, move, split body, hole, thread, replace face — from external dumps (SCHEMA.md §2), or from the history when the stream decoder gives no inputs (see below). |
| `sweeps.rs` | Sweeps, pipes and lofts from external dumps. |
| `components.rs` | Components and occurrences. |
| `refs.rs` | Fingerprints resolved against the replay (bodies by volume and name, surface bodies by name or area, faces through `point_on_face`, edges by midpoint and length), planar faces, the edges a fillet consumed, the faces a replace face replaced, the final comparison. |
| `history.rs` | The bodies stored in the file (`StoredGeometry`: the ASM history states and the stored design) and matching of body sets (volume, area, centre of mass, face count). |
| `report.rs` | The import report (JSON and text). |

### The history as the reference

`core/ffi` reads the ASM history of the `.smbh` blobs and rolls it back
operation by operation. A timeline item's `_f3d.result_no` is the number of
the ASM state its operation made. The states of all blobs are merged in
timeline order into one sequence, oldest first:

- an operation no item names gives one state, before the blob's next named
  one;
- components inserted from other designs bring their own histories, which
  come last;
- without item numbers (external dumps) states are merged by their
  numbers;
- the last state is the stored design.

Matching rules:

- An item with a known state must give exactly that state (measures within
  1e-3). First the replay is brought to the state before it, by a base
  feature for what earlier items changed without being replayed.
- An item without a known state must equal one of the next states:
  exactly (measures within 1e-5 and the same face count), else the closest
  within 1e-3.
- Lofts with end conditions or rails match within 5e-3, with a warning
  beyond 1e-3. Mitcad builds their weights, angles and free ends by its
  own rule (`commands.md`, `loft`), while the file's surfaces may be
  fitted otherwise (between round sections the stored ones are not round,
  0.3–1.2 % below that rule; stored rails with end conditions differ too),
  so most of those fall back.

### What the history settles

The history picks what the stream decoder cannot tell, by trying
candidates against the next state:

- **Face profiles.** An extrusion without a decoded profile may extrude a
  planar face: faces whose area times the distance equals the state's
  volume change become a sketch on the face with its boundary projected
  (linked), and that region is extruded (outwards first; inwards first for
  a cut; on the side of the state's new body). These are tried before the
  last sketch before the item. A join that touches no body is also tried
  as a new body when the state has one more.
- **Profiles.** The decoder knows how many profiles an extrude used, not
  which. In order:
  1. regions whose material the next states change, found by probing a
     point inside each region (in its outline, not its holes:
     `Region::holes`) halfway along the extent on both sides
     (`Kernel::points_inside`); this also merges regions split by
     undecoded construction lines;
  2. sets of any number of regions whose area is within 5 % of the state's
     change over a distance extent's length (bounded subset-sum search,
     nearest six);
  3. subsets of the same count, ranked by how close their prism volume
     comes to the state's change.

  Not tried: a distance extrusion without taper whose prism holds less
  than the state adds or removes (overlaps only take from a prism), and
  one limited to participants that include every body the same
  definition without them changes (same result).
- **Participants** of a join, cut or intersection: all bodies first, then
  the bodies of the sketch's component that the state changed (a cut in
  the file may leave bodies in its way alone).
- **Direction** (the decoder's ±1 and its alternative), symmetric extents,
  **up to an object** (planar faces parallel to the sketch, nearest
  first), **revolve axes** (sketch lines and origin axes, ranked by
  Pappus' volume).
- **Fillet and chamfer edges.** Candidates are the replay's edges whose
  midpoints are off the next state's faces (`Kernel::boundary_distances`,
  seams left out). An edge is assigned to an edge set when a rounding or
  bevel of the set's size fits: the points where it would meet the edge's
  two faces (projected onto them) and the middle of its cross section
  (convex or concave by a point classified just off the edge) lie on the
  state's faces (within 5 % of the size and 1e-3 mm), and the edge's
  midpoint is as far from the state as that size leaves at its corner
  angle (within 10 %). Tried in order: all those edges at once; one
  tangent chain after another (sets and edges in order); then looser
  guesses (the edges at that distance, all, the largest group at one
  distance).
  - A rounding or bevel that removes a neighbouring face exactly (full
    round, chamfer as wide as a step), which OCCT cannot build, is built
    1e-3 smaller (`geometry/src/dressup.cpp`).
  - A result BRepCheck rejects is repaired by shape healing when the
    repair keeps the volume (OCCT turns some roundings on stored bodies
    inside out).
- **Combine** target and tools: the bodies the next state changed.
- **Replace face's replaced faces** (dumps do not record them): faces of a
  body the next state changed none of whose interior points
  (`Kernel::face_points`, up to nine) lie on the state's faces (1e-3 mm);
  neighbours that extend or shorten keep some. They are listed without the
  tangent chain, and the result is matched within 0.5 % (Mitcad's own
  conventions for curved faces and targets). The target comes from the
  dump.
- **Holes** without decoded inputs: each piece of material the state
  removes is a hole along its cylinder's axis, starting at the plane of a
  planar face across the axis at either end of the piece (where the axis
  meets it); simple, counterbore or countersink by the sizes the item has;
  to its depth or through all.
- **Patterns** without decoded inputs: copies of one of the last
  extrusions about the decoded axis (else the origin axes), as many as the
  decoded quantities (else as the state's volume change asks for); the
  extrusion and quantity whose copies come closest to the change first;
  computed `adjust`, then, for an extrusion up to an object, `identical`
  (same result by distances).
- **Mirrors** without decoded inputs: sets of up to four preceding
  features whose changes in the history add up to the state's change.
  Bodies the state has twins of become new bodies; a body is joined with
  its image where the state has a body of twice its volume; else each
  body. The plane is the decoded one, else the origin planes and planes
  through the bodies' and images' centres (as a planar face on them when
  there is one).

### Fallback

- An item with a known state that cannot be translated, fails, or does not
  give its state takes that state at once: one base feature (`base` with
  `replaces`; the changed bodies keep their ids, so later features
  continue on the stored bodies).
- Any other failing item waits. The next feature is tried on the bodies as
  they are and on each of the next history states put in by one base
  feature. The first combination that reaches a state settles it: the
  waiting items become `fallback` (they made that state, or added bodies
  to the state the next feature reaches) or `skipped` (the history shows
  no change by them). Items still waiting at the end take the last state.
- Without a history the first failure replaces every body with the stored
  design and later modelling items are skipped.
- **Final comparison:** the replay is compared with the stored bodies by
  volume; with `compare`, bodies whose measures differ by more than 1e-4
  also by surface deviation and boolean differences
  (`Kernel::compare_shapes`), for at most a minute (booleans of coincident
  or nearly coincident shapes can take minutes or not return). Bodies that
  still differ are replaced by the stored ones (a warning names the base
  feature).

**States that do not rebuild.** A rolled-back ASM state whose conversion
loses faces or does not build is unknown (the report counts them): no base
feature takes it, and an item with that state is taken unchecked (its
first definition that builds). `mitcad-f3d-inspect history` names the
problem edges. Two ASM quirks are handled by the conversion:

- ASM puts the seam of a full cylinder on the line where it touches planar
  faces (a hole tangent to a wall), so the edge is used twice in each
  direction; it is taken as closed.
- B-spline edges whose ends are up to 0.004 mm off their vertices build
  with wider tolerances (gaps up to 0.01 mm accepted).

**Bodies the history already holds.** An item whose state lies behind the
replay (a base feature took the bodies past it; ASM states of reordered
items or other components can precede items the timeline puts earlier)
continues on those bodies: its first definition that leaves them unchanged
(a join of material they already hold) is taken, verified by them. Without
a history the same holds after the fallback to the stored design.

### Time limit, stop and hangs

**Time limit.** `time_limit` (`--time-limit`) seconds after the start, the
remaining items take their history states without trying definitions, and
the final bodies are compared by volume only. The application sets none:
its imports run to the end unless the user stops them.

**Stop.** `Options::stop` is a `RecomputeMonitor` that another thread
cancels. `mitcad-ffi` passes the document's job (`import_f3d` under
`attach_job`); the application's import worker cancels it on Stop and Keep
What Is Imported (`app/files/F3dImport.cpp`). Once cancelled:

- the item being replayed and those after it are given up as after the
  time limit (note "the import was stopped"): with a history they take
  their states, without one the stored bodies stand in; sketches and
  construction geometry still come in;
- while an item's definitions are tried, the request is the document's
  monitor, so the kernel's long operations stop inside evaluation and the
  definition fails at once; nothing else (fallbacks, undo) runs with it;
- the final bodies are compared by volume only;
- the report says where it stopped (`stopped`: the item's `index` and
  `name`; no `item` when the stop came after the last one) and warns.

The monitor's test delay slows each definition tried
(`tools/ui-import-test.sh`). A try that hung after a stop is rerun with
`Options::stop_at`: items before the one it stopped at
(`Progress::stopped`, or the one it hung on if it had not seen the stop
yet) are replayed, the rest given up.

**Hangs.** A kernel call cannot be interrupted. With `hang_limit`
(`--hang-limit`; `test_f3d_import --corpus` defaults to 600 s) the import
runs on its own thread (`core/ffi/src/f3d_import.rs`) with a 256 MiB stack
(OCCT recurses deeply; on a default 2 MiB stack features failed from
overflows) and shares its progress (`Options::progress`: a step counter and
the current item). When the step does not move for that many seconds, the
thread is abandoned and the import reruns on a new document with the item
in `Options::hung_items`: it takes its history state without trying
definitions (note "the geometry kernel did not finish a definition of
it"), and the report warns. A hang while comparing the final bodies makes
the next try compare volumes only; a hang elsewhere brings in the stored
bodies without the timeline. `MITCAD_IMPORT_STALL=compare` simulates a hang
for the test. A tapered extrusion whose arcs shrink past their apex, on
which OCCT's draft loops, is refused before the draft ("the taper closes
the profile", `geometry/src/sweep.cpp`).

### Components

`components.rs` makes the dump's components and places them by the
occurrence tree before the replay:

- placements: the decoder's top-level `transform` (relative to the root)
  and nested `_f3d.local_transform` (relative to the parent), or an
  external dump's `transform2` (full path) made relative to the parent
  occurrence; cm become mm; grounded and hidden occurrences stay so;
- a component used several times comes in once and is placed by each
  occurrence;
- occurrences of components of other documents (`isReferencedComponent`)
  are left out: their bodies are not in the file.

The file has one timeline for the whole design; every item goes into a
component:

- an external dump names it;
- for the decoder, the components whose ASM blobs the item's operation
  changed (`StoredGeometry::item_components`; each component keeps its
  bodies in its own blob, in its own coordinates);
- sketches and construction geometry follow the first feature that uses
  them; other items follow the next item with a component (else the one
  before);
- a definition goes where the bodies, sketches and construction geometry
  it names are when they agree, else into the item's component.

New-component operations become new bodies of that component. Fallback
base features are made per component, and the final comparison wants
every body in its component (a body left in another one is replaced
there). The report counts components, occurrences and items per component
(`components`) and names each item's component.

A component whose bodies are stored only in an `.smb` blob (no `.smbh`
history) has no features; its solids join the stored design unless one of
the last state's bodies is the same, and come in with the final
comparison. The `.smb` blobs of components with a history also hold
feature tools and sketch faces, which stay out. Joints, as-built joints,
occurrence items and other assembly items change no body and are skipped
(occurrence placements come from the tree).

**Across components.** A feature that uses a sketch of another component
gets the sketch imported again into its own component, moved by the two
occurrences' placements onto a planar face of its component or a fixed
construction plane, so it stays parametric. Features that work on another
component's bodies (a combine of bodies of two components, a cut up to
another component's face) fall back: the base feature takes the item's
history state in the item's component, and later items continue
parametrically. Mitcad's features work only on their own component's
bodies.

### Light bulbs

`Importer::light_bulbs`, after the replay: each imported sketch and
construction plane, axis and point keeps the file's light bulb only where
it differs from Mitcad's default (sketch hidden while a feature uses it,
construction geometry shown) — a used sketch the file shows, an unused one
it hides, a hidden plane. A sketch that follows the default keeps nothing,
so deleting its last consumer shows it again.

The light bulb is the item's `props.isLightBulbOn` (external dumps and the
decoder), else a sketch's `detail.isVisible` (external dumps); otherwise
the default decides. The decoder reads it for sketches of the current file
format (sketch class version 18, most of 17) and for construction planes;
older sketches, construction axes and points stay unknown. Copies of a
sketch imported into another component follow the default. The report
counts sketches and construction features, those with unknown light bulb
and those kept (`light_bulbs`).

### Crashes

OCCT can crash on unusual input (a fillet on imported geometry); the
importer turns crashes into errors (`mitcad::geometry::catch_occt_crashes`,
OCCT's signal handlers). On Linux that needs `OCC_CONVERT_SIGNALS`, which
OCCT's CMake config sets only for Debug and Release; the root
`CMakeLists.txt` sets it for every configuration. A crash that corrupts
memory cannot be caught: a fillet with a strip 1e-4 wide did, hence the
1e-3 of the dressup retry.

### Limits

- The stream decoder gives no fillet or chamfer edges, hole positions or
  types, pattern or mirror objects (it gives pattern quantities, circular
  pattern axes and mirror planes when they are origin or construction
  geometry), move or split inputs, construction flags of sketch curves,
  splines, ellipses or texts. Without a history those items become the
  stored bodies. External dumps can record all of them; only the reference
  models have such dumps.
- Features that use another component's bodies fall back (*Across
  components*).
- Fillets with G2 continuity or mid radii, asymmetric fillets, and
  chamfers with miter or blend corners fall back: Mitcad does not build
  them, and they are not approximated by something the history might not
  tell apart.
- Replace face needs the history (for its faces) and its target in the
  replay: a surface body the replay lacks makes it fall back unless the
  target face is planar (then a fixed plane).
- A hole's tapped thread is not decoded: the hole comes in with its drill
  size (a cosmetic thread changes no body).
- Sweeps, pipes and lofts are translated from external dumps only
  (`sweeps.rs`; `commands.md`, `sweep`, `loft`, `pipe`); the decoder
  gives only their parameters, so from an `.f3d` they become stored
  bodies.
- Not translated: coils, ribs and webs (no recorded inputs), sheet bodies
  (`isSolid` false), concentric circle dimensions, polygon, pattern and
  offset sketch constraints (copies and offset curves come in as plain
  curves).

### Corpus test

Real designs are kept outside the repository (`MITCAD_F3D_CORPUS`, default
`~/f3d-corpus`). `test_f3d_import` (`core/tests/test_f3d_import.cpp`):

```bash
test_f3d_import --corpus [dir] [--every N] [--max-items N] \
    [--time-limit S] [--hang-limit S] [--reports DIR]   # build/rel/core
mitcad-cli import-f3d part.f3d --time-limit 300 --report report.json
```

- `--corpus` imports every design of every `.f3d`/`.f3z` under the
  directory, requires each import to finish without error, and prints
  per feature type how items came in (parametric, partial, fallback,
  skipped) and how the final bodies agree with the stored ones. Exits 77
  (skipped) without a corpus. Files are named by position (f01, …), as in
  `core/f3d/CORPUS_REPORT.md`.
- `--reports DIR` keeps each JSON report; `--hang-limit` defaults to 600 s.
- `--models [dir]` replays the reference models' external dumps
  (`MITCAD_F3D_MODELS`, default `~/f3d-models`) and compares with the
  stored volumes.
- The ctest `f3d.corpus_timeline` (`core/CMakeLists.txt`) runs `--every 4
  --max-items 30 --time-limit 60` to stay quick in debug builds. Run the
  full corpus with the release build, e.g. `--time-limit 300 --hang-limit
  300` per design.

Known failure classes:

- extrusions whose result is not the state: regions the decoder cannot
  tell apart, sketches with undecoded curves whose profiles are too small
  for the change, joins and cuts across bodies of several components;
- fillets and chamfers OCCT does not build (`fillet failed` / `chamfer
  failed`), mostly on the right edges: the stored rounding runs over a
  neighbouring face, which OCCT does not do, or the body is a stored one
  with tolerances up to 0.05 mm;
- items after the time limit (very large designs);
- patterns whose input is not one of the last extrusions;
- items whose state an earlier fallback took the replay past, mostly
  fillets and cuts, which cannot leave the bodies unchanged, so they stay
  skipped or fall back;
- undecoded items (`?…`), joints and other assembly items, construction
  axes, and the untranslated types under *Limits*, which are skipped or
  fall back.

## FreeCAD import (.FCStd)

A FreeCAD document is a zip with `Document.xml` (every object and its
properties), `GuiDocument.xml` (visibility, colours) and the OCCT B-rep of
every shape-bearing object's result as FreeCAD last computed it, in its
container's coordinates. `core/freecad` reads the archive and XML
(`roxmltree`, DTDs refused) into typed objects without a kernel.
Enumerations are stored as indices, so it carries each FreeCAD version's
value lists (`data/enums.json`, read from FreeCAD by
`tools/freecad-export/enums.py`).

Entry points: `mitcad-cli import-fcstd <file.FCStd> [--save out.mitcad]
[--report report.json] [--bodies-only] [--reference dump.json] [--set
name=expression]... [--json]`; the `import_fcstd` command, run with OCCT by
`core/ffi/src/fcstd_import.rs`. The application runs it in the import
process, like the `.f3d` import.

### Stage 1: structure and stored bodies

`src/freecad/mod.rs`: App::Part and Assembly → components; Bodies and the
Part workbench's leaf objects → base features of the stored shapes (one
per component); App::Link → occurrences (arrays, links of links, links to
other files read next to it); visibility, colours and the document's unit
system. Placements come from the file, not from attachments. Datums and
joints are left out; features inside Bodies and consumed operands are part
of their results. The report says why for each object. `--bodies-only`
stops here.

### Stage 2: sketches

`core/freecad/src/sketch.rs` reads geometry, constraints and external
geometry; `src/freecad/sketch.rs` translates, `sketches.rs` places and
checks.

- FreeCAD's curves own their points and Coincident constraints join them;
  joined points become one Mitcad point (union-find), as in the `.f3d`
  import.
- Hyperbolas and parabolas become exact rational splines; a B-spline's
  control-point circles become its control points; an ellipse's axis
  lines stay construction lines held on it.
- The plane is an origin plane, a face of an imported body
  (`<feature>:import(j)`) or a fixed construction plane; the definition's
  `frame` keeps FreeCAD's exact placement.
- External geometry on imported bodies becomes linked projections by
  element name. The kernel's `indexed_element` gives a shape's face, edge
  or vertex by FreeCAD's index (OCCT `TopExp::MapShapes` order); every link
  is checked against the same element of the referenced object's stored
  shape.
- A sketch is accepted when Mitcad's solution keeps FreeCAD's points
  within 1e-4 mm (else with driven dimensions, else without constraints)
  and is compared with its stored shape. What Mitcad cannot express
  (tangency with ellipses, Snell's law, distances to circles,
  intermediate features' elements) is listed, and the sketch is
  `partial`.

### Stage 3: history

`history.rs` drives it, `features.rs` translates PartDesign features,
`part.rs` Part workbench objects, `primitives.rs` objects without a Mitcad
feature of their own, `elements.rs` references.

- Every PartDesign Body and every Part workbench result stage 1 made a
  body of (extrusion or revolution of a sketch, primitive, operation) is
  rebuilt as Mitcad features, with the Bodies its booleans use and the
  operands of its tree. All objects, sketches included, come in
  dependency order (ties in document order).
- The file keeps every feature's own result, so each replayed feature is
  checked against FreeCAD's stored shape: volume, area and centre of mass
  of the Body's Mitcad bodies within 1e-6 relative, and the number of
  solids. Where FreeCAD's conventions are uncertain a few readings are
  tried (directions, taper signs, a chamfer's first face, which sketch
  regions are material).
- A feature that cannot be translated, fails, or does not give FreeCAD's
  shape becomes a base feature of its stored shape that replaces the
  Body's body (`replaces`, keeping its id); later features continue on
  it. One whose stored shape equals its base's is skipped.
- References (`Pad.Face6`, `Edge12`) are taken from the referenced
  object's stored shape and found by geometry in the Mitcad body holding
  its result — first as it is now, else as it was right after the object
  (the name then carries through later features). An edge FreeCAD merged
  is all of Mitcad's pieces along it.
- A Body's tip before its last feature suppresses the features after it.
- Bodies are named and shown as in FreeCAD; colours are not kept (Mitcad's
  appearances are a library).
- `primitives.rs` builds: a cone, a prism, a part of a turn as a section
  sketch on a fixed plane turned or extruded (a cone's and a partial
  cylinder's sizes are dimensions of that sketch); a wedge as a ruled
  loft; an ellipsoid (or a part of one: its section turned, then scaled)
  as a scaled sphere; a pad along a slanted direction as a sweep along a
  fixed line; a MultiTransform as a chain of patterns and mirrors, each
  one a pattern or mirror of the one before (the model's patterns of
  patterns). The model's `helix` feature and the hole's counterdrill and
  taper exist for FreeCAD's helices and holes.
- A threaded hole is a hole plus a cosmetic thread (ISO metric, Unified,
  Whitworth incl. BSP `G 1/4`, NPT). A modelled ISO metric or Unified
  thread becomes a modelled `thread`, but Mitcad's basic profile cut from
  FreeCAD's bore differs from FreeCAD's thread by about 1e-3 of the
  volume, so these fall back.
- Datum planes: attached flat with only a normal offset → `offset`
  plane; attached to an origin plane and turned a quarter (or not), with
  the offset along its normal → `offset` from the parallel origin plane;
  turned about the support's x or y axis → `angle` plane; otherwise
  `fixed`.

### Stage 4: parameters and expressions

`params.rs`; `core/freecad/src/expression.rs` parses FreeCAD expressions
into a tree whose references the import resolves; `spreadsheet.rs` reads
cells.

- Before the timeline, user parameters are made from spreadsheets'
  aliased cells (and cells expressions name by address), VarSets'
  properties, sketches' named constraints, and feature properties other
  expressions refer to — in dependency order, with FreeCAD's names where
  Mitcad accepts them.
- Every expression bound to a value the import carries over (a feature's
  length or count, a sketch's dimension, an attachment offset along a
  normal as an offset construction plane, a sketch or datum plane turned
  about its support's x or y axis as an `angle` construction plane, a
  side offset that moves a quarter-turned plane along its normal as an
  `offset` from the parallel origin plane) becomes that value's Mitcad
  expression.
- The translation maps references, units, constants, the conditional and
  functions (written out where Mitcad has no equivalent) and keeps
  FreeCAD's precedence. An integer property's expression is rounded as
  FreeCAD rounds it (`round(Count / 2)`).
- Each translated expression is evaluated and compared with FreeCAD's
  stored value; one that differs or does not translate keeps FreeCAD's
  value, with the reason in the report. Not translatable: a shape's
  measure (e.g. bounding box), side offsets that only move a sketch or
  plane within itself (kept as numbers), turns about other axes or with
  shifts (a fixed plane). Cells of quantities
  Mitcad lacks (e.g. kilograms) are left out.
- Expressions on objects kept as stored shapes (e.g. BIM objects) stay
  unused.

### Not translated (fallback)

Scaled transformations (Mitcad's patterns and mirrors use rigid motions;
its scale feature scales whole bodies); MultiTransforms of the whole body;
conical and growing helices (FreeCAD keeps the profile in planes through
the axis, Mitcad's helix is a screw motion) and helices cut outside;
modelled threads (see above; Whitworth and NPT are never modelled); ISO
tyre valve threads (left out of a `partial` hole); attachment turns about
other axes or with shifts become fixed planes; profiles that are
faces or chosen parts of a sketch; revolutions up to the last face; tapers
along custom directions; pipes with an auxiliary spine or binormal mode;
Part extrusions of faces and Draft objects; Part fillets of varying
radius; binders; Draft and Python objects (`Part::Part2DObjectPython`).

FreeCAD itself fails a revolution up to the first face, a groove up to a
face, and (0.21, 1.0) a draft pulled along an origin axis; the reference
models leave these out.

### Tests

Documents are kept outside the repository (`MITCAD_FCSTD_CORPUS`).
`tools/freecad-export` makes reference models and dumps with FreeCAD 0.21,
1.0 and 1.1 in the isolated distro
([development.md](../../docs/development.md#freecad-reference-models-and-corpus)).
The ctest `freecad.corpus` (`tools/cli/CMakeLists.txt`) imports every
document and compares those with a dump; for models with a variant it
changes a parameter in Mitcad (`import-fcstd --set`) and compares with the
variant FreeCAD recomputed after the same change. Some models need FreeCAD
1.0 (VarSets, extents up to shapes) or 1.1 (Whitworth and NPT threads). Ellipsoids and skewed cylinders are
B-spline and extrusion surfaces that FreeCAD's own measures integrate less
exactly (up to 4e-4) than Mitcad's; the reference check does not compare
those.

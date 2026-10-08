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
| `sketch.rs` | Points, lines, circles, arcs (counter-clockwise), ellipses, splines (control points, weights, full knots), texts (style, flips, multi-line frame and alignment, text along or fitted to a path), constraints and dimensions by type. Ids are renumbered into Mitcad's one id space (points first). Positions are the file's stored solution; the sketch is accepted when Mitcad's solve keeps them (1e-4 mm), else retried with driven dimensions, then without constraints (`partial`). `sketch/groups.rs`: polygons, sketch patterns, offsets and concentric circle dimensions (*Sketch patterns, offsets and polygons* below). |
| `features.rs` | Sketch planes (origin planes; imported construction planes with the exact frame as `frame`; planar faces found geometrically; else a fixed construction plane), construction planes (offset from an origin plane or a face found by distance, else fixed), extrude, revolve, fillet, chamfer. |
| `ops.rs` | Combine, mirror, circular and rectangular patterns, shell, offset faces, move, split body, hole, thread, replace face — from external dumps (SCHEMA.md §2), or from the history when the stream decoder gives no inputs (see below). |
| `sweeps.rs` | Sweeps, pipes and lofts (*Sweeps, pipes and lofts* below). |
| `components.rs` | Components and occurrences. |
| `joints.rs` | Joints, as-built joints, joint origins, rigid groups and ground items (*Joints* below). |
| `refs.rs` | Fingerprints resolved against the replay (bodies by volume and name, or by points on their edges, surface bodies by name or area, faces through `point_on_face`, edges by midpoint and length), planar faces, the edges a fillet consumed, the faces a replace face replaced, the final comparison. |
| `history.rs` | The bodies stored in the file (`StoredGeometry`: the ASM history states and the stored design) and matching of body sets (volume, area, centre of mass, face count). |
| `report.rs` | The import report (JSON and text). |

### The history as the reference

`core/ffi` reads the ASM history of the `.smbh` blobs and rolls it back
operation by operation. A timeline item's `_f3d.result_no` is the number of
the ASM state its operation made. Each component's history counts its own
operations (two components both have an operation 3), so the number names
the operation of the blob of the component that owns the item
(`_f3d.component`, *Components*) when that blob has it; else (a feature
that changes another component's bodies) of the other blobs with that
number that no item of their own component names, those whose bodies it
changed first. The states of all blobs are merged in
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
- An extrusion, fillet or chamfer that gives its known state only
  approximately (beyond 1e-5) tries up to eight more definitions and
  keeps the closest: a set of regions next to the right one, or the
  wrong reading of a two-distance chamfer or asymmetric fillet (which
  face takes which distance is not decoded; the other reading is tried
  right after each definition), can come within 1e-3, and the
  difference grows in later items. A fillet or chamfer is replaced only
  by one that comes closer by a quarter at least: another edge set's
  rounding can come a little closer to a stored spline rounding by
  chance (4.0e-4 against 4.4e-4) and keep later items from building,
  while a right one came closer by half or more in every design looked
  at. A difference the bodies before it
  carried (from an earlier approximate item, such as a fillet stored as
  splines) is not the extrusion's own: one that comes within 1.5 times
  that difference of its state is taken as it is (no definition comes
  closer).
- When an extrusion, or copies the history guessed (patterns and mirrors
  without decoded inputs), still gives its state only approximately, a
  base feature right after it brings the bodies to the state exactly (a
  warning names it): the item stays parametric, but its difference does
  not reach later items, where it keeps a later state (a split, a
  mirror, the stored design) from matching. Other approximations (fillets
  stored as splines) are carried on.
- An item that gives no state on bodies carrying such an approximation
  (beyond 1e-5 from the state before it) is tried again, in half the
  time, after a base feature brings them to that state exactly (a warning
  names it); an item without a state of its own tries the replay's
  current state that way too.
- An item without a known state must equal one of the next states before
  the state of the next item that has one (that state is the later
  item's; so are the fallbacks tried for it): exactly (measures within
  1e-5 and the same face count), else the closest within 1e-3. As it may match any of them, a join is not taken when its
  result, or the state it matches, has less volume on as many bodies than
  the bodies before it, nor a cut when it has more (a join can come
  within the tolerance of a later fillet's state by chance).
- Lofts with end conditions or rails match within 5e-3, with a warning
  beyond 1e-3. Mitcad builds their weights, angles and free ends by its
  own rule (`commands.md`, `loft`), while the file's surfaces may be
  fitted otherwise (between round sections the stored ones are not round,
  0.3–1.2 % below that rule; stored rails with end conditions differ too),
  so most of those fall back.

### Named inputs

Feature inputs in the streams name faces, edges and bodies as the body
blobs name theirs: every face and body of the ASM history carries
`generic_tag_attrib_def` names (a tag chosen by the operation that made
it and the operations that made and changed it), and an input's recipe
lists entities by such names (`core/f3d/src/design/recipe.rs`,
`core/f3d/src/names.rs`). The decoder rolls the `.smbh` blobs back to
just before the item (its `_f3d.result_no`) and finds them there
(`core/f3d/src/design/inputs.rs`):

- **Fillet and chamfer edges**: the edge between the faces named first
  and second; where several are, the one whose start and end touch the
  faces named third and fourth. Faces not in that state are looked for up
  to 24 states earlier (some recipes name a face as it was before later
  operations renamed it). The edge's middle point, length and ends go into
  its fingerprint; the import finds it by them and tries those edges first,
  then the history's guesses (*What the history settles*).
- **Bodies** of moves, splits, body patterns and mirrors: by their tags;
  the middle points of some of their edges let the import find them among
  its bodies (`refs::resolve_body`), else as the only body whose faces
  pass through those points (a replayed sweep or pipe has the stored
  surfaces but its seams and edges elsewhere). A mirror's joining is not
  decoded: separate copies are tried first, then joined ones.
- **Thread faces**: the face named first (a `bounded_face` recipe names
  the faces around it next, which pick one of several); its cylinder
  (origin, axis as the file has it, radius) and a point inside it go into
  the fingerprint, and the import finds the face through the point.
- Moves also get their transform, splits an origin or construction plane
  as the tool.
- Combines, splits, patterns, mirrors and holes (mitcad#67): their
  selections (list inputs of bodies, faces, edges, features and planes),
  as SCHEMA.md §5.3 lists; the import builds them first and then, where
  the history checks the item, tries what the history suggests without
  them, so an input decoded wrongly or not found never makes the item
  worse.

Sketch curves carry their kind (construction, centre line) in their
sketch-curve part; the decoder gives `isConstruction` and `isCenterLine`
where they are set, so construction curves no longer split profiles.

Parameters of the 2026 writers (class version 8) have their own layout;
before it was decoded their features had no parameters, and the import
fell back for most fillets, chamfers, revolves and patterns of such files.
A dimension's parameter also says whether the dimension is driven
(`isDriving: false`), and an offset dimension's flags whether it measures
a diameter about a line (`SketchLinearDiameterDimension`): sketches with
such dimensions keep their points once the parameters are known.

### Sketch patterns, offsets and polygons

The decoder reads what these constraints keep beyond their entity lists
(SCHEMA.md, *Constraint*): a circular pattern's quantity and total angle,
a rectangular pattern's quantities, distances and directions (the copies
are the entities after the originals), and an offset's parent and child
chains, distance and dimension, from the offset object that refers to the
constraint. `sketch/groups.rs` translates them into what Mitcad's own
sketch tools make, each checked against the stored geometry:

- **Polygon**: a construction circle about the centre through the
  corners, and equal sides (`sketch.polygon`).
- **Circular and rectangular patterns**: a `patterns` record whose copies
  are the file's copies, matched to the originals instance by instance
  (a full turn spreads the instances, a partial angle ends at it; a
  rectangular distance is the spacing or the whole extent, whichever the
  copies show). Mitcad's second direction is its first turned a quarter
  and its spacings are positive: a negative distance turns the direction
  and its spacing becomes `-(d8)`, and the two directions are swapped
  where that keeps them at a right angle. The angle and spacings refer to
  the file's parameters, so a parameter edit moves the copies.
- **Offset** of lines, arcs and circles: each offset curve parallel to its
  source at the distance (a `line_distance`), or concentric with it with a
  radius gap (below), all with one parameter, and an `offsets` record
  (curves, results, side as `sketch.offset` orders the chain). The file's
  offset dimension, whose parameter is signed by the side, stands for the
  offset: a negative one gets a parameter of its own,
  `<name>_offset = -(<name>)`, which the sketch adopts.
- **Concentric circle dimension**, and the radius of an offset arc: a
  construction line along a radius from one circle to the other,
  horizontal or vertical toward the dimension's text, its ends on the
  circles and the centre on it, whose length is the dimension.

### Threads, tapped holes and sheets

The decoder reads a thread's object (SCHEMA.md, *ThreadFeature*): its
type, size, designation, class, side, diameters and pitch, whether it is
modelled and over the whole face, and which end a partial one is
measured from; a tapped hole's thread is the hole's sub-item, read alike.
The translation (`ops.rs`, `commands.md` *From the dump IR*):

- **Threads**: a `thread` on the faces found through the history, with
  the size Mitcad's table lists (ISO metric, also the `M` sizes of other
  metric types; Unified; the parallel pipe threads `G <size>-<tpi>` as
  Mitcad's `G <size>`). A class Mitcad does not list for the face's side
  is left out (it only labels the thread), with a note. A partial thread
  starts at the end the file's cylinder axis points to (or the other):
  where Mitcad's cylinder of the face runs the other way, the ends swap.
  Forming screw threads and sizes Mitcad lacks fall back.
- **Sized faces**: a thread in the file, a cosmetic one too, also sizes
  its whole faces, an external one to the thread's major diameter, an
  internal one to its minor diameter (the reference models: a bolt of 10
  mm becomes 9.85 mm with an M10 thread). The translation first puts an
  `offset_face` of each face to that diameter before the `thread`; where
  that does not build (offsets of faces of stored bodies often do not),
  a tube of the material between the two radii along the face (two
  `cylinder`s, one combined out of the other) is cut from or joined to
  the body, and the thread goes on the tube's side; then the thread alone
  is tried. The offset is given 2 s (mitcad#68: offsetting a face of a
  large or stored body took 38 s where the tube then took 1.3 s). For a
  cosmetic thread the volume the sizing adds ranks them against the
  history's change, so a thread whose state shows no sizing is tried
  alone first.
- **Tapped holes**: the hole's own history state has the bore of its
  diameter parameter; the file's thread widens it to the thread's minor
  diameter in an operation of its own, which the import leaves out (about
  1e-4 of the volume). The hole comes in with that bore and a `thread`
  feature on its walls (`hole<i>.wall`, from the hole's start, the wall's
  low end, for a partial thread), not with the hole's own `thread`, which
  would make the bore Mitcad's basic minor diameter; then the minor
  diameter and the hole without its thread are tried. Holes placed by the
  history are tried on all bodies, then on the bodies they go into only,
  and through all where the depth is not decoded.
- **Modelled threads** (mitcad#57): a `thread` with `modeled`, the
  file's `diameters` (`majorDiameter`, `minorDiameter`, `pitchDiameter`
  of `threadInfo`) and an `angle`, one per face, tried first on the faces
  as they are (the thread sizes its part of the face itself), then after
  the sizing above. The profile was found by measuring the bodies stored
  in the files: the basic profile of ISO 68-1 (flanks at 30° to the
  radius, the thread half a pitch wide at the pitch diameter) cut off by
  flat crests and roots at the class's major and minor diameters (the
  middles of the tolerances; no rounding), along the whole face and ended
  by the planes of the face's ends. The reference model's M10x1.5 6g bolt
  (9.85/8.141/8.928 mm) has 3.2e-5 less volume than that exact profile
  (its flanks are fitted splines), and the thread replaces the material:
  an internal M10x1.5 6H thread on a 10 mm bore in a corpus design adds
  the teeth inside the bore down to the minor diameter, the exact
  profile's 234.334 mm³ against the stored 234.334 mm³. Where the helix
  starts is not decoded; the import measures it on the next history
  states (which half of a pitch at the pitch diameter is empty, at four
  angles about the axis) and turns the thread to it, else angle 0. It
  measures where segments along the axis cross the stored bodies' faces
  (`Kernel::segment_crossings`, intersecting only the faces near them):
  classifying the points against every spline face of a stored thread
  took about 5 s a thread, the crossings take milliseconds (mitcad#68). The
  reference bolt and a corpus design's nut, both on axes along +Z, start
  where Mitcad's angle 0 does.
  Whitworth and NPT threads fall back (Mitcad models only 60° threads).
  A tapped hole with a modelled thread gets a modelled `thread` on its
  walls (angle 0: the walls do not exist before the hole).
- **Sheets** (surface bodies): the history's matching compares solids
  only. The sheets of a state come in with the fallbacks that take it, and
  those of the last state at the end, as stored bodies (base features);
  joins, cuts and intersections without participants then name their
  component's solids, so that the sheets are left alone as the file's
  solid operations leave them. Without a history the file's sheets are not
  told from the feature tools and sketch faces its blobs also keep, and
  stay out.

### Sweeps, pipes and lofts

The decoder reads their inputs (SCHEMA.md, *SweepFeature*, *PipeFeature*,
*LoftFeature*; `core/f3d/src/design/build/sweeps.rs`):

- **Paths, guide rails, centre lines and rails**: lists of sketch curves,
  named by the ids the sketch's curves carry (`crv_primary_id`,
  `crv_secondary_id`), or of edges, found in the history as fillet edges
  are (*Named inputs*), as the dumps' `PathEntity` items.
- **Profiles**: a sweep's and a loft section's sketch, from its profile
  source; which of the sketch's regions is not stored in a form decoded.
- **Loft sections** in order: a sketch profile, a sketch point (by its
  `pt_tag`) or a path of a body's edges; the first and last sections'
  end conditions (free, tangent, smooth, direction, tangent point) with
  their weight and angle parameters.
- **Operations** of all three (a pipe's from one design only).

Not decoded: a sweep's orientation, direction and rail scaling, a pipe's
section type and whether it is hollow, closed lofts and participant
bodies. Guide surfaces and edges of other components (through their
occurrences) are recognised and make the item fall back.

`sweeps.rs` translates them with what the history settles: a profile's
region (a sweep's sets as for extrusions; a loft's one region per
section, probed ones first, at most twelve choices of all sections), a
second fraction 0 as none first (a profile at the path's end), both
directions where a partial extent or a rail makes them differ (with a
rail the file's direction flag reads the other way: the reference
models' rail sweep, flipped, is Mitcad's sweep without `flip`), the
perpendicular orientation before the parallel one, a rail's scaling
before stretching and none, a solid circular pipe before a hollow one
and the square and triangular sections, and, without participants, a
join or cut on all bodies, then on those the history changed, then a
join as a new body. The reference models'
sweeps and lofts come in from their streams as from their dumps
(`test_f3d_import --models`). The corpus's sweep, a cut swept only
backwards from a profile at the path's end along a path with a slight
kink between its curves, comes in parametric since Mitcad's sweep rounds
corners whose mitre does not build (mitcad#56).

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
     undecoded construction lines. Such a set, extruded to the side where
     the state changes it, is taken to make the state's change and is
     tried first (a cut deeper than the body removes less than its prism,
     so its prism would rank it behind smaller sets);
  2. sets of any number of regions whose area is within 5 % of the state's
     change over a distance extent's length (bounded subset-sum search,
     nearest six);
  3. subsets of the same count, ranked by how close their prism volume
     comes to the state's change.

  Not tried: a distance extrusion without taper whose prism holds less
  than the state adds or removes (overlaps only take from a prism; left
  out before the likeliest are chosen), and one limited to participants
  that include every body the same definition without them changes (same
  result).
- **Participants** of a join, cut or intersection: all bodies first, then
  the bodies of the sketch's component that the state changed (a cut in
  the file may leave bodies in its way alone).
- **Direction:** the decoded direction vector against the sketch's normal
  (one side through all: the stored vector points away from the
  extrusion), else the decoder's ±1, then the other way; **symmetric
  extents** (half the length each way first, then the whole length);
  **up to an object** (planar faces parallel to the sketch, nearest
  first), **revolve axes** (sketch lines and origin axes, ranked by
  Pappus' volume).
- **Fillet and chamfer edges** the decoder did not find by their names
  (and after those it found, should they not give the state). Candidates
  are the replay's edges whose midpoints are off the next state's faces
  (`Kernel::boundary_distances`, seams left out; each body of the state
  is asked only about the points not on another's faces yet, those whose
  boxes hold the most first, and keeps its faces' projections and the
  distances found for later items; analytic faces are asked first, other
  faces project a point from the nearest point of a grid over them, and a
  point within 1e-5 mm of a face has its distance to it without a search
  of all faces). An edge is assigned to an edge set
  when a rounding or bevel of the set's size fits: the points where it
  would meet the edge's two faces (projected onto them) and the middle of
  its cross section (convex or concave by the side of the nearer of the
  edge's faces a point just off the edge lies on; the solid classifier
  where that does not tell) lie on the state's faces (within 5 % of the
  size and 1e-3 mm), and the edge's
  midpoint is as far from the state as that size leaves at its corner
  angle (within 10 %). Tried in order: all those edges at once; one
  tangent chain after another (sets and edges in order); then looser
  guesses (the edges at that distance, all, the largest group at one
  distance). The next four states are looked at; for an item whose state
  is known, one at a time (`Guesses`): the edges found by their names
  first, then its own state's guesses, the next state's only when those
  gave no state, in the same order as all at once (finding the edges a
  state lost takes seconds on large bodies, and the item's own state or
  the names usually give it).
  - A rounding or bevel that removes a neighbouring face exactly (full
    round, chamfer as wide as a step), which OCCT cannot build, is built
    1e-3 smaller (`geometry/src/dressup.cpp`). Its body then differs
    from the file's by about 1e-6 to 1e-4 of its volume, and so do the
    final bodies
    built on it: most final bodies that are close but not exact come from
    such dressups.
  - A result BRepCheck rejects is repaired by shape healing when the
    repair keeps the volume (OCCT turns some roundings on stored bodies
    inside out).
- **Combine** target and tools: the bodies the next state changed (a body
  with a stored body's measures counts as unchanged even where its faces
  are split otherwise).
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
  to its depth or through all. The removed material is the replayed body
  minus the state's, cut only where the two differ (mitcad#85,
  `removed.rs`, `Kernel::removed_material`): around the faces of the
  state farther than 0.1 mm from the replay's faces (a hole's walls) and
  the replay's faces with every point that far from the state's. Both
  bodies are cut down to boxes around those faces first. The two are near
  copies (a replay's free-form faces lie up to 0.07 mm from the stored
  ones in the `.ipt` test parts), and a boolean of the whole bodies
  intersects every such pair of faces: minutes for one hole, often with a
  wrong result. `MITCAD_NO_REMOVED_REGIONS=1` cuts the whole bodies (for
  comparisons).
- **Patterns** without decoded inputs: copies of one of the last six
  extrusions about the decoded axis (else the origin axes), as many as the
  decoded quantities (else as the state's volume change asks for); the
  extrusion and quantity whose copies come closest to the change first;
  computed `adjust`, then, for an extrusion up to an object, `identical`
  (same result by distances). A rectangular pattern whose item stores
  objects but no directions (the oldest item version, mitcad#74) copies
  the decoded features. Its first direction is guessed along each origin
  axis, the second across it towards another origin axis at 90° (first),
  60° and 120° (a hexagonal lattice: a plate of hexagonal holes with even
  walls), each with both directions symmetric too. For a cut, a copy
  changes the bodies only where its centre (the centre of the material
  the extrusion removed, from its history states) lies in the replay's
  material, so the copies counted there make the change ranked against
  the state's: the copies of a large pattern over a round plate are
  mostly off it, and a lattice of the wrong angle puts fewer on it.
- **Copies of features** act only on the bodies the original feature
  changed where it has no participants (`original_bodies`, mitcad#74): a
  pattern of a hole in a plate leaves a disc of another extrusion that
  its copies reach as it is.
- **Profiles a pattern or mirror needs.** Several sets of regions can give
  an extrusion's state exactly (regions in material a cut already
  removed, or a join adds to), but not the same copies. An extrusion that
  gave its state exactly keeps up to four definitions ranked after the
  one taken. When a pattern or mirror of features gives no state, each
  extrusion it copies is given those definitions in turn, kept only where
  the bodies stay exactly as they are (the extrusion and the items after
  it give their states), and the pattern's candidates of it are tried
  again (at most eight such edits per item). The extrusion's note says
  which pattern chose its profiles.
- **Mirrors** without decoded inputs: sets of up to four preceding
  features whose changes in the history add up to the state's change.
  Bodies the state has twins of become new bodies; a body is joined with
  its image where the state has a body of twice its volume; else each
  body. The plane is the decoded one, else the origin planes and planes
  through the bodies' and images' centres (as a planar face on them when
  there is one). A body joined with its image is symmetric about the
  plane and holds the body: it is tried only where the state has a new
  body of between the body's volume and twice it whose centre lies on
  the plane (mitcad#88; the join of a nearly symmetric body with its
  image was the slowest guess, 119 s in an `.ipt` test part, and a mirror
  of a hole can never be one). The join itself works only where the image
  differs from the body (`Kernel::join_near_copy`, `commands.md`,
  *mirror*).

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
  (`Kernel::compare_shapes`), each for at most ten seconds and all for at
  most a minute (`CompareOptions::seconds`: the deviations are sampled
  first, and the booleans, which on coincident or nearly coincident
  shapes can take minutes or not return, stop at the limit and are then
  missing). When every sample lies within 0.01 mm of the other body the
  booleans are left out (`CompareOptions::booleans_above`, mitcad#69), and
  boolean differences larger than the deviations allow (four times the
  volume they span over both surfaces: the booleans took nearly coincident
  bodies as apart, a relative difference of 2) are unknown, not a
  difference. Bodies that still differ beyond the matching tolerance are
  replaced by the stored ones (a warning names the base feature).

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

**Item time.** An item's definitions are tried for `item_seconds` (60 s;
the part of it on bodies brought to the state before it exactly gets half
again). The definitions are evaluated with a monitor whose deadline is the
end of that time (`RecomputeMonitor::within`, mitcad#69), so that the
kernel's long operations stop inside one definition (a rectangular pattern
of 35 × 35 copies took 113 s): the definition fails ("the item's time was
over before its definition was evaluated") and the item gives up like one
out of time.

**A definition's share** (mitcad#78). One slow definition must not use up
the item's time before those ranked after it are tried (a join onto a plate
of a thousand holes took a minute where the next definitions took a second
or two). Checked against the history (an item with a state of its own, and
each set of bodies of one without), the definitions are tried in two
rounds (`Turns` in `lib.rs`):

1. In rank order, each with a third of the item's time (`SHARE`); the
   last one, when none waits, with the rest. One cut short at its share
   gives way and waits; the same definition limited to participants waits
   behind it untried (the boolean on fewer bodies takes as long).
2. Those that gave way, in rank order, with the rest of the item's time,
   each only while more time is left than it already ran (it cannot finish
   in less).

Definitions that give a state take seconds (up to about 15 s on bodies of
thousands of faces), so one that runs longer is mostly a wrong guess (on a
plate of a thousand holes the wrong ones took one to two minutes each); one
that would give the state within the item's time still does when those
after it are quick. A smaller share would reach more definitions, but each
one cut short and tried again costs its share. When the item gives no
state, its note names the definition cut short after the longest time
("cut short after 20.0 s: definition 3 of 12 (extrude, join, 1 profile)
(and 2 others)"; the trace has the whole definition).

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
the current item). The step moves between kernel calls, also in the
import's own long loops, which tick it after each call
(`mitcad_import::tick`, for the import running on the thread): building
and measuring bodies (a call per body; `Sig::of` ticks), finding faces
and edges by their fingerprints or for a joint's frame, comparing
bodies. Building a body from the file heals it with OCCT's ShapeFix,
which reports its progress face by face (`brep::BuildOptions::progress`,
`f3d_build_body`), and that moves the step too, as does the progress of
OCCT's long algorithms in the definitions tried (booleans, fillets,
sweeps: those a request to stop stops,
`geometry::CancelSource::progressed`). So a long item is no hang; a
kernel call that does not return is (mitcad#82: a large design's first
item, a base feature, healed two bodies of 15 000 and 23 000 faces for
110 s and looked hung under a limit of 60 s). A call can still take
longer than the limit and return: healing one face of that body took
74 s. The tries share such builds (`StoredFile::builds`): a try waits
while another builds a body, and when the watchdog gave the builder up
meanwhile, the builder hands its body over as B-rep data and the next
try reads it instead of building it again (which would take as long and
look hung again; waiting shows no progress either, so a build that does
not return still does). `MITCAD_IMPORT_STALL=slow` makes the file's
first body take 1.5 s to build, for the tests.

When the step does not move for that many seconds, the
thread is abandoned and the import reruns on a new document with the item
in `Options::hung_items`: it takes its history state without trying
definitions (note "the geometry kernel did not finish a definition of
it"), and the report warns. The items before it replay as before, so those
whose definitions gave no state (`Progress::failed_items`) take their
states at once in the next try, with the same note
(`Options::failed_items`): a pattern of a thousand copies took minutes in
every try. A hang while comparing the final bodies makes
the next try compare volumes only; a hang elsewhere brings in the stored
bodies without the timeline. The try given up is abandoned
(`Progress::abandon`, mitcad#71): when its kernel call returns it stops
before its next item or definition (a definition's long kernel operations
stop inside, as on a stop), compares nothing and its result never reaches
the report or the document, so it does not compete with the next try for
the processors and memory. Until then the thread keeps its stack and
memory, so under an address-space limit
(`ulimit -v`) the next thread's 256 MiB stack may not fit: it then starts
on the largest of 64, 16 or 8 MiB that does (a smaller stack can overflow
in OCCT's deepest recursions, which fails that feature, and the report
warns), and when none fits the import ends with an error saying so (it
used to fail with "Resource temporarily unavailable", mitcad#58).
A process that ends while such a thread still runs must not run its
static destructors: on Linux the shared libraries' finalizers destroyed
OCCT's statics under a thread healing a body it built, which crashed
(`mitcad-cli` exited with a segmentation fault after the error that the
import hung, mitcad#82). `abandoned_imports()` (`mitcad-ffi`) counts the
threads given up that still run; `mitcad-cli` and the application's
import worker then end with `std::_Exit` once their output is written.
`MITCAD_IMPORT_STALL=compare` simulates a hang for the tests
(`core/tests/test_f3d_import.cpp`, also under a tight `RLIMIT_AS` on
Linux); `MITCAD_IMPORT_STALL=busy` makes every try build the file's
bodies over and over without progress, so that the import ends hung
while the threads given up are in the kernel
(`cli.import_f3d_hang_exit`, `tools/cli/hang-exit-test.cmake`). A
tapered extrusion whose arcs shrink past their apex, on
which OCCT's draft loops, is refused before the draft ("the taper closes
the profile", `geometry/src/sweep.cpp`). The file is read once, before
the first try, on an import thread of its own and without the watchdog
(reading and converting a large design takes minutes and calls no
kernel): the tries share its decoded design and its bodies converted to
the neutral model (`StoredFile`), and each builds the OCCT shapes it asks
for (`F3dGeometry`), so a try run again neither reads the file again nor
holds a second copy of it beside the abandoned one (mitcad#80).

### Memory

An allocation that fails ends the process: Rust's cannot be caught, and
the import used to lose everything, also the items done (mitcad#80). So
the import guards its memory (`core/ffi/src/f3d_import.rs`,
`MemoryGuard`), on the watchdog's loop or, without one, on a thread of
its own, ten times a second: it measures the process against the
tightest limit it runs under (`core/ffi/src/memory.rs`,
`core/cpp/bridge/memory.cpp`): an address-space (`ulimit -v`) or data
limit, or the memory the system has left (available memory and free
swap) on Linux; a job's memory limit or the commit the system has left
on Windows; the physical memory on macOS; and the `memory_limit` option
(MiB, against the resident memory). Near a Linux limit the memory the C
library's allocator holds free (`mallinfo2`) does not count: it is used
again before more is mapped.

- At 70 % of the limit (and each 5 % more) memory is *tight*
  (`Progress::tight_on_memory`): before its next item the import drops
  the cached results of the definitions it tried and the bodies of the
  history states behind the replay (`Oracle::release_behind`,
  `StoredGeometry::release_before`), which are built again if they are
  asked for; what it imports stays the same.
- At 85 % it is *low* (`Progress::low_memory`): the definition being
  evaluated is cut short as on a stop and its item takes its history
  state (note "the import ran low on memory"), as do the modelling
  items after it until the memory is back under 70 %
  (`Progress::memory_recovered`); the import drops what it can as above,
  and from then on caches at most 256 MiB of results
  (`CACHE_AFTER_LOW_MEMORY`). A definition's own memory goes when it is
  cut short, so the import usually goes on with the next item (a
  rectangular pattern of a thousand copies took 8 GB in one definition).
  The final bodies are compared by volume only if the memory is still
  low then. The report says where it first ran low, how much was in use
  and how many items it gave up (`low_memory`, and a warning).
- A kernel operation whose allocation fails (OCCT's
  `Standard_OutOfMemory`, `std::bad_alloc`) fails with "<operation>:
  out of memory" (`geometry/src/util.hpp`, `KernelError::OUT_OF_MEMORY`)
  instead of ending the process; the import takes it as low memory.
  `MITCAD_TEST_OCCT_OUT_OF_MEMORY=<operation>` makes one fail for the
  tests.

The history's bodies are converted keeping only the distinct ones: each
state holds the bodies that differ from the state after it, not a copy of
every body (a design of 256 items took 5.8 GB before the first item that
way, and the memory the allocator kept afterwards left 40 MB of a 12 GB
address-space limit once OCCT's threads started).

### Components

`components.rs` makes the dump's components and places them by the
occurrence tree before the replay:

- placements: the decoder's top-level `transform` (relative to the root)
  and nested `_f3d.local_transform` (relative to the parent), or an
  external dump's `transform2` (full path) made relative to the parent
  occurrence; cm become mm; grounded and hidden occurrences stay so;
- a component used several times comes in once and is placed by each
  occurrence;
- occurrences of components of other documents (`isReferencedComponent`:
  inserted parts, fasteners) are occurrences of empty components, one per
  component of each document, named after the item that inserted it
  (`Screw1`, `Component Insert2`: the decoder gives each occurrence item
  the occurrence it made), else `Inserted component` (mitcad#75). Their
  bodies are not in the file, but joints, captured positions and ground
  items name them; what is inside them (their own occurrences) is not
  decoded. The report counts them apart (`external`).

The file has one timeline for the whole design; every item goes into a
component:

- an external dump names it;
- for the decoder, the component whose ASM blob the item's operation
  changed (`StoredGeometry::item_components`; each component keeps its
  bodies in its own blob, in its own coordinates), else the component that
  owns the item: every component has a list of its items (class
  `3A6D1E62`, `classes::FEATURE_LIST`, which refers to the component's
  feature manager), and the decoder writes the owner as the item's
  `component` and `_f3d.component`. A feature that changes another
  component's bodies than its owner's (the file's assembly context) goes
  where they are;
- sketches and construction geometry follow the first feature that uses
  them, unless their owner is placed elsewhere than that feature's
  component at that feature's point of the timeline (*Occurrence
  placements*): their geometry is in their owner's coordinates, so they
  stay in the owner and the feature gets a copy (*Across components*).
  One no feature uses goes into its owner;
- other items without an owner follow the next item with a component
  (else the one before);
- a definition goes where the bodies, sketches and construction geometry
  it names are when they agree, else into the item's component; a sketch
  whose owner is known goes on a face of its own component only.

New-component operations become new bodies of that component. Fallback
base features are made per component, and the final comparison wants
every body in its component (a body left in another one is replaced
there). The report counts components, occurrences and items per component
(`components`) and names each item's component.

A component whose bodies are stored only in an `.smb` blob (no `.smbh`
history) has no features; its solids join the stored design unless one of
the last state's bodies is the same, and come in with the final
comparison. The `.smb` blobs of components with a history also hold
feature tools and sketch faces, which stay out. Occurrence items and
other assembly items change no body and are skipped (occurrence
placements come from the tree; *Items without a translation* below);
joints, as-built joints, joint origins and ground items come in as
Mitcad's (*Joints* below).

**Across components.** Mitcad's features work only on their own
component's bodies, so a feature of the file that changes another
component's bodies (the file's assembly context: a part cut by a sketch of
the assembly) goes into the component whose bodies it changes, and what it
uses of other components comes along:

- a sketch of another component is imported again into the feature's
  component, moved by the two occurrences' placements at the feature's
  point of the timeline onto a planar face of its component or a fixed
  construction plane, so it stays parametric (its dimensions keep the
  file's parameters); features at points where the placements differ get
  copies of their own;
- an extrusion up to a face of another component goes up to a fixed plane
  where that face's occurrence places it in the sketch's component at the
  extrusion's point of the timeline.

Both need each component placed once (one occurrence on the way from the
root); else the feature falls back. Features that work on bodies of two
components at once (a combine of bodies of two components) fall back too:
the base feature takes the item's history state in each component, and
later items continue parametrically.

**Occurrence placements.** The transform the file stores for an
occurrence is where the item that made it put it; joints, captured
positions (`Snapshot` items) and rigid groups after it move it without
changing that transform. The decoder reads the occurrence's placement
after each item that placed it (mitcad#81), so the file places an
occurrence at the end of its timeline at its last one (else at its last
captured position, else at its stored transform), and that is the
occurrence's own placement in Mitcad; the captured positions also come in
as `capture_position` features at their points of the timeline
(*Joints*). The history's states are in each component's own
coordinates, so the matching does not depend on placements; only the
copies above do. Those take the placements the file has at the item's
point of the timeline (`Captures::placement_at`, mitcad#86): where the
items before it put the occurrence (else, without its placements, its
last captured position before the item, else its stored transform), not
where the occurrence starts in Mitcad. Where a joint or a move places an
occurrence elsewhere at an item's point in the timeline than the
placements say, the copy does not follow it and the item falls back.
Joints come in only where they hold where the file places the
occurrences (*Joints*); occurrence moves are not replayed (*Limits*).

### Joints

`joints.rs` translates what the decoder reads of joints, as-built joints,
joint origins, rigid groups and ground items (mitcad#55, mitcad#81,
SCHEMA.md §5.3) into Mitcad's joint features (`commands.md`, *Joints*),
and `captures.rs` the captured positions (mitcad#75). Their occurrence
paths (object ids from the joint's component down) become paths of the
occurrences the tree made, those of components of other documents
included (fasteners, inserted parts: empty components, *Components*); a
joint of a level inside a component of another document is left out with
that reason.

**Where the file places the occurrences.** The transform the file stores
for an occurrence is where the item that made it put it; joints, captured
positions and rigid groups after it move it without changing it. The
decoder reads each occurrence's placements after every item that placed
it (`_f3d.placements`, mitcad#81): the occurrences start at their last
ones (else at their last captured positions, else at their stored
transforms), and after each joint item every occurrence must be where it
was before the item, where the file places it after the item, or where it
ends (1e-6 of the size, 1e-7 in rotation), else the definition is taken
back and the next one tried.

- **Joints.** Side one is origin `a` (it moves onto side two, as `a` does
  onto `b`), side two `b`. The file's relation is Mitcad's `frame_a =
  frame_b · motion(values) · Rz(angle) · Tz(offset) · flip` with the
  angle and z offset parameters as they are, `flip` the file's flip (its
  `opposed` byte: the frames' z axes opposed), and the x and y offsets
  moving `b`'s origin within its plane (numbers). The joint test model
  (the reference model `joint_kinds` under `MITCAD_F3D_MODELS`: every kind at known values, flipped and
  not, mitcad#81) settles the senses: Mitcad's values are the file's
  (turns, slides along x, y or z by the motion's slot, a cylindrical
  joint's turn and slide, a pin-slot's turn and slide along x, a planar
  joint's x and y slides and turn); a ball joint's file values are
  `Rz(pitch) · Rx(yaw) · Rz(roll)`, Mitcad's `Rz · Ry · Rx`, so its
  values differ while the placements agree.
- **Checked before it is added** (mitcad#87). The values the motions have
  where the file places the occurrences (at the marker, else right after
  the item) come from the relation (`mitcad_model::joints::motion_values`)
  with the sides' frames; where it does not hold there, no definition is
  added (the note says how far side one is from side two) and the joint is
  kept as an as-built joint at once. Where it holds, one definition is
  added: with the file's limits (rotation limits bound `rz`, slide limits
  the slide; planar and ball limits are left out, which motion they bound
  is not decoded) when they hold the values, with the values as the
  joint's `position` where the rest value is another; else without the
  limits (`its limits left out (outside them: rz is … there)`). A joint
  item takes one recompute instead of one per definition tried and its
  undo.
- **Offsets the file stores rounded** (the owner's decision in mitcad#81):
  where the relation holds but for the offset along the joint's z, by at
  most half a unit of the last digit of the offset parameter's expression
  (`-2.646 mm`: 0.0005 mm; expressions with decimals only), the parameter
  keeps its name and takes the offset the placements give (`its offset …
  from the placements`). The joints of the corpus that seemed rounded
  hold with the stored offset at the placements after them (the stored
  transforms are older): none needs it.
- **Sides.** The frames are the file's stored ones, in the components the
  paths end in. A side goes on geometry of that component at the joint's
  point of the timeline that gives the frame, with a `frame_override` for
  the parts it does not give (a planar face's origin is its middle, so a
  key point elsewhere on it is an override): a joint origin the side
  names, construction geometry its key point or direction names (the
  component's origin point, planes and axes, imported construction
  features), circular edges centred on the frame's origin along its z
  axis, planar faces through it across z, faces of revolution about z
  (edges first when the key point is on an edge; at most eight). The
  faces and edges come from one pass over each body
  (`Kernel::face_geometries`, `edge_geometries`, mitcad#87) instead of
  finding each by its name. Each is checked by resolving it as the joint
  will (`joint_frame`). Else the side is a fixed plane with the frame, and
  the joint is `partial`. A side on a component of another document (a
  fastener's joint origin) goes on the origin of the empty component
  standing for it, with a `frame_override` where the file's frame there is
  not the identity.
- **Kept as as-built joints.** A joint that does not hold becomes an
  as-built joint of its kind at the placements at its point of the
  timeline (`relative` from them; the motion frame side two's), else a
  rigid one, with a warning and the reason; the item is `partial`. A
  joint whose motion is not decoded is left out.
- **As-built joints.** `relative` from the placements at its point of the
  timeline. The file records the joint's frame in each occurrence's
  component; where those frames disagree with the placements a warning
  says so (the placements are kept). Kinds with motions take the recorded
  frame as their motion frame, on geometry of occurrence two's component
  as for a joint's sides, else of occurrence one's, else fixed on
  occurrence two; their limits where the recorded placement is the
  file's (Mitcad's values are 0 there) and the limits hold 0, with 0 as
  the position where the rest value is another.
- **Rigid groups** (mitcad#81; the file stores them as as-built joints of
  motion type 11). A `rigid_group` of the occurrences of the group's
  component its members are or are in: members below the top level
  ("include children" lists the members' children too) and inside
  components of other documents move with the occurrence they are in. A
  group with the component's own geometry (an empty path) joins its
  occurrences to it by rigid as-built joints.
- **Joint origins.** A `joint_origin` of the owning component with the
  file's angle and z offset parameters on geometry that gives its frame
  before them (the component's origin point for the designs read, a face
  or edge as above, else a fixed plane), checked against the file's frame
  (`datums`); else the file's frame fixed without the offsets (`partial`).
  Later items can use it as a joint's side.
- **Ground items.** The occurrence's ground flag (the decoder also sets it
  in the occurrence tree); the item is `parametric` without a feature.
- **Captured positions.** A `capture_position` feature in each component
  whose occurrences the item places, each occurrence at its placement in
  its parent: the file gives the placement of each path in the item's
  component, made relative to the parent path's placement (the same
  item's, else the last captured or stored one). A captured position of
  joints' values stores parameters instead; its positions are those the
  occurrences' placements name at its index (so are those of positions
  with an empty path). A path level the decoder
  does not find (some designs name a sub-assembly's occurrence by an id no
  occurrence has) is the one occurrence placing the component the next
  level sits in; its id then stands for that occurrence in the item's
  other paths. Positions inside components of other documents and of
  grounded occurrences (Mitcad does not move those) are left out with a
  note (`partial`). The features are not checked against the end
  placements: they are the file's placements at their points.

The report counts them (`joints`: as joints, their sides on fixed frames
and on components of other documents, kept as as-built joints, as-built
joints, joint origins, grounded, captured positions, rigid groups, left
out). With `MITCAD_IMPORT_TRACE` each joint item traces its time (`a
joint in 0.005 s (sides 0.004 s, 1 definitions added)`).

In the corpus (10 designs with joint items or captured positions; every
joint is rigid, nearly every one joins a fastener or another part
inserted from another document):

| | Joints | As-built joints | Rigid groups | Captured positions | Joint origins |
|---|---|---|---|---|---|
| mitcad#81 | 9: 8 in (6 sides on fasteners and inserted parts), 1 inside another document | 3: all in | 10: 7 in, 3 whose other members are inside another document | 15: 6 in, 8 in without positions inside other documents, 1 left out | 6: all in |
| Before | 9: 5 in, 3 kept as as-built joints, 1 inside another document | 13 (the rigid groups among them, as rigid as-built joints of their first two members): 10 in, 3 inside another document | not decoded | 15: 4 in, 8 in without positions inside other documents, 3 left out | 6: all in |

The joints kept before as as-built joints held where the file places the
occurrences after them: two seemed 4.5e-4 mm off (a rounded offset), one
fastener 12 mm off, against the transforms the file stores, which are
older than the joints. In the corpus a joint item now takes at most 0.02
s (0.05 to 0.18 s before); on a large design (199 timeline items, a body
of 1239 faces and 2981 edges) with twelve rigid joints onto its own
geometry added to it, the first that holds took 0.22 s (reading the
component's faces and edges), the others that hold 3 to 5 ms and those
that do not 1 to 3 ms (2.3 to 3.0 s each before: each definition tried
added, recomputed and taken back, and every face and edge found by its
name).

### Items without a translation

The decoder names timeline items whose own data it does not read by
their class (`core/f3d/src/design/classes.rs`, `ITEM_TYPES` and
`ITEM_CLASSES`), and the report says what each does:

- **No geometry**, skipped at once (they never wait for a fallback):
  occurrences (`Occurrence`, the item of a new or placed occurrence,
  named after its component), components inserted from other designs
  (`ComponentInsert`, `Fastener`), pasted and derived occurrences
  (`CopyPasteOccurrence`, `DerivedInstance`), patterns of occurrences
  (`RectangularOccurrencePattern`, `CircularOccurrencePattern`: their
  copies are occurrences of the tree, each placed by its stored transform;
  Mitcad has no pattern of occurrences, so they do not follow the
  pattern's quantities and distances), assembly relationships
  (`GeometricRelationship`, `AssemblyRelationship`, motion links, contact
  sets), assembly contexts (`Context`),
  canvases, timeline groups (`Group`, named by their own name; their items
  follow them) and mesh bodies (`MeshFeature`; meshes are left out).
- **Changes bodies, not translated**: the history states stand in for
  them (fallback), with the reason in the report. Sheet metal flanges
  (`FlangeFeature`), embosses (`EmbossFeature`), electronics boards
  (`PCBFeature`), moved faces (`MoveFaceFeature`) and drafts
  (`DraftFeature`), whose inputs are not decoded; cylinder and sphere
  primitives (`CylinderFeature`, `SphereFeature`), whose sizes are
  parameters but whose plane is a face reference not decoded yet;
  components mirrored into new ones (`MirrorComponent`), bodies derived
  from other designs (`DerivedContext`), bodies moved into a new component
  (`ComponentFromBodies`) and bodies copied or moved by pasting
  (`CopyPasteBodies`).
- Lofts of another class are `LoftFeature`s, translated (*Sweeps, pipes
  and lofts*); points of another class are `ConstructionPoint`s, skipped
  without decoded geometry.

Items of classes still unknown keep the type `?<default name>`
("unknown timeline item").

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

- Inputs the streams name (*Named inputs* below) are found only where
  the history state before the item has them by those names: not for
  items without an ASM state of their own (suppressed, or after the
  timeline marker), faces named as they were more than 24 states earlier,
  or edges between the same two faces whose ends do not tell them apart.
  Those edges are left to the history.
- Selections decoded from the streams (mitcad#67) and tried before the
  history's guesses: a combine's target and tools, a split's tool (one
  plane, face or body) and bodies, a pattern's or mirror's objects
  (bodies, features or faces) and a mirror's plane (also a planar face), a
  pattern's axis or directions (origin or construction axes, sketch
  lines, edges, faces), a hole's points, type and an extent through all.
  Not decoded: a hole's extent up to an object and its direction (the
  history settles both), a split with several tool bodies (it falls
  back), a rectangular pattern's direction vectors where the entity is an
  axis (the axis gives them; whether a direction is reversed is not
  stored apart from the distance's sign). Sketch texts are not decoded
  from the streams at all (content, font, frame: a text's placement is a
  matrix in some writers' objects and absent in others), so neither is a
  text's construction flag; texts come in only from external dumps. Planar
  faces named as mirror planes or split tools are not found in the
  history yet (only cylinders and edges are), so those items still fall
  back or take the history's guesses.
- Extrusions: the decoder gives the extent codes (one side, two sides,
  symmetric; per side a distance, an object or through all), the
  distances, tapers and start offset, and the direction as a vector. Not
  decoded: which profiles (only how many, see *What the history settles*),
  the object of an extent up to an object (planar faces parallel to the
  sketch are tried; a two-sided extent with an object side falls back),
  whether a symmetric distance is the whole length, thin walls and
  participant bodies.
- Features that use bodies of two components at once fall back, and so
  do features across components whose components are placed more than
  once (*Across components*).
- Joints (*Joints*): a joint comes in as a joint only where it holds
  where the file places the occurrences (*Occurrence placements*);
  occurrence moves are not replayed as Mitcad's `move_occurrence`.
  Components of other documents are empty (their bodies, joint origins
  and own occurrences are not in the file): joints of occurrences inside
  them are left out, and a side on one stands on its origin (the corpus'
  fasteners' joints hold there at the placements after them). Geometric
  and assembly relationships, motion links and contact sets are left
  out. Not decoded or not settled: which joint origin of another document
  a side names (the decoder gives its id in the other document only), the
  names of components of other documents, captured positions of top-level
  occurrences named by ids no occurrence has where nothing inside them is
  captured too (left out), motion type codes 3 and 5 (none seen), the
  parameter roles of planar and ball joints' limits (left out), and the
  order of a joint origin's angle and offsets (each joint origin is
  checked against the file's frame). The sides' faces and edges are found
  by their geometry, not by the names the decoder gives (joints have no
  history state of their own to find them in).
- Fillets with G2 continuity or mid radii, asymmetric fillets, setback
  corners, and chamfers with miter or blend corners are translated, but
  Mitcad builds them by its own rules (`commands.md`, `fillet`,
  `chamfer`), so they match the history within the looser conventions
  tolerance (a match is kept with a warning). Which face takes which
  offset of an asymmetric fillet is not stored: both are tried.
- Replace face needs the history (for its faces) and its target in the
  replay: a surface body the replay lacks makes it fall back unless the
  target face is planar (then a fixed plane).
- Threads of older writers (thread class versions before 5) are not
  decoded, nor whether a thread is right- or left-handed (right-handed is
  taken).
- Modelled threads: no reference has a partial one, so how a partial
  thread ends and whether the file sizes the rest of the face are not
  settled (the plain thread is tried, then the sized face). On a face
  whose ends are not planes across its axis (a hole into a curved face)
  the geometry kernel's booleans of the helical flanks can give an
  invalid shape: the corpus's one such thread falls back. A modelled
  thread has four faces a turn (the two flanks, the root and the crest:
  the groove is swept a turn per spine edge, as a spine of one edge over
  many turns makes the cut fail), about 60 for an M10 thread 20 mm long,
  which later operations on the body have to take into account.
- Sweeps, pipes and lofts (*Sweeps, pipes and lofts*): sections that are
  faces or sketch curves, guide surfaces, inputs of other components and
  closed lofts are not translated; a loft whose sections' sketches have
  many regions is tried with twelve choices of them.
- Not translated: coils, ribs and webs. A coil's object names its plane
  (a face recipe) and holds a frame and the coil's choices (type, section,
  position, direction, operation) in fields not decoded; the corpus has
  one coil, too little to settle them, so coils come in as stored bodies.
  No design of the corpus has a rib or a web.
- Surface extrusions, revolutions and sweeps (`isSolid` false) are not
  translated: their sheets come in as stored bodies (*Threads, tapped holes
  and sheets*).
- Sketch patterns, offsets and polygons (*Sketch patterns, offsets and
  polygons*) are left out, and their copies and offset curves come in as
  plain curves, where Mitcad has no counterpart: patterns with instances
  on both sides of the originals (symmetric), offsets of splines and
  ellipses (Mitcad computes those curves itself, so the file's cannot be
  kept), offset curves without a source curve at the distance (e.g. extra
  corner arcs), and copies that are not where the pattern puts them
  (suppressed or moved instances). Mitcad's pattern counts are numbers and
  its directions vectors: the file's quantity parameters stay unused, and
  a direction along a sketch line keeps its imported direction. External
  dumps do not record these constraints' entities, so from them they are
  left out.

### Corpus test

Real designs are kept outside the repository (`MITCAD_F3D_CORPUS`, default
`~/f3d-corpus`). `test_f3d_import` (`core/tests/test_f3d_import.cpp`):

```bash
test_f3d_import --corpus [dir] [--every N] [--max-items N] \
    [--time-limit S] [--hang-limit S] [--reports DIR] \
    [--jobs N] [--memory SIZE] [--file-timeout S]       # build/rel/core
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
  (`MITCAD_F3D_MODELS`, default `~/f3d-models`) of lofts and sweeps,
  compares with the stored volumes, and imports each model from its own
  streams too.
- The ctest `f3d.corpus_timeline` (`core/CMakeLists.txt`) runs `--every 4
  --max-items 30 --time-limit 60` to stay quick in debug builds. Run the
  full corpus with the release build, e.g. `--time-limit 300 --hang-limit
  300` per design.
- Each file (each model with `--models`) is imported in a child process of
  its own, `--jobs N` at a time (default: the cores / 4, bounded by the
  memory available / `--memory`, 4G per child by default), the largest
  first; a crash, a hang past `--file-timeout` (3600 s) or a file over its
  memory ends only that file, which counts as failed. The lines come in
  the files' order and the totals are those of `--jobs 1` (every file in
  the one process, as before); [docs/development.md](../../docs/development.md#f3d-corpus-tests)
  has the options.

#### Coverage runs in parallel

A full coverage run, for numbers before and after a change: build both
states with the release preset (keep the binaries of the first), then run
each with every file and compare. The lines are those of a run in one
process, so only the times differ:

```bash
B=build/rel/core   # the binaries of one state
$B/test_f3d_import --corpus --time-limit 300 --hang-limit 300 --jobs 8 \
    --reports before-reports > before.txt
$B/test_brep_import --corpus --jobs 8 > before-bodies.txt
$B/test_exchange --corpus --jobs 8 > before-import.txt
$B/test_f3d_import --models --jobs 8 > before-models.txt
# ... the same with the other state's binaries into after*.txt, then:
diff <(sed -E 's/[0-9.]+ m?s$//' before.txt) <(sed -E 's/[0-9.]+ m?s$//' after.txt)
```

The children's memory adds up: keep `--jobs` × `--memory` within what
the machine (or `systemd-run --user --scope -p MemoryMax=...`) allows. The
line on standard error at the end gives the largest peak memory of a
child and the longest child, e.g. to choose `--memory`.

Known failure classes:

- extrusions whose result is not the state ("not in the file's
  history"): the right regions are not among those tried, mostly in
  sketches that come in partial (left-out constraints that moved curves,
  undecoded curves) or whose regions the stored profile cuts differently;
  results with the state's volume but another area or centre (a region of
  the same area elsewhere); joins and cuts across bodies of several
  components;
- extrusions that "do not cut into any participant body" or find "no
  bodies in the extrude direction": the replay's bodies before them
  already differ (an earlier fallback took a later state, or the sketch is
  placed on a face the replay does not have), or every region set tried
  lies outside the material;
- an extrusion that matches its state only approximately (within 1e-3)
  stays parametric, with a base feature after it (see *Matching rules*),
  so the items after it do not depend on its parameters;
- fillets and chamfers OCCT does not build (`fillet failed` / `chamfer
  failed`), mostly on the right edges: the stored rounding runs over a
  neighbouring face, which OCCT does not do, or the body is a stored one
  with tolerances up to 0.05 mm;
- items after the time limit (very large designs);
- patterns whose input is not one of the last six extrusions, or copies
  features other than extrusions (with their fillets, for example);
- items whose state an earlier fallback took the replay past, mostly
  fillets and cuts, which cannot leave the bodies unchanged, so they stay
  skipped or fall back;
- items without a translation (*Items without a translation*), items of
  unknown classes (`?…`), construction axes, and the untranslated types
  under *Limits*, which are skipped or fall back.

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
  patterns); a linear or polar pattern followed by a Scaled transformation
  of as many occurrences as that pattern with `scale` (its copies scaled
  about the original's centre of mass, mitcad#59). The model's `helix`
  feature and the hole's counterdrill and taper exist for FreeCAD's
  helices and holes.
- Conical and growing helices are helices with a `growth` per turn
  (`Pitch * tan(Angle)`, or Growth) and `"construction": "freecad"`
  (mitcad#83; helices made in Mitcad use Mitcad's own construction),
  built as FreeCAD 1.0 and 1.1 build them (the profile follows the Frenet
  frame of a spiral a hundred times as far from the axis): they match
  those versions' stored shapes to 1e-11. FreeCAD 0.21 built them along an auxiliary spine; its conical
  ones still match (1.8e-7), its growing ones are 1e-6 off and its
  reversed narrowing and left-handed growing ones are other shapes
  (5.9e-3, and 0.21's left-handed growing helix holds 518 mm³ where 1.0
  and 1.1 hold 429), so those fall back. A subtractive helix cutting
  outside the profile (Outside) is a helix that intersects.
- A threaded hole is a hole plus a cosmetic thread (ISO metric, Unified,
  Whitworth incl. BSP `G 1/4`, NPT, tyre valve threads of ISO 4570). A
  modelled ISO metric or Unified thread is first rebuilt as FreeCAD cuts
  it (its `makeThread`): the groove's section (60° flanks, the root P/8
  wide at the major diameter plus the class's clearance) on a plane
  through the axis, swept by a `helix` from a pitch above the top as deep
  as FreeCAD's thread runs, then the hole; then as a modelled `thread`
  (#57's profile with the bore as the minor diameter).
- Datum planes: attached flat with only a normal offset → `offset`
  plane; attached to an origin plane and turned a quarter (or not), with
  the offset along its normal → `offset` from the parallel origin plane;
  turned about the support's x or y axis → `angle` plane; otherwise
  `fixed`.
- Sketches and datum planes whose attachment turn an expression drives
  about a line in their support's plane (its x or y axis or any line
  between) become `angle` planes about that line.

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

MultiTransforms of the whole body, and Scaled transformations other than
after one linear or polar pattern of as many occurrences; profiles that
are faces or chosen parts of a sketch; revolutions up to the last face;
tapers along custom directions; pipes with an auxiliary spine or binormal
mode; Part extrusions of faces and Draft objects; Part fillets of varying
radius; binders; Draft and Python objects (`Part::Part2DObjectPython`).

Translated, but measured off FreeCAD's stored shapes (mitcad#59; the
check's limit is 1e-6, the reference models `pd_helix_more`,
`pd_helix_outside`, `pd_hole_modelled`, `expr_datum_turns`):

| Case | 0.21.2 | 1.0.2 | 1.1.4 | Why |
|---|---|---|---|---|
| Conical helix | 1.8e-7 ✓ | 2e-14 ✓ | 2e-14 ✓ | |
| Growing helix | 1.4e-6 | 5e-14 ✓ | 5e-14 ✓ | 0.21 builds it along an auxiliary spine; its centre is 1.5e-6 of the size from 1.0's |
| Reversed narrowing, left-handed growing helices | 5.9e-3, 1.9 | ✓ | ✓ | 0.21's are other shapes (its left-handed growing helix holds a fifth more) |
| Helix cut outside the profile | 3.4e-6, 2.9e-4 | 1.4e-4, 1.2e-4 | 1.4e-4, 1.2e-4 | FreeCAD refines the intersection (Refine): its stored shape lies 1e-5 of the volume and 1e-4 of the size in the centre from the plain intersection of its own helix and support (measured in FreeCAD 1.1); without Refine the conical one matches (9.9e-7) |
| Modelled ISO metric thread (M6, blind) | 1.3e-6 | FreeCAD fails it | 1.3e-6 | The groove's flanks: FreeCAD sweeps the section by the Frenet frame along a helix approximated turn by turn, Mitcad's helix keeps the axis as its binormal (the area differs by 1.3e-6; volumes agree to 3e-7) |
| Modelled Unified thread (1/4 UNC, through) | 2.0e-6 | 1.9e-6 | 2.0e-6 | As above |
| Modelled tyre valve, Whitworth, NPT threads | | | | FreeCAD rounds the tyre valve crests and cuts Whitworth's 55° profile; Mitcad's modelled thread is the straight 60° one, and does not taper |
| Attachment turns about an axis out of the support's plane, or with a shift | | | | Mitcad's planes turn about lines in them (an `angle` plane); a turn that also turns the plane within itself has no parametric plane: a fixed plane, FreeCAD's value kept |

Tyre valve threads exist from FreeCAD 1.1; their holes import parametric.

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
variant FreeCAD recomputed after the same change. Each document is checked
in a child process of its own, several at once (`mitcad_run_parallel`;
`MITCAD_CORPUS_JOBS`, `MITCAD_CORPUS_MEMORY`, `MITCAD_CORPUS_TIMEOUT` as
for the `.f3d` corpus; `MITCAD_CORPUS_JOBS=1` checks them one after
another), and the lines come in the documents' order. A full run for
numbers before and after a change:
`MITCAD_CORPUS_JOBS=8 ctest --test-dir build/dev -R freecad.corpus -V`,
or with a release build's CLI
`cmake -DCLI=build/rel/tools/cli/mitcad-cli -DRUNNER=build/rel/core/mitcad_run_parallel -DWORK=build/fcstd-corpus -P tools/cli/fcstd-corpus.cmake`.
Some models need FreeCAD
1.0 (VarSets, extents up to shapes) or 1.1 (Whitworth and NPT threads). Ellipsoids and skewed cylinders are
B-spline and extrusion surfaces that FreeCAD's own measures integrate less
exactly (up to 4e-4) than Mitcad's; the reference check does not compare
those.

## .ipt import

mitcad#60. The code is not in this crate: [core/ipt](../ipt/README.md)
reads the file (compound file, property sets, segment database, the B-rep
record, the definitions segment) and decodes the part's design into this
crate's dump IR (`mitcad_ipt::design`); `core/ffi/src/ipt_import.rs` runs
the import and `core/ffi/src/ipt_history.rs` gives the bodies of the ASM
history the B-rep record carries (`StoredGeometry`), state by state, as
the `.f3d` import's `F3dGeometry` does for the `.smbh` blobs. Entry
points: `mitcad-cli import-ipt part.ipt [--save part.mitcad] [--report
report.json] [--reference part.stp [--max-relative X] [--deviation]]
[--bodies-only] [--no-verify] [--no-fallback] [--no-compare] [--time-limit
S] [--dump design.json] [--design design.json] [--json]`, the `import_ipt`
command ([commands.md](../model/src/api/commands.md#ipt-import)) and File ›
Open or Import in the application (the import process, as for `.f3d` and
FreeCAD documents; Cancel only).

- **With the history** (stages 2 and 3, the default): `import_design`
  replays the dump as it replays an `.f3d` design: the part's parameters
  with their expressions (`params.rs`), sketches, work planes and
  features, each checked against the state of the ASM history its
  operation made (the definitions segment's state table names it), with
  the fallbacks, the guesses the history settles (profiles, fillet and
  chamfer edges, hole positions, mirror planes) and the final comparison
  of this crate. The items are those of the browser, in its order.
- **Bodies only** (`--bodies-only`, and files without a definitions
  segment): one base feature per body, solids and sheet bodies alike; a
  body that OCCT cannot build is reported (`skipped`) and left out.
- The document's length unit becomes the part's (the model settings'
  unit code); the material is set when Mitcad's library has it.
- The report lists the part number, material, units, the release that
  saved the file, each B-rep record (ASM version, bodies, the states of
  the ASM history it carries) and each body (solid or sheet, valid,
  volume, area, faces); with the history also the parameters (records,
  model, user, those not the part's), the expressions (translated,
  agreeing with the stored values), the features and the replay's report.
- Not yet: body names and colours, assemblies (`.iam`, stage 4), files
  whose B-rep record is not an ASM file (older releases); what the
  definitions segment does not tell (core/ipt/README.md, *What the files
  do not tell*) comes in as the bodies of the history states.
- A hole whose decoded position does not give its state also tries the
  history's guess (each piece of material the state removes); that guess
  cuts the replay's body with the state's, and when the two are nearly
  coincident (a body a fallback took to the previous state) OCCT's boolean
  can take minutes (two minutes for one hole of the test files): the item
  then has no time left for its definitions and falls back.

### Corpus test

Real part files are kept outside the repository (`MITCAD_IPT_CORPUS`, a
path list of folders). The ctest `ipt.corpus`
([tools/cli/ipt-corpus.cmake](../../tools/cli/ipt-corpus.cmake)) imports
every `.ipt` with `mitcad-cli import-ipt` (with the history), each in a
child process of its own, several at once (`mitcad_run_parallel`;
`MITCAD_CORPUS_JOBS`, `MITCAD_CORPUS_MEMORY`, `MITCAD_CORPUS_TIMEOUT`,
`MITCAD_CORPUS_JOBS=1` checks them in the script's process). Every body
must be built and valid, every expression of the part's parameters must
agree with its stored value (in the file and in Mitcad), and the final
bodies must be the file's; the line per file gives the features' outcomes.
A file with a STEP file of the same part next to it (`<name>.stp` or
`.step`) is compared with it: as many solids, each one's volume and area
within 1e-6 relative, unless the folder's `references.tsv` gives the file
a limit of its own (`<name>.ipt`, a tab, the limit, a tab, why). Such a
limit is for a reference that was not exported from the file and models
the part a little differently: it is the difference measured, rounded up
by a few per cent, so that a change in what the import reads still fails.
`core/ipt/tests/corpus.rs` checks the same files without OCCT: the B-rep
records split out of their segments and every body converts cleanly, the
parameters and their expressions, the features' history states.

The test corpus: 16 public NIST MBE PMI test parts
(11 saved by a 2021 release, ASM 226 and one 227; 5 by a 2024 release, ASM
229), each with STEP files of the same test case that the institute
published separately, made from other CAD systems' models of the case
(sources, terms and checksums: the test-file repository's `ipt/README.md`).
Results (dev build, 2026-10-08):

- Every file reads; every B-rep record splits out of its segment; all 22
  bodies (16 solids, 6 one-face sheets) convert without issues and are
  valid for OCCT's checker.
- The STEP references: one part matches within 1e-6 (3.3e-7). The others
  differ by 5e-4 to 4e-2 in volume or area, because the references model
  the parts differently (different face counts: 270 against 255 faces,
  6 against 8; one plate 3.0 mm thick in the reference, 3.02 mm in the
  part); no published STEP file was exported from these part files (each
  part was compared with all of its case's STEP files, AP203 and AP242:
  none matched better). Their limits are in `references.tsv`.
- Three parts saved by both releases (two saves made independently, ASM
  226 and 229) give the same solids: volume and area within 5e-7 through
  a STEP round trip, 3e-9 between the build reports; two parts were
  changed between the saves.
- With the history (2026-10-08): 2,785 parameters with their expressions,
  all agreeing; 344 features with their history states; of 607 timeline
  items 429 parametric, 64 partial, 102 fallback and 12 skipped (per kind:
  [core/ipt/README.md](../ipt/README.md#results-on-the-test-files)); the
  final bodies are the stored ones in every part.
- Import times: bodies only 0.12 to 0.83 s per file and 45 to 63 MB (dev
  build, which optimises dependencies); with the history 0.2 s to 3 min
  (most under 20 s; the slowest has the hole above). The corpus test
  takes about 3 minutes with 6 files at once.

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
  [--threads N] [--json]`; `--bodies-only` imports the bodies without
  the history;
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
  wrong reading of a two-distance chamfer or asymmetric fillet (the
  other reading is tried right after each definition; which face takes
  which distance is decoded for two-distance chamfers only, *Named
  inputs*), can come within 1e-3, and the
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
  current state that way too. So does a fillet or chamfer with a state
  of its own that failed on the replay's bodies for another reason than a
  result not in the history (OCCT failed or crashed, an edge was not
  found), even on bodies within 1e-5 of the state: every solid comes from
  the stored state, whose edges are split otherwise (an `.ipt` part's
  rounding that crashed OCCT on the replay's bodies built on the stored
  ones). Before, only items without a state of their own were tried on
  the stored state, so an `.ipt` part whose features came to know their
  states lost such a rounding.
- An item without a known state must equal one of the next states before
  the state of the next item that has one (that state is the later
  item's; so are the fallbacks tried for it): exactly (measures within
  1e-5 and the same face count), else the closest within 1e-3. As it may match any of them, a join is not taken when its
  result, or the state it matches, has less volume on as many bodies than
  the bodies before it, nor a cut when it has more (a join can come
  within the tolerance of a later fillet's state by chance).
- A combine's tools that the file says it consumed (`isKeepToolBodies`
  false) stay in the ASM history as they were (the design no longer has
  them; its components' body lists leave them out). Where every tool is
  found by the item that made it and the combine's state holds each tool
  unchanged (the same stored body as in the state before it, with the
  replay's measures of the tool), those stored bodies are left out of the
  history from that state on and out of the
  stored design (`Oracle::retire`, mitcad#96): the combine is checked
  with its tools consumed, and they do not come back with a later base
  feature or the final comparison. Before, only a combine keeping its
  tools gave such a state, and the import kept bodies the design does
  not have.
- The volume an item adds or removes must be the file's too (mitcad#121,
  `history::Change`): its measures within the tolerance relative to the
  bodies let a wrong rounding of a large body pass (a few per cent of a
  small change are below 1e-5 of the body). Its own difference is the
  smaller of its change's from the file's change (the item's state
  minus the state the bodies before it stand for) and its result's
  volume from the state's (the item may take away a difference the bodies
  carried), relative to the larger change; above 1 % the result is not
  taken (unless its faces settle it, below) and the next definition is
  tried (the note of an item that falls
  back says by how much its last one was off). A result that is the state
  exactly (the same faces, measures within 1e-5) and differences below
  1e-6 of the bodies' volume (the measures' noise) are not compared. The
  report gives the difference of each item taken (`change_difference`).
  The change is only as good as the measures: a through-all cut of two
  holes through a cylinder's wall was 2.6 % off (106.6 mm³) because the
  state's holes, bounded by cubics of 132 spans fitted to the
  intersection, were integrated with too few fixed Gauss points; every
  face is integrated span by span now (mitcad#139, mitcad#140,
  `docs/architecture.md`).
- Where the volumes do not settle an item (its own difference above
  0.5 %, `history::UNSETTLED`), its faces do (mitcad#138,
  `src/geometric.rs`): a wrong rounding (its surface 0.15 mm off along its
  whole length), differences at the corners of a rounding only and the
  file's spline approximations (every face within 1e-4 mm, a thin layer
  over the body) all come to a few per cent of a small change. The faces
  the result made or changed (`Kernel::new_faces`: the faces of the
  changed bodies that the bodies before the item do not have, also as the
  same surface, area and centre) are measured against the state's faces,
  and the faces the file's operation made or changed (the state's against
  the state before it) against the result, at up to 16 points spread over
  each (`Kernel::boundary_distances`; 3000 a side at most, so a pattern's
  thousands of faces get fewer each). A face is off when a point lies
  farther than 0.01 mm (`geometric::TOLERANCE`). The result is taken when
  every face off on either side is a corner patch, at most 2 % of that
  side's new area (`geometric::CORNER`) and at most 0.3 mm off
  (`geometric::CORNER_DISTANCE`: the corners of roundings that are the
  file's lay 0.03–0.25 mm off, while corner faces 0.83 and 2.5 mm off were
  other corners), and together they hold at most
  5 % (`geometric::CORNERS`), and when the faces account for the volume
  the change is off: at most twice their area times their distances
  (1e-5 mm at least each; the points are a sample, and a part of a face
  they miss shows in the volume: a cut whose end lay elsewhere on a face
  of 145 000 mm² was 67 mm³ off with every point on the file's faces). A main face off rejects the result,
  also within 1 %. A change more than 10 % off the file's is rejected
  without asking the faces (`history::DIFFERENT`: the differences they let
  pass came to 2–4 %). Where the faces cannot be measured (a state not
  rebuilt) the 1 % rule stays. The largest faces are measured first, and
  the measuring ends at a main face off (then the file's side is not
  measured): a point off a face takes the kernel milliseconds on a large
  body, and a rejected rounding of a design's large body took 10 s a
  definition before. The report gives the faces' verdict
  (`geometric_check`), the trace each side's new area and the faces off.
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
  operations renamed it). The edge's middle point, length, ends and its
  own direction (`_f3d.direction`) go into its fingerprint; the import
  finds it by them and tries those edges first, then the history's
  guesses (*What the history settles*). What the learning dump settled
  (mitcad#96, *Learning the undecoded inputs*):
  - an item's tail ends at its last reference to a health object (some
    fillets refer to one near their start too, and lost their name,
    state and edges);
  - a recipe whose first entity has a negative tag (`"-1029"`) names the
    edge itself first, then its faces; the blob gives edges such names
    too, which pick among the edges between the faces where they are one,
    else the faces after it decide;
  - a recipe may have a second entity list after the first (read over);
  - a set input says what its group selects (the `u32` after `ref
    2DF7DA30 | u32 0`): 8 edges, 16 faces (every edge between the face
    and another is rounded), 9 a face the next group's edges repeat
    (left out); a group without parameter holders belongs to the next
    group that has some;
  - n inputs of one item with the same names, where exactly n edges fit
    them (n edges between the same two faces, without end faces to tell
    them apart), take one edge each;
  - the import finds a closed edge by its circle and length (the file
    gives its middle half way along its parameter, opposite its seam,
    which the replay puts elsewhere), an open one whose middle is up to
    0.05 mm off by its length (the parameter middle of a B-spline or
    ellipse edge), and an edge the replay splits in two by the two pieces
    on its segment or circle whose lengths add up to its
    (`refs::dressup_edge_match`);
  - replay matches with midpoint-plus-length scores tied within
    `1e-9` mm in one body are refused (mitcad#106); coincident copies in
    different bodies keep the existing first-body rule. Repeated aliases
    of the same body and edge name count once. Matcher failures distinguish
    `decoder_unresolved` (no decoded edge geometry),
    `missing_fingerprint` (no external fingerprint geometry),
    `missing_in_replay`, and `ambiguous_in_replay`. Decoder input failure
    notes retain the edge set, input position and decoder's `found` reason.
    Recipe tails remain
    unused for replay matching until their semantics are proved;
  - a two-distance chamfer's first distance lies on the face left of the
    edge's own direction in the file (the face whose coedge runs along
    it, seen against the face's normal): the import sets `flip` so that
    the replay's first distance lands there (per tangent chain, by the
    most of its edges; sets of equal distances keep theirs), and the
    other reading stays a guess after it;
  - the edges found by their names are also tried one tangent chain at a
    time (a feature per chain), right after all at once.
- **Bodies** of moves, splits, body patterns and mirrors: by their tags;
  the middle points of some of their edges let the import find them among
  its bodies (`refs::resolve_body`), else as the only body whose faces
  pass through those points (a replayed sweep or pipe has the stored
  surfaces but its seams and edges elsewhere). A mirror's joining is not
  decoded: separate copies are tried first, then joined ones.
- **Bodies by the item that made them** (mitcad#96): every body input
  also refers to the body's record, which names the timeline item that
  made the body (`_f3d.producer`) and, by the record's position among
  that item's own records, which of its bodies (`_f3d.body_index`;
  `core/f3d/src/design/build/producers.rs`). The import keeps what each
  item made (`producers.rs`): the bodies of the features made for it (a
  split's pieces from the bodies it split, which keep their ids). A body
  input is found there first, then by its names (for an item with several
  bodies the names choose among them, else the index). Where base
  features took the item's state (a fallback), they bring every body that
  differs from the state, and a new body can take the id of one it
  replaces: there the names come first, then the base features' new
  bodies in the item's component. This also names bodies whose recipe
  does not decode (pipes' bodies, bodies later features changed: a
  split's bodies, combine tools) and the copies of a pattern, which the
  names alone cannot tell apart. Bodies made by items off the timeline
  (base features of other designs) are found by their names only.
- **Participants** of joins and cuts of extrusions: their body inputs,
  by the item that made each body (mitcad#96).
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
- **Profiles.** The decoder reads the loops of the profiles an extrusion
  or revolution selected (`_f3d_profile_loops`, mitcad#96: the file's
  curve ids, which piece of each curve, outer loop or hole, the pieces a
  profile is split into). `src/profiles.rs` maps them onto Mitcad's
  regions: a listed profile is a face (its outer loops less its holes),
  and a region is selected when its inside point lies inside one by the
  even-odd rule against the listed curve pieces, with the file's curve
  geometry (a curve cut where the other curves of its loop cross it, the
  pieces numbered along its parameter); where a profile lists curves
  without such geometry (ellipses, splines) or selects nothing, its outer
  loops select the regions whose outer loops share most of their curves.
  Over both private corpora the history accepted the decoded regions for
  1509 of the 1532 settled extrusions that have the loops (most others
  are sets it cannot tell apart from them) and for all 97 settled
  revolutions. Those regions are tried first, before any ranking
  (`Candidate::first`), with the decoded extent; without a history they
  are the ones used. Then, as before the loops were read:
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
- **Participants** of a join, cut or intersection: the decoded ones first
  when every one is found (all bodies when they are all the solids of the
  sketch's component), then all bodies; without decoded ones all bodies
  first, then the bodies of the sketch's component that the state
  changed (a cut in the file may leave bodies in its way alone).
- **Direction:** the decoded direction vector against the sketch's normal
  (one side through all: the stored vector points away from the
  extrusion; a vector oblique to the normal, along the normal: all 36
  such settled extrusions went that way, mitcad#104), else the decoder's
  ±1, then the other way. What the file does not decide (mitcad#104):
  a one-sided cut through all whose flag byte after the extent codes is
  0 goes against the stored vector in 15 of the 22 the history settled
  over both private corpora and along it in 7, and no byte of its record
  or its inputs, nor the side of the sketch plane where the bodies' volume
  lies (along it in 4, against it in 9 of those with the volume on the
  vector's side), nor the material just off the profile tells which;
  some distance extrusions (13 of about 1400 one-sided ones, most with a
  join or cut whose other side gives a state too) and 4 two-sided ones
  with equal or nearly equal sides likewise. The history settles those
  (the other way is the next definition); without a history the first
  is taken; **symmetric
  extents** (the decoded length first, then the other; without one, half
  the length each way first, then the whole length); **up to an object**
  (the decoded object first, towards the side of the sketch its point lies
  on, else by the stream's flag; then, and without a decoded object,
  planar faces parallel to the sketch, nearest first; a two-sided
  extrusion has an input slot for each side up to an object, decoded in
  order onto the sides the parameters `Side1Offset` and `Side2Offset`
  make extents up to an entity, each side up to its face or, where the
  replay lacks the face as it is, its plane, side one towards the side of
  the sketch its object lies on: mitcad#104), **revolve axes**
  (sketch lines and origin axes, ranked by Pappus' volume). A
  revolution's operation (join, cut, new body: the first `u32` after its
  root part, an extrusion's codes) and its axis
  when it is a sketch line (the axis input's curve ids, as a sweep's
  path names its curves; a line of another sketch too) are decoded
  (mitcad#96): the decoded axis comes first, the guessed ones after it;
  a revolution whose profiles, axis and extent are all decoded is tried
  first. Axes that are faces stay guesses.
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
  angle (within 10 %). Edges between faces that meet tangentially are
  no dressup's. Tried in order: all those edges at once; one tangent
  chain after another (sets and edges in order); those edges with the
  gone edges that continue them tangentially (a rounding that ends where
  its edge runs on tangentially takes the next edge's middle away too,
  and Mitcad's fillet follows the chain); the edges whose dressup the
  state has whatever their distance, with their continuations; then
  looser guesses (the edges at that distance, all, the largest group at
  one distance). A definition refused because it holds only a part of a
  tangent chain does not rule out the definitions with more edges. The next four states are looked at; for an item whose state
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
    inside out). A result it rejects only on faces of the stored body that
    were invalid already, away from the rounding, is taken as it is
    (mitcad#107).
  - Where OCCT's rounding fails at the size asked, it is tried again on
    the body with the pieces of its edges merged (stored circles in two
    arcs), then with tighter tolerances of OCCT's fillet (knife edges),
    before the retries below (mitcad#107).
  - A rounding or bevel of a whole circle that runs over a neighbouring
    face (the rim of a hole near a side), which OCCT stops at, is built
    as a ring about the circle's axis when both faces are planes,
    cylinders or cones on it ([fillet](../model/src/api/commands.md#fillet)).
- **Combine** target and tools, when the decoded ones are not found: the
  bodies the next state changed (a body with a stored body's measures
  counts as unchanged even where its faces are split otherwise), the
  decoded operation first.
- **Split bodies** the tool does not divide: the file's split leaves such
  a selected body as it is, Mitcad's fails, so each body alone is tried
  after the decoded set (mitcad#96).
- **Replace face's replaced faces** (dumps do not record them): faces of a
  body the next state changed none of whose interior points
  (`Kernel::face_points`, up to nine) lie on the state's faces (1e-3 mm);
  neighbours that extend or shorten keep some. They are listed without the
  tangent chain, and the result is matched within 0.5 % (Mitcad's own
  conventions for curved faces and targets). The target comes from the
  dump.
- **Holes** whose item's through-all flag is set while the file keeps a
  depth for them (`_f3d_through_all`: a byte after the root part, the
  second in class version 4 and the sixth in version 7; seen in two
  designs only, so a lead, mitcad#96): through all first, as a guess the
  history checks, then the depth.
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
  extrusions about the decoded axis (else the line the axis' direction
  input stores, `_f3d_axis`, mitcad#96, then the origin axes; else the
  origin axes), as many as the
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
- **Pattern and mirror objects** decoded as features with fillets and
  chamfers among them (mitcad#105): without them, as before (the copies
  of the other features carry them where those made the edges they
  round), then with them, where every one of them came in parametric (all
  the features made for each, one per tangent chain where the import split
  it), so that the pattern repeats them on its copies (`commands.md`,
  *Fillets and chamfers among the features*; the report's note says so).
  Where the history checks the item, the definitions with them come
  last, after the history's guesses too: repeating roundings on tens of
  copies of a large body takes tens of seconds, and most such items'
  states do not need it (a design at the time limit lost items to them
  otherwise); without a history, with them first. A circular
  pattern whose axis entity (a face, an edge) is not found in the replay
  turns about the line its direction input stores (a construction
  axis' stores its direction times its length); a construction axis that
  was not imported is that line too. A mirror's `combine` is decoded
  (none of the bodies it makes is new: after its first two references
  `u32 0 | u32 n | n` body records), the other way after it as a guess.
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

- A modelling item the file keeps no result for (its result number is -1
  and one of the three state bytes before its health reference is set:
  suppressed or failed in the file, mitcad#96) is skipped at once: it
  changed nothing in the file, and no state could check a definition of
  it (its edges and faces are not in any state either). Before, every
  definition and guess was tried and the history then skipped it.
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
- OCCT's allocator throws `Standard_OutOfMemory` instead of returning
  null, and counts its failures, per process and per thread with the
  jobs of OCCT's thread pool it ran (patches 0030 and 0031 of the port,
  mitcad#132):
  OCCT's collections wrote to the null block, and a large design under
  the corpus runner's 4 GB limit crashed in the checker
  (`CSLib_Class2d`) while one of the file's bodies was healed. The
  checker and the healing catch OCCT's failures inside and go on (the
  checker then calls a sub-shape invalid), so a moved count is what
  tells: a kernel operation and the checks whose thread's count moved
  (`geometry::failed_allocations_in_thread`) fail "out of memory", a
  body of the file whose build moved it does not build ("building a
  body of the file: out of memory", the import is low on memory), and
  the memory guard takes a process count that moved since its last
  measure (`geometry::failed_allocations`) as low memory ("an
  allocation in the geometry kernel failed"), also for failures on
  threads of no import. An operation on another thread at the same time
  is not failed by them.
  `MITCAD_TEST_OCCT_ALLOCATION_FAILS=<operation>` (or `heal`) makes one
  fail and be caught inside for the tests.

The history's bodies are converted keeping only the distinct ones: each
state holds the bodies that differ from the state after it, not a copy of
every body (a design of 256 items took 5.8 GB before the first item that
way, and the memory the allocator kept afterwards left 40 MB of a 12 GB
address-space limit once OCCT's threads started).

### Definitions evaluated in parallel

Most of an import's time goes to definitions the history rejects: each
candidate (*What the history settles*) is a full geometry evaluation. With
`Options::threads` above one (`--threads`, the `threads` option of
`import_f3d`; `mitcad-cli` and the application use the logical cores, at
most 8; `test_f3d_import` one unless `--threads`), the definitions ranked
after the one being evaluated are evaluated ahead by workers (`ahead.rs`,
mitcad#95), and the import is the same as with one thread, only faster:

- **Rank order.** The import still goes through the candidates one by
  one and decides as before; it evaluates candidate *k* on its document
  while workers evaluate *k+1*, *k+2*, … (up to `threads` − 1 at once, as
  far as the item's attempts and a loose match's closer tries reach), from
  the second candidate of a list on: the first is usually the one taken,
  and workers beside it would only take processors and memory from it.
  When
  it comes to a candidate a worker has evaluated (or waits for it), it
  takes the worker's result instead of evaluating it again. So candidate
  *k* is taken only when every one ranked before it was rejected,
  whichever finished first, and everything that depends on earlier
  outcomes (participants that give what the definition without them gave,
  edges of a fillet that failed, `Turns`) is decided as before. Once the
  import leaves the list (one was taken, or none is left) the workers
  still evaluating are cut short through their monitors, as on a stop; the
  import does not wait for them.
- **Copies of the document.** A worker evaluates on its own copy of the
  document as it is before the item (`Document::fork`: the definition, the
  cached results and the last recompute, shared, with a kernel of its own,
  `Kernel::fork`; no undo history). A feature added there gets the uid it
  would get in the document, and its result the same version. A rejected
  candidate leaves the document as it is (it was undone anyway); its
  results go into the document's cache, as if it had been evaluated there.
  The candidate taken is added to the document again and finds the
  worker's results in the cache (`Document::adopt_results`): the same
  shapes, topological names, uids and notes.
- **Shared shapes.** The workers read the same bodies (the results before
  the item) at once. The geometry's operations never modify their inputs
  (T0e): those whose OCCT algorithms write into their inputs' sub-shapes
  (booleans, fillets, chamfers, shells, healing) work on copies
  (`InputCopy`), a `Shape` keeps what it measured of itself behind a lock,
  and OCCT's handles count references atomically. The cancellation scope
  of the geometry is per thread, and each worker installs the kernel's
  crash handlers and ticks the import's progress (`mitcad-ffi`).
- **Time.** A worker's definition is given the first round's share of the
  item's time (`SHARE`) from when it starts, and the item's deadline. One
  that gave way at its share is evaluated again on the document when the
  import would give it the rest of the item's time (the last one);
  otherwise its result stands. Results that depend on the time (an item
  out of time, an import past its time limit) can differ, as between two
  runs with one thread: with workers more definitions fit into the time.
- **Memory.** Each worker holds its definition's memory and a stack of
  64 MiB (the import's own thread has 256 MiB). No worker starts while the
  process uses half of its tightest memory limit or more (*Memory*;
  `Progress::set_memory_share`), and at 60 % the definitions being
  evaluated ahead are cut short and evaluated on the import's thread
  instead when the import comes to them; the import's own definition keeps
  the memory, as with one thread. A worker's definition that runs out of
  memory is evaluated again on the import's thread too.
- **Stop and hangs.** While the import waits for a worker it looks for a
  stop, the watchdog's give-up and low memory every 20 ms: a worker that
  does not return does not keep the stop from stopping. The watchdog
  counts the workers' progress as the import's; a worker the import waits
  for that hangs leaves no progress once the others are done, so the item
  is given up as before (`Options::hung_items`). Workers whose result is
  no longer wanted may still be in a kernel call when the import ends:
  `running_workers()` counts them, and `mitcad-ffi`'s
  `abandoned_imports()` includes them, so that the process ends with
  `std::_Exit` as after a hang.
- **What stays on the import's thread.** The second round of definitions
  that gave way, the approximate definition taken after a loose match,
  the next history state's fillet and chamfer edge guesses (`Guesses`,
  found between the definitions), the edits of a pattern's extrusions
  (*Profiles a pattern or mirror needs*), and the definitions tried on
  bodies the history holds already (*Bodies the history already holds*).

**Where the time goes.** Workers help only where an item has several
definitions queued and the first ones are rejected; the rest of an import
runs on its own thread as before. On a 199-item design of the private
corpus (time limit 300 s) 46 % of the time went to evaluating definitions
on one thread, 8 % to translating items and the rest to finding fillet
and chamfer edges between the definitions, the edits of a pattern's
extrusions, sketches, the history's bodies and the final comparison;
on a 76-item design 52 % went to definitions and 36 % to finding fillet
edges. Most items take their first definition (sketches, construction
geometry, fillets of edges found by their names, extrusions whose first
profile set is right), and a fillet's or chamfer's edge guesses come one
history state at a time, so its list has one or two definitions when the
workers could start. So an import with eight threads keeps about 1.5 to
2.5 cores busy where a single-threaded one keeps 1 to 2 (the geometry
kernel's own parallel booleans), and designs whose time goes to rejected
extrusions, combines and patterns gain most (up to about twice as fast).
A worker's definition also slows the import's own one where the
processors are all busy: on a machine loaded already, an import with
eight threads can take longer than one with one. *Work beside the
replay* below runs more of an import in parallel.

### Work beside the replay

The timeline's items are replayed one after another; with `threads`
above one the work that does not depend on the replay runs beside it
(mitcad#103), and the import is the same as with one thread:

- **Reading the file.** The design streams of every design of the file
  are decoded first (`mitcad_f3d::design::decode_streams`); only the
  chosen design's named inputs are then looked up in the bodies' history
  (`resolve_inputs`), its items on several threads (each item reads the
  history's blobs only, read once for all). That runs at the same time
  as the conversion of the bodies of every history state to the neutral
  model (`StoredFile::new`), whose states are converted on several
  threads and taken in order as before (`mitcad_import::in_order`), so
  that the same bodies are kept once each. The two share the threads
  half and half. On a large design of the private corpus reading took
  about 140 s on one thread (80 s finding the inputs, 60 s converting);
  with eight threads about 20 s. An item's inputs are looked up in one
  named history state at a time (the state before the item, then up to
  24 earlier ones per blob, until each input is found): naming all of
  them at once on every thread ran the largest design of the backup
  corpus out of a 4 GB limit before the import began (an allocation of
  Rust's, which ends the process, mitcad#132).
- **The history's bodies built ahead.** When the replay asks for a
  history state, the bodies of the next four states that it has not built
  yet are built (healed) and measured on other threads
  (`F3dGeometry::build_ahead`, `mitcad-ffi`): the replay finds them
  built when it comes to them (the same bodies; a shape keeps what it
  measured of itself). A try that asks for a body being built ahead waits
  for it and ticks the watchdog while the build makes progress, so a
  build that does not return still looks hung; the threads count in
  `abandoned_imports()`. Bodies built ahead that no try took are dropped
  with the states behind the replay when memory gets tight. On the large
  design above, building two bodies of 15 000 and 23 000 faces took the
  first item 142 s on one thread.
- **One budget.** The workers that evaluate definitions ahead and the
  threads that build bodies ahead take their threads from the same
  `Options::threads` (`Progress::helper`: the import's own thread is one of
  them), and none starts while the process uses half of its tightest
  memory limit or is low on memory.
- **Planar faces kept per body.** Every sketch looks for its plane among
  the planar faces of all bodies (`refs::planar_faces`); on bodies of
  thousands of faces that took seconds per sketch (12 s on a body of
  23 000 faces, where most of a 256-item design's time went to its 73
  sketches). An import keeps the planar faces it found by the body's
  version (`refs::KeptPlanes`), which names one shape, so a sketch finds
  them again until the body changes. This is quicker with one thread too.

Coverage runs before and after these changes, at the same time with six
threads (time limit 300 s): `MITCAD_F3D_CORPUS` 575 s → 520 s in all
(1.11×, the slowest designs 1.08–1.39×), the backup corpus 4759 s →
3287 s (1.45×; the 256-item design above 1005 s → 341 s, others up to
3×). The reports with one and with eight threads are the same except
where a time limit decides (`MITCAD_F3D_CORPUS` 47 of 48 identical,
the backup corpus 318 of 325, each difference first at an item whose
time ran out or in the time-limited final comparison).

**Why the items are not replayed in parallel.** A profile of the five
slowest designs of each corpus (four and eight threads, time limit
300 s, before the changes above) kept 1.0 to 3.1 cores busy (mostly 1.4
to 2.3). Evaluating definitions took most of the time of the corpus
designs (the workers' share), finding fillet and chamfer edges 10–30 %
of the fillet-heavy ones, sketches up to 70 % of one large design
(the planar faces above), building the history's bodies up to 240 s of
another, and reading the file up to 140 s. Replaying independent items
at the same time would need items that neither read nor change what an
earlier one changes. Most designs are one component (in the slowest
ones 62–100 % of the items with a state change the same component), and
most of a part's items change the same body. Counting only the bodies
each item's state changes (an item reads at least those; a join or cut
with all bodies as participants reads more), the longest chain of
dependent items holds 42–66 % of the items' time on the slowest corpus
designs (a bound of 1.5–2.4× at best); with the items without a state of
their own (sketches, construction geometry, which find their planes on
the bodies) as dependencies of what follows, the chain is 82–100 % of
the time. A speculative item would also have to
run on a copy of the document and the importer's state (report, history
cursor, alternatives, sketches) and be replayed into the document as
the definitions evaluated ahead are, and the features' uids and
topological names of later items depend on every earlier item's. So the
replay stays sequential, and the work beside it runs ahead instead.

Not done: finding the fillet and chamfer edges of several history
states at once (10–30 % of the time of fillet-heavy designs). The search
asks the stored bodies' boundary index (`Kernel::boundary_distances`),
which keeps the projections it set up between calls; asked in another
order on other threads, the same answers are not assured, so it stays on
the import's thread.

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

- a combine whose tools are bodies of another component than its target
  (a part cut by another part) goes into the target's component and
  takes those tools by their occurrence links (`tool_links`,
  `commands.md`, *combine*; mitcad#104): the model reads each tool in its
  component and moves it by the placements at the combine's point of the
  timeline. Such a combine that consumes its tools lists body removals
  in their components instead of body records (`selections.rs`); the
  consumed tools leave their components. Before, these combines fell
  back (5 items of `MITCAD_F3D_CORPUS` and 17 of the backup corpus, all
  but one cuts keeping their tools); all now give their states.

These need each component placed once (one occurrence on the way from the
root); else the feature falls back. Other features that work on bodies of
two components at once fall back too: the base feature takes the item's
history state in each component, and later items continue
parametrically.

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
  distances, tapers and start offset, the direction as a vector, the
  object of an extent up to an object (a face by its names, or a plane;
  mitcad#96) and whether a symmetric distance is the whole length. Not
  decoded: which profiles (only how many, see *What the history settles*),
  a two-sided extent's object side (it falls back), which way a one-sided
  extent through all goes when the stream's flag is not set (the vector is
  the sketch's normal; the history settles it), thin walls and participant
  bodies.
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
    [--time-limit S] [--hang-limit S] [--reports DIR] [--threads N] \
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
- `--threads N` imports with N threads (*Definitions evaluated in
  parallel*, *Work beside the replay*; default 1). Keep `--jobs` ×
  `--threads` within the cores; the reports are those of `--threads 1`
  except where the time decides.
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
  neighbouring face, which OCCT does only where it runs over that face
  along the whole edge (the port's edge overflow, mitcad#121) and around
  whole circles (built as rings), or the body is a stored one with tolerances
  up to 0.05 mm (the classes and counts: *Fillets and chamfers whose
  by-name definition fails in the kernel*);
- items after the time limit (very large designs);
- patterns whose input is not one of the last six extrusions, or copies
  features other than extrusions (with their fillets, for example);
- items whose state an earlier fallback took the replay past, mostly
  fillets and cuts, which cannot leave the bodies unchanged, so they stay
  skipped or fall back;
- items without a translation (*Items without a translation*), items of
  unknown classes (`?…`), construction axes, and the untranslated types
  under *Limits*, which are skipped or fall back.

### Learning the undecoded inputs

Every item whose definition the history settles (*What the history
settles*) is a labelled example of what the stream decoder does not read
yet: the item's raw record and the answer the history accepted
(mitcad#96). The learning dump collects them, and a tester checks rules
from the record's bytes to the answer on them, without geometry.

**Dump.** `mitcad-cli import-f3d part.f3d --learn DIR`, the `learn` field
of the `import_f3d` command, or `MITCAD_IMPORT_LEARN=DIR` (read by
`mitcad-ffi`, so `test_f3d_import --corpus` writes it for every design):

```bash
MITCAD_IMPORT_LEARN=$HOME/f3d-learn build/rel/core/test_f3d_import --corpus \
    --time-limit 300 --hang-limit 300 --jobs 5 --memory 4G
```

The lines are derived from the designs: keep the directory outside the
repository, like the corpora. Per design, `DIR/settled/<design>.jsonl`
holds the items accepted against their known history state and
`DIR/unsettled/<design>.jsonl` the others that had candidates (none
accepted, or accepted unchecked), plus fillet and chamfer items whose
translation failed or offered no candidates (mitcad#106). Their decoded
edge match diagnostics remain available even without history fallback;
`candidates` is empty and `accepted` and `answer` are null when none were
offered or accepted. A rule must not contradict unsettled examples either.
A rerun replaces a design's files. The dump's own work (the
bodies, inputs and sketches it records when an item's definitions are
translated: seconds on a large item) is left out of the item's time and
the import's time limit (`learn::Clock`; the deadlines move by it), so
that a dump run gives no item up that a run without it keeps; candidates
are kept once by a hash of their definitions, and at most 1000 per item
(an item can have tens of thousands, which took gigabytes). The import reports to
`src/learn.rs` at four points: the candidates translated
(`Importer::translate`), ranked (`Importer::rank`), the one accepted
(`Importer::accepted`) and the end of the import; the raw records come
from `mitcad_f3d::design::learn`.

One line (`schema` 1):

| Key | |
|---|---|
| `file`, `item`, `name`, `type`, `class`, `class_version`, `object_id`, `f3d` | the design, the item's timeline index, its type and the decoder's raw item fields |
| `status`, `outcome`, `note` | `settled` or `unsettled`; how the item came in |
| `translation_error` | present only when a fillet or chamfer translation failed: the most recent failure text, including the input position and decoder/replay reason when available. Retained if a later translation offers candidates |
| `known` | what the decoder made of the item (the dump IR's item without `_f3d`) |
| `record` | the item's object and the input objects it refers to, three references deep: `objects[]` with `path` (`item`, `item>0897AF07#0>C46D3EEB#0`: the class's first eight digits and the position among the parent's references to that class), `id`, `class`, `module`, `version`, `len`, `hex` (up to 16 KiB), `sub` (sub-chunk start) and `tokens` (`header`, `root` with its references and attributes, `ref` with `id` and `class`, `str16` with `text`, `sub`). Other timeline items, sketches and their entities, parameters, components and the like are not followed |
| `context.sketches` | per sketch the candidates use (`T<index>`): `regions[]` as Mitcad computes them, in the file's ids (`key`, `outer` and `holes` curve lists, `area` mm², `centroid` and an `inside` point in sketch mm), the `frame` the import placed the sketch on (`origin`, `x_axis`, `y_axis`, `normal`; model mm, the space of an extrusion's stored direction) and `curves` by the file's ids (`object`, `class`, the `u64` root attributes such as `crv_primary_id`, `crv_secondary_id`, `pt_tag`; `type`, `construction`, `geometry` from the dump, sketch space in cm). Curves Mitcad made itself appear as `m<id>` |
| `context.bodies`, `context.edges` | the bodies before the item (`uid`, `volume`, `area`, `center`, `faces`; at most 200) and the edges the candidates name (`mid`, `length`, `body`), mm |
| `context.resolved` | the bodies, faces and edges the decoder gave for the item (by JSON pointer into `known`, e.g. `/detail/edgeSets/0/edges/2`) as the replay's (`{"kind": "body", "uid"}`, `{"kind", "body", "name"}`; `null`: not found), so that a decoded input can be compared with the answer |
| `context.edge_matches` | additive dressup matcher diagnostics for decoded edge inputs at those same pointers (mitcad#106): `status` (`matched`, `decoder_unresolved`, `missing_in_replay`, `ambiguous_in_replay`), `decoder_found` (the decoder's `found` text), `count` and either `matches[]` (`body`, `name`, `mid` mm, `direction`) or ambiguity `candidates[]` (`body`, `name`). The first 64 decoded inputs are considered, with at most 16 matches or candidates per edge; `count` retains the full number. Bodies and names use the file's timeline indices. This uses the actual dressup matcher, including circles and split edges; `context.resolved` retains its older midpoint matching for compatibility. No recipe-tail rule is applied |
| `candidates[]` | in the order tried, the first 1000 (`cost.candidates` counts all, `accepted` is the rank among all): `defs` (Mitcad feature definitions in the file's terms: feature uids as timeline indices `T<i>`, profile regions as `{"sketch": "T<i>", "region": <index in regions>}`, sketch curves by the file's ids), `note`, `guess`, `predicted` |
| `accepted`, `answer` | the accepted candidate's rank and the candidate itself |
| `cost` | `candidates`, `rank`, `seconds` from the translation to the acceptance |

**Tester.** `tools/f3d-learn/learn.py` (Python 3, standard library;
`--data DIR`, default `MITCAD_IMPORT_LEARN`) reads only the dump:

- `stats`: settled and unsettled items per type with their cost;
- `show` / `fields`: examples with their objects, tokens and the gaps
  between them, and one object's fields over all examples;
- `test`: a rule as a Python expression (`--expr`, compared with a target
  such as `regions`, `curves`, `edges`, `flip`, `operation`, `extent`,
  `participants`, `axis`, `def.<key>`), a check (`--check`) or a file
  defining `predict(ex)` or `check(ex)` (`--rule`); prints how many
  examples it explains, contradicts or does not apply to, the
  counterexamples, and with `--unsettled` whether its predictions for
  the unsettled items are among their candidates;
- `search`: rule families over every field of every object (fields are
  addressed `gK+N` / `gK-N` in the K-th gap, `@N`, or `rK` for the K-th
  reference, read as `u8` … `f64` or `sign`): `eq` (the field is the
  value, or the number of selected entities), `map` (each value stands
  for one answer class), `idin` (the field holds an id of an answer
  entity and of no other), `idset` (the object holds the ids of the
  answer's entities), `bits` (bit i selects region i);
- `survey`: per type the record's objects, their lengths and gaps, and
  the fields that vary with their values and how purely they go with a
  target.

Its unit tests (`tools/f3d-learn/test_learn.py`, the ctest
`tools.f3d_learn`) run on a synthetic dataset; `src/learn.rs` and
`mitcad_f3d::design::learn` have their own.

A rule that explains every settled example (or every counterexample with
a reason) and agrees with the unsettled ones goes into `core/f3d` with a
unit test on a synthetic record; the import then tries the decoded
answer first, the guesses after it.

Decoded so far (mitcad#96, *What the history settles* says how the import
uses them): the profile loops of extrusions and revolutions
(`_f3d_profile_loops`), a revolution's operation, profile count and
sketch-line axis, a circular pattern's axis line (`_f3d_axis`) and the
direction of each rectangular pattern direction from a construction axis,
a mirror's `isCombine`, a hole's through-all flag where the file keeps
a depth (`_f3d_through_all`, a lead), and fillet and chamfer edges
(the item tail's anchor, recipes that name the edge itself or carry a
second list, face selections, inputs of the same names, the replay's
edges where the middle does not tell, a two-distance chamfer's sides:
*Named inputs*), and items without a result (*Fallback*). A mirror's plane and the patterns'
quantities the decoder read already (the dump confirms them on every
settled item). For extrusions also the flag byte after the extent codes,
the input slots' roles, the object of an extent up to an object and a
symmetric extent's length (`ExtrudeFields`, `decode::extent_slots`; in
the dump `_f3d.extrude.flag`, `slot_roles`, `full_length` and
`extentOne.entity`). Then (mitcad#104): both sides of a two-sided
extrusion up to objects, a combine's body removals in other components
(*Components*) and an oblique stored extrusion vector.

#### What the file does not settle yet (mitcad#104)

Counted over both private corpora with the learning dump and per-file
traces of the import (the backup corpus repeats some designs of
`MITCAD_F3D_CORPUS`, so these are instances, not distinct items):

- **Flips no byte decides** (*What the history settles*, *Direction*):
  one-sided cuts through all with the flag byte 0, a few distance and
  two-sided extrusions. The history settles them with the next
  definition; files without a history get the first.
- **Patterns and mirrors of features with fillets or chamfers among
  their objects** (the case of a pattern's join whose volume differed
  from its state). A rectangular pattern of a boss, a cut and a chamfer
  between the boss and the plate gave 3979 mm³ less than its state: the
  file's six copies carry the chamfer (755 mm³ each, the five copies
  added come to about that), Mitcad's copies of the features do not (a
  chamfer leaves no tool body to copy; the decoded objects leave
  dressups out). The order of the copies does not matter there (applying
  each element's copies in the features' order gave the same volume). 36
  unsettled patterns and mirrors have such objects (35 settled ones too,
  where the dressups made no difference to the state). Done in
  mitcad#105: patterns and mirrors repeat fillets and chamfers among their
  features on the copies' edges (`commands.md`, *Fillets and chamfers
  among the features*), and the import tries them (*What the history
  settles*). Over both private corpora the patterns and mirrors with such
  objects came in parametric 30 → 39 of 72 (the rest: the copies'
  roundings OCCT does not build, mostly where copies overlap or meet the
  original's rounding, mitcad#107; copies that reach no body after an
  earlier fallback; the time limit), the final bodies as before, the
  import's time unchanged.
- **Fillets and chamfers whose by-name definition fails in the kernel**:
  273 fall back with every edge found by its name. 218 of them round
  edges of stored faces (bodies a base feature brought in, the faces
  named `import`: an earlier fallback, or the file's own base
  features), whose tolerances (up to 0.05 mm) and approximations OCCT's
  dressups often do not build on; 25 round replayed faces after an
  earlier fallback; 29 bodies replayed throughout (radius or distance too
  large 10, rounding cannot end at a vertex 6, invalid shape 3, others
  not the state or names gone). In the samples of the last class the
  rounding is wider than a face next to it (a 90 mm edge rounded wider
  than the 4 mm face beside it; a rounding along a chamfer wider than
  the chamfer), so it runs over or removes that face, which OCCT's
  rolling-ball fillet does not do. These are geometry kernel limits, not
  decoding: fewer fallbacks before them is the cure for the first class.

  Classified in mitcad#107 on the 249 such dressups of both private
  corpora (192 distinct inputs), each built again on its stored input
  body by OCCT's rounding alone (the 1e-3 smaller retry and the rings
  off) and compared with the file's next state (volume within 1e-5 and
  the same face count):

  | Class | Dressups (distinct) | Built by the real rounding |
  |---|---|---|
  | OCCT's result rejected only for faults of the input away from the rounding | 12 (12) | yes: taken as it is |
  | a whole circle stored in two arcs | 3 (3) | yes: the pieces merged first |
  | knife edges (the faces' normals opposite) | 3 (2) | yes: tighter tolerances of OCCT's fillet |
  | built, but not the file's body (faces split otherwise; an input with 11 mm tolerances) | 3 (2) | built, not matching |
  | rounding wider than a face next to it: the face gone in the file's body | 54 (43) | no |
  | the same, the face partly gone | 59 (40) | no |
  | the same, the face kept or not known (state unreadable) | 44 (37) | no |
  | two roundings on the two sides of a face narrower than both | 16 (11) | no |
  | rounding cannot end at a vertex (7: the face it ends on runs on tangentially into a face of the other convexity) | 18 (13) | no |
  | OCCT's result invalid and not the file's body | 11 (10) | no |
  | no start for the rolling ball, or the walk fails, on wide faces | 8 (4) | no |
  | a bevel exactly as wide as its face, contours whose corners fail | 7 (5) | no |
  | OCCT finds no edge to round (faces 1° from tangent), OCCT exceptions | 4 (4) | no |
  | OCCT crashes | 7 (6) | no |

  Where the rounding is wider than a neighbouring face, the file's
  rounding runs over that face or rides its far edge, while OCCT's
  rolling ball stays on the two faces of the edge: it finds no start, the
  walk fails, or it cannot end. Deleting the narrow face first
  (`BRepAlgoAPI_Defeaturing`, its neighbours extended; one attempt, the
  faces next to the edges that the rounding is wider than all along) and
  rounding the edge that takes its place gives the file's body in none
  of them: it applies to 91 of the 249, the deletion fails in 49, the
  rounding in 25, 10 leave no edge where the face was, and the 7 it
  builds all take more material than the
  file's rounding (up to twice: the ball touches the extended neighbour
  instead of riding the narrow face's edge); it costs 6 s a case on
  average, up to 127 s on large bodies. It is not done. OCCT's other
  options (polynomial or quasi-angular sections, its continuity and
  tolerance parameters other than the knife edges', one contour after
  another, healing or merging the input's faces first) build none of
  the rest.

  Coverage with the 1e-3 smaller retry and the rings off in both builds
  (the dressups OCCT's rounding builds), before and after, both corpora:
  fillets parametric 237 → 238 and 967 → 991, chamfers 39 → 39 and
  162 → 164, all items 1049 → 1050 and 4677 → 4702; final bodies as
  before except one body of a design stored twice, exact before and now
  4e-6 from the file's (three of its fillets on a stored body with faulty
  faces come in parametric, matching their states within 1e-5).

  The OCCT port's edge overflow (mitcad#121, patches 0010 to 0012 of
  [the port](../../third_party/vcpkg-ports/README.md)) lets the rolling
  ball run onto the face beyond a narrow face or roll on its far edge
  where the narrow face's contact leaves it along the whole edge (OCCT
  found no start there), and drops the narrow face it consumes. Of the
  249, measured the same way: the face gone in the file's body 6 now the
  file's body (MATCH) and 11 within 1e-3 (NEAR: corner patches or
  approximated blend surfaces other than the file's), 1 off; the face
  kept 2 NEAR; the face partly gone none (OCCT's walks there reach the
  far edge and roll on it as before, then fail at the corners or build
  invalid shapes; where the contact leaves a face near the end of the
  spine OCCT cuts the rounding with the face beyond, as before, which the
  file's body agreed with in the samples). The rest fail in OCCT's
  corners at vertices, in walks that switch to a tangent neighbour on the
  wrong branch of its surface, on narrow faces on both sides of the edge,
  or are chamfers (OCCT has no chamfer rolling on an edge). The other
  classes build as before: where two roundings overlap on a face the
  file's one surface replaces both and the band between them, which
  would merge two stripes (OCCT computes both and fails at the corner
  between them); a chamfer exactly as wide as a face gets no start; and
  where roundings of one fillet consume each other's faces, OCCT stops
  because the two stripes' surfaces intersect.
  Coverage with OCCT's and the patched fillet, both corpora: fillets
  parametric 251 → 253 and 1088 → 1101, all items 1070 → 1072 and
  4874 → 4886, circular patterns 89 → 88 in the backup corpus; final
  bodies exact 525 → 523 and 3169 → 3162, the others within 0.1 %, none
  off. The fillets the patch builds mostly failed before by building
  a wrong body, so the replay took the file's state before them and
  went on exactly; now they come in parametric on the chain's earlier
  approximation (an approximate chamfer before them: up to 2e-4), so
  the final bodies are as far from the file's as that chain, and one
  pattern after such a chain no longer comes within the tolerance. One
  rounding next to a 0.0015 mm sliver now builds onto the face beyond
  it, removing 6 % more or less than the file's (its body 7e-6 off),
  which the import takes; and one fillet after three that now come in
  fails on their result.

  Second round (mitcad#121, patch 0013): the overflow runs over several
  narrow faces in a row (a strip and a sliver beyond it) and on both sides
  of the edge, and two roundings whose contacts cross on a face between
  their edges are one rounding. Of the 249, measured the same way: the
  face gone 6 MATCH / 11 NEAR → 9 / 8 (one family of three now the file's
  body: the ball reaches the face beyond a 0.0005 mm sliver instead of
  rolling on the sliver's edge), the face kept 2 → 3 NEAR, two roundings
  overlapping on a face 0 → 3 NEAR (the volume within 1e-7; the file
  merges two coplanar faces the rounding leaves next to each other). Of
  the NEARs, 9 change the volume as the file's within 1 %; 9 do not (2 to
  56 % off) and are not taken by the import's change comparison. With
  the patches 0020 to 0023 after them, the face partly gone builds 3 NEAR
  too. The rest still fail: corners where a rounding that runs onto faces
  beyond ends (`PerformIntersectionAtEnd`, `PerformOneCorner`), walks that
  fail rolling on an edge, roundings partly wider than the narrow face
  near the spine's ends, chamfers exactly as wide as their face (the
  contact lies on the face's boundary, which the hatching of the analytic
  chamfer leaves out), the second rounding riding on the first one's torus
  and earlier roundings the file's rounding replaces.
  Coverage before (main, with OCCT's overflow of 0010 to 0012 and without
  the change comparison) and after, both corpora at the same time: items
  parametric 1061 → 1034 and 4912 → 4796, fillets 252 → 231 and
  1101 → 1035; final bodies exact 523 → 527 and 3162 → 3180, within 0.1 %
  12 → 8 and 72 → 54, off as before (0 and 1). Nearly all the items that
  no longer come in parametric are results the change comparison rejects
  (above; the smallest 1.0 to 1.5 % off with other faces than the file's)
  or items after them that the replay no longer reaches the same way; the
  rest: the time limit in three large designs (as in other runs), the
  fillet after the sliver rounding above, now the file's, whose edges
  then resolve to other edges (the design is in both corpora), and one
  rounding next to earlier roundings stored as splines that the file
  rounds again (built within 1.7e-4 before, now off). The patch alone
  (the same import before and after): 1 fillet lost and 1 gained in the
  main corpus, 3 lost and 4 gained in the backup corpus.

  Third round (mitcad#121, patches 0014 to 0016): an analytic rounding or
  chamfer exactly as wide as its face (its contact line on the face's far
  edge) is split on that face and stored as a blend on the edge, so that
  the face goes (0014); a rounding rolling on an edge halves its step
  where a section cannot be reframed (0015); and of two roundings of the
  same kind overlapping on a face narrower than both, which 0013 cannot
  make one, the later rolls on the earlier one's contact line and the two
  meet there (0016). Of the 249, measured the same way: two roundings
  overlapping on a face 0 MATCH / 4 NEAR → 6 / 4; the other classes as
  before. The chamfers exactly as wide as their face now get their exact
  surface but fail in the corners at their ends; walked (non-analytic)
  blends along a face's boundary, corners after an overflow onto
  different faces beyond, rolling that pivots on a point or turns at a
  vertex and the ball leaving the edge it rolls on at the start still
  fail. Coverage before (main) and after, both corpora at the same time:
  items parametric 1059 → 1060 and 4905 → 4907, fillets 244 → 245 and
  1092 → 1094, chamfers 56 and 236 as before; final bodies exact
  527 → 529 and 3176 → 3182, within 0.1 % 8 → 6 and 58 → 52, off as
  before (0 and 1). Lost: a two-distance chamfer exactly as wide as its
  face whose ends meet a cylinder (the exact result has a face of
  inconsistent orientation; before, the 0.1 % smaller retry built it),
  and a fillet whose exactly consumed face takes an edge the file keeps.
  The results rejected at 2 to 4 % with the file's faces were compared
  with the file's bodies: one is a different rounding (0.15 mm from the
  file's along its whole length, 2.1 % off), others differ only at the
  corner patches at the ends (2.4 %) or by spline approximations spread
  over the body (2.4 %): no face count or percentage separates them, so
  the tolerance stays 1 %.

  The other classes were traced inside OCCT's fillet (mitcad#122, patches
  0020 to 0023 of [the port](../../third_party/vcpkg-ports/README.md)).
  Measured the same way, on OCCT with the edge overflow, before and after:

  | Class (dressups) | Cause found | File's body before → after |
  |---|---|---|
  | OCCT crashes (7) | an empty restriction curve after a stripe met an obstacle face that does not hold its edge; a seam intersected with a parallel curve; the corner code reading another stripe's surface data | 0 → 2; none crashes now, 3 fail cleanly (no start past the obstacle), 2 build invalid shapes |
  | OCCT exceptions (2) | the corner of a contour closed by its sharp corner, at a vertex of more than three sharp edges, found only one of the contour's ends | 0 → 2 (3 more of the narrow-face classes now come within 1e-3) |
  | faces 1° to 6° from tangent (2) | an edge OCCT's loose tangency test (0.1 rad) calls tangent was refused although asked for | 0 → 1; the other fails in the walk at the ends of a circle between faces 1° apart |
  | OCCT's result invalid (11) | at a shallow corner of an arc and a line, the meeting point of the contact lines taken on the arc's extension | 0 → 1; the others: corner caps at vertices where a stripe ends on its face's boundary, contact lines reset to the far end of a periodic hatch at the spine's start (`SplitKPart`) |
  | rounding cannot end at a vertex (18) | the contact leaves the face near the end of the spine (a face narrower than the rounding there), and the corner (`PerformMoreThreeCorner`) takes an edge that does not touch the vertex for the continuation of another; correcting that gives invalid corners | 0 → 0 |
  | two roundings on the two sides of a narrow face (16), a bevel exactly as wide as its face (7), consumed earlier roundings (3) | the file's rounding consumes the face or the earlier roundings: edge overflow across two stripes, which OCCT does not merge, and chamfers, which the overflow does not cover | 0 → 0 |
  | no start or the walk fails on wide faces (8) | the walk stops where the contact meets a torus' seam or switches to a tangent neighbour | 0 → 0 |
  | built, not the file's body (3) | OCCT's blend split into slivers at a torus seam (the same volume, more faces) | 0 → 0 |

  None of the 249 crashes now (7 before). Coverage before and after the
  patches (both with the edge overflow), both corpora: fillets parametric
  253 → 256 and 1101 → 1112, chamfers 55 → 55 and 231 → 232, all items
  1072 → 1075 and 4881 → 4898; final bodies as before (523 and 3162
  exact). The backup numbers leave out one large design that reaches the
  coverage runner's 4 GB memory limit in both runs (before: the import's
  low-memory stop; after: OCCT's shape checker failing to allocate, a
  segmentation fault in `CSLib_Class2d`); without the limit it imports
  the same before and after.

  The classes left after mitcad#122 were traced further (mitcad#133, patches
  0024 to 0026 of [the port](../../third_party/vcpkg-ports/README.md)), measured
  the same way on OCCT with patches 0010 to 0023, before and after:

  | Class (dressups) | Cause found | File's body (within 1e-3) before → after |
  |---|---|---|
  | no start or the walk fails on wide faces (8) | a rounding along a whole circle whose contact line on a plane crosses six slots: SplitKPart's pieces were sorted with a stale piece, repeated and lost, and the walks between them threw (3, one design, fixed by 0024); a contact line crossing a notch in its plane (2) or a near-tangent spline edge whose approximation fails (2); a rounding wider than the earlier roundings on both sides of its edge (1, the edge overflow's class) | 0 (0) → 0 (3); the file keeps one torus across the slots and trims it with the slots' faces, OCCT runs the rounding along the slot walls (0.7 % of the rounding's volume) |
  | rounding cannot end at a vertex, the face continuing tangentially (7) | the rounding's face turns tangentially into a rounded wall at the end and the contact on it ends off any edge (the OnSame state), the end cut by the rounded wall and the faces beyond it: OCCT's intersection at end gave up on a contact off an edge (5, two designs, the face on either side of the rounding, fixed by 0026: the face's edge at the vertex is prolonged to the contact, the spine's edge cut there, the ends on the cylinder taken in its period); the rounded wall at the end is an earlier rounding that the file extends along the new one in a way OCCT's corners do not build (2) | 0 → 5 (the file's body) |
  | rounding cannot end at a vertex, other (11) | an end cut by a sliver, a cylinder's corner patch and the next sliver (2, one design, fixed by 0025: the intersection at end took the other sliver's crossing of their shared circle, did not prolong the slivers' edges to crossings beyond their ends nor cut the corner patch's edges, counted the cylinder's seam as an edge at one corner, and did not cut the end's intersection at the seam); two roundings meeting on a face narrower than both (5, the edge overflow's class; mitcad#121's patch 0013 does not build them yet); the file extending a cone at the end (2); end corners of a radius of 100 crossing on the face at the end (1); an edge of 0.0006 mm at the vertex (1) | 0 → 2 (the file's body) |
  | OCCT's result invalid (10) | the edge chain ends on both sides of a notch the file's rounding runs across (6, two designs: the file continues the torus across the notch and trims it with the notch's faces; SplitKPart also resets the contact point to the far end of the periodic hatch there; the chains end at faces 0.17 to 0.57 mm wide under roundings of 0.3 to 0.5 mm, so the rounding overflows them as in the narrow face class); side faces narrower than the rounding (2, the edge overflow's class); a face of 2e-5 mm² in the input (1); a chamfer, not traced (1) | 0 → 0 |
  | built, not the file's body (3) | a contact circle tangent to two edges of the plate (the hatch sees two crossings 0.008 mm long, which OCCT walks as slivers; merging them gives the file's faces but a plane face touching itself, which the file splits); an input with 11 mm tolerances (2) | 0 → 0 |
  | faces 1° from tangent (1), crashes that now fail cleanly (3) | the rolling ball's radius exceeds the convex curvature radius of a face (10 on a circle of 8, 1 past a corner of 0.5): the walk reports a twist and stops | 0 → 0 |
  | crashes that now build invalid shapes (2) | the end of a concave rounding where the chain turns convex tangentially (1); not traced (1) | 0 → 0 |

  Over both corpora (fallbacks on, with patches 0024 to 0026 against
  without) fillets came in parametric 4 and 15 times more and as
  fallbacks 4 and 21 times less; no final body is off. A few final bodies
  move from exact to within 0.1 %: a fillet that now builds runs on a body
  where an earlier fillet failed and was accepted within the history's
  tolerance, so the earlier fillet's missing rounding reaches the final
  body instead of being replaced by the stored state.

- **Edge names several edges fit** ("n edges fit the names"): 118 edges
  in 25 items. The recipe's tail after its type string, `i32 -1 | i32 -1
  | u32 n` then per vertex of the edge a record of entity indices
  separated by `-1`, appears to list the faces around each end of the
  edge in cyclic order (the edge's two faces and the end face, in an
  orientation that differs between otherwise equal recipes), followed by
  two words (`0 0`) or longer index lists; it would tell mirror-image
  edges apart, but the decoder's B-rep walk has no cyclic face order
  around vertices yet. Not decoded in this round.
- **Named edges the replay lacks**: of the settled fillets whose every
  edge the decoder found but whose answer is a guess, 85 have named edges
  that are not in the replay's bodies (no by-name definition); 21 more
  had the by-name definition rejected, the answer differing from the
  names both ways (the replay's bodies differ from the file's there).
- **One tangent chain at a time** (`by_chains`): building the
  definition takes no measurable time (0.4 s over 1547 items); its
  evaluation does. Over the 125 files with fillets that fell back or took
  a guess, it was evaluated 300 times for 72 s (5 % of those fillets'
  1488 s): after an approximate match of the edges at once 68 times
  (23 gave the state or came closer, 12 s), after a kernel failure 218
  times (6 gave the state, 57 s), after another state 14 times (1). No
  failure kind, chain count or whether the chains meet at a vertex
  separates the 6 from the 212, so it stays after every failure.

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
those. Nor does it compare the measures of shapes brought in as FreeCAD
stored them where FreeCAD's are those of OCCT's plain fixed Gauss points
(`Kernel::fixed_point_properties`, `placed[].fixed`): Mitcad integrates
faces bounded by B-splines of many spans more exactly (mitcad#139; 2e-6 of
the volume on the hydraulic cylinders of the assembly example).

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
S] [--hang-limit S] [--dump design.json] [--design design.json] [--json]`, the `import_ipt`
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
  With `Options::validate` a definition that gives an item's state but
  leaves a body OCCT's checker finds invalid is not taken: the item takes
  the file's bodies (a cut checked only near its change made a part's
  body invalid).
  An item whose state the history does not hold, between states it does
  (a feature suppressed in the file, `StoredGeometry::item_without_result`),
  is skipped as one the file keeps no result for, as an `.f3d` item with
  result number -1 is: before, its definitions were tried against the
  states up to the next item's, and it was skipped or taken into a later
  fallback, often with a reason that did not apply (*its result is not in
  the file's history*, *the extrusion does not cut into any participant
  body*, *its edges were not decoded …*).
  A stored body of several lumps (disjoint solids in one body, as a join
  of a piece that touches nothing makes it in the file) comes in as one
  body per lump in the history states and the stored design, as the
  replay makes them (Mitcad keeps disjoint solids apart); before, such a
  join's replay had one body more than its state and fell back. With `hang_limit` (`--hang-limit`; the application 90 s,
  `ipt.corpus` 300 s) the replay runs on its own thread, watched as the
  `.f3d` import's (*Hangs* above): an item whose kernel call does not
  return (a fuse of two revolved regions that never ended) takes the
  file's bodies in the next try.
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
- Assemblies (`.iam`, stage 4) are an import of their own, which imports
  each part this way (*.iam import* below).
- Not yet: body names and colours, files whose B-rep record is not an ASM
  file (older releases); what the
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
- With the history (2026-10-09): 2,785 parameters with their expressions,
  all agreeing; 344 features with their history states; of 607 timeline
  items 450 parametric, 53 partial, 92 fallback and 12 skipped (per kind:
  [core/ipt/README.md](../ipt/README.md#results-on-the-test-files)); the
  final bodies are the stored ones in every part.
- Import times: bodies only 0.12 to 0.83 s per file and 45 to 63 MB (dev
  build, which optimises dependencies); with the history 0.2 s to 3 min
  (most under 20 s; the slowest has the hole above). The corpus test
  takes about 3 minutes with 6 files at once.

## .iam import

mitcad#60, stage 4. [core/ipt](../ipt/README.md#assemblies) reads the
assembly (referenced files, occurrences, placements, the range boxes and
display transforms the file stores); `core/ffi/src/iam_import.rs` makes
Mitcad's components and occurrences of them and imports each part with
the `.ipt` import above, in a document of its own: its bodies become a
base feature of a component, or with `--history` its replayed design is
copied into one (`Document::add_component_copy`). Entry points: `mitcad-cli
import-iam assembly.iam [--save out.mitcad] [--report report.json]
[--search folder]... [--history [--no-verify] [--no-fallback]
[--no-compare] [--time-limit S]] [--json]` (`import-ipt` takes an `.iam`
too), the `import_iam` command
([commands.md](../model/src/api/commands.md#iam-import)) and File › Open
or Import in the application (the import process, as for parts).

- Each part file comes in once, its stored bodies as base features (with
  `--history` its features as the `.ipt` import replays them), and each
  occurrence is an occurrence of its component;
  sub-assemblies are components with their own occurrences.
- Referenced files are found as saved, relative to the assembly as it was
  saved, by the saved path's tail, or by name in the search folders; a
  part found so is checked to be the referenced document by the version
  id both files store. Files that are not found are empty components,
  placed, and reported.
- Checks against the file: each placement against the transform the file
  displays the occurrence with, each part's bodies within the range box
  the file stores for it (parts saved again after the assembly only
  reported).
- Not yet: constraints and joints (the stored placements are taken as
  they are), design views and positional representations, the model state
  an occurrence uses (the part comes in in its active state).

### .iam corpus test

`iam.corpus` ([tools/cli/iam-corpus.cmake](../../tools/cli/iam-corpus.cmake))
imports every `.iam` under `MITCAD_IPT_CORPUS` with `mitcad-cli
import-iam`, the corpus folders searched by name, each in a child process
of its own, several at once (as `ipt.corpus`). Every occurrence that is
not suppressed must be placed, every part file found must import, every
placement must agree with the displayed one and every part's bodies must
lie within their stored range box; files that are not in the corpus
(library parts, files of other projects) are reported, not failed.

Results (dev build, 2026-10-09) on a private project tree of 166
assemblies: see [core/ipt/README.md](../ipt/README.md#results-on-the-assemblies).

# The .f3d import: the history as the reference

How the replay is checked against the ASM history in the file: the history
as the reference, the named inputs, sketch patterns, threads, sweeps, path
patterns and what the history settles. Part of the [.f3d import](f3d.md)
notes; the [import documents](../README.md#documents) list the other topics.

## The history as the reference

`core/ffi` reads the ASM history of the `.smbh` blobs and rolls it back
operation by operation. A timeline item's `_f3d.result_no` is the number of
the ASM state its operation made. Each component's history counts its own
operations (two components both have an operation 3), so the number names
the operation of the blob of the component that owns the item
(`_f3d.component`, [*Components*](f3d-components.md#components)) when that blob has it; else (a feature
that changes another component's bodies) of the other blobs with that
number that no item of their own component names, those whose bodies it
changed first. A history blob belongs to the component whose items name
most of its operations (mitcad#156), where they name more of them than
another component's items do and more than of another blob; the other
blobs go by the bodies they share with a component's `.smb` blob (the
numbers count per component, so inserted parts, whose base features are
all their component's operation 3, name every such blob alike). (By the bodies
alone, a body moved into a new component, which is in that component's
`.smb` too, linked the root's history to the new component; then neither
history was its component's, every item of the design went into one
component, and the items of the other components missed their states.)
A component made from bodies has no result number of its own; its
operation is that of the base feature that put the bodies into the new
component's history (`_f3d.made_result_no`). The states of all blobs are
merged in timeline order into one sequence, oldest first:

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
  feature for what earlier items changed without being replayed. The
  item's state itself is asked for when the item first needs it, after its
  first definition was evaluated unless its translation reads it before
  (mitcad#701, [*An item's own state asked for when first needed*](f3d-performance.md#an-items-own-state-asked-for-when-first-needed-mitcad701));
  where it cannot be rebuilt, nothing of the try is kept and the item's
  definition is taken unchecked.
- Operations no item names that come right before an item's own in its
  blob, after the operation of the item before, may be the item's own: a
  pattern makes one per copy and names the last (mitcad#156). Where the
  replay is at the state before them, the item is tried from there first,
  in half its time; else as above, from the state right before its own.
  Before, a base feature brought every such pattern to its last copy but
  one (149 base features in the corpora, most before patterns). An item
  that has no definition there because inputs of it were not decoded (a
  path pattern's path) is not tried again from the state right before its
  own: it fails so on any bodies, and building that state (its last copy
  but one) took 6.6 s for a pattern of 372 copies of a body of 8 000
  faces (mitcad#658); its fallback brings its state from its start.
- An item whose operation the history holds but which left every body as
  it was, and which none of its definitions reproduces (each changes the
  bodies or fails; an item without a translation, such as a base feature
  that brings no body, stays as before), failed in the file: an extrusion
  that found no body to cut. It is
  skipped with a placeholder (`failed in the file: …`), and the bodies
  stay as the items before left them, with no base feature (mitcad#156);
  only a body that differs from the state by more than the geometric
  tolerance is brought to it (mitcad#285).
  The file stores no failure state of its own for it: the three state
  bytes of the item tail and its health object are those of a healthy
  item. An operation that changed a body (a feature that computed with a
  warning) is replayed as any other.
- An item that names the operation an item before it already names (both
  have its result number), so that the history holds no operation of its
  own, and which none of its definitions reproduces, failed in the file
  too, translated or not: its state is the state before it (mitcad#343,
  [*A chamfer that names the rounding's operation before it*](f3d-rounds-3.md#a-chamfer-that-names-the-roundings-operation-before-it-round-11-of-mitcad343) below).
- An extrusion, fillet or chamfer that gives its known state only
  approximately (beyond 1e-5) tries up to eight more definitions and
  keeps the closest: a set of regions next to the right one, or the
  wrong reading of a two-distance chamfer or asymmetric fillet (the
  other reading is tried right after each definition; which face takes
  which distance is decoded for two-distance chamfers, and a distance and
  angle's face from the file's flag, *Named inputs*), can come within
  1e-3, and the
  difference grows in later items. A fillet or chamfer is replaced only
  by one that comes closer by a quarter at least: another edge set's
  rounding can come a little closer to a stored spline rounding by
  chance (4.0e-4 against 4.4e-4) and keep later items from building,
  while a right one came closer by half or more in every design looked
  at. A difference the bodies before it
  carried (from an earlier approximate item, such as a fillet stored as
  splines) is not the extrusion's own: one that comes within 1.5 times
  that difference of its state is taken as it is (no definition comes
  closer). Nor does any for an extrusion whose faces settle it: where
  every face its result made or changed lies on its state within the
  geometric tolerance, every face the file's operation made or changed
  lies on its result, and those faces account for the volume its change
  is off (as for the volume check below, by the faces each side changed),
  it is taken without trying the others (mitcad#658: a cut down the wall
  of a bin with 372 ribs, its volume 6.1e-4 off where its faces lay on
  the file's, tried eight more definitions of 2–10 s each on a body of
  8 000 faces before taking the first).
- When an extrusion, or copies the history guessed (patterns and mirrors
  without decoded inputs), still gives its state only approximately, a
  base feature right after it brings the bodies that differ from the
  state by more than the geometric tolerance (0.01 mm) to it (a warning
  names it): the item stays parametric, but its difference does not
  reach later items, where it keeps a later state (a split, a mirror, the
  stored design) from matching. A body within the tolerance, the item's
  own among them, stays as the replay made it (mitcad#285). Other
  approximations (fillets stored as splines) are carried on.
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
  states lost such a rounding. Any other item one of whose definitions the
  geometry kernel failed on (an operation that failed or built an invalid
  shape, a taper that failed) is tried on the stored state the same way
  (mitcad#325: an extrusion whose decoded cut came out invalid on the
  replay's bodies).
- The note of an item that falls back says why its decoded definition (the
  first one not guessed) failed, where it failed and the guesses after it
  only built other results or nothing that reached a body: "fillet failed;
  … ; the other definitions: its result is not in the file's history". The
  note of the last definition tried alone named a symptom of the guesses,
  not the item's cause (mitcad#325: the census counted kernel failures of
  the decoded definition as "not in the file's history").
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
- A combine can still select copies removed when a pattern was resized
  (mitcad#674). A tool reference is omitted only when its named body is
  absent from the file's own history state, has no producer and cannot
  resolve in the replay. A document GUID must be the local target's,
  proved by its producer's bodies. Ambiguous or unnamed recipes, body
  geometry evidence and unknown or external documents are kept strict.
  The remaining tools must all
  resolve and be nonempty; their selection counts as inferred. The same
  omission applies before consumed tools are retired, with the stored-id
  and geometry checks above unchanged. The decoded keep-tools flag stays
  authoritative: a cached tool in the ASM history is not evidence it was
  kept in the design.
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
  `core/import-replay/src/geometric.rs`): a wrong rounding (its surface 0.15 mm off along its
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
  farther than the geometric tolerance (`geometric::tolerance`, the
  option `geometric_tolerance`, 0.01 mm by default; mitcad#224). The result is taken when
  every face off on either side is a corner patch, at most 2 % of that
  side's new area (`geometric::CORNER`) and at most 0.3 mm off
  (`geometric::CORNER_DISTANCE`: the corners of roundings that are the
  file's lay 0.03–0.25 mm off, while corner faces 0.83 and 2.5 mm off were
  other corners), and together they hold at most
  5 % (`geometric::CORNERS`), none of them on a body the other side's item
  left as it was (each new face is measured against the other side's
  nearest body; where that body is one the other side did not change,
  a face off is a face the other side has no counterpart for, not a
  corner: mitcad#343), and when the faces account for the volume
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
  For a result that is not the state at all, `MITCAD_IMPORT_TRACE_MISS`
  (with `MITCAD_IMPORT_TRACE`) traces where it differs, for the first three
  misses of an item: each side's new faces that lie off the other side
  (area, distance, the point) and the faces before it each side changed.
- Lofts with end conditions or rails match within 5e-3, with a warning
  beyond 1e-3. MitCad builds their weights, angles and free ends by its
  own rule (`commands.md`, `loft`), while the file's surfaces may be
  fitted otherwise (between round sections the stored ones are not round,
  0.3–1.2 % below that rule; stored rails with end conditions differ too),
  so most of those fall back.

## Named inputs

Feature inputs in the streams name faces, edges and bodies as the body
blobs name theirs: every face and body of the ASM history carries
`generic_tag_attrib_def` names (a tag chosen by the operation that made
it and the operations that made and changed it), and an input's recipe
lists entities by such names (`core/f3d-decode/src/recipe.rs`,
`core/f3d-names/src/lib.rs`). The decoder rolls the `.smbh` blobs back to
just before the item (its `_f3d.result_no`) and finds them there
(`core/f3d-design/src/inputs.rs`):

- **Fillet and chamfer edges**: the edge between the faces named first
  and second; where several are, the one whose start and end touch the
  faces named third and fourth. Faces not in that state are looked for up
  to 24 states earlier (some recipes name a face as it was before later
  operations renamed it). The edge's middle point, length, ends and its
  own direction (`_f3d.direction`) go into its fingerprint; the import
  finds it by them and tries those edges first, then the history's
  guesses (*What the history settles*). What the learning dump settled
  (mitcad#96, [*Learning the undecoded inputs*](f3d-learning.md#learning-the-undecoded-inputs)):
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
  - each edge input stores the edge's curve as the input saw it (a line,
    arc, elliptical arc or spline, `_f3d.curves`): where the names fit
    several edges, the curve picks the one that is the stored edge
    (mitcad#306, [*Stored edge curves*](f3d-learning.md#stored-edge-curves-mitcad306) below), before the tail's order and
    the n-of-n rule;
  - the import finds a closed edge by its circle or ellipse and length
    (the file gives its middle half way along its parameter, opposite its
    seam, which the replay puts elsewhere; an ellipse's axes follow from
    its two ends of an axis, its direction at the middle and its length), an open one whose middle is up to
    0.05 mm off by its length (the parameter middle of a B-spline or
    ellipse edge), an open one whose middle is within 0.01 mm and whose
    length is up to 2 % off (a blend's approximated length), an open one by
    its two ends (within 1e-3 mm) and a length up to 2 % off where its
    middle is further off (a blend's or spline's parameter middle lies
    elsewhere on the replay's curve), and an edge the replay splits into
    pieces: two or more edges of one body on its segment or circle whose
    lengths add up to its and, on an open edge, follow each other from its
    start to its end; an open edge that is not a line (an elliptical arc,
    a spline) by pieces that follow each other by their ends from its
    start to its end, when their lengths add up to its and the file's
    middle lies within 2 % of its length of them (looked for first along a
    face every piece lies on, then along faces each piece shares with the
    one before, then over any edges, mitcad#343)
    (`refs::dressup_edge_match`). Two edges between the
    same ends of the same length are not told apart;
  - the geometric tolerance (mitcad#224, the import option
    `geometric_tolerance`, 0.01 mm by default): the replay's geometry
    differs from the file's by the kernels' noise (a chamfer built 1 µm
    otherwise moves the circle a later fillet names from z = 33.100 to
    33.099 mm). Where nothing is within the exact 1e-3 mm, the edge
    matcher looks again within the tolerance (the middle, the points on
    the file's circle, segment or ellipse, the ends, the sums of pieces),
    as do the lookups of faces through a point (`refs::resolve_face_where`,
    and `refs::face_resolved_uniquely` with the same tolerance), of edges
    by their middle (`refs::resolve_edge_where`) and of vertices
    (`refs::resolve_vertex`). The uniqueness rules stay: a decoded input
    found once within the tolerance is `read`; the item's report gives the
    largest such distance (`geometric_deviation`), the design's report the
    tolerance. Tolerances of 1e-3 mm or less turn the second look off;
  - the two edges of a sliver face (mitcad#224): a dressup that takes a
    face away exactly is built 1e-3 smaller (below), which leaves a strip
    that narrow where the file's two faces meet in one edge (two chamfers
    of 1 mm on the edges of a 2 mm band meet in a circle; the replay keeps
    a band 2 µm high). Where the edge found has a twin of its length on
    the file's curve within the geometric tolerance that shares a face
    with it (the strip), the file's edge is both (`refs::with_sliver_twin`;
    two such closed edges within 1e-3 are both, not ambiguous). A 0.2 mm
    fillet of the circle where such chamfers meet came `inferred` from the
    history's guess of both edges; it is `read` now;
  - the edge's body: the edge refers to its body's record like a body
    input does, so `_f3d.producer` and `_f3d.body_index` name the item that
    made the body. Bodies that touch have edges in the same place (the slot
    of one is the wedge of another); the import matches the edge among the
    bodies that item made first (`Importer::match_dressup_edge`), and among
    all bodies only where those hold none;
  - replay matches with midpoint-plus-length scores tied within
    `1e-9` mm in one body are refused (mitcad#106); coincident copies in
    different bodies of other items keep the existing first-body rule. Repeated aliases
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
    other reading stays a guess after it; a distance-and-angle chamfer's
    distance and angle lie on that face too where the file's flag
    (`isFlipped`) is off, on the other face where it is on (mitcad#343,
    round 16), whichever order the replay's edge name puts its faces in;
  - the edges found by their names are also tried one tangent chain at a
    time (a feature per chain), right after all at once. Two edges are
    one chain where an end of each lies within the import's geometric
    tolerance of the other's and their tangents are parallel there
    (mitcad#1343: an extrusion's end face whose loop joins the sketch's
    curves with the edges of the face it lies on has its edges' ends 1 to
    2 µm apart, which split a rounding of 44 edges round a ring into 9
    features that no longer built, and two more roundings after it).
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
  `core/f3d-design/src/build/producers.rs`). The import keeps what each
  item made (`producers.rs`): the bodies of the features made for it (a
  split's pieces from the bodies it split, which keep their ids; a
  combine's own bodies, the pieces a cut makes, without its target, which
  keeps the record of the item that made it: the combine's records are
  its new pieces only, so a removal that names the
  combine's first record takes the first new piece, not what is left of the
  target). A body
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
- **Inputs on other components' bodies** (mitcad#161): an extrusion's
  extent or profile face, a sweep's, pipe's or loft's path, rail and
  guide surfaces and a pattern's, mirror's or split's inputs that are in
  no state of the item's own history are looked for in the other
  components' histories at the item's point of the timeline: the state
  after the last item of that history's component before it, else up to
  24 states earlier. A history blob belongs to the component whose items
  name most of its operations (where that blob is also the one whose
  operations the component's items name most). Result numbers follow
  the timeline only within a component, and the names' operation numbers
  need not be states of the history they are in (a component's history
  may start later in the numbering than its names). Such an input gets
  `_f3d.component` (the component's object id): its geometry is in that
  component's coordinates, and the import looks for it among that
  component's bodies. Fillets', chamfers', threads' and shells' inputs
  are on the bodies they change and are not looked for elsewhere. Backup
  corpus: 21 items of 17 files get such inputs.
- **Faces of any surface type** (mitcad#161): besides planes and
  cylinders, a face gets its surface type (`Cone`, `Sphere`, `Torus`,
  else `NurbsSurface`, with the analytic data) and a point inside it: on
  a free-form surface (a B-spline, a thread flank's ruled surface) the
  point of its parameters farthest inside the face's edges taken into
  them, on one of the rulings a ruled surface was built from, where it is
  exact; on a cone, sphere or torus half way across the face as for a
  cylinder. A free-form face also gets `_f3d.face_points`, points spread
  over it. The import takes any free-form face of the replay for a
  free-form face of the file.
- **Helical edges** (mitcad#161): an edge on a helix the decoder knows by
  samples gets its middle, ends, direction and length from the cubic
  through the samples (the straight segments between them were up to
  0.1 mm off a thread's helix and 0.15 % short).
- Combines, splits, patterns, mirrors and holes (mitcad#67): their
  selections (list inputs of bodies, faces, edges, features and planes),
  as SCHEMA.md §5.3 lists; the import builds them first and then, where
  the history checks the item, tries what the history suggests without
  them, so an input decoded wrongly or not found never makes the item
  worse.

Sketch curves carry their kind (construction, centre line) in their
sketch-curve part; the decoder gives `isConstruction` and `isCenterLine`
where they are set, so construction curves no longer split profiles.
Splines (their NURBS data), ellipses and elliptical arcs are decoded
since mitcad#154 ([*The published format descriptions*](f3d-undecoded.md#the-published-format-descriptions-mitcad154)); splines come in
as fixed splines, the decoder not telling fitted from control-point
ones, and a spline out of its sketch's plane is refused.

Parameters of the 2026 writers (class version 8) have their own layout;
before it was decoded their features had no parameters, and the import
fell back for most fillets, chamfers, revolves and patterns of such files.
A dimension's parameter also says whether the dimension is driven
(`isDriving: false`), and an offset dimension's flags whether it measures
a diameter about a line (`SketchLinearDiameterDimension`): sketches with
such dimensions keep their points once the parameters are known.

A driven dimension whose parameter the file has (and the import created)
gets that parameter as its read-only reference (mitcad#250, `reference`
in commands.md's *Values*): the parameter becomes the sketch's, with the
value the sketch measures, and expressions that use it follow; no
reference of the dimension's own is made beside it. The .ipt import's
driven dimensions between projected geometry with a parameter in the
part's table, and the FreeCAD import's named reference constraints, come
in the same way. Where the model refuses it (a feature before the sketch
uses the parameter), the sketch is added without these references.

## Sketch patterns, offsets and polygons

The decoder reads what these constraints keep beyond their entity lists
(SCHEMA.md, *Constraint*): a circular pattern's quantity and total angle,
a rectangular pattern's quantities, distances and directions (the copies
are the entities after the originals), and an offset's parent and child
chains, distance and dimension, from the offset object that refers to the
constraint. `sketch/groups.rs` translates them into what MitCad's own
sketch tools make, each checked against the stored geometry:

- **Polygon**: a construction circle about the centre through the
  corners, and equal sides (`sketch.polygon`).
- **Circular and rectangular patterns**: a `patterns` record whose copies
  are the file's copies, matched to the originals instance by instance
  (a full turn spreads the instances, a partial angle ends at it; a
  rectangular distance is the spacing or the whole extent, whichever the
  copies show). MitCad's second direction is its first turned a quarter
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

## Threads, tapped holes and sheets

The decoder reads a thread's object (SCHEMA.md, *ThreadFeature*): its
type, size, designation, class, side, diameters and pitch, whether it is
modelled and over the whole face, and which end a partial one is
measured from; a tapped hole's thread is the hole's sub-item, read alike.
The translation (`ops.rs`, `commands.md` *From the dump IR*):

- **Threads**: a `thread` on the faces found through the history, with
  the size MitCad's table lists (ISO metric, the ISO 261 series up to
  M300, above M64 with fine pitches only, also the `M` sizes of other
  metric types; the forming (tapping) screw threads `ST3.5x1.3`, also
  stored without the `ST`; Unified; the parallel pipe threads `G <size>-<tpi>` as
  MitCad's `G <size>`). A class MitCad does not list for the face's side
  is left out (it only labels the thread), with a note. A partial thread
  starts at the end the file's cylinder axis points to (or the other):
  where MitCad's cylinder of the face runs the other way, the ends swap.
  Sizes MitCad lacks fall back. A cylindrical face the file keeps whole
  may be in pieces in the replay (a join across a bore splits it where
  the file's face goes on): the thread is tried first on the face with the
  pieces of the same cylinder end to end with it (a modelled thread one
  per piece, each piece's start angle measured), then on the face alone.
- **Sized faces**: a thread in the file, a cosmetic one too, also sizes
  its whole faces, an external one to the thread's major diameter, an
  internal one to its minor diameter (the reference models: a bolt of 10
  mm becomes 9.85 mm with an M10 thread). The translation first puts an
  `offset_face` of each face to that diameter before the `thread`. A
  chamfer at an end of the face (a cone about its axis from the face's end
  circle, narrowing away from a shaft or widening away from a bore) moves
  with it along the radius, as the bodies stored after the thread show it
  (mitcad#335): a 2 mm chamfer at a 16 mm bolt's free end, sized to
  15.822 mm, still starts where the face ends, and its circle on the end
  face shrinks by as much (the chamfer of the sized face). Each such cone
  gets an `offset_face` of the radial step times the cosine of its angle
  to the axis. (The sizing of the face alone left the cone, and the thread
  on the bolt took 5.1 mm³ (0.9 %) less than the file's, the cone up to
  0.063 mm off; the profile itself, measured across the stored thread
  along the axis at radii 0.01 mm apart, matched the stored flanks, crests
  and roots within 1e-4 mm.) Where
  that does not build (offsets of faces of stored bodies often do not),
  a tube of the material between the two radii along the face (two
  `cylinder`s, one combined out of the other) is cut from or joined to
  the body, and the thread goes on the tube's side; then the thread alone
  is tried. The offset is given 2 s (mitcad#68: offsetting a face of a
  large or stored body took 38 s where the tube then took 1.3 s) on the
  import's clock, which leaves the machine's load out ([time
  limits](f3d-performance.md#time-limit-stop-and-hangs), mitcad#874):
  the two routes do not give the same faces, so the route must not hang
  on the load. In a debug build a thread's offset took 0.9 s; beside
  four other imports on a busy machine it ran past 2 s of the wall clock
  (pinned to two processors: 2.65 s), the thread came in on the tube's
  faces, and a rounding along the thread later failed on them and ran out
  of its item's time. A debug build's checks scale the 2 s with the own
  start's time (`own_start_time_scale`), as it is a release build's
  time. For a
  cosmetic thread the volume the sizing adds ranks them against the
  history's change, so a thread whose state shows no sizing is tried
  alone first.
- **Tapped holes**: the hole's own history state has the bore of its
  diameter parameter; the file's thread widens it to the thread's minor
  diameter in an operation of its own, which the import leaves out (about
  1e-4 of the volume). The hole comes in with that bore and a `thread`
  feature on its walls (`hole<i>.wall`, from the hole's start, the wall's
  low end, for a partial thread), not with the hole's own `thread`, which
  would make the bore MitCad's basic minor diameter; then the minor
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
  by the planes of the face's ends: a cone or countersink beyond an end
  (a bolt's end chamfer, a countersink above a tapped hole) keeps its lip
  unthreaded, so the import's modelled threads, a tapped hole's too, have
  `ends` `flat` (mitcad#1302; the file's threads have no end chamfers).
  The reference model's M10x1.5 6g bolt
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
  where MitCad's angle 0 does. The groove's middle at each angle is the
  middle of the stretch between the crossings where the segment leaves
  and enters the material, and the angle is kept to 1e-4° (mitcad#161:
  the 48 samples a pitch and tenths of a degree placed a thread's flanks
  up to 0.015 mm off, too far for a sweep guided by the flank to find
  it; now within a micrometre). Region probes (*What the history
  settles*) classify their points the same way, by where a segment from
  each point along the sketch's normal out of the body crosses it (they
  took a minute for a sketch of 80 regions beside a stored thread of 720
  spline faces).
  Whitworth and ISO 7-1 threads fall back (MitCad models only 60°
  threads; NPT tapered, mitcad#733). A tapped hole with a modelled thread
  gets a modelled `thread` on its walls at the angle its state shows,
  measured on the hole made alone first (the walls do not exist before
  the hole, mitcad#733).
- **Sheets** (surface bodies): the history's matching compares solids
  only. The sheets of a state come in with the fallbacks that take it, and
  those of the last state at the end, as stored bodies (base features);
  joins, cuts and intersections without participants then name their
  component's solids, so that the sheets are left alone as the file's
  solid operations leave them. Without a history the file's sheets are not
  told from the feature tools and sketch faces its blobs also keep, and
  stay out.

## Sweeps, pipes and lofts

The decoder reads their inputs (SCHEMA.md, *SweepFeature*, *PipeFeature*,
*LoftFeature*; `core/f3d-design/src/build/sweeps.rs`):

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

Not decoded: a sweep's direction and rail scaling and participant
bodies. Edges of other components (through their occurrences) are
recognised and make the item fall back. Guide surfaces are found in the
history as other face inputs are (mitcad#161, *Named inputs*): a point
inside each, and more points spread over a free-form one; `sweeps.rs`
translates them into `guide_surfaces` (mitcad#157): the face through the
point among the bodies of the component the decoder found it in, and
every piece of it where the replay splits it (the faces through the
other points, and pieces of the same face that go on smoothly across an
edge). A path or guide surface on another component's body (the file's
assembly context: a nut's thread swept along the mating part's flank)
links that body (`body_links`, as a combine's `tool_links`), and the
sweep goes into its profile's component. A path edge the replay splits
(a thread's helix, one edge per turn) is found as the chain of edges
that go on smoothly from its start to its end with its length and
middle (`refs::resolve_edge_chain`). A guide rail given beside guide
surfaces is not used.

The corpus case (backup f262, item 9: a profile across a nut's thread
swept along the crest of the mating part's thread, guided by its flank)
now gets its inputs, links and definitions, and still falls back: the
profile is taller than the thread's pitch, so the sweep passes over
itself six times, and the geometry does not build it (the pipe shell
makes a self-intersecting solid; swept in pieces, the pieces' union
fails or loses pieces on the real thread, `sweep:
NCollection_IndexedDataMap::FindFromKey`, invalid wires). The
definitions take the item's time (about 50 s of the import).

`sweeps.rs` translates them with what the history settles: a profile's
region (a sweep's sets as for extrusions; a loft's one region per
section, probed ones first, at most twelve choices of all sections), a
second fraction 0 as none first (a profile at the path's end), both
directions where a partial extent or a rail makes them differ (with a
rail the file's direction flag reads the other way: the reference
models' rail sweep, flipped, is MitCad's sweep without `flip`), a rail's
point where it crosses the plane square to the path (`rail_point`
`plane`, mitcad#469), the
perpendicular orientation before the parallel one, a rail's scaling
before stretching and none, a solid circular pipe before a hollow one
and the square and triangular sections, and, without participants, a
join or cut on all bodies, then on those the history changed, then a
join as a new body. The reference models'
sweeps and lofts come in from their streams as from their dumps
(`test_f3d_import --models`). The corpus's sweep, a cut swept only
backwards from a profile at the path's end along a path with a slight
kink between its curves, comes in parametric since MitCad's sweep rounds
corners whose mitre does not build (mitcad#56).

## Path patterns (mitcad#468)

A path pattern item (`PathPatternFeature`) becomes MitCad's
`path_pattern` (`ops.rs`, `commands.md` *Patterns*):

| The file's | MitCad's |
|---|---|
| `inputEntities` (bodies, features or faces of one body, as a rectangular pattern's) | `objects` |
| `path`: one sketch curve | `{"sketch", "curve"}` |
| `path`: several lines and arcs of one sketch (mitcad#464) | `{"sketch", "curves"}`, joined end to end as a sweep's path |
| `path`: edges | not translated: MitCad's path pattern follows sketch curves only (the item falls back, the report names the reason) |
| `quantity`, `distance` | `quantity`, `distance` (arc length along the path) |
| `patternDistanceType` Spacing / Extent | `distance_type` `spacing` / `extent` |
| `startPoint`, `isFlipDirection`, `isOrientationAlongPath`, `isSymmetric` | `start`, `flip`, `along_path`, `symmetric` |
| the stream decoder's objects' centre (`_f3d_objects_center`, cm, mitcad#673) | `start_point` (mm): the original sits at the path's point nearest to it |
| the stream decoder's count of the bodies the pattern made (`_f3d_new_bodies`), 0 for a pattern of features | `combine`: the copies of a feature that made a new body join its body |
| `patternComputeOption`, `suppressedElementsIds` | `compute`, `suppressed_elements` |

The stream decoder (`core/f3d-design/src/build/patterns.rs`, `path`)
reads the item's class and its data object: the objects (the item's
first selection), the quantity (its integer input), the distance (the
`PathPatternDelta` parameter), the path (a list input of entity and edge
inputs, read as a sweep's path), the flags and the objects' centre, and
after the body records the tail a rectangular pattern also has: the
compute option, the distance type and the suppressed element numbers
(only an empty list is read). The data object holds, after its root
part:

`u8 | u32 | ref quantity holder | u32 | ref distance holder | 5 bytes |
f64[3] centre | u8 1 | u8 flip | u8 along path | u8 symmetric | ref path
input | f64 | ref distance holder | ref key point | 8 bytes | u32 n | n
body records | 8 bytes | u32 compute | u32 kind | i32 -1 | …`

The layout comes from two path patterns: the reference model's (a body
patterned four times along a straight sketch line, spacing, none of the
flags set; its decoded fields are its dump's,
`core/f3d/tests/design_models.rs`) and the one design of the corpora
with one (`b832916a`, mitcad#658, mitcad#673: 372 copies of a rib's
extrusion and its rounding along the closed outline of a bin, by extent,
identical, one body record). The reference model, without body records,
did not tell the 8 zero bytes before the count from those after the
records (16 bytes after the count were read before); the design's record
places the count after the first 8. The `f64[3]` is the patterned
objects' centre in cm (the reference model's cube's, (0.25, 0.25,
0.25)), not a point of the path: the design's ribs' centre lies 61 mm
above the outline, 3 mm along one of its straight sides. Of the four
bytes before the path input, the first is 1 in both; the design sets the
third and turns its copies along the path (without it the copies on the
other sides stand across the wall), the reference model none. The
second and the fourth are read as the flip and the symmetry, in the
order the settings are listed elsewhere; both are 0 in both files, so
neither file confirms them (along a closed path whose copies go round
it once, flipped and symmetric copies lie at the same places). The
`startPoint` the API gives (a fraction, 0 in the reference model's dump)
is not found among the bytes; the 8 zero bytes before the body-record
count may hold it.

The original's place on the path is where the path comes nearest the
objects' centre (`start_point`): the reference model's comes in as with
`start` 0 (its line starts under the cube), the design's ribs where the
centre lies by its side. Along a closed chain (the design's outline of
eight lines and arcs) the copies go round it again past its end; the
design's extent, the outline's length less 2 mm, spaces its 372 copies
evenly round it (`commands.md` *Patterns*).

The pattern makes no body (the item object's own `u32 0 | u32 n` body
records, as a mirror's `combine`, are none) while it copies a feature
that made a new body (the rib's extrusion, whose body the outline's
extrusion then joined): the file keeps the copies in that body. The
import sets `combine`, and MitCad's pattern joins the copies' bodies to
it in one operation (a compound of them, as they are apart). The copies
of the feature's own solid cannot carry the rounding patterned with it,
which that body holds at every copy, so only the definition that
repeats the rounding on the copies is tried there (the one without it
took 7–11 s of the item's 30 s).

The reference model comes in parametric and verified against its
history state both from its stream (every setting read since
mitcad#673; before, the defaults, inputs inferred) and from its dump
(every setting read): `test_f3d_import --models` (the ctest
`f3d.models_loft`) checks it with the reference lofts and sweeps, its
four bodies' total volume against the dump's final bodies. The design's
pattern is parametric with its inputs read (the `f3d.corpus_designs`
entry of `b832916a`): 13 s for its 372 copies, rounded on each copy
(2.7 s, finding each copy's edges on its own body from where the last
was found: scanning every body for every copy took 5 s) and joined to
the body (8.4 s, OCCT's boolean and the result's check), within 5 GB.

## What the history settles

The history picks what the stream decoder cannot tell, by trying
candidates against the next state:

- **Face profiles.** An extrusion without a decoded profile may extrude a
  planar face: faces whose area times the distance equals the state's
  volume change become a sketch on the face with its boundary projected
  (linked), and that region is extruded (outwards first; inwards first for
  a cut; on the side of the state's new body). These are tried before the
  last sketch before the item. A join that touches no body is also tried
  as a new body when the state has one more. The faces the decoder reads
  in the profile ([*Parts of items, face profiles and sketch heights*](f3d-components.md#parts-of-items-face-profiles-and-sketch-heights)
  below) come before these guesses.
- **Profiles.** The decoder reads the loops of the profiles an extrusion
  or revolution selected (`_f3d_profile_loops`, mitcad#96: the file's
  curve ids, which piece of each curve, outer loop or hole, the pieces a
  profile is split into). `core/import-replay/src/profiles.rs` maps them onto MitCad's
  regions: a listed profile is a face (its outer loops less its holes),
  and a region is selected when its inside point lies inside one by the
  even-odd rule against the listed curve pieces, with the file's curve
  geometry (a curve cut where the other curves of its loop cross it, the
  pieces numbered along its parameter; lines, arcs, circles, ellipses,
  elliptical arcs and splines, the last three met numerically); where a
  numerically cut curve's pieces contradict the piece count the file gives, or
  it selects nothing, its outer
  loops select the regions whose outer loops share most of their curves.
  Over both private corpora the history accepted the decoded regions for
  1509 of the 1532 settled extrusions that have the loops (most others
  are sets it cannot tell apart from them) and for all 97 settled
  revolutions. Those regions are tried first, before any ranking
  (`Candidate::first`), with the decoded extent; without a history they
  are the ones used. Checked against the item's own state, their
  definitions (the decoded extent, each choice of participants) are
  translated and tried on their own first, and the other sets below only
  when none of them gave the state (mitcad#181: probing the next states
  for those sets took 10–30 s an extrusion on bodies of thousands of
  faces, mostly in vain); they keep their order, and the others come in
  the order they would have had among all. If a later pattern or mirror
  may copy the extrusion, all definitions are translated together so an
  exact match still keeps alternatives for *Profiles a pattern or mirror
  needs*. This conservatively includes every later pattern and mirror:
  decoded body selections can also fall back to feature copies. The learning dump
  (`MITCAD_IMPORT_LEARN`) still gets all of them at once. Then, as before
  the loops were read:
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
  of all faces), by more than 1e-4 of their length (at least 1e-4 mm),
  and by that much more than off the faces of the file's state before the
  item (mitcad#440: an edge as far off that state was not taken away by
  the item; its bodies but those the replay and the next state both have
  as they are). An edge between faces so nearly tangent that the item's
  dressup there would lie within that distance of its middle counts where
  its middle moved off by half that depth at least (such edges are looked
  for on the faces of the edges found). An edge is assigned to an edge set
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
  and MitCad's fillet follows the chain: also where a third sharp edge
  meets the vertex, at which OCCT's own chain stops, mitcad#224,
  `commands.md`, fillet); the edges whose dressup the
  state has whatever their distance, with their continuations; then
  looser guesses (the edges at that distance, all, the largest group at
  one distance). A definition refused because it holds only a part of a
  tangent chain does not rule out the definitions with more edges. The next four states are looked at; for an item whose state
  is known, one at a time (`Guesses`): the edges found by their names
  first, then its own state's guesses, the next state's only when those
  gave no state, in the same order as all at once (finding the edges a
  state lost takes seconds on large bodies, and the item's own state or
  the names usually give it). Once the item's own state lost edges of the
  replay, the later states are not looked at (mitcad#181): their guesses
  add only edges the item's state did not lose, which no definition that
  gives that state rounds, or the same edges again (a fillet of a design at
  the time limit found the same 32 edges in three later states, 13 s).
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
    then with the strips narrower than the import's tolerance (0.01) at
    the selected edges' vertices collapsed into edges (mitcad#299), before
    the retries below (mitcad#107).
  - A rounding or bevel of a whole circle that runs over a neighbouring
    face (the rim of a hole near a side), which OCCT stops at, is built
    as a ring about the circle's axis when both faces are planes,
    cylinders or cones on it ([fillet](../../model-api/src/commands.md#fillet)).
- **Combine** target and tools, when the decoded ones are not found: the
  bodies the next state changed (a body with a stored body's measures
  counts as unchanged even where its faces are split otherwise), the
  decoded operation first. A join or intersection that consumes its tools
  gives the same solid whichever of its bodies is the target: once one
  gave bodies the history rejects, the same combine with another target
  is not built (mitcad#181). On bodies that carry an earlier item's
  approximation (*Matching rules*) only the decoded combine is tried; its
  guesses come in the try on the state before it exactly, where they are
  as good: on the near-coincident faces of such bodies each boolean took
  10–15 s, on the state's 0.2 s (two combines of a design took a minute
  each; where the bodies cannot be brought to that state, the guesses are
  tried on the bodies as they are after all).
- **Split bodies** the tool does not divide: the file's split leaves such
  a selected body as it is, MitCad's fails, so each body alone is tried
  after the decoded set (mitcad#96).
- **Replace face's replaced faces** (dumps do not record them): faces of a
  body the next state changed none of whose interior points
  (`Kernel::face_points`, up to nine) lie on the state's faces (1e-3 mm);
  neighbours that extend or shorten keep some. They are listed without the
  tangent chain, and the result is matched within 0.5 % (MitCad's own
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
  otherwise); without a history, with them first. Where the file's compute
  option is optimized (read from the pattern's data object, above) the
  definition with them copies the faces the features made instead of
  rounding at each copy (`commands.md`, *Optimized: copying the faces*):
  the pattern of a pin and its two roundings on 14 × 77 copies, which
  OCCT's rounding failed on at copy 133 to 146 (71 s to fail), is replayed
  in 7 s and matches the file's state to 2e-5 in volume (14 copies where
  the support under the pin's foot ends at an opening's edge differ by
  15 mm³, the file keeping a little more of them than the copies' own
  faces give; 3 faces). A circular
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

## Grid-unit designs: pastes, aligns, counts and start faces (mitcad#641)

A design driven by grid-cell counts (`b35176e6`: a base, a frame, a chute
of stacked rings and a catch, every size a count of 28 mm cells) came in
with 11 of its 27 items falling back: base features stood in for the
frame, the catch and the chute's rings, so a change of the grid parameters
reached little of the design. Each cause and its fix, all general:

- **Pastes and derives keep their bodies in a base feature.** A paste of
  bodies (`CopyPasteBodies`) and a derive from another design
  (`ExternalDerived`, the base class of `DerivedContext`) list a base
  feature among their sub-features, which holds the bodies they bring and
  carries the operation they made (the item's own result number is -1).
  The decoder gives such an item that operation (`_f3d.made_result_no`, as
  for a component made from bodies), and a body record whose producer is
  such a base feature names the item as its producer
  (`build/producers.rs`): the paste's source body, the combine's target
  and tools and the moved body of a later move resolved, where before none
  of them did ("its source bodies were not decoded").
- **A pattern of a paste** names the paste's base feature (an entity
  input whose target is the sub-feature): it stands for the paste, and a
  pattern or mirror of an item that only brings bodies (a paste, a base
  feature, a derive) copies the bodies it brought (`ops.rs`,
  `brings_bodies`).
- **Aligns** (`AlignFeature`, class `359F7902`) keep the bodies they move
  (a body input) and the motion they made (a move's rigid matrix): they
  come in as moves by that matrix. The geometry they align (key points,
  directions and frames of faces) is not decoded, so the move does not
  follow it; its inputs count as inferred.
- **A derive of another design** that the file does not hold is static in
  the source too: its base feature's state stands in, and the report says
  why.
- **Pattern counts** are read from their parameters (`countU`, `CountV`;
  every pattern of the corpora has them), not from the integer the item
  stores: a count such as `ChuteElements = round(ChuteExtent /
  ChuteElementHeight)` stays an expression.
- **A pattern along an origin axis stored against it**: the file stores a
  rectangular pattern's direction vector too; where it points against
  the origin axis it names (the chute's rings: Z with (0, 0, -1) and a
  negative distance, the copies going up), the distance is negated
  (`-(d177)`), so that the copies go the file's way and follow the
  parameter.
- **An extrusion from a face**: a start face's selection takes the place
  of the first byte after the extrusion's root part (class version 8), so
  every field after it was read 10 bytes early (the operation 0, the
  extents nonsense). It is read as `FromEntityStartDefinition` with the
  face and its `ProfileOffset`, which the face profiles' extrusions use
  too.
- **Sweep paths of a body's edges** whose ends meet within the model's
  chain tolerance (1e-6 of the path's size) but not within 1e-7 (the
  curves of edges of a face extruded from a sketch whose solve placed
  its curves within its tolerance) share one vertex
  (`geometry/src/spine.cpp`, `path_wire`); before, the sweep failed with
  "does not join the curve before it".

Recomputing with the grid parameters changed needed the model to follow
as well:

- **The sketch's origin point is fixed.** Every sketch's first point is
  its origin point (flag bytes 3 and 5 set, on exactly one point a sketch
  in the 40 designs looked at); it came in as a free point, so a sketch
  tied to the origin slid when a dimension changed (the base's corner
  left the origin by 13 mm). The decoder sets `isFixed`.
- **Offsets keep their side** (`sketch/offset.rs`, `seed`): an offset
  made by constraints starts each solve with each result where the offset
  of its source curve is now (a chain joined at coincident ends, as the
  import's offsets of offsets are, by where its ends meet); before, a
  linked projection that moved by more than the distance (28 mm against
  2 mm and 0.2 mm) left the stale offsets on the other side, where the
  solver kept them, and the slot's thin cut failed. A corner two results
  share takes where their offset lines cross, or where an offset line
  crosses an offset arc's circle, or two offset arcs' circles cross, and
  an arc's free ends take its source's angles, so an offset already in
  place (as imported) starts where the file has it: a first version put a
  line-arc corner in the middle of the two ends, and a bearing design's
  sketch (5ac5d0be, a non-tangent corner) then no longer solved with its
  projections; a second kept an arc's ends at its own stale angles, and
  the grid design's sketches then lost edges when a grid cell was added.
- **A face as the profile**: an extrusion of one face the decoder found in
  its profile, by a distance from the face, is MitCad's extrusion of that
  face (`profile_face`, mitcad#535), tried before a sketch on the face with
  its boundary projected. The face follows the model as the file's face
  profile does; the projected boundary of the chute's end face did not
  follow when a parameter moved its edges by a grid cell ("its curves no
  longer match the source's"), so the ring on it kept its old size.

- **Tangent corners of offsets are held where the curves touch**
  (mitcad#890, mitcad#891; `sketch/offset.rs`, `tangent_corners`). The
  frame's outlines are offsets of offsets of its projected edges (2 mm,
  0.2 mm and 3 mm in), each corner a line going on tangentially into an
  arc. A result line and arc held only by their own constraints (parallel
  at the distance, about the source's centre) fixed their shared corner to
  the second order: the solve left it 2e-5 mm along the line from where
  the line touches the arc (the square root of its 1.6e-11 mm rounding), so
  the extruded outlines turned by about 1.6e-6 rad at every corner. Three
  cells wider, that lost the slot's projections (their source no longer
  came back curve for curve), the two pieces of the frame's start face
  swapped their `#0` and `#1` (pieces are numbered by their centres rounded
  to a nanometre; both lie on the frame's middle in x, so the turns decided
  their order) and the chamfers lost their edges; on a chute four cells deep and high, the
  sweep cut along the turned corners (its path the extruded outline's
  edges, its profile's inner part lying on the chute's inner wall) left
  the material of that part as four rings of their own instead of
  removing it. The solve now holds each such corner
  where the curves touch, the sketch counts it as fixed (dof 44 → 8 for
  the frame's slot sketch), and the outline is exact (turns of 1e-15 rad).

With these, the design recomputes without an error with each of `BaseX`,
`DepthY` and `BaseZ` raised by a cell and with all three: the frame and
the catch one cell wider and deeper, the chute one cell taller with two
rings more (the test `f3d.corpus_parameters`, `tools/cli/f3d-parameters.cmake`;
the boxes are derived from the design's expressions, as no reference export
of the change exists yet). Also `BaseX` three cells more, on the design as
imported and on a square chute (`BaseZ` and `DepthY` 4), and back: the
three bodies only, no warning the import did not give, and the volumes
grow by the same amount for each cell (frame 9987.04 mm³, chute
6044.70 mm³, catch 16599.52 mm³ from `BaseX` 4 to 8).


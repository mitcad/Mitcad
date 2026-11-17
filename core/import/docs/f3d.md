# .f3d import

The `.f3d` import of `core/import`: entry points and modules, the crashes
it survives, its limits and the failure classes the corpus shows. The notes
on how it works are split by topic into the `f3d-*.md` documents listed in
the [README](../README.md#documents); a new section goes into the document
of its topic.

Entry points:

- `mitcad-cli import-f3d part.f3d [--save part.mitcad] [--report
  report.json] [--design <doc>] [--dump dump.json] [--no-verify]
  [--no-fallback] [--no-compare] [--time-limit S] [--hang-limit S]
  [--threads N] [--references DIR]... [--no-references] [--json]`;
  `--bodies-only` imports the bodies without the history;
- the document command `{"cmd": "import_f3d", "path": …}` and
  `Document::import_f3d_timeline(path, json)`, implemented in
  `core/ffi/src/f3d_import.rs`;
- Rust: `mitcad_import::import_design(&mut doc, &dump, &mut geometry,
  &options)`.

## Modules

| Module | |
|---|---|
| `params.rs` | User and model parameters with the file's names and expressions, in dependency order. Expressions that do not evaluate become values; names MitCad reserves get `_1`. A model parameter is adopted by the feature or sketch that made it (`Document::adopt_parameters`). |
| `sketch.rs` | Points, lines, circles, arcs (counter-clockwise), ellipses, splines (control points, weights, full knots), texts (style, flips, multi-line frame and alignment, text along or fitted to a path), constraints and dimensions by type. Ids are renumbered into MitCad's one id space (points first). Positions are the file's stored solution; the sketch is accepted when MitCad's solve keeps them (1e-4 mm), or keeps them within the import's geometric tolerance with the sketch's regions the file's ([*A sketch solved within the geometric tolerance*](f3d-rounds-4.md#a-sketch-solved-within-the-geometric-tolerance-mitcad396) below), else retried with the fewest constraints and dimensions that keep it off the file's geometry inactive, then with all of them inactive (`partial`; [*Inactive constraints and dimensions*](#inactive-constraints-and-dimensions-mitcad721) below). `sketch/groups.rs`: polygons, sketch patterns, offsets and concentric circle dimensions ([*Sketch patterns, offsets and polygons*](f3d-history.md#sketch-patterns-offsets-and-polygons) below). |
| `features.rs` | Sketch planes (origin planes; imported construction planes with the exact frame as `frame`; planar faces found geometrically; else a fixed construction plane), construction planes (from the entities the decoder reads, mitcad#178: offset, angle, tangent, mid plane, two edges, along a path; through three points, tangent at a point, offset through a point, mitcad#186; else offset from an origin plane or a face found by distance; each checked against the file's plane; else fixed), construction points (at a vertex, at the centre of a circle, where three planes meet) and axes (through two points, along an edge or sketch line, through a round face, where two planes meet) checked against where the file has them (mitcad#186; else fixed), extrude, revolve, fillet, chamfer. |
| `ops.rs` | Combine, mirror, circular and rectangular patterns, shell, offset faces, move, split body, hole, thread, replace face — from external dumps (SCHEMA.md §2), or from the history when the stream decoder gives no inputs (see below). |
| `sweeps.rs` | Sweeps, pipes and lofts ([*Sweeps, pipes and lofts*](f3d-history.md#sweeps-pipes-and-lofts) below). |
| `components.rs` | Components and occurrences. |
| `joints.rs` | Joints, as-built joints, joint origins, rigid groups and ground items ([*Joints*](f3d-components.md#joints) below). |
| `relationships.rs` | Assembly relationships as constraint sets, tangent relationships as tangent rows ([*Assembly relationships*](f3d-components.md#assembly-relationships) below). |
| `refs.rs` | Fingerprints resolved against the replay (bodies by volume and name, or by points on their edges, surface bodies by name or area, faces through `point_on_face`, edges by midpoint and length), planar faces, the edges a fillet consumed, the faces a replace face replaced, the final comparison. |
| `history.rs` | The bodies stored in the file (`StoredGeometry`: the ASM history states and the stored design) and matching of body sets (volume, area, centre of mass, face count). |
| `report.rs` | The import report (JSON and text). |
| `timeline_notes.rs` | The report's outcomes on the timeline (mitcad#170): import notes on fallbacks' base features, partial items and placeholders of items suppressed or failed in the file ([*The outcomes on the timeline*](f3d-fallbacks.md#the-outcomes-on-the-timeline) below). |
| `suppressed.rs` | Items suppressed in the file as suppressed features with their definitions (mitcad#180, [*Items suppressed in the file*](f3d-fallbacks.md#items-suppressed-in-the-file) below). |
| `references.rs` | Components of other documents filled from those documents' own imports, and the captured positions inside them (mitcad#196, [*Components of other documents*](f3d-components.md#components-of-other-documents-mitcad196) below). |
| `reference_folder.rs` | Where the files of the documents a design refers to are: the reference folder next to it, folders and files given, an `.f3z` package's designs (by its `DesignDescription.json`, mitcad#229), the GUIDs the files' manifests list (mitcad#196). |
| `appearances.rs` | The file's appearances and physical materials on the replayed bodies and faces ([*Appearances and materials*](f3d-appearances.md#appearances-and-materials) below, mitcad#228). |
| `scaffold.rs` | What the imports share (mitcad#583): the rewriting of the `$k` placeholders in a candidate's definitions (a placeholder is a whole `$k`, or `$k` followed by `:` or `.`; `$12` is the thirteenth feature, never the second followed by a 2), the core of a candidate (its definitions; the `.f3d` and FreeCAD candidates add their own fields) and the end of an import (notes on the timeline, one undo step; used by the FreeCAD and `.solidcraft` imports). |

## Crashes

OCCT can crash on unusual input (a fillet on imported geometry); the
importer turns crashes into errors (`mitcad::geometry::catch_occt_crashes`,
OCCT's signal handlers). On Linux that needs `OCC_CONVERT_SIGNALS`, which
OCCT's CMake config sets only for Debug and Release; the root
`CMakeLists.txt` sets it for every configuration. A crash that corrupts
memory cannot be caught: a fillet with a strip 1e-4 wide did, hence the
1e-3 of the dressup retry.

## Limits

- Inputs the streams name ([*Named inputs*](f3d-history.md#named-inputs) below) are found only where
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
  decoded: which profiles (only how many, see [*What the history settles*](f3d-history.md#what-the-history-settles)),
  a two-sided extent's object side (it falls back), which way a one-sided
  extent through all goes when the stream's flag is not set (the vector is
  the sketch's normal; the history settles it), and thin walls. Participant
  bodies come from their explicit input slot when that slot and its body
  references decode (mitcad#186); unresolved selections retain the older
  reading and history alternatives.
- Features that use bodies of two components at once fall back, and so
  do features across components whose components are placed more than
  once (*Across components*).
- Joints ([*Joints*](f3d-components.md#joints)): a joint comes in as a joint only where it holds
  where the file places the occurrences (*Occurrence placements*);
  occurrence moves are not replayed as MitCad's `move_occurrence`.
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
  MitCad builds them by its own rules (`commands.md`, `fillet`,
  `chamfer`), so they match the history within the looser conventions
  tolerance (a match is kept with a warning). Which face takes which
  offset of an asymmetric fillet is the set's flip (decoded from the
  streams since mitcad#154, [*The published format descriptions*](f3d-undecoded.md#the-published-format-descriptions-mitcad154)); the
  other reading is still tried after it. The stream decoder gives these
  options, a fillet's corners, each set's continuity, tangency weight and
  tangent chain, and a chamfer's corners since mitcad#154; before, it gave
  constant radii only and the import built the defaults.
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
- Sweeps, pipes and lofts ([*Sweeps, pipes and lofts*](f3d-history.md#sweeps-pipes-and-lofts)): sections that are
  faces or sketch curves, guide surfaces and inputs of other components
  are not translated; a loft whose sections' sketches have
  many regions is tried with twelve choices of them.
- Not translated: ribs and webs. A coil's plane, frame and choices (type,
  section, position, rotation, operation) are decoded since mitcad#25; a
  coil comes in where it rises along its plane's normal and starts on its
  x axis. No design of the corpus has a rib or a web.
- Surface extrusions, revolutions and sweeps (`isSolid` false; the stream
  decoder tells surface extrusions since mitcad#154) come in as the solid
  features with `surface` set (mitcad#455): a new surface body of the
  walls of the profile's regions, or, for an extrusion whose profile is
  not regions, of the curves it names, else of its sketch's curves when
  the sketch has no regions (a guess the history settles, as a thin
  extrusion's walls). A surface revolution or sweep of open curves is not
  translated (the decoder does not give their curves). Other surface
  features (patches, trims, boundaries, offsets and the like) are classes
  the stream decoder does not know: their sheets come in as stored bodies
  ([*Threads, tapped holes and sheets*](f3d-history.md#threads-tapped-holes-and-sheets)).
- Sketch patterns, offsets and polygons (*Sketch patterns, offsets and
  polygons*) are left out, and their copies and offset curves come in as
  plain curves, where MitCad has no counterpart: patterns with instances
  on both sides of the originals (symmetric), offsets of splines and
  ellipses (MitCad computes those curves itself, so the file's cannot be
  kept), offset curves without a source curve at the distance (e.g. extra
  corner arcs), and copies that are not where the pattern puts them
  (suppressed or moved instances). MitCad's pattern counts are numbers and
  its directions vectors: the file's quantity parameters stay unused, and
  a direction along a sketch line keeps its imported direction. External
  dumps do not record these constraints' entities, so from them they are
  left out.

## Known failure classes

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
- items without a translation ([*Items without a translation*](f3d-components.md#items-without-a-translation)), items of
  unknown classes (`?…`), construction axes, and the untranslated types
  under *Limits*, which are skipped or fall back.


## Inactive constraints and dimensions (mitcad#721)

A sketch is added at three levels, each through the variants of its
reference curves (locked, free with the points that hold the own curves
pinned, free; with and without the derived offsets and the projections):

1. every constraint and dimension the file has, active; where the file's
   own positions break some of them (a sketch not solved again after the
   geometry it is linked to changed: the solve's `stored_violations`),
   again with those inactive;
2. the fewest that keep the sketch off the file's geometry inactive,
   everything else active: the sketch is first added with all of them
   inactive (it must then keep the file's positions), and the inactive ones
   its solution does not meet (the solve's `unmet_inactive`) are the
   candidates; `sketch::inactive::fewest` starts from them (from all, when
   the sketch does not solve with only those inactive) and gives back to
   the solve every group the sketch still solves to the file's geometry
   with, halving the groups it does not (a bisection: one bad constraint
   among n takes about 2 log2 n solves, at most 64 after the first). It
   never asks for the sketch with none of them inactive, which level 1
   showed fails (mitcad#723). Only the first variant whose sketch solves
   with all of them inactive is searched; the others cost one add each;
3. all of them inactive, without the patterns and offsets.

Each attempt is checked as the sketch itself: the file's positions within
1e-4 mm, or within the geometric tolerance with the file's regions (the
active constraints'). Inactive ones keep their parameters and carry
`inactive_reason` "not met in the imported file"; the sketch view draws
them pink and offers Activate (commands.md, *Inactive constraints and
dimensions*). Patterns and offsets the file's positions do not keep cannot
be inactive: they are left out. The note names each inactive one (`1
constraint inactive, not met in the file: coincident k7 (p3 on c5)`), and
the item is `partial`. The FreeCAD import does the same with FreeCAD's
solution (`freecad/sketches.rs`). Both readers give each dimension once,
as the file has it (driving with its value, or driven). Tests:
`sketch::inactive::tests` (also: the search never asks for none or all
inactive),
`tests::constraint_tests` (a parallel constraint and a second width among
good ones; a dimension 4 µm off and a coincidence 50 µm off a linked
spline come in inactive), `freecad::tests` (a parallel constraint).

## Units and up direction (mitcad#1937)

**Length unit.** The design segment keeps six unit systems
(`UnitSystems`: `CmMKS`, `MmMKS`, `MMKS`, `InchImperial`, `Imperial`,
`Custom`), and the `AssetSettings` record (the segment's first object)
refers to the one the design uses. That system's `ModelingLength` entry
(property `modelingLengthName`) names the unit as text: `millimeter`,
`centimeter`, `meter`, `inch` or `foot`. The decoder writes it as
`document.default_length_units` (`mm`, `cm`, `m`, `in`, `ft`), and the
replay sets the design's unit to it before the parameters
(`Importer::set_units`). Model coordinates are centimetres whichever the
unit, so nothing else changes. Until mitcad#1937 the stream decoder did
not read it and every design came in in millimetres.

**Up direction.** A design is modelled with Z up or with Y up, a choice
made when the design was started. The file has no separate flag for it;
its standard named views carry it. Each named view record (UTF-16 name
`TOP`, `FRONT`, `RIGHT`, `HOME`) refers to a camera record that ends with
the eye, a `u32`, the target and the up vector (`f64` each, cm). A Z-up
design's Front view looks along +Y with +Z up (MitCad's own Front); a
Y-up design's looks along -Z with +Y up, and in MitCad's views it lay on
its side. The decoder writes the views as `document.named_views`.

**The choice: turn the design, not the views.** MitCad has no per-design
up direction (the view cube, the standard views and named views are Z up
everywhere), so after the replay the import turns a design whose Front
view is not MitCad's by the quarter turns that take it there (Y up: 90
degrees about X), with Reorient Design (`mitcad-import-replay`'s
`upright`). That is a rewrite of the definitions, not a move: the design
stays parametric, sketches go onto the turned origin planes with their
frames, and coordinates, exports and views all agree on Z up. It is part
of the import's undo step and is reported as `upright` (commands.md,
*The design's length unit and up direction*). Where Reorient Design
refuses (a feature that would fail turned, a body that would not be where
the turn takes it) the design stays as modelled, with a warning. The
documents a design refers to are not turned: the design's placements of
their components are in their own coordinates (`Options::upright` is set
for the design opened only). A Front view off the model axes (set by the
user at an angle) leaves the design as it is.

Tests: `build::settings::tests` (`mitcad-f3d-design`: the unit system the
settings refer to, the named views' cameras), `tests::upright_tests`
(`mitcad-import`, mock kernel: a Y-up block stands on Z up in inches, a
Z-up one and a referred document are not turned), Reorient Design's own
in `reorient_tests` (a base feature without bodies, a Move Face), and the
ctest `cli.f3d_reported_house` (a reported Y-up design in inches under
`MITCAD_MITCAD_REPORTED`, skipped without it: its unit, the turn, and its
occurrences placed as the file places them, turned).

# FreeCAD import (.FCStd)

Part of [mitcad-import-fcstd](../README.md); the other imports'
documents are indexed in [core/import](../../import/README.md#documents).

A FreeCAD document is a zip with `Document.xml` (every object and its
properties), `GuiDocument.xml` (visibility, colours) and the OCCT B-rep of
every shape-bearing object's result as FreeCAD last computed it, in its
container's coordinates. `core/freecad` reads the archive and XML
(`roxmltree`, DTDs refused) into typed objects without a kernel.
Enumerations are stored as indices, so it carries each FreeCAD version's
value lists (`data/enums.json`, read from FreeCAD by
`tools/freecad-export/enums.py`).

Hostile files meet bounds well above anything FreeCAD writes (the test
corpora's largest values in parentheses), so that they end in an error or
a warning, never in a stack overflow, a panic or an unbounded allocation
(mitcad#932): XML elements nested deeper than 256 levels are left out with
a warning (9), a value kept as XML keeps 64 levels (4), an expression may
nest 256 levels, counting parentheses, signs and operators of a chain (two
parentheses, four operators), a sketch B-spline needs ascending finite
knots, more poles than its degree (a periodic one's poles wrap around),
a degree up to 25 (3) and
multiplicities adding up to 4096 at most (10). A folder listing an object
twice places it once, and an import places at most a million objects and
link elements (under a thousand objects).

Entry points: `mitcad-cli import-fcstd <file.FCStd> [--save out.mitcad]
[--report report.json] [--bodies-only] [--reference dump.json] [--set
name=expression]... [--json]`; the `import_fcstd` command, run with OCCT by
`core/ffi/src/fcstd_import.rs`. The application runs it in the import
process, like the `.f3d` import.

## Stage 1: structure and stored bodies

`src/lib.rs`: App::Part and Assembly → components; Bodies and the
Part workbench's leaf objects → base features of the stored shapes (one
per component); App::Link → occurrences (arrays, links of links, links to
other files read next to it); visibility, colours and the document's unit
system. Placements come from the file, not from attachments. Datums and
joints are left out; features inside Bodies and consumed operands are part
of their results. A binder's `Context` (the Body or Assembly it is in)
consumes nothing (mitcad#952): only its `Support` is what it binds, so a
Body with binders stays a body. The report says why for each object.
`--bodies-only` stops here.

## Colours as appearances (mitcad#793)

`src/appearances.rs`, after the history (or after stage 1 with
`--bodies-only`), on every body the import made, stored or rebuilt:

- `GuiDocument.xml` gives each object's colours: 0.21 `ShapeColor`,
  `Transparency` (percent) and `DiffuseColor` (one per face, its alpha the
  face's transparency); 1.0 `ShapeAppearance` (one material for the shape
  or one per face, each with its transparency).
- Each colour (its RGB bytes and opacity) is one document appearance,
  `FreeCAD colour <r> <g> <b>` (`, <n> % opaque` when not opaque), made
  once for every body and face that has it.
- A body gets its object's colour: the shape's, or with colours per face
  the one most faces have. FreeCAD's default grey (0.21's 204 204 204,
  1.0's default appearance 204 204 230) makes none: such a body keeps
  MitCad's look.
- A face of another colour than its body's gets that colour's appearance
  (also FreeCAD's grey on a body of another colour, so that it looks as in
  FreeCAD). FreeCAD numbers faces in its stored shape's order: on a body
  of the stored shape that is the face at the same index; on a body the
  history rebuilt (also one whose last feature fell back to a base
  feature) it is the face of the same geometry, found as references are
  (`Importer::planned_faces`). A face FreeCAD's `Refine` merged from
  several of MitCad's is not found; the warning counts such faces per
  body.
- The report gives `appearances` (made) and per body its `color`,
  `appearance` and `face_appearances`. Physical materials are not
  imported.

## Stage 2: sketches

`core/freecad/src/sketch.rs` reads geometry, constraints and external
geometry; `src/sketch.rs` translates, `sketches.rs` places and
checks.

- FreeCAD's curves own their points and Coincident constraints join them;
  joined points become one MitCad point (union-find), as in the `.f3d`
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
- External geometry on another sketch (mitcad#952) becomes a linked
  projection of that sketch's curve or point (`{"sketch": …, "curve": …}`
  or `{"sketch": …, "point": …}`, mitcad#904), when the import made that
  sketch earlier on the timeline: an edge is the sketch's curve along the
  edge of its stored shape (a line with the same ends, a circle or arc of
  the same centre, radius and plane; also 1.1's defining external
  geometry, which is part of the sketch's shape), a vertex the sketch's
  point there. An edge, vertex or face of a binder
  (`PartDesign::SubShapeBinder`, `ShapeBinder`) is found the same way in
  the sketches its `Support` names (also through a Body's path, `Body004`
  with `Sketch072.`); a face is the sketch's curves around it, one
  projection for each of FreeCAD's curves. A binder of a feature's result
  is that feature's element, found by its geometry.
- A body's element of a result of several solids is looked for in each of
  their bodies. Geometry of another component (a Body with a component of
  its own, at a placement or shown by links) is projected through the
  occurrences where the document places the two components first (the
  record's `link`, mitcad#100), so it follows when either moves.
- The projection follows its source when it changes. Other external
  geometry is fixed in place from the stored shapes; the report says why
  (`fixed in place (not linked: <reason>)`): an edge FreeCAD merged from
  several of Mitcad's, a sketch made later or not at all, a source whose
  projection does not match FreeCAD's copy, another document.
- A sketch is accepted when MitCad's solution keeps FreeCAD's points
  within 1e-4 mm (else with the fewest constraints and dimensions that keep
  it off inactive, else with all of them inactive: `partial`, mitcad#721,
  `crate::sketch::inactive` as in the .f3d import; the notes name them)
  and is compared with its stored shape. What MitCad cannot express
  (tangency with ellipses, Snell's law, distances to ellipses and
  splines, intermediate features' elements) is listed, and the sketch is
  `partial`.
- Distances to circles and arcs (FreeCAD 1.0's, mitcad#955), read from
  the stored geometry as the gap outside the circle or the one inside it,
  whichever is the value: a line outside a circle is MitCad's
  `tangent_distance`; the others are a `concentric_distance` from the
  circle to a construction circle about its centre: through the point
  (a point's distance), touching the line across it, touching the other
  circle (two circles outside each other: about the first; one inside the
  other: about the inner one, touching the outer one from inside; about
  the same centre: the two circles' own concentric distance). A distance
  neither gap gives is left out with both readings. A right angle between
  a line and external geometry's circle holds the line on the circle's
  projected centre.

## Stage 3: history

`history.rs` drives it, `features.rs` translates PartDesign features,
`part.rs` Part workbench objects, `primitives.rs` objects without a MitCad
feature of their own, `elements.rs` references.

- Every PartDesign Body and every Part workbench result stage 1 made a
  body of (extrusion of a sketch or of another object's faces, revolution
  of a sketch, face of sketches, primitive, operation) is rebuilt as
  MitCad features, with the Bodies its booleans use and the
  operands of its tree. All objects, sketches included, come in
  dependency order (ties in document order).
- The file keeps every feature's own result, so each replayed feature is
  checked against FreeCAD's stored shape: volume, area and centre of mass
  of the Body's MitCad bodies within 1e-5 relative (mitcad#539), and the number of
  solids. Where FreeCAD's conventions are uncertain a few readings are
  tried (directions, taper signs, a chamfer's first face, which sketch
  regions are material).
- A feature that cannot be translated, fails, or does not give FreeCAD's
  shape becomes a base feature of its stored shape that replaces the
  Body's body (`replaces`, keeping its id); later features continue on
  it. One whose stored shape equals its base's is skipped.
- References (`Pad.Face6`, `Edge12`) are taken from the referenced
  object's stored shape and found by geometry in the MitCad body holding
  its result — first as it is now, else as it was right after the object
  (the name then carries through later features). Whole circles, and
  the straight seams of a face of revolution (a hole's wall), are the
  same wherever each side started them (mitcad#835). An edge FreeCAD merged
  is all of MitCad's pieces along it, each named by the two faces it lies
  between (`E{a|b}` without a number, every edge between them) where all
  the edges between those faces are pieces of it (mitcad#793): a sketch
  edit upstream that changes the pieces still finds the edge.
- Fillets and chamfers with `UseAllEdges` are a set of every edge of the
  body (`all_edges`, mitcad#793), taken when the feature is computed, not
  the edges the body had at the import: edges a sketch edit adds are
  rounded too, and edges it takes away are no error.
- A Body's tip before its last feature suppresses the features after it.
- Bodies are named, shown and coloured as in FreeCAD (colours: above). A
  replayed result that FreeCAD keeps as one
  shape of several solids (a Part extrusion of a face of two sketches) is
  all the MitCad bodies made of it (mitcad#541): the first named after
  the object, the others `<label> (2)`, …, measured together against the
  object's place.
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
- Scaled transformations anywhere in a MultiTransform, and
  MultiTransforms of the whole body (mitcad#539): a Scaled transformation
  first (or alone, or a Scaled feature of its own) is a `scale_pattern` of
  the originals and the transformations after it patterns of it; after
  others it is the `scale` of the pattern or mirror before it, with
  `scale_groups` when its occurrences are fewer than that one's elements
  (FreeCAD scales the copies before it in that many runs, each copy about
  its own centre: the model's scale does too, through patterns of
  patterns). Two linear patterns and a Scaled one over all their copies
  are one pattern of two directions with `scale`. More runs than the
  pattern or mirror before it has elements (mitcad#542): the runs split
  the copies of the patterns before it, so each of those carries the same
  scale counted over all the copies (`scale_copies`), and the model
  scales each copy relative to the inner copy it is made of. Several
  Scaled transformations: their scales on the patterns before each
  (`scales` where one scales already), the factors multiplying.
- What FreeCAD 1.0 and 1.1 store (mitcad#542, measured on
  `pd_scaled_general`, `pd_scaled_inner` and `pd_scaled_several`): once a
  Scaled transformation follows others, every copy but the original whose
  factor is 1 is left out of the result, also the later transformations'
  copies of it (0.21 keeps them). The import's first reading for those
  versions gives each pattern the `skipped_copies` of its own such copies;
  the patterns after it still copy them (`dropped_copies` in
  `features.rs`).
- A MultiTransform of the
  whole body (1.0's TransformMode "Transform body", 1.1's "Whole shape")
  is a chain: the first transformation a pattern or mirror of the body,
  each next one of the one before (mitcad#547: the model's pattern of a
  pattern of bodies copies the body as it was before them at every
  product of the elements), all with `combine`, which is FreeCAD's
  product of the transformations. A Scaled transformation of the whole
  body (mitcad#542, mitcad#547): FreeCAD scales about the Body's origin
  carried with each copy (`pd_scaled_body`: the scaled body's box starts
  at the origin, the copy's at the copy's origin); it is read as in
  features mode (a `scale_pattern` of the body first; scales, runs over
  the copies of the patterns before, the copies 1.0 and 1.1 leave out as
  `skipped_copies` of the body's copies), with `scale_center` the Body's
  origin (`pd_scaled_body_more`: after two transformations each copy at
  its own factor, the bosses' centres as FreeCAD stores them).
  Reference models `pd_multitransform_body`, `pd_scaled_body` and
  `pd_scaled_body_more` (1.0 and 1.1), `pd_scaled_general`,
  `pd_scaled_inner` and `pd_scaled_several`.
- Conical and growing helices are helices with a `growth` per turn
  (`Pitch * tan(Angle)`, or Growth) and `"construction": "freecad"`
  (mitcad#83; helices made in MitCad use MitCad's own construction),
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
- A hole is drilled at its sketch's circle and arc centres, or, without
  any, at its points: each drawn point once, in the sketch's order, and no
  construction point (FreeCAD's sketch shape leaves those out). Coincident
  points are one MitCad point and one hole; listing it twice made the
  hole's tools overlap and the cut gave no solid (mitcad#835, reference
  model `pd_hole_points`).
- Datum planes: attached flat with only a normal offset → `offset`
  plane; attached to an origin plane and turned a quarter (or not), with
  the offset along its normal → `offset` from the parallel origin plane;
  turned about the support's x or y axis → `angle` plane; otherwise
  `fixed`.
- Sketches and datum planes whose attachment turn an expression drives
  about a line in their support's plane (its x or y axis or any line
  between) become `angle` planes about that line.
- Part extrusions (`part.rs`, mitcad#539) follow FreeCAD's lengths: the
  direction's length forward when LengthFwd and LengthRev are both zero,
  Symmetric the forward length in all (LengthRev unused), a reverse
  length alone one side against the direction.
  - Of a sketch into faces (Solid off): FreeCAD's prism of the sketch's
    wires, walls without caps, is a surface `extrude` of the sketch's
    curves (not construction; 1.1's defining external geometry too); off
    the normal along FreeCAD's direction (`direction`); tapered, a
    surface `extrude` of the sketch's regions with the tapers (curves
    take none).
  - Of a sketch along a direction off its normal (mitcad#541): an
    `extrude` with that `direction`, its lengths along it (parametric)
    and its tapers growing the profile in its plane, as FreeCAD's
    `makeDraft` grows the wires (no longer a sweep along a fixed line).
  - Of another object's faces (a Part::Face, a mirrored face, a scripted
    object's face that fell back; Solid on or off, FreeCAD extrudes a
    face into a solid either way): an `extrude` of each face of its
    bodies (`profile_face`, mitcad#535) with the tapers, along the face's
    normal (`Dir` tells along or against it; the other way is the second
    reading) or, off it, along `Dir` (`direction`, mitcad#541; a curved
    face only so, untapered), then a base feature without bodies that
    removes the faces' bodies, which the extrusion consumes (as a
    mirroring does). A base of several bodies (a Part::Face of several
    sketches) extrudes the faces of all of them; where another object
    uses the base too, the extrusion works on the copy of its first body
    and its others, which stay for that object.
  - Of an object's edges (mitcad#541; a Part Circle, RegularPolygon,
    Polygon or Line, a Draft wire: an object without faces that only
    Part extrusions use, `included`): a fixed `construction_plane` (the object's placement
    where the edges lie in it, else the plane through its straight edge
    and `Dir`) and a `sketch` of the edges as fixed geometry on it, then
    the extrusion of that sketch (Solid on: its material regions, those
    inside an even number of its loops, any number of them, mitcad#547:
    `"$<i>:profiles"` in a candidate is the regions of the sketch added
    i-th; off: a surface extrusion of its curves). `partial`: the
    object's own sizes (a circle's radius) are not carried over.
  - Edges in no one plane (mitcad#547; Solid off, along `Dir`): grouped by
    planes they lie in (a line in the plane through it facing the
    direction, a conic or a planar spline in its own), each group a fixed
    sketch and a surface `extrude` of its curves along the `direction`;
    together FreeCAD's prism of the wire, as several surface bodies.
  - A helix (mitcad#547; a Part::Helix from its Pitch, Height, Radius and
    LocalCoord, or one helix edge; Solid off, along `Dir`): a surface
    `sweep` of a fixed line from the helix's start along the direction
    (back and forward by the lengths) along a fixed `helix` path,
    `parallel`: the line moved along the helix without turning sweeps the
    helix's prism (`partial`: fixed sizes).
  - A conical Part::Helix, a helix among other edges, a spline in no
    plane (mitcad#548; Solid off, along `Dir`): with the walls of the
    edges in planes as above, each curve in no plane swept the same way
    by a line along a fixed path of its own: a helix edge's `helix`, a
    spline's (the curve the file stores) a `fixed_curves` path; a conical
    Part::Helix is its stored curves as a `fixed_curves` path (the one
    Part helix reader, `fixed_paths.rs`, shared with Part sweeps). A
    spline that does not start at its first pole (not clamped) falls
    back.
  - FreeCAD fails these itself, and so they fall back with that reason
    (no parametric counterpart when FreeCAD marks them invalid): a curved
    face, a helix or edges in no one plane along the base's normal
    (FreeCAD finds the normal of a plane only); tapered ones (it tapers by
    offsetting planar wires); Solid on of a helix or of edges in no one
    plane (it makes faces of planar closed wires only).
- A Part::Face of sketches (face makers Bullseye, Cheese, Extrusion; the
  simple one, mitcad#547, where no wire lies in another, which then makes
  the same faces) is a `patch` of each sketch's regions inside an even
  number of wires. The simple face maker's faces of nested wires
  (mitcad#548), overlapping, one filling each wire: a region without
  holes is its wire's face; a region with holes is patched, the opening
  of its outer wire patched (`patch` `boundary.opening`: the loop's
  plane) and the region's patch removed by a base feature without bodies
  (which opening is the outer one is the kernel's geometric order: the
  first, else the last reading).

## Stage 4: parameters and expressions

`params.rs`; `core/freecad/src/expression.rs` parses FreeCAD expressions
into a tree whose references the import resolves; `spreadsheet.rs` reads
cells.

- Before the timeline, user parameters are made from spreadsheets'
  aliased cells (and cells expressions name by address), VarSets'
  properties, sketches' named constraints, and feature properties other
  expressions refer to — in dependency order, with FreeCAD's names where
  MitCad accepts them.
- Every expression bound to a value the import carries over (a feature's
  length or count, a sketch's dimension, an attachment offset along a
  normal as an offset construction plane, a sketch or datum plane turned
  about its support's x or y axis as an `angle` construction plane, a
  side offset that moves a quarter-turned plane along its normal as an
  `offset` from the parallel origin plane) becomes that value's MitCad
  expression.
- The translation maps references, units, constants, the conditional and
  functions (written out where MitCad has no equivalent) and keeps
  FreeCAD's precedence. An integer property's expression is rounded as
  FreeCAD rounds it (`round(Count / 2)`).
- Each translated expression is evaluated and compared with FreeCAD's
  stored value; one that differs or does not translate keeps FreeCAD's
  value, with the reason in the report. Not translatable: a shape's
  measure (e.g. bounding box), side offsets that only move a sketch or
  plane within itself (kept as numbers), turns about other axes or with
  shifts (a fixed plane). Cells of quantities
  MitCad lacks (e.g. kilograms) are left out.
- Expressions on objects kept as stored shapes (e.g. BIM objects) stay
  unused.

## Face profiles, chosen sketch edges, Part fillets of two radii (mitcad#539)

`profiles.rs`, `part.rs`; the reference models `pd_profile_faces`,
`pd_profile_edges` and `part_fillet_variable`.

- A Pad's, Pocket's, Revolution's or Groove's `Profile` that is a planar
  face of a body (`(Pad, Face6)`) is the extrusion's or revolution's
  `profile_face`: the face found by geometry in the body holding the
  object's result, as other references are, swept along its outward
  normal (FreeCAD's profile normal is the face's normal as it is
  oriented) or turned about the axis. Extrusions go by distances only
  (Length, TwoLengths, Midplane), as MitCad extrudes a body face; a taper,
  an extent up to something or a custom direction off the face's normal
  is not translated. Without `AllowMultiFace` FreeCAD takes only the first
  element the link names; with it, several faces are not translated.
  Revolutions of a body face are MitCad's `revolve` with `profile_face`,
  made for this (`Kernel::revolve_face`).
- A sketch `Profile` with chosen edges (`(Sketch, [Edge1, Edge5, …])`)
  counts only with 1.0's `AllowMultiFace`: FreeCAD then makes faces of the
  wires those edges form. The import takes the regions of the whole
  sketch that lie inside the regions the chosen curves bound on their own
  (in FreeCAD's material reading: inside an even number of their wires,
  1.1; the outer ones with their holes, 1.0). Without `AllowMultiFace`
  (and in 0.21) FreeCAD ignores the chosen edges and takes the whole
  sketch; the import does too (before, any chosen edge was a fallback).
- A link to a whole object is no sub-element in older files
  (`count="0"`) and one empty sub-element in FreeCAD 1.x
  (`count="1"`, `<Sub value=""/>`, as in every Pad's and Pocket's
  `Profile` of a 1.2 document); the reader takes both (and an empty `sub`
  of a list entry) as the whole object, for every link (mitcad#708).
- A `Part::Fillet` edge of two radii (`(edge, R1, R2)`) is FreeCAD's
  `BRepFilletAPI_MakeFillet::Add(R1, R2, E)`: R1 at the start of the
  edge's curve, R2 at its end. The import gives each such edge a set of
  its own with a `variable` radius from R1 at `start_vertex`, the MitCad
  vertex at the first point of the stored edge's curve (MitCad's single
  edge rounding is the same OCCT call); radii within OCCT's confusion
  (1e-7 mm) are one constant radius, their mean. A closed edge (no start
  vertex) is tried with the radii either way. An edge whose rounding runs
  on into tangent edges FreeCAD did not list is rounded along the whole
  chain by MitCad and may then differ from FreeCAD's (the check decides).

## Binders, extents across components, turns over a full turn (mitcad#955)

- A Pad's, Pocket's, Revolution's, Groove's or Hole's `Profile` that is a
  binder (`SubShapeBinder`, `ShapeBinder`) of one whole sketch, not moved
  or offset: in the sketch's own component that sketch; in another
  component (a binder in one Body of a sketch in another) a sketch of the
  binder's, hidden, on the bound sketch's plane seen through the
  occurrences where the document places the two components first, each of
  the bound sketch's profile curves and points a linked projection of its
  curve or point (`{"sketch": …, "curve": …}` with the `link`), so the
  profile follows the sketch. A hole drills at the projected circles'
  centres, else at the projected points, as it does on the sketch. A hole
  also drills at the circles and arcs of 1.1's defining external geometry
  (part of the sketch's shape, whose circles FreeCAD drills at; before,
  a reported model's hole missed one of its five places: off by 1.2e-2).
- A face of a binder of a feature's result named by an extent (`UpToFace`
  = (binder, `Face44`)) is that feature's face found by its geometry, as
  external geometry finds it (mitcad#952).
- An extent up to a face or a body of another component (a Body at a
  placement): the face is read in its body's component through the
  occurrences where the document places the two components first (the
  definition's `object_links`, the model's mitcad#762).
- A symmetric extent up to a face (1.1's SideType Symmetric, Midplane):
  FreeCAD extrudes side one up to the face (moved by the offset) and
  mirrors that side across the sketch plane. Up to a planar face (or a
  plane) parallel to the sketch it is `two_sides`: up to the face, and as
  far the other way (a distance: it does not follow the face). Up to
  other objects it falls back with that reason (MitCad's extrusions have
  no mirrored side).
- A Revolution of TwoAngles adding up to more than a full turn: FreeCAD
  turns the profile back by Angle2 and revolves by the sum, which OCCT's
  revolution takes less a whole turn (100° and 280°: 20°). The turn runs
  from 360° less Angle2 to Angle past the profile: `two_sides` with
  `angle2` = Angle2 − 360° (negative: the model's side two then ends past
  the profile, mitcad#955).
- Sketches attached to an origin's plane named as the origin's element
  (FreeCAD 1.x's `(Origin, XY_Plane013)`, or the role `XY_Plane`) lie on
  that plane (MitCad's origin plane there), as those attached to the plane
  itself (before: "parallel to it (its support is not imported)").
- External geometry whose link the file no longer has (FreeCAD keeps its
  last copy, its source missing) is fixed in place from the copy, so the
  constraints on it hold (before: left out with them).
- Two sides with a negative length (1.1's SideType Two sides, the
  TwoLengths type): FreeCAD's side then ends on the other side's side, the
  solid between the two ends; MitCad's `signed` sides are tried first (a
  side through all or up to an object is cut off there), then each side
  its own way.
- An extent up to a planar face MitCad's body does not have as one face
  (FreeCAD split or merged it otherwise) is up to that face's plane, fixed,
  as FreeCAD takes the face's surface without its limits where the profile
  reaches past them.
- A hole from a sketch bound into another Body may have centres where that
  Body has no material: FreeCAD drills there too, cutting nothing. Such a
  centre (its axis missing the body's box, or no edge of FreeCAD's stored
  result about its axis) gets no modelled thread groove and no thread on
  its wall.
- A circle or arc seen edge on (a sketch square to its plane) projects to
  the line between its exact extremes, as FreeCAD's copy holds it; a
  sketch point at a line's end is not the edge along that line.

## Binders of another Body's sketch (mitcad#963)

- The sketch of the binder's that a Pad, Pocket, Revolution, Groove or
  Hole takes for a binder of one whole sketch of another component
  (mitcad#955, above) lies on the bound sketch's own plane (an origin
  plane, a construction plane or a face of that component) with the
  sketch's `plane_link` through the occurrences where the document places
  the two components first (mitcad#100), and the bound sketch's own frame,
  so that it follows that plane as its projections follow the curves; a
  plane of a third component (the bound sketch's own `plane_link`) is
  seen from the binder's component through the same occurrence. Where
  that plane is not found at the binder's place on the timeline (a face
  that later features of the other Body took away), it lies on the origin
  plane of the binder's component parallel to it, else on a fixed plane,
  as before.
- A sketch attached to a face of such a binder (the face the binder makes
  of the bound sketch) lies on the plane of the binder's sketch, with its
  `plane_link`, when that is the sketch's plane, with the note `on the
  plane of <sketch>, which <binder> binds`. The binder's sketch is made
  for it before the sketch, also when no feature takes the binder; for a
  binder of a sketch of the sketch's own component that plane is the bound
  sketch's. Before, such sketches lay on a plane parallel to the face
  (`its support is not imported`).

## A reported assembly's last fallbacks (mitcad#964)

- A modelled thread's groove is cut about every centre the hole drills
  and the body reaches: the solved positions of the sketch's circles, of
  1.1's defining external circles and arcs, or of its points (before:
  the sketch's own circles only, so a hole drilled at an external arc's
  centre had no thread there and its result was off FreeCAD's by 1e-3).
- A Pad or Pocket along a reference axis or a custom direction parallel
  to the profile's normal: FreeCAD's Direction is the one before Reversed
  turns it, so Reversed pads or pockets the other way, and that is the
  first reading.
- A Pad whose tool lies flush with two tangent walls of the body, which
  OCCT's exact union got wrong on MitCad's body (its faces merged by the
  earlier booleans, where FreeCAD's history keeps them split): such a
  join loses volume and is tried again with a fuzzy value
  (commands.md, "Joins that lose volume").

## Not translated (fallback)

Part sweeps (mitcad#548) through several sections of which one is an
open wire or a wire in no plane (MitCad's loft of curve sections takes no
centre line), and of a section of several wires; Part extrusions
(mitcad#548) of a spline in no plane that does not start at its first
pole (a swept line needs the path's start); binders (skipped: sketches
project their geometry, see stage 2; features take the sketch one binds, see
above); Draft and
Python objects (`Part::Part2DObjectPython`). The Part extrusions FreeCAD
fails itself (a curved face, a helix or non-planar edges along the
base's normal, tapered or with Solid on) fall back with that reason.

Each fallback has a group in the report (`features[].fallback_group`,
mitcad#539), so that the corpus test shows which kind grows:

- `missing` (missing in MitCad): FreeCAD's definition needs a feature or
  an option the translation does not have yet; its reason says "not
  translated". These are the ones to build.
- `no_counterpart` (no parametric counterpart): a scripted object (a type
  ending in `Python`, such as Draft's `Part::Part2DObjectPython`, whose
  shape comes from a script MitCad does not run), a plain shape
  (`Part::Feature`, without a definition), or an object FreeCAD itself
  failed (its stored shape is invalid, e.g. a modelled thread FreeCAD
  1.0 did not make). These stay fallbacks.
- `other`: translated, but no reading built or gave FreeCAD's shape
  within the check (or an input of it was not found).

The readable report adds the group to each fallback's line and the counts
(`fallbacks by group: 1 missing in Mitcad, 4 without a parametric
counterpart, 0 other`); `freecad.corpus` prints them per file and in
total.

Translated, but measured off FreeCAD's stored shapes (mitcad#59; the
check's limit is 1e-6, the reference models `pd_helix_more`,
`pd_helix_outside`, `pd_hole_modelled`, `expr_datum_turns`):

| Case | 0.21.2 | 1.0.2 | 1.1.4 | Why |
|---|---|---|---|---|
| Conical helix | 1.8e-7 ✓ | 2e-14 ✓ | 2e-14 ✓ | |
| Growing helix | 1.4e-6 | 5e-14 ✓ | 5e-14 ✓ | 0.21 builds it along an auxiliary spine; its centre is 1.5e-6 of the size from 1.0's |
| Reversed narrowing, left-handed growing helices | 5.9e-3, 1.9 | ✓ | ✓ | 0.21's are other shapes (its left-handed growing helix holds a fifth more) |
| Helix cut outside the profile | 3.4e-6, 2.9e-4 | 1.4e-4, 1.2e-4 | 1.4e-4, 1.2e-4 | FreeCAD refines the intersection (Refine): its stored shape lies 1e-5 of the volume and 1e-4 of the size in the centre from the plain intersection of its own helix and support (measured in FreeCAD 1.1); without Refine the conical one matches (9.9e-7) |
| Modelled ISO metric thread (M6, blind) | 1.3e-6 | FreeCAD fails it | 1.3e-6 | The groove's flanks: FreeCAD sweeps the section by the Frenet frame along a helix approximated turn by turn, MitCad's helix keeps the axis as its binormal (the area differs by 1.3e-6; volumes agree to 3e-7) |
| Modelled Unified thread (1/4 UNC, through) | 2.0e-6 | 1.9e-6 | 2.0e-6 | As above |
| Modelled tyre valve, Whitworth, NPT threads | | | | FreeCAD rounds the tyre valve crests and cuts Whitworth's 55° profile; MitCad's modelled thread is the 60° one, tapered 1:16 for NPT (mitcad#733) |
| Attachment turns about an axis out of the support's plane, or with a shift | | | | MitCad's planes turn about lines in them (an `angle` plane); a turn that also turns the plane within itself has no parametric plane: a fixed plane, FreeCAD's value kept |

Tyre valve threads exist from FreeCAD 1.1; their holes import parametric.

FreeCAD itself fails a revolution up to the first face, a groove up to a
face, and (0.21, 1.0) a draft pulled along an origin axis; the reference
models leave these out.

## Pipe modes, tapers along custom directions, turns to the last face (mitcad#539)

- Pipes (AdditivePipe, SubtractivePipe) in every orientation mode are
  sweeps: Standard keeps the profile's angle to the spine, Fixed is the
  `parallel` orientation, Frenet the sweep's Frenet `frame` (tried first,
  then the Standard reading: along a planar spine they differ only where
  its bend changes side), Auxiliary the `frame` towards the auxiliary
  spine's sketch edges (FreeCAD's `AuxilleryCurvelinear`, in 1.1 `AuxiliaryCurvilinear`, by default on:
  the points at the same fraction of the two lengths; off: where the
  auxiliary spine crosses the plane square to the spine), Binormal the
  `frame` `constant_binormal`. FreeCAD sweeps a pipe in its own placement
  (1.0 and 1.1 take the profile's) and takes `Binormal` there as it is,
  so the import turns it by the pipe's `Placement` (and the Body's); the
  frame keeps that binormal, turning the profile only about it (not the
  sweep's fixed `binormal`, which keeps the path's tangent). MitCad's
  geometry sweeps them with the pipe shell modes FreeCAD sets (OCCT's
  Frenet, auxiliary and constant binormal frames), within 1e-7 mm as
  FreeCAD does; the OCCT port's patch 1030 corrects the curvilinear
  rule's frame's second derivative, which bent the pipe between its
  sections (a bar turned by an auxiliary spine held 784.9 of its
  800 mm³).
- A Pad or Pocket along a custom direction off the sketch normal is an
  `extrude` along that `direction` (no longer a sweep along a fixed line):
  its lengths parametric, along the direction (FreeCAD's over the cosine
  with `AlongSketchNormal`), and its tapers grown in the profile's plane by
  the length along the direction times their tangent, as FreeCAD's
  draft of the profile does (an offset wire moved along the direction, a
  ruled loft). The taper's sign and the direction's are tried both ways.
- A Revolution up to the last face is a revolution through the Body's
  body to its last face; FreeCAD 1.0 and 1.1 compute neither up to the
  first nor up to the last face ("not yet supported"), so no file holds
  one to check. A Groove through all (1.1) is a full turn; FreeCAD 1.0's
  Groove turns by its Angle whatever its Type, so a groove of any other
  type is also read by its angle.
- Reference models: `pd_pipe_modes` (Binormal along a sloped arc, its
  binormal in the pipe's placement; Frenet,
  Auxiliary with and without curvilinear points; variant BarWidth),
  `pd_taper_direction` (tapered pads along custom directions, along the
  normal and the direction, a circle, symmetric, a pocket; variant Taper)
  and `pd_revolution_upto` (a groove through all, a reversed revolution up
  to a face; FreeCAD 1.0 and later; variant GrooveWidth).

## Part sweeps, framed sections, face sections, several faces (mitcad#543)

- `Part::Sweep` of sketches along a sketch's edges (`Spine`) is a new
  body: one section a `sweep`, several a `loft` along the spine as its
  centre line; `Frenet` the Frenet `frame` (tried first, then at right
  angles to the spine: FreeCAD's corrected Frenet frame, which MitCad's
  default frame is along a planar spine), `Solid` off a surface.
- PartDesign pipes through several sections (Transformation Multisection)
  in the Frenet, Binormal and Auxiliary modes are lofts along the spine
  with the mode's frame (the loft's `frame`, the pipe shell's modes as the
  sweep's); the Fixed mode since mitcad#547 (below).
- A section of a loft or a pipe, or the profile of a pipe, that is a
  planar face of a body (`Profile` = (feature, `Face<n>`)) is the loft's
  `face` section or the sweep's `profile_face` (mitcad#543: the sweep of a
  body face, the face found in the body holding the feature's result).
- A Pad, Pocket, Revolution or Groove of several faces (`AllowMultiFace`)
  is one feature per face, the first with the feature's operation, the
  others joined to it (`" (face 2)"`, …; as many as there are since
  mitcad#547).
- Reference models: `part_sweep` (a Part sweep, in the Frenet mode,
  through two sections, as a shell; variant TubeRadius) and
  `pd_pipe_sections` (pipes through two sections in the Frenet, Binormal
  and Auxiliary modes, a pipe and a loft of a block's top face, a pad of
  two faces in FreeCAD 1.0 and later; variant SectionRadius).

## Fixed pipes through sections, Part sweeps of other objects, many faces (mitcad#547)

- PartDesign pipes through several sections in the Fixed mode are lofts
  along the spine with `orientation` `parallel`: the sections keep their
  direction and only move along it (the pipe shell in a fixed frame, as
  FreeCAD builds it).
- `Part::Sweep` along or of objects that are not sketches
  (`sweep_inputs.rs`): a cylindrical Part helix as the spine is a fixed
  `helix` path from its Pitch, Height, Radius, LocalCoord and placement;
  edges of an object with a body here (`(Box, Edge3)`) the body's edges by
  name; edges of an object without one (a Part Line, Circle, Polygon, a
  Draft wire) one straight edge a fixed line, else the edges as fixed
  geometry of a sketch on a fixed plane (the object's placement where
  they lie in it, else the plane through them). A section that is an
  object's closed edges in one plane (a Part Circle, RegularPolygon,
  Ellipse) is such a sketch's one region; an object of one vertex (a Part
  Vertex) a fixed `point` section of the loft. Objects of edges or a
  vertex that only Part extrusions and sweeps use are `included`, without
  a body; the sweep is `partial` (their own sizes are not carried over).
- Profiles of any number of faces (`AllowMultiFace`): one feature per
  face, `" (face 9)"`, `" (face 10)"`, … after the eighth.
- Reference models: `pd_pipe_fixed_sections` (pipes through two and three
  level circles along a rising arc in the Fixed mode; variant
  SectionRadius), `part_sweep_objects` (sweeps along right- and
  left-handed Part helices, a Part arc and a Part line, of a sketch, Part
  circles, a Part hexagon and a Part vertex; variant WireRadius) and
  `pd_many_faces` (a pad of ten faces, FreeCAD 1.0 and later; variant
  Rise).

## Part sweeps: conical helices, edges and sections in no plane, open sections, PartDesign results (mitcad#548)

- One reader of Part helices for extrusions and sweeps
  (`fixed_paths.rs`, `Importer::part_helix`): a cylindrical helix is a
  fixed `helix` path from its sizes; a conical one (Angle) has no fixed
  helix path, so its stored edges (FreeCAD's B-splines of it) are a path
  of `fixed_curves` (a model path of fixed lines, arcs and B-splines).
- A spine of edges in no plane of an object without a body (a plain
  shape's spline, a Draft wire leaving its plane) is a `fixed_curves`
  path.
- One section of an open wire (a Part arc) is a surface sweep of its
  fixed sketch's curves, one of a wire in no plane (a warped Part polygon)
  a surface sweep of `fixed_curves`: FreeCAD makes no caps of either, a
  shell whatever Solid says. Several sections of which one is such a wire
  fall back (MitCad's loft of curve sections takes no centre line), as
  does a section of several wires.
- Spines and sections of PartDesign features' results (`(Pad, Edge<n>)`,
  `(Pad, Face<n>)`): the Body is replayed for them (a Part object's
  operand that is a PartDesign feature replays its Body), the edges and
  faces found in the Body's body. A Part sweep's spine and sections only
  read their objects: they make no copy of a body and use none up (the
  objects stay for the others).
- Reference model `part_sweep_more` (a sweep along a conical Part helix,
  along a spline in no plane, of an open Part arc and a warped Part
  polygon into surfaces, along a PartDesign block's top edge; variant
  WireRadius).

## Tests

Documents are kept outside the repository (`MITCAD_FCSTD_CORPUS`).
`tools/freecad-export` makes reference models and dumps with FreeCAD 0.21,
1.0 and 1.1 in the isolated distro
([development.md](../../../docs/development.md#freecad-reference-models-and-corpus)).
The ctest `freecad.corpus` (`tools/cli/CMakeLists.txt`) imports every
document and compares those with a dump; for models with a variant it
changes a parameter in MitCad (`import-fcstd --set`) and compares with the
variant FreeCAD recomputed after the same change. A file whose history
fell back gets its fallbacks by group on its line (`fallbacks: 1 missing,
4 no counterpart, 0 other`), and the last line has the totals. Each document is checked
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
exactly (up to 4e-4) than MitCad's; the reference check does not compare
those. Nor does it compare the measures of shapes brought in as FreeCAD
stored them where FreeCAD's are those of OCCT's plain fixed Gauss points
(`Kernel::fixed_point_properties`, `placed[].fixed`): MitCad integrates
faces bounded by B-splines of many spans more exactly (mitcad#139; 2e-6 of
the volume on the hydraulic cylinders of the assembly example).


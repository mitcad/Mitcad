# Mitcad document API

The application and `mitcad-cli` drive a document only through JSON:
`Document::command`, `query`, `preview`, `run_script` and `report`
(`core/model/src/api/mod.rs`, exposed to C++ by `core/ffi/src/lib.rs`).
Geometry comes back as `Shape` handles (`body_shape`, `profile_shape`,
`preview_body_shape`, `preview_tool_shape`), so a new command needs no
change to the C++ bridge. A rejected command or query raises an error
(`rust::Error` in C++) with a message for the user and leaves the document
unchanged. The model's design (recompute, caches, file format, threads) is
in [docs/architecture.md](../../../../docs/architecture.md).

## Identities and names

| What | Form | Notes |
|---|---|---|
| feature | `"F7"` | assigned when created and saved in the project file; its timeline position is separate |
| body | `"F7.b0"` | the creating feature and a number; its display name (`Body1`) is separate and can be renamed |
| component, occurrence | `"C2"`, `"O3"` | `C0` is the root component; names `Plate` and `Plate:2` are separate (see [Components and occurrences](#components-and-occurrences)) |
| sketch point, curve | `"p4"`, `"c3"` | one id space per sketch; `sketch.add_rectangle` makes lines `c<k>`..`c<k+3>` (line i from corner i to i + 1, corners counter-clockwise from the anchor) and then their corner points, a circle a curve and its centre |
| sketch constraint, dimension | `"k2"` | one id space per sketch |
| sketch text | `"t5"` | numbered with the points and curves |
| glyph contour of a text | `"t5.g2.c1"` | contour 1 of character 2 (spaces and line breaks counted) of text t5; usable wherever a curve id can stand in a name (`r{t5.g2.c0}`, `F3:side(t5.g2.c0)#1`, `#k` numbering the contour's pieces) |
| profile region | `"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"` | the sorted segments of its outer boundary; a circle alone is `"r{c5}"`; an end where several curves meet is `c2+c5`, a free end `-` |
| face | `"F3:side(c1[c4,c2])"`, `"F3:end(r{…})#1"` | named by the feature that made it; each feature type lists its face names below; `#k` numbers the pieces of a split face |
| edge | `"E{<face>\|<face>}"` | the two faces that meet there, sorted by text; `#k` when several edges lie between the same faces |
| vertex | `"V{<face>\|<face>\|…}"` | the faces that meet there, sorted |

The grammar is in `core/model/src/topo.rs`. A reference without `#k`
means every piece: `"E{F2:end(…)|F2:side(c1[c4,c2])}"` rounds both halves
of an edge a cut divided. A segment `c3[c2,c4]` is the piece of curve c3
between the curves c2 and c4, in the curve's own direction.

## Values

Every length or angle of a definition is a parameter. In commands a value
is a parameter name (`"d3"`), a number in millimetres or radians, or an
expression (`"d1 / 2 + 5 mm"`, `"30 deg"`; syntax and unit rules in
`core/model/src/expr/mod.rs`).

- A number or an expression creates a dimension parameter owned by the
  feature (the next free `d<n>`, with a comment like `Extrude1
  distance`). Editing a feature with a number or an expression in a slot
  whose parameter the feature owns changes that parameter instead.
- New dimension parameters show lengths in the document's length unit
  (`set_units`) and angles in degrees. A slot whose name ends in `angle`
  (`size.angle`, `extent.angle`, `dimensions[k3].angle`) is an angle,
  every other slot a length unless its feature says it is unitless
  (fractions, revolutions, weights, counts). A slot refuses a parameter of
  another kind; a unitless parameter's number is used as it is.
- Parameters a feature owns are deleted with it, unless another feature
  or parameter still uses them (they then become user parameters).
- Decimal point or comma: `"1,5 mm"` is `"1.5 mm"`. A comma directly
  between two digits is a decimal comma (also `,5` and `5,`); any other
  comma separates function arguments (`max(a, b)`, `max(1, 5)`), and `;`
  always does. `max(1,5)` is therefore `max(1.5)`, and its error says to
  write `max(1; 5)`. No thousands separators: `1,000` is 1 and `1 000` an
  error. Parameters keep their expressions with decimal points, in the
  project file and in the `parameters` query.
- Parameters are evaluated in dependency order. Cycles, unknown names,
  syntax errors and unit mismatches reject the command. Renaming a
  parameter rewrites the expressions that use it (features refer to
  parameters by id).
- A parameter change recomputes only the features whose inputs changed
  value; a parameter command that changes no value (a rename, a comment,
  an equivalent expression) recomputes nothing.

Defaults are metric ([docs/architecture.md](../../../../docs/architecture.md#units)):
leaving out a unit means millimetres (degrees for angles). A new document
and a project file without `units` are in mm and degrees, and bare numbers
in expressions take the document's unit. `export` writes STEP and IGES in
mm (`unit`), `import_file` reads STL and OBJ in mm (`unit_mm` 1),
`sketch.import_dxf` reads a drawing that names no unit in mm; a file's own
units (STEP, IGES, a DXF's `$INSUNITS`, an .f3d design's) are kept. A
`thread` or tapped `hole` without a `standard` is ISO metric. Densities
are g/cm³ (steel, 7.85, without a material), masses kg, moments of inertia
kg mm². Tests: `api::metric_defaults_tests`, `tools/ui-units-test.sh`.

## References to geometry

Every feature or query that takes a plane, an axis, a point, a face, a
body or sketch curves takes it in one form (`GeomRef`,
`core/model/src/features/geom_ref.rs`):

| JSON | Meaning |
|---|---|
| `"xy"`, `"xz"`, `"yz"` | origin planes; frames XY: x = +X, y = +Y; XZ: x = +X, y = -Z (normal +Y); YZ: x = -Z, y = +Y (normal +X) |
| `"x"`, `"y"`, `"z"`, `"origin"` | origin axes and point |
| `"F5"` | the datum (plane, axis or point) of construction feature F5 |
| `{"body": "F2.b0", "face": "F2:end(r{c5})"}` | a face; a plane when planar (outward normal), an axis when a cylinder, cone or torus |
| `{"body": "F2.b0", "edge": "E{…\|…}"}` | an edge; an axis (straight: along the edge's curve; circular: through the centre) or a point (a circle's centre, a line's middle) |
| `{"body": "F2.b0", "vertex": "V{…\|…\|…}"}` | a vertex; a point |
| `{"body": "F2.b0"}` | a body (a face feature's tool) |
| `{"sketch": "F1", "curve": "c4"}` | a sketch curve; a line is an axis from its start to its end. `"curves": ["c1", "c3"]` several, neither all (a face feature's tool) |
| `{"sketch": "F1", "point": "p3"}` | a sketch point |
| `{"origin": [x, y, z], "normal": [x, y, z], "x_axis": [x, y, z]}` | a fixed plane (`x_axis` optional) |
| `{"origin": [x, y, z], "direction": [x, y, z]}` | a fixed axis |
| `{"point": [x, y, z]}` | a fixed point |

- Frames: an origin plane has the frame above. A planar face and a fixed
  plane without `x_axis` have the model origin projected onto the plane
  as origin and the model X axis projected as x axis (Y when the plane
  faces ±X), with the face's outward normal; sketches on faces, mirror
  planes, primitives and extents to faces all use it.
- The axes of cylinders, cones, tori and circular edges point along their
  largest component (+Z for a hole along Z).
- A face name may name the pieces of a split face when they lie on one
  surface; an edge or vertex name must name one (`#k`).
- Older files' forms with a `"type"` (`{"type": "origin_plane", "plane":
  "yz", "offset": d}`, `{"type": "origin", …}`, `{"type": "plane" | "face"
  | "edge" | "vertex" | "construction" | "fixed" | "sketch_curve" | "line"
  | "body" | "sketch", …}`; a face without `body` means the feature's own
  body) are read and saved in the forms above.

### Paths

A path (of a sweep, a pipe or a path pattern, a sweep's guide rail, a
loft's centre line or rails, a rib's or web's curves, a construction
plane or point along a path) is a `PathRef`:

| JSON | Meaning |
|---|---|
| `{"sketch": "F1", "curves": ["c1", "c4"]}` | curves of a sketch (any order), joined end to end |
| `{"sketch": "F1", "curves": ["c1"], "chain": true}` | also every curve joined to them end to end (construction curves only when listed) |
| `{"sketch": "F1", "curve": "c1"}` | one curve; a circle starts on its sketch x axis and runs counter-clockwise |
| `{"body": "F2.b0", "edges": ["E{…}", …]}` | connected edges of a body, in order; each name must name one edge |
| `{"start": [x, y, z], "end": [x, y, z]}` | a fixed line |

A path runs from the free end of the first listed curve or edge when it
ends the chain, else along the first listed curve's own direction (a
closed chain from that curve's start). A path may not branch or have
gaps; a closed curve is a path by itself. A path pattern's path is a
sketch line, arc or circle, or a fixed line (not edges).

## Feature definitions

The `def` of `add_feature`, `edit_feature` and `preview`, and the `def`
the `feature` query returns. Project files store the same fields with
parameter names. Each type is a module `core/model/src/features/<type>.rs`.
Lengths in millimetres, angles in radians; every length, angle, quantity,
fraction and factor is a value ([Values](#values)).

Shared rules:

- `operation`: `new_body`, `join`, `cut`, `intersect`, or `new_component`
  (new bodies in a new component placed in the feature's component, see
  [Components and occurrences](#components-and-occurrences));
  `participants`: the bodies a join, cut or intersection works on (all
  bodies when empty or left out). Their results are described under
  [extrude](#extrude); "as for extrude" elsewhere means the same.
- A feature that asks for something the geometry kernel cannot do fails
  with a message starting `unsupported: ` (`mitcad_model::features::UNSUPPORTED`);
  the .f3d importer then falls back to the body the file stores.
- A face or edge that a later edit removed is explained (`face X no
  longer exists after Fillet2`, `... does not exist because Fillet1 is
  suppressed`).
- A feature that succeeds with a caveat has warnings (`timeline` status
  `warning`): a fillet or chamfer built 0.1 % smaller than asked (at the
  exact size it takes a face away, which OCCT cannot build) or built as
  rings about its circles ([fillet](#fillet)), a sketch
  text whose font is not installed. Evaluators call `EvalContext::warn`;
  `Kernel::notes` gives what the geometry gave up
  (`geometry::Shape::notes`).

### sketch

```json
{"type": "sketch", "plane": "xy",
 "entities": [
   {"id": "c1", "type": "line", "start": "p2", "end": "p3"},
   {"id": "p2", "type": "point", "at": [0, 0], "fixed": true},
   {"id": "p3", "type": "point", "at": [60, 0]},
   {"id": "c4", "type": "circle", "center": "p5", "radius": 5},
   {"id": "p5", "type": "point", "at": [45, 20]}],
 "constraints": [{"id": "k1", "type": "horizontal", "line": "c1"}],
 "dimensions": [{"id": "k2", "type": "length", "line": "c1", "value": 60},
                {"id": "k3", "type": "diameter", "curve": "c4", "driven": true}]}
```

Code in `core/model/src/sketch/`. Coordinates are sketch plane
millimetres, angles radians. Build sketches with the
[sketch commands](#sketch-commands), which pick the ids.

- `plane`: `"xy"` (default), `"xz"`, `"yz"` (the origin datums' frames), a
  construction plane feature before the sketch (`"F5"`), or a planar face
  `{"face": "<face name>", "body": "F2.b0"}` (`body` optional) of the
  bodies before the sketch (frame as in [References to
  geometry](#references-to-geometry)).
- `plane_link` (optional): `{"source": "O1", "target": "O2"}` when the
  plane is another component's ([Geometry of other
  components](#geometry-of-other-components)); `projections` may have a
  `link` of the same form.
- `frame` (optional): the sketch's own frame in the plane's frame,
  `{"origin": [x, y, z], "x_axis": [..], "y_axis": [..]}` in the plane
  frame's (x, y, normal) coordinates; imports keep a file's exact sketch
  transform this way.
- `entities`: points `p<n>` and curves `c<n>` in one id space. Curves
  refer to points, and curves that share a point are joined there: `line`
  (`start`, `end`, `centerline`), `circle` (`center`, `radius`), `arc`
  (`center`, `start`, `end`, counter-clockwise), `ellipse` (`center`,
  `major`: the end point of the major axis, `minor_radius`),
  `elliptical_arc` (also `start`, `end`), `spline` (`degree`, `control`
  points, optional `weights` and `knots`; default clamped uniform on
  [0, 1]) and `fitted_spline` (`points`: a cubic through them). Flags:
  `construction` (not in profiles), `fixed`, `reference` (projected).
  Positions and radii are the last solved ones and the start of the next
  solve; a new definition may give any reasonable positions.
- `constraints` (`k<n>`, shared with dimensions): `coincident` (`point`,
  `entity`: a point or a curve), `horizontal`/`vertical` (`line`),
  `horizontal_points`/`vertical_points` (`a`, `b` points), `parallel`,
  `perpendicular`, `collinear`, `tangent`, `smooth` (G2), `equal`,
  `concentric` (`a`, `b` curves), `midpoint` (`point`, `curve`),
  `symmetric` (`a`, `b`: points or curves, `axis`: a line).
- `dimensions`: `distance`, `horizontal_distance`, `vertical_distance`
  (`a`, `b` points), `point_line_distance` (`point`, `line`),
  `line_distance` (`a`, `b` parallel lines), `length` (`line`), `angle`
  (`a`, `b` lines; the quadrant they are in when it is added), `radius`,
  `diameter` (`curve`), `arc_length` (`arc`), `major_radius`,
  `minor_radius` (`ellipse`), `linear_diameter` (`axis`, `entity`). A
  driving dimension has a `value` (slot `dimensions[k3].<type>`, so
  `angle` is an angle and the others lengths); a driven one has `"driven":
  true` and only measures. `text`: where the value is shown.
- `projections`: linked projections, `{"source": "<edge, face or
  vertex>", "body": "F2.b0", "entities": ["c7", "p8", …]}`; the entities
  follow the source when the model changes (`sketch.project`). The stored
  positions are those of the last edit of the sketch; the evaluation moves
  them (in steps, so that geometry constrained to them keeps its side), the
  `sketch` query shows them moved and the next edit stores them.
- `texts`: `{"id": "t9", "text": "…", "at": [x, y], "height": h,
  "angle": a, "font": "…", "bold": false, "italic": false}`, optional
  `align` (`left`, `center`, `right`), `valign` (`baseline`, `bottom`,
  `middle`, `top`), `spacing` (extra character spacing, % of the height),
  `frame` (three point ids: a corner and its neighbours along the text's
  x and y, the corners of a construction rectangle the text fills; `at`
  and `angle` follow it), `path` (`{"curve": "c4", "above": true, "fit":
  false}`: the text runs along the curve, fitted to its length when
  `fit`), `flip_x`, `flip_y`. Multi-line text breaks at `\n`. Each glyph
  contour (`t9.g<k>.c<j>`, from the font's outlines through the kernel's
  `font_glyphs`; the bundled Droid Sans when the font is missing) bounds
  profile regions like a closed curve: a letter is a region with its
  counters as holes, a counter a region of its own.
- `patterns`: copy groups, `{"id": "k5", "type": "circular", "center":
  "p3", "count": 6, "angle": "d4", "entities": ["c2", "p7"], "copies":
  [{"index": [1, 0], "entities": {"c2": "c9", "p7": "p10"}}, …]}` or
  `{"type": "rectangular", "direction": [1, 0], "count": [3, 2],
  "spacing": ["d5", "d6"], …}`. Each solve binds every copy to its
  original with the pattern's transform (solver constraints `Translated`,
  `Rotated`, `TurnedDirection`, `EqualSize`; not stored as constraints),
  so the copies follow the originals and add no degrees of freedom. Slots
  `patterns[k5].angle`, `.spacing1`, `.spacing2`. A new count regenerates
  the copies, keeping those that stay. Copies cannot be patterned again
  or moved on their own.
- `offsets`: `{"id": "k8", "curves": ["c1", "c4"], "distance": "d6",
  "left": false, "results": ["c10", "c11"], "corners": [{"join": 0,
  "arc": "c12"}], "derived": true}`: the source chain, the distance (slot
  `offsets[k8].distance`, not negative; `left` is the side) and the curves
  made.
  - A chain of lines and arcs, or a circle, is offset exactly by
    constraints: the results are ordinary curves, and dragging one drags
    the source along (the distance dimension drives).
  - A chain with an ellipse, an elliptical arc or a spline is `derived`:
    its curves are computed from the solved source after every solve
    (lines and arcs exactly, an ellipse or spline as a C1 cubic B-spline
    within 1e-4 mm of the exact offset), with round arcs at outer corners
    and trimmed inner corners. Constraints cannot refer to them;
    `sketch.drag` or `sketch.move` of them moves the whole source chain
    by the same motion.
  - The distance changes only in `sketch.edit_offset`. The source's
    constraints apply (a fixed source holds its offset). A copy
    (`sketch.move` with `copy`) is an ordinary curve that no longer
    follows the source. Each corner's kind is fixed when the offset is
    made: an edit that would change it is refused (as .f3d offsets fail
    likewise).

Evaluation solves the sketch (an over- or inconsistently constrained
sketch fails with the conflicting constraint ids) and cuts the
non-construction curves into profile regions: the faces of the planar
arrangement, with islands as holes; overlapping shapes split into
separate regions, and curves on each other (a circle drawn twice) bound
them once, the one with the lower id. A region reference whose key no longer exists means the
region with the most curves in common. Older files' `shapes` (rectangles
and circles) are read as entities, constraints and dimensions with the
same curve ids and parameters.

### extrude

```json
{"type": "extrude",
 "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
 "extent": {"type": "distance", "distance": 20},
 "flip": false,
 "operation": "join",
 "participants": ["F2.b0"]}
```

- `profiles`: regions of one sketch; touching regions fuse into one body
  (and taper as one region).
- `start` (optional): `{"type": "profile_plane"}` (default), `{"type":
  "offset", "offset": s}` (the profile moved s along the sketch normal,
  whatever `flip` says) or `{"type": "object", "object": <object>,
  "offset": s}` (the profile projected along the sketch normal onto a
  plane or planar face, moved s along its normal; the distance is then
  measured from there, and a start plane not parallel to the sketch takes
  no taper).
- `extent`, each with an optional `"taper"` (radians; positive widens the
  profile away from the start: the outside grows and holes shrink):
  - `{"type": "distance", "distance": d}`: one side; negative goes the
    other way;
  - `{"type": "to_object", "object": <object>, "offset": o}`: one side up
    to an object; a positive offset makes the extrusion longer;
  - `{"type": "through_all", "both_sides": false}`: past every
    participant body (all bodies when there are none), one way or both;
  - `{"type": "symmetric", "distance": d, "full_length": false}`: d each
    way, or d in total (`isFullLength` in .f3d designs);
  - `{"type": "two_sides", "side1": <side>, "side2": <side>}`: side one
    along the direction, side two against it, each `{"type": "distance",
    "distance": d}`, `{"type": "to_object", "object": …, "offset": o}` or
    `{"type": "through_all"}` with its own `"taper"`. A side of distance 0
    leaves one side.
- `<object>`: `{"type": "plane", "plane": p}` (a plane reference: origin
  plane, construction plane, a planar face as an unbounded plane, or a
  fixed plane), `{"type": "face", "body": b, "face": f, "chained": false}`
  (the face's surface continued past its edges; with `chained` the face
  and the faces next to it) or `{"type": "body", "body": b, "through":
  false}` (to its first face reached, or through it).
- `thin` (optional): `{"location": "side1" | "center" | "side2",
  "thickness": t, "side2": {"location": …, "thickness": …}}`: walls along
  the profile curves instead of the regions. Side 1 lies away from the
  region's material (outside the outer boundary, into a hole), side 2
  towards it. `side2` is the wall of side two of a two-sided or symmetric
  extent when it differs.
- `flip`: reverses the direction (the sketch normal by default).
- `operation`, `participants`: see the shared rules.

Faces: `side(<segment>)`; `outer(<segment>)` and `inner(<segment>)` for
thin walls (side 1 and side 2); `start(<region>)` and `end(<region>)`.
With one side the start is the cap at the start and the end the far cap;
with two sides (and symmetric) the start is the far cap of side one and
the end the far cap of side two (`startFaces` and `endFaces` in .f3d
designs). The faces of an object where the extrusion ends get the cap's
name. A side face that a two-sided taper bends at the start is two pieces
(`#0`, `#1`); `mid(<region>)` is what remains of the start between two
sides of different walls.

Operations: new body makes a body per solid (`<extrude>.b0`, `.b1`, …).
Join fuses the tool with every participant it touches; joined bodies
merge into the first of them, and a tool solid that touches nothing
becomes a new body. Cut and intersect work on each participant they
reach; a cut through all of a body removes it, and a cut that splits a
body keeps the body for the first piece in geometric order and makes the
others new bodies of the cut. A join, cut or intersection that reaches no
participant fails.

### revolve

```json
{"type": "revolve",
 "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
 "axis": "y",
 "extent": {"type": "angle", "angle": 1.5708},
 "operation": "new_body"}
```

- `axis`: a line reference: a sketch line `{"sketch": uid, "curve":
  "c4"}` (from its start to its end; any line, also a construction or
  centre line), an origin axis, a construction axis `"F7"`, a straight
  edge `{"body": b, "edge": e}` or a fixed axis. `project_axis: true`
  projects it into the sketch plane (`isProjectAxis` in .f3d designs).
- `extent` (positive by the right hand rule about the axis direction):
  `{"type": "angle", "angle": a}` (one side; negative turns the other
  way), `{"type": "full"}`, `{"type": "symmetric", "angle": a}` (a to each
  side, as .f3d designs measure it), `{"type": "two_sides", "angle1": a1,
  "angle2": a2}` or `{"type": "to_object", "object": <object>}` (the
  positive way to the first face of the object reached).
- `operation`, `participants`: as for extrude.

The profile may touch the axis but not cross it. Faces are named as an
extrusion's; a full turn has no caps.

### hole

```json
{"type": "hole",
 "placement": {"type": "face", "body": "F2.b0", "face": "F2:end(r{…})", "points": [[30, 20, 20]]},
 "diameter": 10,
 "kind": {"type": "counterbore", "diameter": 16, "depth": 5},
 "tip_angle": 2.0595,
 "extent": {"type": "distance", "depth": 10},
 "thread": {"designation": "M10x1.5", "class": "6H", "modeled": false}}
```

- `placement`: `{"type": "face", "body": b, "face": f, "points": [[x, y,
  z], …]}` (points put onto a planar face's plane, one hole each),
  `{"type": "face_offsets", "body": b, "face": f, "edge1": e1, "offset1":
  o1, "edge2": e2, "offset2": o2}` (one hole at those distances from the
  lines of two edges, on the face's side of both) or `{"type":
  "sketch_points", "sketch": uid, "points": [[u, v], "c5", "p3", …]}`
  (sketch coordinates, the centres of the sketch's circles, arcs and
  ellipses, or its points; `setPositionBySketchPoints` in .f3d designs).
  Holes go against the face or sketch normal, into the material; `flip`
  turns them round.
- `kind`: `{"type": "simple"}` (default), `{"type": "counterbore",
  "diameter": D, "depth": d}`, `{"type": "countersink", "diameter": D,
  "angle": a}` (the cone's full angle), `{"type": "counterdrill",
  "diameter": D, "depth": d, "angle": a}` (a counterbore whose floor is a
  cone of the full angle narrowing to the hole).
- `taper` (optional): the wall's angle to the axis, positive narrowing the
  hole from its diameter at the start as it goes deeper (a counterbore's
  floor and a countersink's or counterdrill's cone meet the leaning wall;
  the drill point starts from the wall's end). Not with a `thread`.
- `tip_angle`: the drill point's full angle, 118° when left out; `flat:
  true` for a flat bottom.
- `extent`: `{"type": "distance", "depth": d}` (to the shoulder, from the
  start; the point is extra), `{"type": "through_all"}` (past every
  participant, flat) or `{"type": "to_object", "object": <object>,
  "offset": o}` (the cylinder ends where each axis first meets the
  object, the point beyond).
- `thread` (a tapped hole): `standard` (`iso_metric` default, `unified`,
  `whitworth`, `npt`, `tyre_valve`), `designation`, `class`, `right_handed` (default
  true), `modeled` (ISO metric, Unified and tyre valve only), and `length`, `offset`
  from the hole's start (the whole wall when left out); as for the
  [thread](#thread) feature.
  The bore is the size's basic minor diameter; `diameter` is then ignored.
- `participants`: the bodies to cut, all when left out.

Faces: `hole<i>.wall`, `hole<i>.tip` (point or flat bottom),
`hole<i>.cbore_wall` (a counterbore's or counterdrill's),
`hole<i>.cbore_floor`, `hole<i>.csink`, `hole<i>.cdrill` (a
counterdrill's cone), `hole<i>.top` (at the start, gone after the cut); i
numbers the positions from 0.

### thread

```json
{"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
 "thread": {"designation": "M10x1.5", "class": "6g"},
 "modeled": false, "length": 12, "offset": 2, "location": "high_end"}
```

- `faces`: cylindrical faces, all internal (hole walls) or all external.
- `thread`: `standard` (`iso_metric` default, `unified`, `whitworth`,
  `npt`, `tyre_valve`), `designation` (`M10x1.5`, `M10` for the coarse
  pitch, `1/4-20 UNC`; Whitworth `1/4-20 BSW`, `1/4-26 BSF`, the parallel
  pipe threads `G 1/4`; NPT `1/4-18 NPT`; the tyre valve threads of ISO
  4570 `5V1`, `8V1`, … on the 60-degree profile, mitcad#59), `class` (`6g`
  external, `6H` internal; Whitworth `Close`, `Medium`, `Free`, `A`, `B`
  external, `Medium`, `Normal` internal; NPT and tyre valve `Standard`;
  checked against the face),
  `right_handed`. Sizes: `core/model/data/threads.json` (the
  `thread_sizes` query). An NPT size is listed at its pipe's outside
  diameter; its 1:16 taper is not kept.
- `modeled`: build the 60° profile on the faces (ISO metric, Unified and
  tyre valve only, the others are cosmetic only); otherwise cosmetic: no geometry,
  listed by the `threads` query. The profile is ISO 68-1's: flanks at
  30° to the radius, half a pitch wide at the pitch diameter, flat crests
  and roots on the major and minor diameters. Along the threaded part the
  material becomes the thread: removed beyond the profile, and added up
  to the crest where the face lies inside it (a shaft thinner than the
  major diameter, a bore wider than the minor one). The part ends in
  planes across the axis, and runs out through ends of the face with no
  material beyond them (a shaft's free end, a hole's mouth).
- `diameters` (modelled only): `{"major": D, "minor": D1, "pitch": D2}`,
  the profile's diameters, e.g. a tolerance class's (the middles of its
  tolerances, as `.f3d` designs store them: M10x1.5 6g is 9.85, 8.141,
  8.928); the size's basic diameters (D, D1 = D − 5H/4, D2 = D − 3H/4)
  when left out.
- `angle` (modelled only): the thread turned about its axis (right-hand
  rule), 0 when left out. At angle 0, with the axis pointing up (+Z; axes
  across Z towards +Y, the X axis towards +X), from the face's end at the
  low end of that axis and at the reference direction (the X axis
  projected across the axis; Y for axes within about 25° of X), an
  external thread's groove spans the first half pitch at the pitch
  diameter, an internal thread's tooth the same half pitch (a bolt and a
  nut threaded from the same plane mate).
- `length` and `offset`: part of each face, from the end its cylinder's
  axis points to (`location`: `high_end`, default) or from the other
  (`low_end`); the whole face when left out.

A modelled thread's new faces (flanks, crests and roots) are
`thread(<face>)`. The work is local: only the body's faces near the
thread take part, so a large body does not slow it down.

### fillet

```json
{"type": "fillet", "body": "F2.b0", "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 2}
```

The short form is one set of edges with a constant radius. The general
form lists edge sets:

```json
{"type": "fillet", "body": "F2.b0", "sets": [
  {"edges": ["E{…}"], "size": {"type": "constant", "radius": 5}, "tangent_chain": false},
  {"faces": ["F2:end(r{…})"], "size": {"type": "chord_length", "length": 3}},
  {"edges": ["E{…}"], "size": {"type": "variable", "start": 2, "end": 4, "start_vertex": "V{…}",
   "mid": [{"position": 0.25, "radius": 3}]}, "continuity": "tangent"},
  {"edges": ["E{…}"], "size": {"type": "constant", "radius": 2}, "continuity": "curvature",
   "tangency_weight": 1.5},
  {"edges": ["E{…}"], "size": {"type": "asymmetric", "distance1": 2, "distance2": 4, "flip": false},
   "reference_face": "F2:side(c1[c4,c2])"}],
 "rolling_ball_corners": true}
```

- A set rounds its `edges` and every edge of its `faces`; an edge may be
  in one set only. Any number of fillets per body.
- `size`: `constant` (`radius`), `chord_length` (`length`, the width
  across the rounding; the faces must meet at a constant angle along the
  chain), `variable` or `asymmetric`.
- `variable`: `start` at `start_vertex` (an end of the chain, or the
  kernel's start when left out) to `end` at the chain's other end,
  through the `mid` radii at their positions along the chain (0 to 1 from
  the start, increasing). The radius follows OCCT's smooth interpolation
  of all the radii, level at both ends, along the whole tangent chain
  (not confirmed to be the law of `.f3d` variable fillets). Mid radii
  around a closed chain are unsupported.
- `asymmetric`: `distance1` from the edge along the set's
  `reference_face` (or along the first face of each edge's name), and
  `distance2` along the other face; `flip` swaps the faces. The
  cross-section is a conic, the affine image of the circular arc, tangent
  to both faces (Mitcad's own convention).
- `tangent_chain` (default true): the edges that continue an edge
  smoothly are rounded too. The kernel always follows tangent chains, so
  a set without it whose chain reaches an unselected edge is unsupported.
- `continuity`: `tangent` (G1, default) or `curvature` (G2). A G2 set
  touches the faces where the G1 rounding of its radius would (the
  asymmetric one at its distances) and its quintic cross-section follows
  the faces' curvature there; `tangency_weight` (0.1 to 2, default 1)
  pulls it closer to the edge's corner (larger) or to the chord (smaller).
  Mitcad's own shape; variable radius G2 sets are unsupported.
- Asymmetric and G2 sets are built from a chamfer at their contact
  distances whose bevels are replaced by blend surfaces (along circles
  between faces of revolution about their axis the cross-section turned
  about it, elsewhere Gordon surfaces through cross-sections and the
  chamfer's contact lines; the faces a chain ends on take the end
  cross-sections). Their chains must end on planar faces or close on
  themselves; a chain that meets another rounded edge at a vertex, or a
  non-planar end face, is unsupported. Rolling ball sets of the same
  fillet are built first.
- `rolling_ball_corners` (default true): false asks for setback corners
  where three or more rounded edges meet: the roundings stop 1.5 times
  the largest radius there from the vertex and an N-sided patch tangent
  to them and to the faces (OCCT's plate surface) closes the corner,
  named `corner(<vertex>)` (Mitcad's own convention). An edge shorter
  than the setback is unsupported.
- Slots: `radius`, `chord_length`, `start_radius`, `end_radius`,
  `mid[k].radius`, `distance1`, `distance2`, `tangency_weight` (unitless)
  for the first set, `sets[i].radius`, … for the others.
- New faces: `<fillet>:fillet(<edge>)` along every rounded edge (also the
  ones a tangent chain adds) and `<fillet>:corner(<vertex>)` where rounded
  edges meet.
- Rings: OCCT's roundings and bevels stop where they would run over the
  edge of a neighbouring face (the rim of a hole closer to a side than
  its radius). When OCCT fails and every selected edge is a whole circle
  between two faces of revolution about its axis with straight sections
  (planes square to the axis, cylinders, cones), the cross-section turned
  about the axis is cut from the body (convex edges) or joined to it
  (concave), running over what it meets; constant radius and chord
  length sets, and chamfers of every size (`geometry/src/ring_dressup.cpp`).
  The feature has a warning saying so.
- Second tries of OCCT's own rounding, before any of the above (mitcad#107):
  when it fails at the size asked, it is built again on the body with the
  pieces of its edges merged (a whole circle in two arcs, a line in
  pieces between the same two faces; the new faces carry the names of
  every piece), then with tighter tolerances of OCCT's fillet (knife
  edges, where the faces' normals are opposite). A result OCCT's checker
  rejects only where the input body already was invalid, on faces the
  rounding neither changed nor touches, is taken as it is (stored bodies
  of imported designs are at times invalid in places); other invalid
  results are healed as before, never those of a second try.
- Edge overflow (mitcad#121, OCCT's fillet as patched in the port): a
  rounding wider than a face next to its edge along the whole edge runs
  onto the face beyond that face's far edge, or, where it cannot touch
  that face, rolls on the far edge; the narrow face vanishes (also all
  round a closed edge). It runs over several narrow faces in a row (a
  strip and a sliver beyond it) and over narrow faces on both sides of
  the edge, as long as the ball touching the faces it reaches clears
  those it passes; otherwise it rolls on the last edge before a face it
  cannot touch. Two roundings of the edges of a face narrower than both
  (their contacts on it cross) are one rounding over that face, from the
  face beyond one edge to the face beyond the other. Where a rounding is
  wider only along part of the edge, OCCT's rounding rolls on the far
  edge where its contact reaches it, and near an end of the edge it is
  cut by the face beyond, as before. Chamfers do not overflow.

### chamfer

```json
{"type": "chamfer", "body": "F2.b0", "edges": ["E{…|…}"],
 "size": {"type": "equal_distance", "distance": 2}, "flip": false}
```

`size` is `{"type": "equal_distance", "distance": d}`, `{"type":
"two_distances", "distance1": d1, "distance2": d2}` or `{"type":
"distance_angle", "distance": d, "angle": a}` (0 < a < π/2). The first
distance and the angle (from that face) are measured on the first face of
each edge's name (`E{<first>|<second>}`), or on the second with `flip`.
The general form has edge sets with a reference face:

```json
{"type": "chamfer", "body": "F2.b0", "sets": [
  {"edges": ["E{…}"], "faces": ["F2:end(…)"],
   "size": {"type": "two_distances", "distance1": 5, "distance2": 3},
   "reference_face": "F2:side(c4[c3,c1])", "flip": false, "tangent_chain": true}],
 "corner": "chamfer"}
```

- `reference_face`: the face the first distance and the angle are
  measured on; every edge of the set must border it, and `flip` takes the
  edge's other face. Tangent chains as for fillets.
- Slots: `size.distance`, `size.distance1`, … for the first set,
  `sets[i].size.distance`, … for the others.
- `corner` shapes the vertices where three or more bevelled edges meet:
  `chamfer` (default) is OCCT's corner face; `miter` runs the bevels on
  until they meet, with no face of its own (a corner of convex and
  concave edges is unsupported); `blend` stops the bevels 1.5 times their
  largest distance from the vertex and closes the corner with a patch
  tangent to them and through the faces' edges, named `corner(<vertex>)`.
  Where no three bevelled edges meet, `miter` and `blend` make the plain
  chamfer with the warning `the miter corner type shapes nothing here`
  (whether .f3d corner types change two-edge vertices is unknown).
- New faces: `<chamfer>:chamfer(<edge>)`, `<chamfer>:corner(<vertex>)`.
- Bevels of whole circles that run over a neighbouring face are built as
  rings, as for fillets.
- A failed bevel is built again on the body with merged edge pieces, and
  an input's own faults away from it are taken over, as for fillets.

### Face features: shell, draft, offset_face, delete_face, replace_face, split_body, split_face

These work on named faces of one body (`body`, `faces`). The plane, face,
body or sketch curves a feature works against (`plane`, `target`, `tool`)
is a [reference](#references-to-geometry): a plane (`"yz"`, `"F5"`, a
fixed plane), a face `{"body": "F2.b0", "face": "F2:end(…)"}`, a body
`{"body": "F4.b0"}` or sketch curves `{"sketch": "F6", "curves": ["c1",
"c3"]}`. Two fields go with it: `offset` (optional, a slot) moves a plane,
not a face, along its normal; `direction` (split body and split face,
optional) is the direction sketch curves are swept along, the sketch
normal by default.

```json
{"type": "shell", "body": "F2.b0", "faces": ["F2:end(…)"], "inside": 2, "outside": 0,
 "tangent_chain": true, "rounded": false}
```

Shell: the `faces` are removed (none: a closed void); the wall runs
`inside` into and `outside` out of the surface (at least one, not
negative). `tangent_chain` (default true) also removes faces tangent to a
removed one; `rounded` rounds the outer corners. The outside of the wall
keeps the face names; the inside is `<shell>:offset(<face>)` and the end
of the wall where a removed face was `<shell>:offset_cap(<face>)`.

```json
{"type": "draft", "body": "F2.b0", "faces": ["F2:side(c1[c4,c2])"],
 "plane": {"body": "F2.b0", "face": "F2:start(…)"}, "angle": 0.0873,
 "symmetric": false, "angle2": null, "flip": false, "tangent_chain": true}
```

Draft with a fixed plane (a plane or a planar face): the faces turn about
their intersection with it by `angle` (|a| < π/2, not zero). The pull
direction is the plane's normal, or from a face into its material; `flip`
reverses it. A positive angle removes material on the pull side, so a
side face drafted about the bottom face leans in towards the top.
`symmetric` drafts both sides of the plane by `angle`, `angle2` sets the
other side's angle: the faces are split at the plane (pieces `#k`) and
each side narrows away from it. Without `tangent_chain`, a face tangent
to a drafted one is unsupported. Parting line drafts are not modelled.

```json
{"type": "offset_face", "body": "F2.b0", "faces": ["F2:end(…)"], "distance": 5}
{"type": "delete_face", "body": "F2.b0", "faces": ["F3:fillet(E{…})"]}
{"type": "replace_face", "body": "F2.b0", "faces": ["F2:end(…)"],
 "target": {"origin": [0, 0, 25], "normal": [0, 0, 1]}, "tangent_chain": true}
{"type": "replace_face", "body": "F2.b0", "faces": ["F2:end(…)"], "target": {"body": "F4.b0"}}
```

Offset face (also Press Pull on a face): the faces move along their
normals, positive out of the material; the neighbouring faces extend or
shorten along their own surfaces; the faces keep their names.

Delete face removes the faces and extends the neighbours until they close
the gap, failing when they cannot.

Replace face replaces faces of any surface type with a target: a plane, a
face of a body (any surface type; a face of the body itself works too) or
a body (all its faces, such as a surface body; not the body itself).

- The neighbours extend or shorten along their own surfaces to it; the
  new face is `<feature>:replace(<face>)` (pieces `#k`) and the
  neighbours keep their names.
- Planar faces onto a plane or a planar face move there. Otherwise the
  region between the faces and the target, closed by the neighbours
  continued along their surfaces and by the target (when it is one face)
  continued along its own, is added to or removed from the body;
  B-splines continue by extrapolation, so their results are close, not
  exact.
- Mitcad's own rules: the target need not cover the faces' neighbourhood
  (it is continued); a face with the target on both sides goes to the
  nearer side (the smaller region); a region that would reach past the
  body's other faces does not count (`the target passes beyond the other
  faces of the body`).
- The tangent chain also replaces faces tangent to the replaced ones,
  such as the straight sides of a slot with its round end; replacing only
  the round end needs `"tangent_chain": false`.
- Other failures: `the target does not meet the body where the faces
  are`, `the target splits the body`, `face … could not be replaced` (the
  target does not reach all of it) and, for planar faces onto a plane,
  `the target is perpendicular to face …`.

```json
{"type": "split_body", "bodies": ["F2.b0"], "tool": "yz", "offset": 20, "extend": true}
{"type": "split_face", "body": "F2.b0", "faces": ["F2:end(…)"], "tool": {"sketch": "F6"},
 "direction": [0, 0, 1], "extend": true}
```

Split body cuts each body with a plane, a face (with `extend`, the
default, grown along its surface: planes, cylinders, cones and spheres),
a body or sketch curves; a body the tool does not divide fails it. The
pieces come in order along a planar tool's normal (else in geometric
order): the first keeps the body (uid and name), the others are new
bodies of the split (`<split>.b0`, …, named `Body<n>`). The cut faces are
`<split>:split` (`<split>:split(<tool face>)` for a face or body tool)
and faces cut in two get `#k`.

Split face splits the faces along the tool (the same kinds; sketch curves
swept along `direction`); the shape does not change and the pieces keep
the face's name with `#k`. Every face must be split.

### sweep

```json
{"type": "sweep", "profiles": [{"sketch": "F2", "region": "r{c1}"}],
 "path": {"sketch": "F1", "curves": ["c1"], "chain": true},
 "orientation": "perpendicular", "twist_angle": 0, "taper_angle": 0,
 "guide_rail": {"sketch": "F1", "curves": ["c9"]}, "profile_scaling": "scale",
 "extent": {"type": "partial", "fraction": 0.5, "fraction2": 1}, "flip": false,
 "operation": "new_body"}
```

Sweeps, lofts, pipes, coils, ribs and webs: kernel inputs in
`core/model/src/sweeps.rs`, geometry in
`geometry/src/{path_sweep,loft,rib,spine}.cpp`. Fractions of a path,
revolutions and loft weights are unitless values. Examples:
`tools/cli/tests/f3_*.json`.

- `profiles`: regions of one sketch (touching regions sweep together,
  holes stay holes). The sweep starts where the path crosses the
  profile's plane (the crossing nearest to the profile), or at the path's
  point nearest to the profile; a profile beside the path is swept as it
  is placed against it.
- `path`: a [path](#paths).
- `orientation`: `perpendicular` (default; the profile keeps its angle to
  the path, without twisting about it) or `parallel` (it keeps its
  direction and only moves; where the path turns back through the
  profile's plane, the copies passing over each other are fused into one
  body).
- `twist_angle`: the profile's turn about the path over the swept length
  (right hand about the path's direction). `taper_angle`: positive
  widens; the profile point farthest from the path moves out by `s
  tan(taper)` at distance `s` (a circle on the path becomes a cone; not
  confirmed against .f3d designs).
- `guide_rail`: turns the profile towards the rail and with
  `profile_scaling` `scale` (default) sizes it by the rail's distance
  from the path, `stretch` only towards the rail, `none` not at all. The
  rail point is the one at the same fraction of the rail's length. Twist
  and taper are ignored with a rail.
- `extent`: `{"type": "full"}` (default: the whole path both ways from the
  profile, once round a closed path) or `{"type": "partial", "fraction":
  f1, "fraction2": f2}`: f1 of the path's part beyond the profile, f2 of
  the part before it (`distanceOne` and `distanceTwo` in .f3d designs;
  left out, the whole part before it, none on a closed path, where f1 + f2
  <= 1).
- `flip`: runs the path the other way (`isDirectionFlipped` in .f3d
  designs).

Twists, tapers and rails sweep copies of the profile placed along the
path through a smooth loft: the path must be smooth, the orientation
perpendicular and the sweep not round a whole closed path (otherwise
`unsupported: …`). Sharp corners of a plain sweep are mitred; a corner
whose mitre does not build (a slight kink between path curves) is
rounded, or else the profile turns through it. A profile within the
modelling tolerance (1e-7 mm) of an end of the path is at that end:
`fraction` 0 with the whole `fraction2` sweeps only backwards from it,
and an extent that covers none of the path is an error.

Faces: `side(<segment>)` from each profile segment, `#k` pieces where the
path has several curves; `start(<region>)` and `end(<region>)`: with the
profile at an end of the swept part `start` is at the profile, otherwise
`start` ends side one (beyond the profile) and `end` side two, as for
extrusions. A sweep round a whole closed path has no caps. With the
`parallel` orientation, where the path turns back through the profile's
plane the copy there is a face of its own, `turn(<region>)` (`#k` when it
turns back several times).

### loft

```json
{"type": "loft", "sections": [
   {"type": "profile", "sketch": "F1", "region": "r{…}"},
   {"type": "face", "body": "F4.b0", "face": "F4:end(r{c1})"},
   {"type": "point", "point": "F7"},
   {"type": "point", "point": {"sketch": "F3", "point": "p5"}}],
 "start_condition": {"type": "free"}, "end_condition": {"type": "point_sharp"},
 "centerline": {"sketch": "F9", "curves": ["c1"]}, "rails": [],
 "closed": false, "ruled": false, "operation": "new_body"}
```

- `sections`: two or more, in order: sketch regions without holes, faces
  of bodies (their outer boundary), and points (a point reference) only
  first or last. Sections of different numbers of edges are matched by
  the kernel.
- `ruled`: straight between neighbouring sections; smooth otherwise (two
  sections are straight either way; three circles give the parabola
  through their radii).
- `closed`: from the last section back to the first, without caps and
  without point sections. A smooth closed loft is tangent where it
  closes: its faces interpolate the sections periodically, which needs
  sections of as many edges, starting at corresponding points.
- `centerline`: a [path](#paths); keeps the sections at right angles to
  it on the way (the part of it between the first and the last section).
- `start_condition`, `end_condition`: how the loft leaves its first
  section and reaches its last one. `free` (default) and `point_sharp` (a
  point section's default) impose nothing. The others set the takeoff:
  the derivative of the surface across the section, from the section into
  the loft, its length given over the span to the next section (the
  derivative by the parameter from 0 at one section to 1 at the other;
  checked against the bodies stored in the reference models `loft_*`,
  kept outside the repository in `MITCAD_F3D_MODELS`):
  - `direction` (`angle`, `weight`; a sketch profile): at `angle` from
    the profile's normal (the side of the next section), tilted out of the
    profile for a positive angle, -90° < angle < 90°; `weight` times the
    distance between the sections' centres long.
  - `tangent` (`weight`; a face of a body): each face next to the section
    continued across its edge (G1); `weight` times the mean distance
    between the section's vertices and the matching ones of the next
    section long. `smooth` also with that face's curvature across the
    edge (G2).
  - `point_tangent` (`weight`; a point): a rounded tip; the surface leaves
    the point at right angles to the line to the next section's centre,
    each point of that section by `weight` times its distance from the
    line (a circle to a point is a paraboloid).

  At a section's corners the neighbouring faces leave along the line
  where their takeoff planes meet. Weights are greater than 0 and at most
  10. Slots: `start_condition.angle`, `start_condition.weight`,
  `end_condition.weight`, …

  With two sections and a condition at one end only, the free end
  follows a rule too: every point reaches it with the derivative `2 c -
  (c·d) d` (`c` the chord from the conditioned point to the matching one,
  `d` the takeoff's direction), the end of the parabola that leaves along
  `d` by `c·d`, so a direction or tangent of weight 1 straight across is
  that parabola. Above weight `w` = 1 the part along `d` is `c·d - (w -
  1) max(0, c·d - r / w)`, `r` the mean distance between the sections'
  corresponding vertices (one value for the whole section). That is `r /
  2` at weight 2, and below 0 for large weights, the curves then rising
  beyond the far section and coming back to it. The stored bodies of the
  reference models of weights 1.5, 2 and 3 straight across
  (`loft_direction_w15`, `_w2`, `_w3`, `_squares_w2`) have exactly these
  curves; tilted takeoffs above weight 1 follow the same rule, not
  checked against stored bodies. The curves across are cubic; with
  `smooth` quintic, their second derivative at the section that of the
  cubic along `d` and the neighbouring face's curvature across, and at
  the far section the cubic's. With more sections the free end follows
  the interpolation.
- `rails`: [paths](#paths) the loft's surface passes through. Each must
  meet every section (within 1e-4 mm, or 1e-6 of the sections' size) and
  run through them in order; beyond them it is ignored. A rail splits the
  sections' edges where it meets them (pieces `#k` of their faces). Each
  face is bent towards the rails by their distance from the loft without
  them, in shares that fall from 1 at a rail to 0 at the neighbouring
  rails along the section, with no slope at the sections' vertices, so a
  closed section stays smooth along its rails. A single rail moves every
  section with it (circles through one bent rail give the cylinder
  shifted along it). With end conditions too, the rails decide where they
  run.
- With end conditions or rails the loft is Mitcad's own surface
  interpolation (the sections matched edge for edge, each face's rows of
  poles interpolated with the ends' derivatives); without them the
  kernel's through-sections. The importer accepts these lofts within
  0.5 % of the file's history, with a warning beyond 0.1 %: the stored
  bodies build round sections from polynomial approximations of their
  circles, and the stored loft surfaces between them are not round (lofts
  of circles with a direction condition are stored 0.3 to 1.2 % smaller
  than the same rule on true circles); with a rail and an end condition
  together the stored surface is fitted otherwise.
- Errors: a condition that does not fit its section (`direction` at a
  sketch profile, `tangent` and `smooth` at a face, point conditions at a
  point); end conditions of a closed loft; end conditions or rails of a
  ruled one; a centre line and rails; a rail that misses a section or
  runs through them out of order. Unsupported (`unsupported: …`): end
  conditions with a centre line, rails with a point section, rails the
  matching of the sections' edges does not keep at corresponding points.

Faces: `side(<key>)` from the edges of the first section that is not a
point, keyed by segment for a region and by edge name for a face
(`F7:side(E{…})`), `#k` pieces between sections of a ruled loft;
`start(<key>)` and `end(<key>)` at the first and last sections, keyed by
the region or the face name (`F7:start(F4:end(r{c1}))`).

### pipe

```json
{"type": "pipe", "path": {"sketch": "F1", "curves": ["c1"], "chain": true},
 "section": "circular", "size": 8, "thickness": 1,
 "extent": {"type": "partial", "fraction": 0.75}, "operation": "new_body"}
```

- `section`: `circular` (default), `square` or `triangular` (equilateral);
  `size` is the circle's diameter, or the diameter of the circle round the
  square or the triangle (as for .f3d coils; not confirmed for pipes). The
  section is at right angles to the path at its start; a square's sides
  and a triangle's first corner follow the path's plane (a corner of the
  triangle along the path's normal in its plane).
- `thickness`: makes it hollow, the wall inside the section (`isHollow`
  and `sectionThickness` in .f3d designs); less than the radius of the
  circle inside the section.
- `extent`: as for a sweep from the path's start; `fraction2` only on a
  closed path (the part before the start).

Faces: `side<i>` for the section's edges (`side0` for a circle; the
square's and the triangle's sides in turn), `inner<i>` inside a hollow
pipe, `start` and `end` as for a sweep.

### coil

```json
{"type": "coil", "plane": "xy", "center": [0, 0], "diameter": 40,
 "helix": {"type": "revolutions_and_pitch", "revolutions": 3, "pitch": 10},
 "angle": 0, "clockwise": false, "section": "circular",
 "section_position": "on_center", "section_size": 4, "operation": "new_body"}
```

- `plane`: an origin plane, a construction plane or a planar face;
  `center` in its coordinates. The axis is the plane's normal; the coil
  starts on the plane on the side the plane's x axis points to and turns
  counter-clockwise about the normal unless `clockwise`.
- `helix`: `revolutions_and_height`, `revolutions_and_pitch`,
  `height_and_pitch`, or `spiral` (`revolutions`, `pitch`: flat, growing
  `pitch` in radius per turn). Height and pitch are measured at the
  section's centre.
- `angle`: a cone, positive widening as it rises (not for a spiral).
- `section`: `circular`, `square`, `triangular_external` (a corner
  pointing out) or `triangular_internal`; `section_size` the diameter of
  the circle or of the circle round the square or triangle.
  `section_position`: `inside`, `on_center` (default) or `outside` the
  diameter. The section is at right angles to the helix (not confirmed
  for .f3d designs).

Coils whose turns would overlap, or whose section reaches the axis, fail.
Faces: `side<i>`, `start`, `end`.

### helix

```json
{"type": "helix", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
 "axis": "z", "pitch": 5, "revolutions": 4, "left_handed": false, "flip": false,
 "operation": "new_body"}
```

The profiles turn about the axis (a line reference, as a revolve's) while
they move along its direction (against it with `flip`): `revolutions`
turns (need not be whole) rising `pitch` each, right-handed about the
direction of travel unless `left_handed`. Every section of the result in
a plane through the axis is the profile turned there (a screw motion; a
coil's sections are instead fixed shapes at right angles to its helix).
A height is given as a ratio: `"<height> / <pitch>"` for `revolutions` or
`"<height> / <revolutions>"` for `pitch` (as the FreeCAD import and the
application write it). `operation` and `participants` as for extrude.

`growth` (a length, optional; mitcad#59) widens the helix: the profiles
move that far out from the axis per turn (negative: in); a cone's angle
is the growth `pitch * tan(angle)`. `construction` (mitcad#83) says how a
growth is built:

- `"mitcad"` (the default for commands): the screw motion, the profiles
  also moved out along the direction from the axis towards their centre
  (across their plane's normal when the centre is on the axis) in
  proportion to the turn. Every section in a plane through the axis is
  the profile turned there and moved out, the same for every profile
  position, growth sign and direction, so the volume of a profile in a
  plane through the axis is its area times `2 pi revolutions` times its
  centre's mean distance from the axis. A narrowing that would take the
  profiles to the axis fails.
- `"freecad"` (what the FreeCAD import writes): as FreeCAD builds its
  conical and growing helices, so that they import exactly. The profiles
  follow the Frenet frame of a spiral of the same pitch and growth a
  hundred times as far from the axis, which FreeCAD raises along the axis
  by ten thousand times the profiles' height above the axis' origin when
  the growth is not positive (otherwise by that much of the origin's
  height); the profiles stay nearly in planes through the axis, and a
  narrowing helix whose profiles sit at the axis' origin runs the other
  way, as in FreeCAD.

A growing helix always stores its construction. A command without one
gives Mitcad's, or keeps the construction of the helix it edits; in files
written before the field (by the FreeCAD import, mitcad#59) a growing
helix without one is FreeCAD's, so it keeps its shape. Without a growth
both are the screw motion and the field is left out unless given.
Faces: `side(<segment>)`, `start(<region>)` at the profile and
`end(<region>)` (`geometry/src/path_sweep.cpp`).

### rib, web

```json
{"type": "rib", "curves": {"sketch": "F6", "curves": ["c1"]}, "thickness": 4,
 "thickness_location": "symmetric", "extent": {"type": "to_next"}, "flip": false,
 "participants": []}
{"type": "web", "curves": {"sketch": "F9", "curves": ["c1", "c4"]}, "thickness": 2,
 "extent": {"type": "depth", "depth": 10}}
```

- A rib grows from one open chain of sketch curves in the sketch plane, at
  right angles to the chain's chord, to its left (the sketch normal up) or
  with `flip` to its right; its thickness is across the sketch plane,
  `symmetric`, `side1` (along the sketch normal) or `side2`.
- A web grows from each curve (lines and arcs) along the sketch normal
  (against it with `flip`); its thickness is in the sketch plane, side 1
  to the left of the curve.
- `extent`: `to_next` (up to the faces of the participant bodies it
  reaches first, which must close it off; otherwise it fails) or
  `{"type": "depth", "depth": d}`. The result joins the participants (all
  bodies when left out); one that reaches none becomes a new body.

Faces of a rib: `side(<curve>)` along the curves, `wall1` and `wall2` (its
sides along and against the sketch normal), `tip0` and `tip1` at the
chain's start and end, `end` at the depth. Of a web, per curve:
`wall1(<curve>)`, `wall2(<curve>)`, `tip0(<curve>)`, `tip1(<curve>)`,
`start(<curve>)` in the sketch plane and `end(<curve>)` at the depth.

### Patterns

```json
{"type": "rectangular_pattern",
 "objects": {"type": "features", "features": ["F4"]},
 "direction1": {"axis": "x", "quantity": 3, "distance": 20},
 "direction2": {"axis": "y", "quantity": 2, "distance": 20, "symmetric": false},
 "distance_type": "spacing", "compute": "adjust", "suppressed_elements": [4]}
{"type": "circular_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
 "axis": "z", "quantity": 4, "angle": 6.283185307179586, "symmetric": false}
{"type": "path_pattern", "objects": {"type": "faces", "body": "F2.b0", "faces": ["F4:side(c1)"]},
 "path": {"sketch": "F9", "curve": "c1"}, "quantity": 4, "distance": 15,
 "distance_type": "spacing", "start": 0, "flip": false, "along_path": false, "symmetric": false}
```

Patterns, mirrors, combine, moves and primitives: modules `pattern.rs`,
`mirror.rs`, `combine.rs`, `moves.rs`, `align.rs`, `scale.rs`,
`primitive.rs`. Examples: `tools/cli/tests/f4_*.json`.

- `objects`: `bodies` (each copy a new body), `features` (their tool
  bodies combined with the bodies again by the feature's operation; only
  features that leave a tool, see [Adding to the API](#adding-to-the-api),
  and fillets and chamfers, below)
  or `faces` of one body (with planar caps where they meet the body they
  bound a boss, joined, or a pocket, cut). A pattern or mirror of
  features among the `features` (a pattern of patterns) stands for its
  originals at each of its elements (the original's too, not the
  suppressed ones): the copies go to every product of the inner and the
  outer elements, the inner one applied first. A feature that the
  `features` reach at the same place more than once (a mirror of a
  feature and of a mirror or pattern whose original it is) is copied once
  there. Patterns of bodies or faces cannot be patterned again.
- Quantities include the original. `distance_type`: `spacing` between
  neighbours or `extent` from the first element to the last. A negative
  distance or angle goes the other way. A full turn spaces a circular
  pattern by angle / quantity, a partial angle by angle / (quantity − 1).
  `symmetric`: the quantity (and extent or angle) on each side of the
  original. Path distances are arc lengths along the path; `start` (0…1)
  is where the original sits on the path; `along_path` turns the copies
  with the path.
- `scale` (optional, a factor; mitcad#59): the copies grow (or shrink)
  element by element up to that factor: element e of n is scaled by
  1 + (scale − 1) · e / (n − 1) about the centre of mass of the first
  object (a feature's tool, the first body; not for faces) carried to the
  element (FreeCAD's Scaled transformation after a pattern).
- Elements: element 0 is the original. Along a direction the positions
  are 0, 1, …, q − 1, then −1, …, −(q − 1) when symmetric; a rectangular
  pattern numbers direction 1 first, element = k1 + n1 · k2.
  `suppressed_elements` leaves elements out (not 0).
- `compute` (features and faces): `adjust` (default) applies the elements
  one after another, rebuilding a feature's tool in place where the
  feature can, and skips elements that reach no body (a cut or join
  applies them in one boolean when that gives the same bodies: nothing
  split apart); `identical` and `optimized` move the feature's tool to
  every element and combine all at once. The pattern fails if no copy
  reaches a body.
- `original_bodies` (features; default false, mitcad#74): the copies of a
  feature without participants of its own act only on the bodies that
  feature changed (still there), as in `.f3d` designs, where a pattern of
  a hole leaves another body its copies reach alone; by default they act
  on the feature's participants, every body without them. The `.f3d`
  import sets it for patterns and mirrors of features.
- Names: copied faces are `<pattern>:inst<element>(<original face>)`,
  e.g. `F5:inst4(F4:side(c1))`; their edges follow. A pattern of a
  pattern names its copies of the inner copies around the inner names
  (`F6:inst1(F5:inst2(F4:side(c1)))`). Copies of bodies are
  bodies `<pattern>.b<k>`, k = (element − 1) · (number of bodies) +
  (position of the body), so suppressing an element keeps the other ids.
- The `preview` of a pattern lists `elements` (see [Preview](#preview)).

#### Fillets and chamfers among the features

A `fillet` or `chamfer` may be among the `features` of a pattern or mirror,
usually with the features whose edges it rounds (mitcad#105):

```json
{"type": "rectangular_pattern",
 "objects": {"type": "features", "features": ["F4", "F5", "F6"]},
 "direction1": {"axis": "x", "quantity": 3, "distance": 20}, "distance_type": "spacing"}
```

- It leaves no tool body, so it is not copied but applied again at each
  element, on the copies of its edges, with its own sizes (parameters)
  and options. The features go in timeline order then: a fillet after
  the copies of the features before it, whatever the order of `features`.
- The copies' edges are found by their names: in an edge name, a face
  that a copied feature made becomes that face's copy at the element
  (`<pattern>:inst<element>(…)`, inner instances of a pattern of patterns
  inside), a face of another feature stays. A chamfer between a copied
  boss `F4` and the plate `F2` it stands on, `E{F2:end(r{…})|F4:side(c1)}`,
  is repeated on `E{F2:end(r{…})|F7:inst1(F4:side(c1))}`.
- An edge whose copy no body has by that name (none of its faces copied,
  as for a fillet patterned alone over holes an earlier pattern made, or
  its other face is another face at the copy's place) is found by its
  place: the edge whose middle (within 1e-4 mm), direction and length are
  those of the original edge, on the body before the fillet, moved by the
  element's transform. An edge found neither way (a copy that reaches no
  body) is left out with the warning `<fillet> is not repeated where the
  copies lack its edges (elements …)`; faces (`faces` sets) are found by
  name only.
- The copies' new faces are the pattern's: `<pattern>:fillet(<the copy's
  edge>)`, `<pattern>:chamfer(…)`, `<pattern>:corner(…)` (e.g.
  `F7:chamfer(E{F7:inst1(F4:end(r{c1}))|F7:inst1(F4:side(c1))})`). A fillet
  among the objects that rounds a copied chamfer's edges is repeated on the
  copy of that chamfer's face the same way.
- A two-distance or distance-angle chamfer or an asymmetric fillet without
  a reference face measures its first distance on the first face of each
  edge's name; where the copy's name puts the other face first, the copy
  takes it with `flip` swapped, so that the distances stay on the faces the
  original measured them on (also with a mirror). Sets by `faces`, and
  edges found by their place, keep the sorted order of the copies' names.
- The copies go on the body that has their edges (the fillet's own body
  first), all elements in one operation; when that fails, one element after
  another. When that fails too, the pattern starts again with the features
  of one element after another (each element's copies of all the objects,
  then the next element's): where copies overlap (pockets around a hub),
  the rounding of one copy otherwise ends at the edges of the next. A
  fillet whose copies fail even so fails the pattern (`Fillet1 at
  element 2: …`), as does one whose copies find no edge at all (`no copy
  of the edges of Fillet1 is on a body`) and one that failed itself.
  `scale` with fillets or chamfers is unsupported.

### mirror

```json
{"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
 "plane": "yz", "combine": false, "compute": "adjust"}
```

A pattern of one copy, element 1 (`<mirror>:inst1(<face>)`). Mirrored
bodies are new bodies; `combine` joins each to its original when they
touch. Mirrored features reflect their finished tool (never a rebuilt
one); `original_bodies`, and fillets and chamfers among the features, as
for patterns.

The mirror image of a nearly symmetric body lies on the body almost
everywhere, nearly but not exactly, and a boolean of the whole shapes
intersects every such pair of faces (minutes for free-form faces, often
with a wrong result). `combine` therefore joins an image that differs
from its body in at most half of the faces of each only where they
differ by more than 0.1 mm (`Kernel::join_near_copy`, mitcad#88):
material of the image within 0.1 mm of the body's faces is left out, and
an image that differs nowhere by more gives the body unchanged. Any other
image is joined whole. `MITCAD_NO_NEAR_COPIES=1` joins every image whole
(for comparisons).

### combine

```json
{"type": "combine", "target": "F2.b0", "tools": ["F4.b0"], "operation": "join",
 "keep_tools": false, "new_component": false}
```

`join` gives the target one body of the union (several solids when they
do not touch); `cut` removes the tools' union, the first piece in
geometric order keeping the target's id and the others becoming bodies of
the combine; `intersect` keeps the common volume as the target (fails
when empty). Tools are removed unless kept; faces keep their names
through the boolean. With `new_component` the target goes into a new
component.

The target is a body of the combine's component; a tool may be a body of
another component when `tool_links` names where it is seen (mitcad#104):

```json
{"type": "combine", "target": "F7.b0", "tools": ["F2.b0"], "operation": "cut",
 "keep_tools": true, "tool_links": {"F2.b0": {"source": "O1", "target": "O2"}}}
```

Each link is an `OccurrenceLink` ([Geometry of other
components](#geometry-of-other-components)): `source` the occurrence path
to the tool's component, `target` the one to the combine's (default: its
first path). The tool is read there and moved into the combine's
coordinates by the occurrences' placements at the combine's point of the
timeline, so the combine follows when either occurrence moves; a consumed
tool (without `keep_tools`) leaves its own component. A link for a body
that is not a tool is refused; a tool of another component without a link
fails the combine (`body F2.b0 (from Extrude1) belongs to A; …`).

### move, align, scale

```json
{"type": "move", "bodies": ["F2.b0"], "copy": false,
 "transform": {"type": "translate_xyz", "x": 10, "y": 5, "z": 0}}
```

`transform`: `{"type": "free", "matrix": [[r00, r01, r02, tx], [r10, …],
[r20, …]]}` (a rotation and a translation), `translate_xyz` (`x`, `y`,
`z`), `translate_along` (`axis`, `distance`), `rotate` (`axis`, `angle`,
right-handed), `point_to_point` (`from`, `to`), `point_to_position`
(`point`, `x`, `y`, `z`). A moved body keeps its id and face names; with
`copy` the moved copies are new bodies `<move>.b<j>` with faces
`<move>:inst1(<face>)` (a copy that does not move is a copy and paste).

```json
{"type": "align", "bodies": ["F2.b0"],
 "from": {"type": "plane", "plane": {"body": "F2.b0", "face": "F2:start(…)"}},
 "to": {"type": "plane", "plane": "xy"}, "flip": false, "angle": 0}
```

`from` and `to` are `{"type": "point", "point": …}`, `{"type": "axis",
"axis": …}` or `{"type": "plane", "plane": …}`: a point goes onto a
point, line or plane; an axis onto an axis (direction and line; circle
centres meet); a plane onto a plane with the normals opposite (faces
against each other) and origins together (a face's middle). `flip` turns
the moved geometry over; `angle` turns about the target axis or normal.

```json
{"type": "scale", "bodies": ["F2.b0"], "point": "origin",
 "scale": {"type": "uniform", "factor": 2}}
```

`scale`: `{"type": "uniform", "factor"}` or `{"type": "non_uniform", "x",
"y", "z"}` along the model axes; factors greater than zero. A non-uniform
scale turns faces into B-splines (names stay).

### box, cylinder, sphere, torus

```json
{"type": "box", "plane": "xy", "corner": [0, 0],
 "length": 30, "width": 20, "height": 10, "symmetric": false, "operation": "new_body", "participants": []}
{"type": "cylinder", "plane": …, "center": [u, v], "diameter": 20, "height": 30, "symmetric": false, "operation": …}
{"type": "sphere", "plane": …, "center": [u, v], "diameter": 30, "operation": …}
{"type": "torus", "plane": …, "center": [u, v], "diameter": 40, "section_diameter": 10,
 "position": "on_center" | "inside" | "outside", "operation": …}
```

Placed on the plane at plane coordinates (the frame a sketch on the plane
has), height or axis along the plane's normal (negative height: against
it). `operation` and `participants` as for extrude. Faces:
`<feature>:bottom`, `top`, `side0`..`side3` for a box (side0 at the
corner's y, side1 at x + length, counter-clockwise) and `side0` for
curved faces.

### construction_plane, construction_axis, construction_point

```json
{"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 10}}
```

Named `Plane1`, `Axis1`, `Point1`; they make no bodies. Their result is a
datum, which follows parameter edits and is cached like a body.

| Feature | `definition.type` | Fields | Result |
|---|---|---|---|
| plane | `offset` | `plane`, `distance` | the plane moved along its normal |
| plane | `angle` | `line`, `angle`, `plane` | through the line, the plane turned about the line (right-handed) |
| plane | `tangent` | `face` (cylinder or cone), `angle`, `plane` | touching the face where the plane's normal, turned about the axis, leaves it |
| plane | `midplane` | `plane1`, `plane2` | halfway (parallel: the first's normal) or the bisector with normal n1 - n2 |
| plane | `two_edges` | `line1`, `line2` | lines in one plane; normal d1 × d2 |
| plane | `three_points` | `point1`, `point2`, `point3` | normal (p2 - p1) × (p3 - p1), x along p2 - p1 |
| plane | `edge_and_point` | `line`, `point` | normal d × (p - o) |
| plane | `tangent_at_point` | `face`, `point` | tangent where the face is nearest to the point |
| plane | `along_path` | `path`, `distance`, `physical` | normal to the path at a fraction (`physical`: a length in mm) |
| plane | `normal_at_point` | `path`, `point` | normal to the path where it is nearest to the point |
| plane | `fixed` | `origin`, `x_axis`, `y_axis` (numbers) | a non-parametric plane (`setByPlane` in .f3d designs, the importer's fallback) |
| axis | `circular_face` | `face` (cylinder, cone, torus) | its axis (along its largest component) |
| axis | `perpendicular_at_point` | `face`, `point` | the face normal at the face point nearest to the point |
| axis | `normal_to_face_at_point` | `face`, `point` | through the point, along that normal |
| axis | `two_planes` | `plane1`, `plane2` | their intersection, along n1 × n2 |
| axis | `two_points` | `point1`, `point2` | from the first to the second |
| axis | `edge` | `edge` (a line) | along the edge |
| axis | `fixed` | `origin`, `direction` | a non-parametric axis |
| point | `point` | `point` | a vertex or another point |
| point | `two_edges` | `line1`, `line2` | where the lines (extended) meet |
| point | `three_planes` | `plane1`, `plane2`, `plane3` | their common point |
| point | `center` | `entity` (circle edge, sphere or torus face) | its centre |
| point | `edge_and_plane` | `line`, `plane` | where the line meets the plane |
| point | `along_path` | `path`, `distance`, `physical` | as the plane along a path |
| point | `fixed` | `point` | a non-parametric point |

Fractions and lengths beyond a path's ends continue straight along the
end tangent. A plane's frame (`x_axis`, `y_axis`) is what a sketch on it
uses: offsets and angles move the reference frame; other planes take the
model X axis projected onto them (Y when X is along the normal). Inputs
that do not fit (parallel lines, collinear points, lines that pass each
other) fail the feature with a message. The `preview` of a construction
feature adds its `datum`.

### base

```json
{"type": "base", "source": "bracket.step",
 "bodies": [{"name": "Bracket", "color": [0.8, 0.8, 0.8],
             "brep": {"format": "occt", "compression": "zlib", "size": 6146, "data": "eJy1V..."}},
            {"name": "Scan", "mesh": true, "brep": {…}}],
 "operation": "new_body", "participants": []}
```

Bodies without history: imported files, the bodies of .f3d files and the
.f3d importer's fallback (`features/base.rs`). Normally created with
`import_file` or the [shape API](#base-features-from-shapes), not written
by hand.

- `bodies`: B-rep data (OCCT's binary format, zlib-compressed, base64;
  `compression` may be `"none"`) with an optional `name` and `color`
  (sRGB, 0..1); `mesh` marks a mesh body (triangles only, from STL or
  OBJ). In a project the data is a reference into the project's store
  (`sha256` instead of `data`, see [Project files](#project-files)).
- `operation`: `new_body` (default; body i is `<feature>.b<i>`, named
  after its `name`, made unique with ` (2)`, …), or `join`, `cut`,
  `intersect` with `participants` (all bodies when empty), with the same
  results as an extrusion's. Mesh bodies can only be new bodies; placed
  by a moved or turned occurrence they are measured and reported where
  the design shows them, like other bodies.
- `replaces`: `["F2.b0", …]` (new bodies only): body i takes the id and
  display name of the i-th listed body, so later features that use that
  body continue on the new one; listed bodies beyond the new ones are
  removed. The importers' fallbacks use it.
- Faces are `<feature>:import(<j>)`, j counting the faces of all bodies in
  order (body 0's first) in the kernel's face order, so the names stay as
  long as the data does. Edges and vertices follow:
  `E{F4:import(0)|F4:import(3)}`.

### Component features

```json
{"type": "component_from_bodies", "bodies": ["F2.b0"]}
{"type": "move_occurrence", "occurrences": ["O2"],
 "transform": {"type": "translate_xyz", "x": 0, "y": 0, "z": 30}}
{"type": "capture_position", "positions": [{"occurrence": "O2",
 "transform": [[1, 0, 0, 0], [0, 1, 0, -50], [0, 0, 1, 0]]}]}
```

`move_occurrence` takes the motions of `move` (rigid ones only) and is
named `Move<n>` like moves of bodies; `capture_position` is
`Position<n>`. See [Components and occurrences](#components-and-occurrences).

## Commands

`command` takes one command object (`{"cmd": "<name>", …}`) or an array
of them. An array runs in order, each command its own undo step, and
stops at the first failure (`commands[2]: …`); the result is the array of
results.

Every result has `"recomputed"`: the number of features the command
evaluated (results from the cache, including a preview's, are not
counted), and `"error"`: the first failed feature (`"Fillet1: …"`) or
null. A command that took results from the [result store](#result-store)
has `"restored"`, their number. A failed feature does not reject the
command: it is skipped, and the `timeline` query shows its error.

### Timeline and parameters

| Command | Fields | Result |
|---|---|---|
| `add_feature` | `def`, `name` (optional), `component` (optional: a component's uid or name; the active component when left out) | `uid`, `name`, `parameters` (created) |
| `edit_feature` | `uid`, `def` (same type) | `parameters` (created) |
| `delete_feature` | `uid`, `dependents` (default false) | `deleted`: uids in timeline order |
| `suppress_feature` | `uid`, `suppressed` (default true) | |
| `reorder_feature` | `uid`, `index` (new timeline index) | |
| `rename_feature` | `uid`, `name` | |
| `rename_body` | `uid` (body), `name` | |
| `set_marker` | `position`: the number of features before the marker | |
| `add_parameter` | `name`, `value` (number in mm, or an expression) or `expression`, `unit` (optional), `comment` | |
| `set_parameter` | `name`, and any of: `value` (number in mm or rad, written in the parameter's unit, or an expression) or `expression`, `unit`, `comment`, `favorite` | `changed`: parameters whose values changed |
| `rename_parameter` | `name`, `new_name` | |
| `delete_parameter` | `name` (only when no feature and no other parameter uses it) | |
| `set_units` | `length`: the document's length unit (`mm`, `cm`, `m`, `in`, `ft`, …) | `changed` |
| `group_features` | `features` (uids of a run of features next to each other in the timeline, in any order, none in a group yet), `name` (default the next free `Group<n>`) | `name` |
| `ungroup` | `name` | the features stay |
| `rename_group` | `name`, `new_name` | |
| `recompute` | | |
| `sketch.*` | see [Sketch commands](#sketch-commands) | |

- New features go in at the timeline marker, which moves after them.
- `delete_feature` refuses while other features refer to the feature
  (`Extrude1 is used by Fillet1; …`); with `"dependents": true` they go
  too (the `dependents` query lists them).
- `reorder_feature` refuses a position where a feature would come before
  something it refers to (the `can_reorder` query asks first).
- Features after the marker are rolled back (not evaluated); suppressed
  features are skipped.
- After a command, new bodies get display names (`Body1`, `Body2`, … the
  next free number), which are saved.
- A timeline group stays a run: deleted features leave it, a feature
  added or moved between its members joins it, a member moved away
  leaves it (the only member takes the group along), an empty group goes.
  Saved as `"groups": [{"name", "features"}]`; a favourite parameter as
  `"favorite": true`.

### Undo, revisions and the saved state

| Command | Fields | Result |
|---|---|---|
| `undo`, `redo` | | `label` of the step, or null |
| `merge_undo` | `depth`: the undo depth before the action, `label` | merges the undo steps after `depth` into one step named `label` (clears redo); `undo_depth`. Refuses a `depth` past the last step |
| `mark_saved` | | `revision`; the definition state is the project file's (saved or opened). No undo step, nothing recomputed |

- Undo and redo restore the whole definition (parameters, timeline,
  marker, body names). Cached results make them fast.
- One user action is often several commands (a rectangle snapped to the
  origin is `sketch.add_point`, `sketch.rectangle` and a `coincident`
  constraint; a drag a `sketch.drag` per mouse move). The caller reads
  `undo_depth` (`document` query) before the action, runs its commands
  and merges them, or undoes back to that depth to take the action back.
- Every accepted command gives the state a new revision, also one that
  recomputes nothing (a rename, a named view); a rejected command, a
  command that changes nothing, a recompute and a preview keep it. Undo
  and redo take back the revision of the state they restore, so undoing
  to the saved state is unmodified again; `merge_undo` keeps the current
  revision. A new command after undo gets a new revision even when it
  makes the same definition. `update_links` gives a new revision when it
  takes new bodies (no undo step).
- A new document is not `modified`. A project file read
  (`Document::from_json`, the bridge's `load_document`) is modified until
  `mark_saved`, since it need not be the file it will be saved to (an
  imported design, recovered work). Saving (`to_json`) does not mark it
  saved by itself.
- `Document::to_json_with_marker(marker)` (bridge `to_json_with_marker`)
  writes the project file with another marker without changing the
  document (autosave while an edit has rolled the timeline back).

### Visibility and display

| Command | Fields | Result |
|---|---|---|
| `set_feature_visible` | `uid` (a sketch or a construction plane, axis or point), `visible` | undo step `Show Sketch1` / `Hide Plane1` unless nothing changes; nothing recomputed |
| `set_body_visible` | `uid`, `visible` | |
| `set_body_material` | `uid`, `material` (an id below, or null for the default, steel) | |
| `set_body_appearance` | `uid`, `appearance` (an id of the library or the document, see [Appearances](#appearances), or null for the default look) | |
| `set_origin_visible` | `visible` | the root's Origin folder shown; undo step `Show Origin` / `Hide Origin` unless nothing changes; nothing recomputed |
| `set_isolation` | `items`: `{"body": "F3.b0", "occurrence": "O1"}` (a body, in the occurrence that places it, none for the root's) or `{"occurrence": "O1/O4"}` (an occurrence by its path); none: unisolate | undo step `Isolate 2 item(s)` / `Unisolate`; an occurrence that is not there is refused |

- Body commands are undo steps (`Hide Body1`, `Show Body1`, `Set Material
  of Body1`, …) unless nothing changes; the body must exist at the
  marker. The `bodies` query and the project file list `"visible":
  false`, `"material"` and `"appearance"` when set. Materials: `steel`
  (7.85 g/cm³, the default), `stainless_steel`, `cast_iron`, `aluminum`,
  `copper`, `brass`, `titanium`, `abs`, `pla`, `nylon`, `polycarbonate`,
  `glass`, `water` (`core/model/src/analysis.rs`).
- Sketches and construction features: by default a sketch is hidden
  while a feature uses it (a profile, curves or points; suppressed
  features and those after the marker count too) and shown while none
  does; construction geometry is shown. A light bulb set with
  `set_feature_visible` wins; it is kept and saved with the feature
  (`"visible": true` or `false`) only where it differs from the default,
  so showing a hidden sketch that no feature uses forgets it.
- The command that gives a sketch its first consumer or takes its last
  one away forgets the sketch's light bulb in its own undo step: the new
  extrusion's sketch hides, and shows again when that extrusion is
  deleted, even if it was hidden by hand meanwhile. A second consumer
  keeps a light bulb set by hand. Deleting a feature forgets its
  visibility; `paste_new` and copied designs start with the defaults.
  Code: `document/browser.rs`.
- The display state (Origin folder, isolation) is per user: the project
  file keeps it as `"display": {"origin": true, "isolated": [...]}` (left
  out when nothing is set), and a project keeps it outside the project
  file ([Project files](#project-files)). Isolation lists only the items
  there at the marker; none left isolates nothing.
- Occurrences have `set_occurrence_visible` ([Components and
  occurrences](#components-and-occurrences)).

### Named views

| Command | Fields | Result |
|---|---|---|
| `add_named_view` | `name` (the next free `NamedView<n>` when left out), `eye`, `target` (points, mm), `up`, `perspective` (default false), `height` (mm of the model the view shows at the target), `replace` (default false: a name in use is refused) | `name`; undo step `Add Named View Detail` (`Change Named View Detail` when replaced) unless nothing changes |
| `rename_named_view` | `name`, `new_name` | |
| `delete_named_view` | `name` | |

Named views change no geometry and recompute nothing; they are saved in
the project file (`"views"`, left out when there are none). The view
named `Home` is the document's home view.

### Analyses

Section analyses kept in the document (mitcad#41,
`core/model/src/document/analyses.rs`): the browser's Analysis folder.

| Command | Fields | Result |
|---|---|---|
| `add_analysis` | `def`, `name` (the next free `Section<n>` when left out) | `name`; undo step `Add Section1`. It is shown and the others are hidden |
| `edit_analysis` | `name`, `def`, `visible` (optional: true shows it and hides the others, false hides it; as it was when left out) | undo step `Edit Section1` unless nothing changes |
| `set_analysis_visible` | `name`, `visible` | undo step `Show Section1` / `Hide Section1` unless nothing changes; showing hides the others |
| `rename_analysis` | `name`, `new_name` | undo step `Rename Section1` |
| `delete_analysis` | `name` | undo step `Delete Section1` |

A `def` is `{"type": "section", "plane": <plane reference>, "offset": mm,
"flip": bool}` (`offset` default 0, `flip` default false): the bodies cut
at `plane` moved by `offset` along its normal, the side the normal points
to cut away; `flip` reverses the normal. The plane is a [plane
reference](#references-to-geometry) (origin plane, construction plane,
planar face, fixed plane); `add_analysis` and `edit_analysis` refuse one
that is no plane at the marker, and other fields than these.

The `analyses` query lists them in the order added: `name`, `type`,
`visible`, the `def`'s fields, and `section`: {`origin`, `normal`} (the
plane in design coordinates at the marker, offset and flip applied; the
origin is the plane's origin, see [References to
geometry](#references-to-geometry)), or `error` when the plane cannot be
found there (a face of a body that is gone or rolled back). A face
reference follows the model by its topological name, as features' do.

- At most one analysis is shown at a time: one section cuts the bodies.
  Showing one hides the others, also in a file read with several shown
  (the first shown stays).
- Analyses change no geometry and recompute nothing. They are saved in
  the project file (`"analyses": [{"name", "visible" (only when false),
  "type", "plane", "offset", "flip"}]`, left out when there are none;
  files without it have none), and the comparison of versions lists them
  by name.

### Appearances

How bodies look (mitcad#46, `core/model/src/appearance.rs`,
`core/model/src/document/appearances.rs`): a physically based parameter
set, a subset of OpenPBR's and the Principled BSDF's. Mitcad's library is
built in and read-only; a document keeps appearances of its own, saved in
the project file. A body names its appearance by id
(`set_body_appearance`), and single faces can have their own
([Appearances of faces](#appearances-of-faces), mitcad#53).

| Parameter | Range | Default | |
|---|---|---|---|
| `base_color` | sRGB [r, g, b], 0–1 | [0.8, 0.8, 0.8] | a dielectric's diffuse colour, a metal's reflection, the tint of transmitted light |
| `metalness` | 0–1 | 0 | 1 for metals |
| `roughness` | 0–1 | 0.5 | 0 a mirror finish |
| `specular` | 0–1 | 1 | the weight of a dielectric's specular reflection |
| `transmission` | 0–1 | 0 | light going through (glass, clear plastic) |
| `ior` | 1–5 | 1.5 | index of refraction |
| `coat`, `coat_roughness` | 0–1 | 0, 0.03 | a clear coat over the base (lacquer, car paint) |
| `emission`, `emission_color` | 0–1000, sRGB | 0, [1, 1, 1] | emitted light: 1 is the colour's own radiance |
| `opacity` | 0–1 | 1 | a cut-out (glass transmits instead) |
| `texture` | `{"path", "size": [w, h] mm (default 100 x 100), "rotation" rad, "projection": "box" (default) or "planar", "data"}` | none | an image for the base colour (PNG or JPEG), projected in the body's coordinates: `planar` along Z, `box` from the axis each point of the surface faces most; one repeat of the image is `size`, turned by `rotation` about the projection's axis. `path` is relative to the project file's folder or absolute; `data` (base64 of the image file, at most 32 MB) embeds the image in the project file ([Textures](#textures)). The rendered view draws it, the shaded view shows the base colour |

| Command | Fields | Result |
|---|---|---|
| `create_appearance` | `id` (letters, digits, `_`, `-`; the next free `custom<n>` when left out), `name` (`Appearance<n>` when left out), `based_on` (a library or document appearance whose parameters it starts from; the defaults above without it), any parameters | `id`; undo step `Create Appearance Red` |
| `edit_appearance` | `id`, `name` and any parameters (`"texture": null` removes the texture) | undo step `Edit Appearance Red` unless nothing changes; the library's are refused (copy one with `based_on`) |
| `delete_appearance` | `id` | `bodies`: those that used it, which get the default look, and `faces` (`{"body", "face"}`) that used it, which get their body's, in the same undo step `Delete Appearance Red` |

- Ids are unique among the library's and the document's appearances, and
  names too; parameters out of range and unknown fields are refused.
- The `appearances` query lists the library's appearances in the
  application's order, then the document's: `id`, `name`, the
  parameters, `library` (true for the built-in ones), `display_color`
  (sRGB: the colour of the shaded view and exports; the base colour),
  `bodies` (the uids of bodies that use it, also those not at the marker)
  and `faces` (`{"body", "face"}` of faces that use it).
- Library: `steel_satin`, `aluminum_anodized`, `brass_polished`, `copper`,
  `cast_iron`, `paint_red`, `paint_blue`, `paint_green`, `paint_yellow`,
  `paint_black`, `paint_white`, `plastic_black`, `plastic_orange`,
  `rubber`, `glass`, `wood_oak` (their colours as before mitcad#46),
  `chrome`, `gold_polished`, `steel_polished`, `plastic_red`,
  `plastic_white`, `plastic_blue`, `plastic_clear`, `glass_clear`,
  `glass_frosted`, `wood_walnut`, `wood_maple`, `emissive_white`.
- A body without an appearance, or with an id in neither list (an
  imported one), has the default look; a body's physical material does
  not choose its appearance.
- The commands change no geometry and recompute nothing. The project file
  keeps the document's appearances as `"appearances": [{"id", "name",
  parameters...}]` (left out when there are none; parameters left out of
  a file are the defaults; files without it have none), and the
  comparison of versions lists them by id.

### Appearances of faces

Faces of a body can have appearances of their own, which override the
body's (mitcad#53, `core/model/src/document/bodies.rs`): a label area, a
painted face, a rubber pad.

| Command | Fields | Result |
|---|---|---|
| `set_face_appearance` | `uid` (body), `faces` (topological names of faces of the body at the marker, `F2:side(c1[c4,c2])`), `appearance` (an id, or null: the faces show the body's again) | undo step `Set Appearance of Face of Body1` / `Set Appearance of 2 Faces of Body1` / `Clear Appearance of Face of Body1` unless nothing changes; faces that are not there and names that are no face names are refused |
| `clear_face_appearances` | `uid` | undo step `Clear Face Appearances of Body1` unless the body has none |

- A face appearance names its face as assigned (the canonical text of
  the name) and follows it as features' face references do: a later
  feature that changes the face keeps the name, all pieces of a split
  face (`#k`) keep it. When a change renames the face (a sketch edit after
  which a side face's segment ends on another curve: `F2:side(c1[c4,c2])`
  becomes `F2:side(c1[c4,c5])`), the name finds the face of the same
  feature and role whose key has the most curves in common with it (for a
  side face: the same curve), as a profile's region key does; the name
  kept is still the one assigned. A face that is gone finds nothing and
  its appearance is kept for when it comes back (undo).
- The `bodies` query lists them per body as `face_appearances`: `[{"face"
  (as assigned), "appearance", "faces" (a name of each face it finds at
  the marker; empty when the face is gone)}]`, left out when there are
  none.
- They recompute nothing. The project file keeps them with the body's
  other attributes: `"face_appearances": {"F2:side(c1[c4,c2])":
  "paint_red"}` (left out when there are none; files without them have
  none); copied designs rename the feature ids in the names. The
  comparison of versions lists them with the body.

### Textures

An appearance's texture is an image file or an image embedded in the
project file (mitcad#53).

- `"data"` in a `create_appearance` or `edit_appearance` texture embeds the
  image: base64 of a PNG or JPEG file (checked by its first bytes; at most
  32 MB). `path` then names the file it came from. The `appearances` query
  lists an embedded texture with `"embedded": true` and `"image_sha256"`
  (the digest of the image file) instead of its data; that form sent back
  keeps the embedded image (`"embedded": true` without `data` keeps the
  image the appearance has, or that of `based_on` for a new one; refused
  when there is none), so the application changes a texture's size or
  rotation without sending the image again. A texture without `data` and
  `embedded` reads its file again.
- The `appearance_image` query (`id`) gives an embedded image: `data`
  (base64), `format` (`png`, `jpeg`) and `sha256`.
- The project file keeps the image in the texture's `data`; the
  comparison of versions shows `sha256 <digest>` for it.

### Render settings

How the rendered view (`docs/rendering.md`) lights and shows the design
(mitcad#47, `core/model/src/render_settings.rs`), kept per document so a
design keeps its look. Sections of fields with defaults: the four below
and `output`, the final render's ([Render output](#render-output),
mitcad#48); later sections are added the same way, and files without a
section have its defaults. The user's lights are a list beside them
([Render lights](#render-lights), mitcad#54).

| Field | Values | Default | |
|---|---|---|---|
| `environment.preset` | `studio`, `studio_white`, `studio_dark`, `outdoor`, `image` | `studio` | built-in light setups (procedural, no image files): `studio` soft key, fill and top lights in light grey surroundings; `studio_white` bright white surroundings, very soft light; `studio_dark` dark surroundings, a key and two rim lights; `outdoor` a clear sky and the sun; `image` an HDR image all around |
| `environment.strength` | 0–100 | 1 | a factor on all of the environment's light |
| `environment.rotation` | rad, within two turns | 0 | turns the studio's lights or the image about Z (counter-clockwise from above) |
| `environment.sun_elevation` | 0–pi/2 rad | pi/4 | `outdoor`: the sun's height above the horizon |
| `environment.sun_azimuth` | rad, within two turns | 5/4 pi | `outdoor`: where the sun is, counter-clockwise from X seen from above (5/4 pi: front left) |
| `environment.image` | a path | none | `image`: an equirectangular `.hdr` or `.exr` file, relative to the project file's folder or absolute; kept when another preset is chosen |
| `background.mode` | `view`, `color`, `environment` | `view` | behind the bodies: the view's own background (View > Environment), `background.color`, or the environment itself |
| `background.color` | sRGB [r, g, b], 0–1 | [1, 1, 1] | |
| `ground.shadows` | bool | true | a ground that shows only the bodies' shadows (a shadow catcher); without it there is no ground |
| `ground.reflections` | bool | false | the ground is glossy and catches the bodies' reflections too (with `shadows`) |
| `ground.height` | mm | none | the ground's Z; none: under the lowest body |
| `film.exposure` | -10–10 stops | 0 | each stop doubles the light |
| `film.view_transform` | `standard`, `filmic`, `neutral` | `standard` | from light to screen colours: `standard` sRGB, brighter than white clips; `filmic` a filmic curve (soft highlights, contrast in the darks); `neutral` base colours stay as they are under white light, only highlights are compressed |

| Command | Fields | Result |
|---|---|---|
| `set_render_settings` | per section (`environment`, `background`, `ground`, `film`, `output`) an object of the fields that change; `null` gives a field its default (none for `image` and `height`) | `changed`; undo step `Change Render Settings` unless nothing changes |
| `reset_render_settings` | | `changed`; the defaults, undo step `Reset Render Settings` unless they are already |

- Values out of range, unknown fields and sections are refused with the
  section's name (``ground: unknown field `depth` ``); nothing changes then.
- The commands change no geometry and recompute nothing.
- The `render_settings` query gives every section with every field (`image`
  and `height` only when set), and `lights`.
- The project file keeps them as `"render": {"environment": {...},
  "background": {...}, "ground": {...}, "film": {...}}` when they are not
  the defaults (fields left out of a file are the defaults); the
  comparison of versions lists each changed field as a document change
  (`render.environment.preset studio -> outdoor`).

### Render lights

Lights of the user's own (mitcad#54), besides the environment's: the
render settings' `lights`, a list in the `render_settings` query (`[]`
without any), saved with the other sections (files without it have no
lights) and changed by commands of their own. Lengths in mm, angles in
radians, Z up. Every light keeps every field; those of other kinds are
ignored.

| Field | Values | Default | |
|---|---|---|---|
| `id` | letters, digits, `_`, `-` | `light<n>` | what the commands name it by; cannot change |
| `name` | text | `Light<n>` | |
| `type` | `point`, `spot`, `area`, `sun` | `point` | a point (or small ball) shining all around; a point light in a cone; a glowing rectangle or disc (soft light); parallel light from far away (no position) |
| `enabled` | bool | true | off: kept, lights nothing |
| `space` | `world`, `camera` | `world` | `camera`: `position` and `direction` are relative to the camera (x to the right, y up, z toward the viewer, from the eye): the light follows the view |
| `position` | [x, y, z] mm, within 10⁷ | [0, 0, 200] | where it is (not for `sun`) |
| `direction` | [x, y, z], not zero | [0, 0, -1] | where it shines (spot, area, sun); kept as a unit vector |
| `color` | sRGB [r, g, b], 0–1 | [1, 1, 1] | |
| `power` | 0–10⁶ | 5 (sun 2) | point, spot, area: watts (a 5 W point light 300 mm away gives 4.4 W/m² where it falls straight, about the default studio's key light); sun: its irradiance in W/m² |
| `size` | 0–10⁶ mm | 20 | point, spot: the ball's diameter (0: hard shadows); area: the width, or the disc's diameter |
| `size_y` | 0–10⁶ mm | 20 | area: the rectangle's height |
| `shape` | `rectangle`, `disc` | `rectangle` | area |
| `spot_angle` | more than 0, at most pi | pi/4 | spot: the cone's full angle |
| `spot_blend` | 0–1 | 0.15 | spot: how softly the cone's edge fades |
| `angle` | 0–pi/2 | 0.02 | sun: its disc's angular diameter (0: hard shadows) |

| Command | Fields | Result |
|---|---|---|
| `add_render_light` | the light's fields (`type` sets the default `power`), all optional; `id` when not the next free `light<n>` | `id`; undo step `Add Light <name>` |
| `edit_render_light` | `id`, the fields that change (`null`: the field's default) | `changed`; undo step `Change Light <name>` unless nothing changes |
| `delete_render_light` | `id` | undo step `Delete Light <name>` |

- At most 64 lights; ids are unique. Values out of range, unknown fields
  or kinds and unknown ids are refused with the light's place
  (``lights.light1.power must be between 0 and 1000000, got -1``);
  nothing changes then. `set_render_settings` has no `lights`;
  `reset_render_settings` removes them with the other settings.
- The comparison of versions lists a light added or removed
  (`render.lights.light1 (none) -> Light1 (area)`) and its changed fields
  (`render.lights.light1.power 40 -> 60`).
- The model only keeps the lights; the rendered view and the final
  render light the design with them (`docs/rendering.md`, "Lights").

### Render output

The final render to an image file (mitcad#48; File > Render Image and
`mitcad-cli render`, `docs/rendering.md` "Final render"): the render
settings' fifth section, `output`, with the same rules as the others
(`set_render_settings` with `"output": {...}`, null for a field's
default; saved when not the defaults, so files of mitcad#47 open with
them; compared field by field, `render.output.format png -> exr`).

| Field | Values | Default | |
|---|---|---|---|
| `output.width` | 16–16384 px | 1920 | the image's width |
| `output.height` | 16–16384 px | 1080 | the image's height with `aspect` `fixed` |
| `output.aspect` | `view`, `fixed` | `view` | `view`: as high as the view is in proportion (the command line, without a view, takes width x height); `fixed`: width x height, and the view shows the image's frame |
| `output.samples` | 1–65536 | 128 | samples per pixel (adaptive sampling can stop converged pixels earlier) |
| `output.time_limit` | 0–86400 s | 0 | stop after this long even before the samples are done; 0: no limit |
| `output.denoise` | bool | true | denoise once the samples are done |
| `output.transparent` | bool | false | an alpha channel instead of the background: only the bodies and their shadows (not for JPEG) |
| `output.format` | `png`, `png16`, `jpeg`, `exr` | `png` | PNG (8 or 16 bits) and JPEG as the view shows the render (exposure and view transform, sRGB); OpenEXR (half floats) the render's linear light without exposure and view transform, alpha premultiplied |
| `output.quality` | 1–100 | 90 | JPEG's quality |

- Width, height, samples, time limit and quality out of range are refused
  with the field (``output.samples must be between 1 and 65536, got 0``).
- The model only keeps these settings; the application and `mitcad-cli`
  render with them.

## Queries

`{"query": "<name>", …}`; the result is JSON. Bodies are those at the
timeline marker.

| Query | Fields | Result |
|---|---|---|
| `document` | | `features`, `marker`, `bodies` (count), `undo`, `redo` (labels or null), `undo_depth`, `units` {`length`, `angle`}, `warnings` (what loading the file changed, such as renamed parameters), `revision`, `modified` (not the state last marked saved), `display` {`origin`, `isolated`}; with components `components`, `occurrences` (counts), `active_component` |
| `timeline` | | `marker`, `groups` (when any: `name`, `features` in timeline order) and `features`: `uid`, `name`, `type`, `suppressed`, `status` (`ok`, `warning`, `error`, `suppressed`, `rolled_back`, `pending` before the first recompute), `error` (the error, or the warnings joined with `; `), `component` (when not the root), for a sketch before the marker `dof` (0 when fully constrained), for a sketch or construction feature `visible` and `visible_set` (its light bulb was set) |
| `feature` | `uid` | `uid`, `name`, `type`, `suppressed`, `status`, `error`, `visible`, `visible_set`, `warnings` (when any), `def` (parameters by name) |
| `parameters` | | `name`, `expression`, `unit` (`""` for unitless), `value` (mm or rad), `text` (the value in its unit, `"12.5 mm"`), `comment`, `kind` (`user`, `model`), `owner` (the feature of a model parameter), `dependencies` (names its expression uses), `favorite` |
| `evaluate` | `expression`, `kind`: `length` (default), `angle` or `unitless` | `value` (mm or rad), `text` (in the document's unit, `"40 mm"`), `references` (parameter names used), `expression` (with decimal points: `"d1 * 1,5"` gives `"d1 * 1.5"`) |
| `appearances` | | the library's and the document's appearances with their parameters ([Appearances](#appearances)) |
| `appearance_image` | `id` | an appearance's embedded texture image: `data` (base64), `format`, `sha256` ([Textures](#textures)) |
| `render_settings` | | the document's render settings, every section and field ([Render settings](#render-settings)) |
| `bodies` | `properties`, `volumes` | `uid`, `name`, `component` (when not the root's), `visible`, `material`, `appearance`, `face_appearances` ([Appearances of faces](#appearances-of-faces)) (when set); with `"properties": true` also `kind` (`solid`, `sheet`: faces or open shells, `mesh`, `empty`), `volume`, `area`, `center`, `bbox` {`min`, `max`} (in the component's coordinates), `faces`, `edges` (counts); with `"volumes": true` only `volume` (cheaper) |
| `profiles` | `hashes` | `sketch`, `sketch_name`, `region`, `consumed` (an extrude uses it); with `"hashes": true` also `hash` (16 hex digits of the region's geometry and sketch frame: the same while the profile's face would be the same) |
| `faces` | `body` | per face: `names`, `surface` (`plane`, `cylinder`, …), `area` |
| `edges` | `body` | per edge: `name`, `curve` (`line`, `circle`, …), `length` |
| `dependents` | `uid` | `dependents`: `uid`, `name` of the features that refer to it, directly or through others, in timeline order (what `delete_feature` with `dependents` deletes too) |
| `can_reorder` | `uid`, `index` | `ok`, and `error` (the reason `reorder_feature` would give) when not |
| `sketch` | `uid` | see [Sketch commands](#sketch-commands) |
| `threads` | | threads on the bodies (thread features and tapped holes): `feature`, `name`, `body`, `face`, `standard`, `designation`, `class`, `right_handed`, `modeled`, `major_diameter`, `minor_diameter`, `pitch`, and from the face `internal`, `radius`, `start`, `end` (points on the axis where the thread begins and ends) |
| `thread_sizes` | `standard` (`iso_metric`, `unified`, `whitworth`, `npt` or `tyre_valve`; all when left out) | `standards`, ISO metric first, then Unified, Whitworth (bolt sizes, then the pipe sizes `G 1/4`), NPT and tyre valve threads: `standard`, `title` (the thread type's name), `default` (true for ISO metric), `sizes` (smallest first: `size` as the table names it, `10`, `1/4`, `#10`; `major_diameter` mm; `designations`, the coarse pitch or UNC first, each one `thread` and `hole` accept), `classes_external`, `classes_internal`, `default_class_external` (6g, 2A, Medium, Standard), `default_class_internal` (6H, 2B, Medium, Standard) |
| `named_views` | | in the order added: `name`, `eye`, `target`, `up`, `perspective` (when true), `height` |
| `recompute_times` | | the features the last recompute evaluated (cache hits left out), in timeline order: `uid`, `name`, `type`, `ms` (wall time of its evaluation); and `ms`, their sum |
| `changes_since_saved` | | `known` (the state last marked saved is on the undo or redo stack, or is this one), `steps`: oldest first, the labels of the undo steps made since (`Change d3`), or `Undo <label>` for those undone since, leaving out those that changed only the display state; empty when nothing else changed or not known (never saved, a step dropped by a new command after undo) |
| `report` | | `timeline` and `bodies` with properties; with components also `components` and `instances` (with properties) |

Other queries: [analysis](#analysis-queries), [components](#component-commands-and-queries),
`dxf_info` ([DXF](#dxf-into-sketches)), `cache` ([Caches](#caches)).

- `evaluate` reads the expression as a value slot of that kind does: a
  bare number takes the document's unit of the kind (`"90"` as an angle
  is 90 degrees); a result of the wrong dimensions, a syntax error or an
  unknown name rejects the query with the reason.
- `report` as text is `Document::report(false)` (what `mitcad-cli`
  prints).
- `mitcad-cli info <file> --timings` prints the slowest features after
  opening a project; the ctests `cli.perf_recompute` and
  `cli.perf_join_recompute` do so for generated models
  (`tools/cli/tests/perf_model.json`, `perf_join_pattern.json`: a pattern
  joined near bodies it does not touch).
- For picking, C++ asks the shape directly: `Shape::name_of_edge`,
  `Shape::names_of_face` and `Shape::find_edges`
  (`geometry/include/mitcad/geometry/shape.hpp`).

### Analysis queries

| Query | Fields | Result |
|---|---|---|
| `datums` | `origin` (also the origin datums) | per datum: `uid`, `name`, `type` and its geometry |
| `datum` | `uid` (`F3` or `xy`) | `type` `plane`: `origin`, `normal`, `x_axis`, `y_axis`; `axis`: `origin`, `direction`; `point`: `point` |
| `properties` | `bodies` (default all), `density`, `densities` {uid: g/cm³} | `bodies`: `uid`, `name`, `material`, `density`, `volume`, `area`, `mass`, `center_of_mass`, `inertia` (3 × 3, at the centre of mass), `principal_moments`, `principal_axes`; `total`: `volume`, `area`, `mass`, `center_of_mass`, `inertia` |
| `measure` | `a`, `b` (optional) selections | `a`, `b`: `kind` and `volume`, `area`, `length`, `point`, `circle` {`center`, `axis`, `radius`, `sweep`} as they apply; with `b` also `between`: `distance`, `on_a`, `on_b`, `inside`, `angle` (null unless planar faces or straight edges) |
| `interference` | `bodies` (default all), `min_volume` (mm³, default 1e-6) | `bodies` (count), `pairs`: `a`, `b`, `names`, `volume` |
| `section` | `plane`, `bodies` (default all) | `plane`, per body `edges`, `faces`, `length`, `area`, and the totals |
| `compare_step` | `file`, `bodies`, `step_body` (name or index), `samples`, `fuzzy`, `max_deviation`, `max_relative` | `step_bodies`, `volume`, `step_volume`, `volume_only_mitcad`, `volume_only_step`, `relative_difference`, `max_deviation`, `mitcad_to_step` and `step_to_mitcad` {`max`, `rms`, `at`, `samples`}, `bounds_difference`; with limits `pass` and `exceeded` |

- Units: mm, mm², mm³, g/cm³, kg, kg mm², radians. Density:
  `densities[uid]`, else the body's material, else `density` for bodies
  without one, else steel.
- A selection is `{"body": "F2.b0"}` with an optional `"face"`, `"edge"`
  or `"vertex"`, `{"datum": "F5"}`, or text: `"Body1"`, `"F2.b0"`,
  `"Body1/F2:end(r{c5})"`, `"xy"`, `"F5"`. A section `plane` is a plane
  reference.
- Kernels without an analysis answer with an error (`the geometry kernel
  does not support …`).
- `Document::analysis(query, json)` (C++ `analysis`) answers
  `properties`, `measure`, `interference`, `section`, `compare_step` and
  `datums` as text (`mitcad-cli`) or pretty JSON.

`analysis_shape(request)` returns a shape handle for display, null when
there is none:

| `shape` | Fields | Shape |
|---|---|---|
| `datum` | `uid` (`F3`, `xy`), `size` (default 100) | a square face, an edge or a vertex |
| `section` | `plane`, `body`, `curves` (default false) | the section faces, or curves |
| `clip` | `plane`, `body` | the body without the half-space on the plane's normal side |
| `interference` | `a`, `b` (bodies) | their overlap |

## Preview

`preview` takes an `add_feature` or `edit_feature` command and evaluates
it on a copy of the document without committing it. The result:

```json
{"uid": "F5", "name": "Extrude3", "status": "ok", "error": null, "warnings": [],
 "bodies": [{"uid": "F2.b0", "name": "Body1", "changed": true}], "removed": [], "tool": true}
```

- `bodies`: all bodies at the marker after the command (`changed`: new or
  modified by it); `removed`: the bodies it removes; `warnings`: what a
  feature that succeeded built with caveats (the `status` stays `ok`).
- A pattern adds `elements`: every element's transform as 3 rows of 4 by
  element number, suppressed ones too. A construction feature adds its
  `datum`.
- A command that places occurrences elsewhere (a joint, a
  `move_occurrence`; mitcad#55) adds `placements`: each occurrence whose
  placement in the design changes, `path` (uids from the root, `O1/O4`;
  an occurrence inside a moved one is listed too) and `transform` (4x4,
  in the design), so a preview can show the bodies where it puts them.
- `preview_body_shape(uid)` returns a body as the command leaves it and
  `preview_tool_shape()` the extrusion before its boolean.
- The next command or `clear_preview` drops the preview; committing the
  same command reuses its results.

## Scripts

`run_script` (`mitcad-cli run`) takes an array of steps: commands,
comments (`{"comment": "…"}`) and expectations (`{"expect": {…}}`), which
check the document after the steps before them. Examples:
`tools/cli/tests/*.json`.

| Expectation | Checks |
|---|---|
| `"bodies": n` | bodies at the marker |
| `"body": "Body1"` (name or uid) | `"volume"`, `"area"` (relative `"tolerance"`, default 1e-6), `"faces"`, `"edges"` (counts), `"has_face"`, `"has_edge"`, `"no_edge"` (names), `"min"`, `"max"` (bounding box corners, the same tolerance), `"bbox": {"min", "max"}`, `"center"` (of mass; `"tolerance"` in mm) |
| `"total_volume"` | all bodies |
| `"occurrence"` (a path, or an occurrence placed once) with `"body"` | the body as placed: `volume`, `area`, `center`, `bbox`, `min`, `max` |
| `"instances"`, `"world_volume"` | the visible placed bodies: count, sum of volumes |
| `"feature": "Fillet1"` (name or uid) | `"status"`, `"error_contains"` |
| `"parameter": "d3"` | `"value"` |
| `"sketch": "Sketch1"` (name or uid) | `"regions"` (its region keys in any order), `"dof"` |
| `"query": {…}` | `"result"`: the query's answer contains it (objects: the keys given; arrays: element by element; numbers within the relative `"tolerance"`, default 1e-6), e.g. `{"expect": {"query": {"query": "datum", "uid": "F3"}, "result": {"origin": [0, 0, 30]}}}` |

The first failing step stops the script: `steps[4]: expected Body1 to have
volume 48000, got 47961.37`.

## Sketch commands

Every `sketch.*` command but `sketch.create` names its sketch with
`"sketch": "F1"` and is one undo step (`Add Line to Sketch1`). An edit
works on a copy of the definition: the sketch is solved from the edited
positions with the current parameter values and the solution is stored.
An edit after which the sketch no longer solves is rejected, naming the
constraints in conflict, as is a constraint or driving dimension the
others already imply (add the dimension driven instead).

A point input is `[x, y]` (a new point) or `"p3"` (an existing point,
which joins the new curve to it). Values follow [Values](#values); a new
dimension gets a model parameter owned by the sketch (`Sketch1 fillet
radius`). Tool sizes (`radius`, `width`, `count` …) are plain numbers: a
tool only draws, and dimensions are added separately.

Results of editing commands: `entities` (new points and curves),
`constraints`, `dimensions` (new ids), `parameters` (created), `texts`,
`removed` (when anything was removed), `made` (the main curves the tool
made, in order) and `status` (as the `sketch` query: `dof`,
`fully_constrained`, `redundant`, `conflicts`).

| Command | Fields | Notes |
|---|---|---|
| `sketch.create` | `plane` (default `xy`; `xz`, `yz`, a construction plane `"F5"` or `{"face": …, "body": …}`), `frame`, `name` (optional), `occurrence` and `context` (optional: a plane of another component, see [Geometry of other components](#geometry-of-other-components)) | result `uid`, `name` |
| `sketch.add_rectangle` | `corner` [x, y], `width`, `height` (values) | a dimensioned rectangle: fixed first corner, horizontal and vertical constraints, `width` and `height` dimensions; result also `curves`, `region` |
| `sketch.add_circle` | `center` [x, y], `diameter` (value) | fixed centre and a diameter dimension; result also `curves`, `region` |
| `sketch.add_point` | `at`, `fixed` | |
| `sketch.add_line` | `start`, `end` (point inputs), `construction`, `centerline` | |
| `sketch.rectangle` | `mode`: `two_point` (`a`, `b` corners), `three_point` (side `a`–`b`, `c` on the opposite side), `center` (`center`, `corner`); `construction` | four joined lines (counter-clockwise), horizontal and vertical (perpendicular for `three_point`), a construction diagonal with the centre at its midpoint for `center` |
| `sketch.circle` | `mode`: `center` (`center` point input, `radius`), `two_point` (`a`, `b`: a diameter), `three_point` (`a`, `b`, `c`), `two_tangent` (`lines` [2], `radius`, `near`), `three_tangent` (`lines` [3]); `construction` | tangent constraints for the tangent modes |
| `sketch.arc` | `mode`: `three_point` (`start`, `through`, `end`), `center` (`center`, `start`, `end`: the direction of the end, `clockwise`), `tangent` (`from`: an end point of a curve, `end`); `construction` | `tangent` adds a tangent constraint |
| `sketch.polygon` | `center`, `vertex`, `sides` (3–256), `inscribed` (default true; false: `vertex` is a side's middle), `construction` | joined lines, equal, on (or tangent to) a construction circle |
| `sketch.slot` | `mode`: `center_to_center` (`a`, `b`), `center_point` (`center`, `end`: an end's centre), `arc` (`center`, `start`, `end`); `width` | two end arcs, two tangent sides, a construction centre line or arc with the arc centres at its ends, equal end arcs |
| `sketch.ellipse` | `center` (point input), `major` (end of the major axis), `minor_radius`, `start` and `end` (point inputs; an elliptical arc), `construction` | |
| `sketch.spline` | `points` (point inputs), `degree` (optional), `construction` | without `degree` a cubic through the points; with it, a B-spline with these control points |
| `sketch.add_text` | `text`, `at` (or `frame`), `height`, `angle`, `font`, `bold`, `italic`, `align`, `valign`, `spacing`, `frame` (three point ids, or `{"corner": [x, y], "diagonal": [x, y]}`: a new construction rectangle, turned by `angle`), `path`, `flip_x`, `flip_y` | the glyphs bound profiles |
| `sketch.edit_text` | `id`, and any of `text`, `at`, `height`, `angle`, `font`, `bold`, `italic`, `align`, `valign`, `spacing`, `path`, `flip_x`, `flip_y` | |
| `sketch.add_constraint` | `constraint`: as in the definition, without `id` | |
| `sketch.add_dimension` | `dimension`: as in the definition, without `id`, `value`, `driven`; `value` (default: what it measures now), `driven`, `text` [x, y] | |
| `sketch.set_dimension` | `dimension` (`k3`), `value` | a number or an expression changes the dimension's own parameter; a parameter name makes the dimension use that parameter; a driven dimension becomes driving |
| `sketch.set_driven` | `dimension`, `driven` (default true) | driving again keeps the measured value |
| `sketch.set_dimension_text` | `dimension` (`k3`), `text` [x, y] | where the value is shown; undo step `Move Dimension in Sketch1` |
| `sketch.remove` | `items`: `p3`, `c4`, `t5`, `k6` | a point takes the curves that use it, a curve its points no other curve uses; constraints and dimensions on removed entities go too |
| `sketch.set_construction` | `curves`, `construction` (default true) | |
| `sketch.set_centerline` | `lines`, `centerline` (default true) | |
| `sketch.set_fixed` | `entities`, `fixed` (default true) | |
| `sketch.drag` | `entity`: a point with `to` [x, y], or a curve with `by` [dx, dy] or `radius` | pulls as far as the constraints allow; a derived offset's curve or point pulls its source chain by as much |
| `sketch.move` | `entities`, `by` [dx, dy] or `rotate` {`center` (point input), `angle`}, `copy` | a move keeps the constraints (the points are dragged); a copy copies the constraints among the entities; a derived offset's curves move their source chain, their copies are ordinary curves; `made` lists the copies |
| `sketch.trim` | `curve`, `at` [x, y] | removes the piece around `at` between the curves crossing it; a curve nothing crosses is deleted |
| `sketch.extend` | `curve`, `at` | extends the end nearest `at` (a line or an arc) to the next curve; an arc extended all the way round becomes a circle |
| `sketch.fillet` | `a`, `b` (lines), `radius` (value) | a tangent arc with a radius dimension; the corner stays as a point on both lines, and a length dimension of either line becomes a distance to it, so dimensions keep their meaning |
| `sketch.chamfer` | `a`, `b`, `distance`, and `distance2` or `angle` (values) | a line and its distance (and distance or angle) dimensions from the kept corner |
| `sketch.offset` | `curves` (a chain of joined curves, or one closed curve), `distance` (value) | to the right of the chain direction (a closed curve: positive is out); one parameter for the distance; an `offsets` record (derived with ellipses and splines); result `constraints` [the offset's id] |
| `sketch.edit_offset` | `offset` (`k8`), `distance` (value), `flip` | the distance changes, `flip` moves it to the other side; the curves keep their ids |
| `sketch.mirror` | `entities`, `axis` (a line) | copies with symmetric constraints; points on the axis are shared |
| `sketch.circular_pattern` | `entities`, `center` (point input; a new point is fixed), `count` (originals included), `angle` (value, default a full turn) | a `patterns` record; result `constraints` [the pattern's id] |
| `sketch.rectangular_pattern` | `entities`, `direction` (default [1, 0]), `count` [n1, n2], `spacing` [s1, s2] (values) | as above |
| `sketch.edit_pattern` | `pattern` (`k5`), and any of `count` (n, or [n1, n2]), `angle`, `spacing`, `direction`, `center`, `entities` (the originals) | the copies follow |
| `sketch.project` | `source`: an edge, a face (its boundary) or a vertex of the bodies before the sketch, `body` (optional), `linked` (default false), `occurrence` and `context` (optional: geometry of another component, see [Geometry of other components](#geometry-of-other-components)) | fixed reference entities; a linked projection follows its source when the model changes |
| `sketch.import_dxf` | `path` (.dxf), `unit` (of a drawing that names none: `mm` default, `cm`, `m`, `in`, `ft`), `at` (the sketch point the drawing's origin goes to, default [0, 0]), `layers` (only the entities on these layers; all when left out; a layer with nothing on it is refused) | as other edits, and `curves`, `points` (shared points made), `text_count`, `warnings`; undo step `Insert DXF into Sketch1`; see [DXF](#dxf-into-sketches) |

The `sketch` query (`{"query": "sketch", "uid": "F1"}`) returns:

- `uid`, `name`, `plane`, `frame` (`origin`, `x_axis`, `y_axis`, `normal`
  in model coordinates, or null when the sketch did not evaluate),
  `solved`, `error`;
- `entities`: the definition's (linked projections, and what is
  constrained to them, where the last evaluation put them: their sources'
  current geometry), with solved `at` for points, `geometry`
  for curves (line `start`, `end`; circle `center`, `radius`; arc also
  `start_angle`, `end_angle`, `start`, `end`; ellipse `major_radius`,
  `minor_radius`, `rotation`; splines `degree`, `control`, `weights`,
  `knots`) and `fully_constrained`; entities a derived offset computes
  have `"derived": true`;
- `constraints`, `dimensions` (with parameter names, `measured` and
  `expression`), `projections`;
- `texts`, also `family` (the font used), `fallback` (the bundled font
  instead of the one asked for) and `outline`: `[{"id": "t9.g0.c0",
  "curves": [geometry, …]}]`;
- `patterns` and `offsets`, also `values` and `value`: `parameter`,
  `value`, `expression`;
- `regions`: `key`, `area`, `centroid`, `loops` (segment keys, outer loop
  first); a glyph's region also `text` and `letter` (false for a
  counter);
- `dof`, `fully_constrained` (entity ids), `redundant` (constraint ids
  implied by the others) and `conflicts` (sets of ids that contradict
  each other).

## Components and occurrences

The design is a root component (`C0`, named `Root` until renamed); other
components (`C1`, …) are placed in it, or in each other, by occurrences
(`O1`, …, named `<component>:<n>`, e.g. `Plate:2`). Code:
`core/model/src/assembly.rs`, commands in `document/components.rs`.
Examples: `tools/cli/tests/f6_*.json`.

- **One timeline.** Every feature belongs to a component: the active one
  when it is added (or `component` of `add_feature`). A feature works on
  the bodies of its own component, in that component's coordinates: a
  body of another component fails the feature (`body F3.b0 (from
  Extrude2) belongs to Component1; …`), and a sketch or construction
  geometry of another component is refused when the feature is added.
  Sketch planes and projections, and a combine's tools, may name another
  component's geometry with where it was picked ([Geometry of other
  components](#geometry-of-other-components)).
- **Shared definitions.** A component's bodies are built once, in its own
  coordinates; every occurrence shows them placed by its transform, and a
  component placed in another is placed again with every occurrence of
  that one. The `bodies` query lists the definitions, the `instances`
  query the bodies as placed in the design. For display, draw each
  instance's `body_shape` moved by its `transform`.
- **New components.** `create_component` makes an empty one placed in the
  active component. The operation `new_component` of extrude, revolve,
  the primitives and base features, and combine's `new_component`, make a
  component placed in the feature's component (identity), named
  `Component<n>`, holding the bodies the feature creates (a combine: its
  target); editing the operation away removes it (refused while something
  was added to it), and deleting the feature deletes it unless something
  else is in it. `components_from_bodies` adds, per body, a
  `component_from_bodies` feature in the body's component: the body moves
  into a new component named after it, where it is, keeping its id and
  name; later features on it belong to that component.
- **Placements.** An occurrence's transform is rigid (rows of a rotation
  and a translation, `[[r00, r01, r02, tx], …]`, in its parent's
  coordinates). Timeline features place occurrences of their own
  component: `move_occurrence` moves them, `capture_position` puts them
  at captured placements. `set_occurrence_transform` sets an occurrence's
  own placement while no feature before the marker places it, or with
  `"capture": true` adds a `capture_position` feature. A grounded
  occurrence does not move: the command is refused and a feature that
  would move it fails (`Plate:2 is grounded`).
- **Copies.** `copy_occurrence` places the same component again (one
  definition). `paste_new` makes a new component, a copy of the
  definition with its history: its features (new ids and default names;
  references among them follow), their own parameters (copied with their
  expressions; other parameters are shared), the components placed in it
  and, for a component a new-component operation made, that feature. A
  component made from bodies, or linked, is copied as its bodies (a base
  feature). Both go into the active component, which must not be in the
  copied one.
- **External components.** `insert_component` places another project
  file in the active component: linked (default) as a read-only
  component whose bodies are a base feature holding the file's visible
  bodies as that design places them (its link: `path`, a digest of the
  text and the feature), or with `"link": false` as a copy of the file's
  design with its history (its root becomes the new component; its user
  parameters come along, renamed `name_1` when the name is taken). Linked
  components cannot be activated or edited. `update_links` (with the
  project file's directory as `base` for relative paths) reads the linked
  files again and takes the bodies of those that changed; a missing file
  keeps the saved bodies. Callers run it when they open a project (no
  undo step; what changed is a load warning).
- **Deleting.** `delete_occurrence` deletes an occurrence; the last one
  of a component deletes the component with its features, the feature
  that made it and what depends on them, and the components only it
  contained (a component made from bodies gives them back to its parent).

### Component commands and queries

| Command | Fields | Result |
|---|---|---|
| `create_component` | `name` (optional), `transform` (optional), `activate` (default true) | `component`, `occurrence`, `name` |
| `components_from_bodies` | `bodies` | `components`: `feature`, `component`, `name` |
| `activate_component` | `component` (uid or name; `C0` the root) | `component` |
| `rename_component` | `component`, `name` (unique) | |
| `ground_occurrence` | `occurrence`, `grounded` (default true) | |
| `set_occurrence_visible` | `occurrence`, `visible` | |
| `set_occurrence_transform` | `occurrence`, `transform`, `capture` (default false) | `feature`: the capture, or null |
| `copy_occurrence` | `occurrence`, `transform` (optional) | `occurrence`, `name` |
| `paste_new` | `occurrence`, `transform` (optional) | `component`, `occurrence`, `name` |
| `delete_occurrence` | `occurrence` | `deleted`: features |
| `insert_component` | `path` or `library` ([Component libraries](#library-parts)), `link` (default true), `transform`, `name`, `base` (directory of relative paths) | `component`, `occurrence`, `name` |
| `update_links` | `base` (optional) | `messages` |

| Query | Fields | Result |
|---|---|---|
| `components` | | `active`; `components` (the root first): `uid`, `name`, `created_by`, `link` (path), `library` (a library part's record, or null), `features`, `bodies`, `occurrences` (count); `occurrences`: the tree from the root, each `uid`, `name`, `path`, `component`, `transform` (rows, in the parent, at the marker), `world` (4x4 into the design), `grounded`, `visible`, `children` |
| `instances` | `properties`, `hidden` (default false) | the bodies as placed: `path` (occurrence uids), `occurrence` (path name, empty for the root's), `component`, `component_name`, `body`, `name`, `visible`, `transform` (4x4); with `properties` the placed body's `volume`, `area`, `center`, `bbox` |

- An occurrence is given by its uid (`O3`), its name (`Plate:2`) or its
  path from the root (`Arm:1/Pin:2` or `O1/O4`: the last one).
- A `transform` is 3 or 4 rows of 4 numbers (a 4x4 matrix ends with `[0,
  0, 0, 1]`), or `{"translation": [x, y, z], "rotation": {"axis": [x, y,
  z], "angle": a}}` (both optional; the rotation about the origin comes
  first).
- Each command is an undo step (`New Component Plate`, `Ground Plate:1`,
  `Hide Plate:1`, `Paste Plate:2`, `Paste New Plate (1)`, `Insert
  part.mitcad`, …); hiding, renaming and activating recompute nothing.
- The text report (`mitcad-cli info`) lists the components and the placed
  bodies.
- Project file (version 2 and later), when there are components:
  `root_component` (the root's name when renamed), `components` (`uid`,
  `name`, `created_by`, `link`), `occurrences` (`uid`, `component`,
  `parent`, `number`, `transform` rows unless the identity, `grounded`,
  `"visible": false`), `active_component`, and a feature's `component`
  when it is not the root. Files without them load with every feature in
  the root component.

## Joints

Joints connect occurrences and say how they may move against each other
(mitcad#55). They are timeline features of the component they are added
in (the active one, or `component`), recomputed in order like
`move_occurrence` and `capture_position`, and edited, suppressed,
reordered and deleted like other features (`edit_feature` takes the `def`
the `feature` query gives). Code: `core/model/src/joints.rs` (kinds,
frames, what recompute does), `features/joint.rs` (definitions),
`document/joints.rs` and `api/joints.rs`, the joint solver
`core/solver/src/rigid.rs` ([README](../../../solver/README.md#joints-rigid-bodies));
tests `joint_tests.rs`, `joint_solver_tests.rs`, `core/solver/tests/rigid.rs`,
`tools/cli/tests/joints.json`.

Joints are stored, checked, reported and solved (below), the
application's ASSEMBLE group adds, edits, drives, drags and animates them
([app/COMMANDS.md](../../../../app/COMMANDS.md), "Joints"), and the
`.f3d` import brings the file's joints in as these features
([.f3d import with the timeline](#f3d-import-with-the-timeline), *From
the dump IR*).

### Kinds and frames

A joint joins origin `a` to origin `b`. Each origin resolves to a frame
(an origin and x, y, z axes) on geometry of an occurrence's component,
moved into the joint's component by the occurrences' placements. The joint
holds when

`frame_a = frame_b · motion(values) · Rz(angle) · Tz(offset) · flip`

`motion` is the kind's free motions at some values, along and about frame
`b`'s axes, composed in the order listed; `flip` turns frame `a` over
(half a turn about its x axis). Without `flip` the two z axes point the
same way: to put a face on a face (both outward normals), flip.

| Kind | Free motions | Values |
|---|---|---|
| `rigid` | none | |
| `revolute` | turns about z | `rz` |
| `slider` | slides along `slide_axis` (`x`, `y` or `z`, default `z`) | `tx`, `ty` or `tz` |
| `cylindrical` | slides along z, turns about it | `tz`, `rz` |
| `pin_slot` | slides along x, turns about z | `tx`, `rz` |
| `planar` | slides along x and y, turns about z | `tx`, `ty`, `rz` |
| `ball` | turns about z, y and x | `rz`, `ry`, `rx` |

Slides are millimetres, turns radians (right-handed).

**Origins.** `{"occurrence": "O1/O4", "geometry": <reference>,
"frame_override": {...}}`. `occurrence` is a path of occurrences from the
joint's component (uids in definitions; `add_joint` also takes names,
`Arm:1/Pin:2`), empty or left out for the component's own geometry (which
never moves). `geometry` is a [reference](#references-to-geometry) to
geometry of the component the path ends in, resolved there:

| Geometry | Origin | z axis |
|---|---|---|
| planar face | its middle (`Kernel::face_plane`) | the outward normal |
| cylindrical or conical face | the axis point nearest to the face's middle | the axis (along its largest component) |
| toroidal face | the centre | the axis |
| spherical face | the centre | the component's z |
| circular edge | the centre | the axis (along its largest component) |
| straight edge | the middle | along the edge |
| vertex, sketch point, construction point, the origin, a fixed point | the point | the component's z |
| sketch circle or arc | the centre | the sketch's normal |
| sketch line, construction or origin axis, fixed axis | the start or the axis's origin | along it |
| origin plane, construction plane, joint origin, fixed plane | its origin | its normal (its x axis kept) |

Other x axes are the model X projected across z (Y when z is along X).
`frame_override` replaces parts of the frame, in the component's
coordinates: `origin` [x, y, z], `z_axis`, `x_axis` (projected across z).
A body is no origin. A sketch or construction geometry of another
component than the path's is refused when the joint is added
(``a: Sketch1 is in Plate, not in Pin``); a body of another component
fails the joint at recompute (`body F2.b0 is not in Pin at this point of
the timeline`).

### Feature definitions

```json
{"type": "joint", "kind": "revolute",
 "a": {"occurrence": "O2", "geometry": {"body": "F4.b0", "face": "F4:start(r{…})"}},
 "b": {"occurrence": "O1", "geometry": {"body": "F2.b0", "face": "F2:end(r{…})"}},
 "offset": 0, "angle": 0, "flip": true,
 "limits": {"rz": {"min": "-90 deg", "max": "90 deg", "rest": "30 deg"}},
 "position": {"rz": "45 deg"}}
{"type": "as_built_joint", "kind": "revolute", "a": "O2", "b": "O1",
 "origin": {"occurrence": "O2", "geometry": "F9"},
 "relative": [[1, 0, 0, 100], [0, 1, 0, 0], [0, 0, 1, 0]], "limits": {}}
{"type": "joint_origin", "geometry": {"body": "F2.b0", "face": "F2:end(r{…})"},
 "frame_override": {"origin": [5, 5, 10]}, "offset": 0, "angle": 0, "flip": false}
{"type": "rigid_group", "occurrences": ["O2", "O3"]}
```

- `joint` (named `Joint<n>`): `kind`, `slide_axis` (sliders only), `a`,
  `b`, `offset` (along z, a length), `angle` (about z), `flip`, `limits`,
  `position`; `offset`, `angle` and `flip` default to 0 and false. The
  two origins must be on different occurrences, and at most one may be
  the component's own geometry.
- `limits`: per free motion of the kind, `{"min", "max", "rest"}`, each
  optional and a [value](#values) (parameters and expressions: a turn's
  slots are angles, `limits.rz.min_angle`, a slide's lengths,
  `limits.tz.min`). min ≤ rest ≤ max is checked when the joint is added or
  edited, and a parameter change that breaks it fails the joint.
- `position`: per free motion of the kind, the value it is driven to (a
  [value](#values): `position.rz.position_angle`, `position.tz.position`),
  within its limits (checked like `rest`). `drive_joint` and
  `drag_occurrence` set it.
- A free motion is **driven** (held at a value) by its `position`, else
  by its `rest` value; without either it is **free**: it moves as the
  joints need, stays within its limits, and otherwise stays where it is.
  Driven values say where a motion is, not whether it can move: degrees
  of freedom leave them out.
- `as_built_joint` (`AsBuiltJoint<n>`): two occurrence paths `a` and `b`
  joined where they are: `relative` is `a`'s placement in `b`'s
  coordinates when it was made (rows of a rotation and a translation;
  `add_as_built_joint` records it; without it the joint takes the
  placements as they are at its point of the timeline). `origin` (optional,
  an origin as above) is the frame of the free motions, `b`'s coordinates
  without it; an origin on `a`'s side is taken where the recorded
  relation puts it. The joint holds when `a`'s displacement from where
  the recorded relation puts it is the kind's motions in that frame
  (values 0 at the recorded relation). Same kinds, limits and position.
- `joint_origin` (`JointOrigin<n>`): construction geometry of its
  component, a frame on `geometry` (as an origin's) with `frame_override`,
  then moved by `angle` about and `offset` along its z axis and turned
  over by `flip`. Its datum is a plane with that frame (the `datums`
  query; a sketch can be placed on it); joints use it as `"F9"`. It has a
  light bulb like construction geometry.
- `rigid_group` (`RigidGroup<n>`): two or more occurrences placed in the
  feature's component that move as one from its point of the timeline:
  the joints after it move them together, and a `move_occurrence` of one
  member moves the others by the same motion (`capture_position` sets
  each occurrence it lists). A group with a grounded member stays where
  it is: a move of a member fails (`Base:1 is grounded`).
- Joint features recompute nothing of the bodies and are never written
  to the result store.

### What recompute does

A **unit** is a top-level occurrence of the joint's component (the one
its path starts with) with those its rigid groups join to it; it moves
as one rigid body. A unit is fixed when a member is grounded; the
component's own geometry (an empty path) is fixed too.

At its point of the timeline, a joint resolves its frames (in the
components its paths end in, with that point's bodies, sketches and
construction geometry) and the joint solver solves it together with the
joints in effect before it in the same component that share moving units
with it, directly or through each other, from where the occurrences are
at that point:

- What needs no iteration is placed first, in timeline order: a joint
  between a side that is fixed (or joined to something fixed by the
  joints before it) and one that is not moves the free side, with every
  unit joined to it so far, so that the joint holds at its driven values
  and, for free motions, at the values nearest to where it is; between
  two free sides side `a` moves onto side `b`. An open chain needs
  nothing else.
- Joints that close a loop are iterated: the units move as little as
  possible (turns weighed by the size of the joints' layout) until every
  joint holds, driven motions at their values and free motions within
  their limits.
- The units that moved get placement changes of the joint feature (each
  occurrence's placement set), applied as `move_occurrence`'s are; the
  occurrences' own placements stay the starting ones (`state` `placed`,
  `moved`; `satisfied` when nothing had to move). Placements set by
  joints are not cached: grounding, ungrounding and moving an occurrence
  recompute them.
- Joints whose sides cannot move (both fixed, or both in one unit) only
  hold or fail: `both sides are fixed (Pin:1 and Plate:1) and the joint
  does not hold`, `both sides move with Pin:1, Pin:2 and the joint does not hold`, or `… is not
  at its position`.
- Joints that contradict each other fail the joint feature, which moves
  nothing: `over-constrained: Joint1 (at its rz position), Joint2 and
  Joint4 cannot all hold` (the rank analysis of the joints' equations,
  newest last; `at its … position` names driven values in the
  contradiction). A loop that cannot close from where the occurrences are
  (lengths or limits keep it apart) fails with `the joints cannot all
  hold (Joint1, Joint2, Joint3 and Joint4) from where the occurrences
  are: …`. The joints after it are solved without it.
- A path below a top-level occurrence only locates geometry: the joint
  moves the top-level occurrence (the whole sub-assembly), never an
  occurrence inside it; joints inside a sub-assembly are the
  sub-component's own features, solved in its coordinates.
- At the marker each joint reports its frames and values where the
  occurrences are; a later feature that moved a side (a
  `move_occurrence`) leaves `values` null and `solved` false.

A joint whose origins cannot be resolved (an occurrence that is gone, a
face that is no longer there, a body of another component) fails like any
feature and moves nothing.

### Commands and queries

| Command | Fields | Result |
|---|---|---|
| `add_joint` | `kind`, `slide_axis`, `a`, `b` (origins; `occurrence` by uids or names from the component), `offset`, `angle`, `flip`, `limits`, `position`, `name`, `component` (the active one when left out) | `uid`, `name`, `parameters` (created), `state`, `solved` |
| `add_as_built_joint` | `kind`, `slide_axis`, `a`, `b` (occurrence paths), `origin`, `limits`, `position`, `name`, `component` | the same; `relative` recorded from the placements at the marker |
| `add_rigid_group` | `occurrences` (two or more, placed in the component), `name`, `component` | `uid`, `name` |
| `drive_joint` | `joint` (uid or name), `values` (per motion a [value](#values), or null to take the position away) | `uid`, `parameters` (created), `state`, `values`; one undo step `Drive Joint1` |
| `drag_occurrence` | `occurrence` (uid, name or path), `point` (in the parent component's coordinates, moving with the occurrence; its origin when left out), `target` | as `joint_drag`; one undo step `Drag Pin:1` |

```json
{"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "crank_angle", "tz": null}}
{"cmd": "drag_occurrence", "occurrence": "Rocker:1", "point": [130, 0, 0], "target": [140, -10, 0]}
```

- `drive_joint` edits the joint's `position`: a parameter (`crank_angle`)
  makes the mechanism follow the parameter.
- `drag_occurrence` keeps where `joint_drag` puts the occurrences: their
  own placements become the dragged ones (refused for an occurrence a
  `move_occurrence` or `capture_position` before the marker places:
  `capture the position instead`), and every driven motion that moved is
  driven to its new value (its `position` becomes the number). The
  joints before the marker then reproduce the dragged placements.

| Query | Fields | Result |
|---|---|---|
| `joints` | | `joints`: every joint and as-built joint in timeline order: `uid`, `name`, `type`, `kind`, `slide_axis` (sliders), `component`, `a` and `b` {`occurrence` (path name), `path` (uids)}, `motions`, `limits` (per motion `min`, `max`, `rest` as values), `position` (per driven motion its value), `status` and `error` (the feature's), `state` (`placed`, `satisfied`, or the status: `failed`, `suppressed`, `rolled_back`), `solved` (it holds at the marker), `message` (why not), `moved` (occurrences placed), `values` (per free motion, at the marker; null when the joint does not hold there), `within_limits`, `beyond_limits`, `frames` {`a`, `b`: `origin`, `x_axis`, `y_axis`, `z_axis` in the joint's component, at the marker; an as-built joint's motion frame as each side carries it}; `rigid_groups`: `uid`, `name`, `component`, `occurrences`, `names`, `active`, `status`, `error`; `dof`: per component with joint features, `component`, `total`, `overconstrained`, `redundant` and `conflicting` (joint uids), `units` (`occurrences` moving as one, `names`, `grounded`, `dof`, `joints`) |
| `joint_dof` | `occurrence` (uid, name or path) | in its parent component: `occurrence`, `name`, `component`, `grounded`, `group` (the occurrences moving with it), `dof`, `motions` (none when grounded, all six without joints, its joint's with one, null with more), `joints` (`uid`, `name`, `kind`, `other`, `motions`) |
| `joint_drag` | `occurrence`, `point`, `target` (as `drag_occurrence`) | `placements` (the occurrences that would move: `occurrence`, `name`, `transform` rows in the parent component), `values` (per joint uid of the parent component, its motions' values); changes nothing |
| `joint_frame` | `occurrence` (a path from `component`, uids or names; empty: its own geometry), `geometry`, `frame_override`, `component` (the root when left out) | the frame the origin gives at the marker, placed by the path into `component`'s coordinates: `origin`, `x_axis`, `y_axis`, `z_axis`; `component` (the one the path ends in) and `local` (the frame in its coordinates); refused for geometry of another component than the path's |

- Degrees of freedom are exact: the rank of the joints' equations where
  the occurrences are (the joint solver's analysis; driven values left
  out). `total` is what all units can do together (6 per unit that is not
  grounded less the rank); a unit's `dof` is what it can do with the
  other units held where they are, 0 when grounded. A four-bar linkage
  has `total` 1 while each link alone has 0. `redundant` lists joints
  whose equations repeat earlier ones (they hold: two hinges on one axis,
  the closing joint of a planar loop), `conflicting` those that
  contradict earlier ones where the occurrences are; `overconstrained`
  when either is not empty. Only joints that recomputed count.
- `joint_drag` pulls `point` toward `target` while every joint in effect
  at the marker in the occurrence's parent component holds (driven
  motions follow like free ones, limits hold), moving the other units as
  little as possible: as near as the joints let it. A grounded
  occurrence does not drag (`Crank:1 is grounded`). For a UI's drag
  preview: call it per pointer position, then `drag_occurrence` on
  release.
- `delete_occurrence` deletes the joints, as-built joints and rigid groups
  that name an occurrence that is gone, with what depends on them; they
  are in its `deleted`. Copies of components (`paste_new`, an inserted
  copy) take their joints along, naming the copies' occurrences.
- Joint features are saved as features (project file round trip); files
  without them have none.

## Import and export

### Files

| Command | Fields | Result |
|---|---|---|
| `import_file` | `path` (.step/.stp, .iges/.igs, .brep/.brp, .stl, .obj), `name` (of the feature, optional), `unit_mm` (STL and OBJ: mm per file unit, default 1) | `uid`, `name`, `bodies`: `uid`, `name`, `kind` |
| `export` | `path`, `bodies` (uids or names; all at the marker when left out), `format` (by extension when left out: `step`, `iges`, `stl`, `obj`, `brep`, `3mf`), `schema` (`ap214` default, `ap242`), `unit` (STEP, IGES: `mm` default, `cm`, `m`, `in`, `ft`), `refinement` (STL, OBJ, 3MF: `low`, `medium` default, `high` or `{"deviation": mm, "angle": radians}`), `ascii` (STL), `colors` (body uid or name: sRGB in [0, 1]), `coordinates` (`design` default, `component`), `occurrence` (a path) | `path`, `format`, `bodies` (uids); 3MF also `meshes`, `skipped` |
| `export_sketch` | `sketch` (uid or name), `path` (.dxf), `version` (`r2000` default, `r12`) | `path`, `entities` |

- `import_file` adds one base feature: a body per STEP part or solid,
  IGES solid, BRep compound member or OBJ object, one mesh body for STL.
  Bodies keep the file's names and colours; `source` is the file name.
  One undo step (`Import bracket.step`).
- `export` writes the bodies with their display names and colours
  (`colors`, else the colours of imported bodies; for STEP, IGES, OBJ,
  3MF). Mesh bodies go only to STL, OBJ and BRep. It changes nothing (no
  undo step, `recomputed` 0).
- `export_sketch` writes the sketch's curves in sketch coordinates and
  millimetres (`$INSUNITS` mm), each curve segment of its profiles once.
  Periodic splines are not written.

### Coordinates and placements of exports

Every format has the bodies where the design shows them:

- A body is placed by the shown occurrences of its component (each a copy
  in the file), by all of them when none is shown; a body of the root
  once, as it is.
- `coordinates: "component"`: each body once, in its own component's
  coordinates.
- `occurrence` (uids `O1/O4` or names `Arm:1/Pin:2`): only the placements
  by this occurrence and the occurrences in it. Listed bodies must be
  placed there (`Pin is not placed in Arm:1`); without `bodies`, the
  bodies it places. A hidden occurrence asked for is written.
- STL and 3MF: the model triangulates each body (`Kernel::triangle_mesh`
  within `refinement`: vertices shared along the faces' edges, triangles
  facing outwards; a mesh body as it is) and places the mesh. An STL file
  is one solid (named after the file) of every placed copy; binary, or
  text with `ascii`.
- The kernel's formats: `Kernel::write_file` gets each body with
  `ExportBody::placements` (each occurrence's transform, named by its
  path; none with `coordinates: component`, and a root body's one
  placement moves nothing). The geometry (`geometry/io`,
  `io::Body::placements`) writes them as locations, never changing a
  body's geometry:
  - **STEP:** when a body moves (a placement that moves it, or several),
    the file is an assembly named after the file: each body a part (its
    name and colour) with an instance per placement, named after the
    occurrence path (`Arm:1/Pin:2`); a root body is an instance that
    does not move. Otherwise the parts alone. Mitcad's own import
    flattens the assembly into a body per instance named after the part.
  - **IGES, OBJ, BRep** (and the kernel's STL): a moved copy of the body
    per placement, named after the body with the occurrence path in
    brackets when there are several (`Pin (Arm:1/Pin:2)`). A mesh body is
    moved by its location only (OBJ, BRep).

```json
{"cmd": "export", "path": "assembly.step"}
{"cmd": "export", "path": "local.stl", "bodies": ["Bracket"], "coordinates": "component"}
{"cmd": "export", "path": "pin.igs", "bodies": ["Pin"], "occurrence": "Arm:2"}
```

Tests: `core.model` (`exchange_tests.rs`: occurrences turned, moved and
copied; an occurrence alone, hidden ones, component coordinates, text
STL, errors), `geometry.io` (`test_placements`: STEP assemblies with
instance names, IGES and OBJ copies, BRep and STL, a mesh body moved),
`cli.exchange` (with OCCT: each format read back where the occurrence
places the body), `tools/ui-import-test.sh` (the Export dialog's
Coordinates row), `tools/ui-print-test.sh`.

### 3MF

```json
{"cmd": "export", "path": "part.3mf", "bodies": ["Bracket", "F4.b0"],
 "refinement": "high", "colors": {"Bracket": [0.78, 0.16, 0.16]}}
```

`export` with `format` `3mf` (or a `.3mf` path) writes a 3MF file
(`core/3mf`, the core specification, millimetres) for slicers: one build
item, an object named after the file whose components place one mesh
object per body, so slicers load the bodies as the parts of one object in
their relative positions.

- A mesh object is named after its body; a body with a colour refers to a
  base material of that colour, named after it, in one `basematerials`
  group. A component placed twice gives two components of the one mesh
  object.
- Sheet and empty bodies cannot be printed: listed in `bodies` they are
  an error (`Surface is a sheet body; 3MF takes solids and mesh bodies`);
  when all bodies are written they are left out and listed in `skipped`.
- The result adds `meshes` (per written body, in order: `body`, `name`,
  `triangles`, `volume` of the mesh in mm³, which differs from the body's
  by at most its area times the deviation) and `skipped` (`body`, `name`,
  `reason`).
- Tests: `core.3mf` (writer and reader), `core.model` (`export` with the
  mock kernel), `geometry.io` (`indexed_mesh`: closed meshes facing
  outwards within the deviation's volume), `core.exchange` and
  `cli.export_3mf` (with OCCT).

### DXF into sketches

| Command / query | Fields | Result |
|---|---|---|
| `sketch.import_dxf` | see [Sketch commands](#sketch-commands) | |
| `dxf_info` query | `path` (.dxf) | `unit` (`mm`, `in`, …, or null when the drawing names none), `unit_mm` (null when none), `entities`, `layers` (those with entities, in the layer table's order: `name`, `entities`, `visible` (false when off or frozen), `bounds`), `bounds` {`min`, `max`} of all entities, in drawing units |

`sketch.import_dxf` reads lines, arcs, circles, ellipses and elliptical
arcs, splines (control points with knots and weights, or fit points),
points and text (`mitcad-dxf`: polylines are split into lines and arcs,
block references expanded), converted to millimetres. End points and
centres closer than 1e-6 mm become one sketch point, so joined curves
stay joined; nothing is constrained or dimensioned. `warnings` lists what
the reader approximated and entities left out. A drawing with nothing a
sketch can hold is refused.

### Base features from shapes

An importer that builds bodies itself adds them without files:

- Rust: `Document::add_base_feature(BaseInput { name, source, bodies:
  vec![ImportBody { name, color, shape }], operation, participants,
  replaces })` (`core/model/src/exchange.rs`) returns the feature and its
  bodies; `add_base_features(label, …)` adds several as one undo step.
  The shapes are kernel shapes (`SharedPtr<Shape>` with OCCT), stored as
  B-rep data.
- C++: `Document::add_base_feature(const ShapeList&, json)` with
  `{"name", "source", "operation", "participants", "bodies": [{"name",
  "color"}, …]}` (all optional; `bodies` by shape index).
- `Document::import_f3d(path, json)` imports the bodies of an .f3d/.f3z
  file without history, one base feature per body (`{"history": true}`
  also takes the `.smbh` blobs, `{"owners": true}` bodies of which only
  faces are saved, `{"text": true}` returns a summary instead of JSON).
  `mitcad::bridge::f3d_bodies` (`core/cpp/bridge/exchange.hpp`, also
  callable from Rust) builds the bodies of an .f3d file into a
  `ShapeList` with their blob, record and build report.

### .f3d import with the timeline

`{"cmd": "import_f3d", "path": "part.f3d", …}` (C++ `Document::command`,
or `Document::import_f3d_timeline(path, json)` for the report alone)
imports an .f3d design with its timeline replayed as Mitcad features
([core/import/README.md](../../../import/README.md)): parameters with
their expressions, sketches, construction geometry and features, each
checked against the file's ASM history; an item that cannot be replayed
becomes a base feature holding the file's bodies after it. The design's
components and occurrences come in as Mitcad's and every item goes into
its component: the one whose bodies it changes, else the one that owns it
(the design's new-component operations are new bodies there). Occurrences
take the placements the file gives them at the end of its timeline; the
file's joints, as-built joints, joint origins, rigid groups and ground
items come in as [joint features](#joints) where they hold there (else as
as-built joints, with a warning), other assembly relationships are
skipped. One undo step (`Import part.f3d`). The bridge handles the command
(`core/ffi/src/f3d_import.rs`), not the model, because it needs the
file's bodies built with OCCT.

| Field | Meaning |
|---|---|
| `design` | the document of an `.f3z` package (its label or the part after `!`); the one with the longest timeline when left out |
| `dump` | an external dump (JSON, `core/import/SCHEMA.md` §2) replayed instead of the file's own design streams; the file still gives the bodies |
| `no_verify` | do not check the replay against the ASM history (and take only first guesses) |
| `no_fallback` | leave items that cannot be replayed out instead of using the file's bodies |
| `no_compare` | compare the final bodies by volume only, not by surface deviation |
| `time_limit` | seconds; after them the remaining items take the file's bodies without trying definitions |
| `hang_limit` | seconds without progress after which the import is taken to hang in the geometry kernel (a new document only; the import makes progress between kernel calls, also in its own long loops, so only a kernel call that does not return counts, mitcad#82): the import runs on a thread of its own and is run again with the item it hung on taking the file's bodies (hung comparing the final bodies: by volume only; elsewhere: the stored bodies without the timeline); the report warns. The try given up stops once its kernel call returns (before its next item or definition; a definition's long kernel operations stop inside) and its result is dropped (mitcad#71) |
| `text` | `import_f3d_timeline` returns a readable report |
| `report_path` | also write the JSON report to this file |
| `list` | `import_f3d_timeline` only lists the designs: `{"designs": [{"label", "items"}]}` |
| `memory_limit` | MiB the import may take besides the process's own limits (mitcad#80): low on memory (85 % of the tightest limit, always watched), the import cuts the definition being tried short and tries no more until the memory recovers; those items take the file's bodies, and the report's `low_memory` (`item`, `name`, `memory`, `items`) and a warning say so (`core/import/README.md`, *Memory*) |
| `threads` | how many threads the import uses at once (mitcad#95, mitcad#103; default 1, the application gives the logical cores up to 8): workers evaluate the definitions ranked after the one being evaluated, each on its own copy of the document, and the import takes their results in rank order; the file's bodies are read and the history's bodies built ahead of the replay on the same threads. It is the same import as with one thread, only faster (`core/import/README.md`, *Definitions evaluated in parallel*, *Work beside the replay*) |
| `learn` | a directory for the learning dump (mitcad#96): one JSON line per modelling item with its raw record, its candidates and the one the history accepted (`core/import/README.md`, *Learning the undecoded inputs*); `MITCAD_IMPORT_LEARN` when left out |

The result: `file`, `design`, `items` (timeline items), `counts` (items
per outcome: `parametric`, `partial`, `fallback`, `skipped`) and
`report`: per design `parameters` (imported, as values, renamed,
skipped, mismatched), `items` (`index`, `name`, `type`, `outcome`,
`features`, `verified`, `note`, `component` when not the root),
`history` (`states`, `built`, `matched`, `reached_end`), `bodies` (each
of the file's stored solids with `file_volume`, the replayed body it
matched, `volume_difference`, `max_deviation`, `relative_difference`;
the last is missing when the boolean differences did not finish in
time, were left out because every sample lay within 0.01 mm, or gave
more than the deviations allow),
`extra_bodies`, `warnings`, `components` (`components` made,
`occurrences` placed, `external` occurrences of other documents placed
as occurrences of empty components, `items` per component), `joints`
(`joints` that came in as joints, their `fixed_sides` and
`inserted_sides` (on components of other documents), joints
`kept_as_built`, `as_built` joints, joint `origins`, `grounded`
occurrences, captured `positions`, `skipped` items), `stopped` when
the import was stopped and `low_memory` when it ran low on memory.

Stopping: the import is not undone as a whole, so a job does not cancel
it. Run under a job (`attach_job`, [Progress and
cancellation](#progress-and-cancellation)), the job's cancel is a request
to stop early: the definition being tried is cut short, the modelling
item being replayed and the ones after it take the file's bodies without
trying definitions, as after `time_limit` (note `the import was
stopped`; sketches and construction geometry still come in), the final
bodies are compared by volume only, and the command succeeds with what
was imported. `report.stopped` has `item` (the timeline index of the
first item given up) and its `name`, no `item` when the stop came after
the last one; a warning says so too. The job's test delay slows each
definition tried. `import_f3d` without the timeline is not stopped by a
job.

#### From the dump IR

How the importer maps IR features (`core/import/SCHEMA.md` §5.3) to the
definitions above. Lengths in the IR are cm (× 10), angles rad; values
bind to parameters by name; B-rep fingerprints are resolved to Mitcad
names before the feature. A `Path` is dumped as its `PathEntity` items:
sketch curves (`sketch_entity`, the curve's imported id) →
`{"sketch", "curves"}`, edge fingerprints resolved against the replay →
`{"body", "edges"}` (a path mixing sketches or bodies is unsupported).
"Fallback": the item takes the file's bodies. Where the stream decoder
gives a combine's target and tools, a hole's points or a pattern's or
mirror's objects (mitcad#67), the definitions built from them are tried
first, then those the file's history suggests without them.

| IR `objectType` | Mitcad | Fields |
|---|---|---|
| `FilletFeature` | `fillet` | each `edgeSets[]` item a set: `ConstantRadiusFilletEdgeSet` → `constant` (`radius`), `ChordLengthFilletEdgeSet` → `chord_length`, `VariableRadiusFilletEdgeSet` → `variable` (`startRadius`, `endRadius`; `midRadii` with `midPositions` → `mid`), an asymmetric set (`offsetOne`, `offsetTwo`, `isFlipped`) → `asymmetric` (`distance1`, `distance2`, `flip`; which face takes which offset is unknown, so the other way round is a guess the history may pick); `continuity` Curvature (or legacy `isG2`) → `curvature` with `tangencyWeight` → `tangency_weight`; `isTangentChain` → `tangent_chain`; `isRollingBallCorner` → `rolling_ball_corners`; entities that are faces → `faces`, a feature → its faces. G2, asymmetric and mid radius sets and setback corners follow Mitcad's own conventions, so they are matched against the history within 0.5 %; what the kernel cannot build falls back |
| `ChamferFeature` | `chamfer` | `EqualDistanceChamferEdgeSet` → `equal_distance`; `TwoDistancesChamferEdgeSet` → `two_distances` with `reference_face` = the face left of each edge (sets split by face), `isFlipped` → `flip`; `DistanceAndAngleChamferEdgeSet` → `distance_angle` with `reference_face` = the face right of the edge, `isFlipped` → `flip`; `cornerType` `ChamferCornerType`, `MiterCornerType`, `BlendCornerType` → `corner` `chamfer`, `miter`, `blend` (miter and blend matched within 0.5 %) |
| `ShellFeature` | `shell` | `inputEntities` faces → `faces`, a body → none; `insideThickness`, `outsideThickness`, `isTangentChain`, `shellType` RoundedOffset → `rounded` |
| `DraftFeature` | | not translated (fallback) |
| `OffsetFacesFeature` (Press Pull) | `offset_face` | `faces` (one feature per body), `distance` |
| `DeleteFaceFeature` | | not translated (fallback) |
| `ReplaceFaceFeature` | `replace_face` | `targetFaces`: a construction plane → `plane`, one face → `face` (a planar face of a body the replay lacks → a fixed plane), all faces of one body or a body → `body`. Dumps do not record the faces replaced (`sourceFaces` or `inputFaces` are read when a dump has them): the history gives them, the faces of the body the next state changed whose points it no longer has on its faces; listed in full with `tangent_chain` false. Matched against the history within 0.5 %; without a history, or with a target the replay lacks (a curved face of a surface body), fallback |
| `SplitBodyFeature` | `split_body` | `splitBodies` → `bodies`, `splittingTool` → plane / face / body / sketch, `isSplittingToolExtended` → `extend`; IR body names after the step name the pieces |
| `SplitFaceFeature` | | not translated (fallback) |
| `CombineFeature` | `combine` | `targetBody` → `target`, `toolBodies` → `tools`, `operation` (Join/Cut/IntersectFeatureOperation), `isKeepToolBodies` → `keep_tools` (tools consumed: the history's copies of them are left out from the combine's state on, mitcad#96); `isNewComponent`: the combine goes into the component the import puts the item in |
| `MirrorFeature` | `mirror` | `patternEntityType` + `inputEntities` → `objects` (Faces → faces of one body, Features, Bodies; not Occurrences), `mirrorPlane` (origin `XY`/`XZ`/`YZ` → origin plane; other construction planes → a fixed plane from `geometry`; face fingerprint → face), `isCombine` → `combine` (the stream decoder's, then the other way as a guess; not given: separate copies, then joined), `patternComputeOption` → `compute` |
| `RectangularPatternFeature` | `rectangular_pattern` | `directionOne/TwoEntity` → axes (origin X/Y/Z, edge, fixed from `directionOne/Two` when the entity is null or a sketch line), `quantityOne/Two`, `distanceOne/Two`, `isSymmetricInDirectionOne/Two`, `patternDistanceType` (Extent/Spacing), `patternComputeOption`, `suppressedElementsIds` → `suppressed_elements` matched by `outputs.patternElements[].transform`; without directions (the oldest item version, mitcad#74) the history checks guesses: direction one along an origin axis, two across it at 90°, 60° or 120°, both symmetric or not (`core/import/README.md`) |
| `CircularPatternFeature` | `circular_pattern` | `axis` (an axis entity not found: the line the stream decoder's `_f3d_axis` gives), `quantity`, `totalAngle` → `angle`, `isSymmetric`, `patternComputeOption`, `suppressedElementsIds` |
| `PathPatternFeature` | `path_pattern` | `path` (a sketch line, arc or circle → `{"sketch", "curve"}`; else a fixed line), `quantity`, `distance`, `patternDistanceType`, `startPoint` → `start`, `isFlipDirection` → `flip`, `isOrientationAlongPath` → `along_path`, `isSymmetric` |
| `MoveFeature` | `move` | bodies of `inputEntities`; `transform` (rigid, cm) → `free` matrix, or `moveFeatureDefinition` (`TranslateXYZ`, `TranslateAlongEntity`, `Rotate`, `PointToPoint`, `PointToPosition`) to keep it editable; moves of faces are unsupported |
| `CopyPasteBody` | `move` with `copy: true` | `props.sourceBody`, `translate_xyz` 0, 0, 0 |
| Align (not in the API) | `move` (`free`) or `align` | |
| `ScaleFeature` | `scale` | `inputEntities` (bodies), `point` (origin point → origin, vertex, else fixed), `isUniform`, `scaleFactor`, `xScale`, `yScale`, `zScale` |
| `Box/Cylinder/Sphere/TorusFeature` | `box`, `cylinder`, `sphere`, `torus` | sizes from `parameters.model` by `createdBy`; the placement (not in the API) from the body: a fixed plane |
| `SweepFeature` | `sweep` | `profile` → `profiles` (as for extrude), `path` → `path`, `guideRail` → `guide_rail`, `orientation` (Perpendicular/ParallelOrientationType), `twistAngle` → `twist_angle`, `taperAngle` → `taper_angle`, `profileScaling` (SweepProfileScale/Stretch/NoScalingOption) → `profile_scaling`, `distanceOne`/`distanceTwo` → `extent` `partial` unless both 1, `isDirectionFlipped` → `flip` (with a guide rail the other way first; then the other direction where a partial extent or a rail makes it differ), `operation`, `participantBodies`; `guideSurfaces`, `extent` FullExtents and `isSolid` false → fallback. From the stream decoder, which gives a profile's sketch only and neither `orientation`, `profileScaling` nor `isDirectionFlipped`, the history picks among the sketch's regions, `distanceTwo` 0 as none or as given, both directions, perpendicular then parallel (scale, then stretch and none with a rail) |
| `LoftFeature` | `loft` | `loftSections[]` by `index`: `entity` a profile → `profile`, a face → `face`, a path of a body's edges (`PathEntity` items of `BRepEdge`s, or one edge; the section of tangent and smooth conditions) → `face`, the face of that body the edges go round (each of them borders it and it has no others), a sketch point, a construction point or a vertex → `point`; first and last `endCondition`: `LoftFreeEndCondition` → none, `LoftPointSharpEndCondition` → `point_sharp`, `LoftDirectionEndCondition` → `direction` (`angle`, `weight`), `LoftTangentEndCondition` → `tangent`, `LoftSmoothEndCondition` → `smooth`, `LoftPointTangentEndCondition` → `point_tangent` (`weight`); `centerLineOrRails` with `isCenterLine` → `centerline`, else `rails`; `isClosed` → `closed`; `operation`, `participantBodies` (the IR has no ruled loft). A loft with end conditions or rails is kept when it matches the history within 0.5 % (see [loft](#loft)). A profile without its area (the stream decoder gives its sketch only) is each of the sketch's regions in turn, the history picking, at most twelve choices of all sections |
| `PipeFeature` | `pipe` | `path`, `sectionType` (Circular/Square/TriangularPipeSectionType) → `section`, `sectionSize` → `size`, `isHollow` with `sectionThickness` → `thickness`, `distanceOne`/`distanceTwo` → `extent`, `operation`, `participantBodies`. Without `sectionType` and `isHollow` (the stream decoder): a solid circle first, then hollow with `sectionThickness`, square and triangular, the history picking |
| `CoilFeature` | | not translated (fallback): the sizes are known (`parameters.model` by `role`), not the coil type, section, direction and placement ([README](../../../import/README.md), *Limits*) |
| `RibFeature`, `WebFeature` | | not translated (fallback): no inputs are recorded besides thickness and depth |
| `ThreadFeature` | `thread` | `inputCylindricalFaces` (resolved through `point_on_face`) → `faces`; `threadInfo`: `threadType` ISO Metric profile and other metric `M` sizes → `iso_metric`, ANSI Unified → `unified`, BSP Pipe Threads → `whitworth` with `G <size>-<tpi>` → `G <size>`, others fallback; `threadDesignation` → `designation`, `threadClass` → `class` when Mitcad lists it for the face's side (else left out), `isRightHanded` → `right_handed`; `isModeled` true → `modeled` with `diameters` from `majorDiameter`, `minorDiameter` and `pitchDiameter` and the `angle` the next history state shows on each face (one `thread` per face; Whitworth and NPT fall back), tried first without sizing the faces; `isFullLength` false → `length` (`threadLength`), `offset` (`threadOffset`) and `location` (`threadLocation`, swapped where Mitcad's cylinder axis runs against the face's `geometry.axis`). First with an `offset_face` of each face to the thread's major (external) or minor (internal) diameter, as the file sizes threaded faces, then with a tube between the two radii (`cylinder`s and `combine`s) cut from or joined to the body, then without |
| `HoleFeature` tapped (`holeTapType` Tapped) | `hole` + `thread` | the hole with its diameter (then `tappedHoleInfo.minorDiameter`), and a `thread` on `hole<i>.wall` with the size of `tappedHoleInfo` (as for `ThreadFeature`; for `thread.isModeled` modelled with its diameters, angle 0), and for `thread.isFullLength` false `length` and `offset` from the hole's start (`low_end`); then the hole without the thread |
| `HoleFeature` | `hole` | `position` (the stream decoder: every point of `_f3d_positions`) on the planar face named by `holePositionDefinition` or through the point, `holeType` Simple/Counterbore/Countersink with `counterboreDiameter`/`Depth` or `countersinkDiameter`/`Angle`, `holeDiameter`, `tipAngle` (180° → `flat`), `extentDefinition` `DistanceExtentDefinition` → `distance`, `ThroughAllExtentDefinition` or `AllExtentDefinition` → `through_all` (others fall back; with the stream decoder's `_f3d_through_all` through all first, checked by the history), `isDefaultDirection` false → `flip`; without a position, the holes the history shows |
| `Joint` | `joint` | in the component its occurrence paths start in (`_f3d.context_component`); `occurrenceOne` → `a`, `occurrenceTwo` → `b` (`_f3d.path` of occurrence objects → occurrence uids, occurrences of components of other documents included: they are occurrences of empty components; a level inside one leaves the joint out); each side's stored frame (`_f3d.frames`, else `geometryOrOrigin*`) on geometry of the side's component that gives it (a joint origin, construction geometry, a circular edge, a planar face or a face of revolution, with a `frame_override` for the rest; on a component of another document its `"origin"`), else a fixed plane; `jointMotion` → `kind` (`slide_axis` by the motion's slot), the values the motions have where the file places the occurrences (from the frames, mitcad#87: a joint that cannot hold there is not added), `rotationLimits` → `rz`'s and `slideLimits` → the slide's `limits` where they hold those values (else left out), the values → `position` where the rest value is another; `_f3d.opposed` → `flip`, `angle` → `angle`, `offset` → `offset` (an offset the file stores rounded takes the placements' value, mitcad#81), `offsetX`/`offsetY` move `b`'s origin. Kept only where every occurrence stays where it was or goes where the file places it after the item or at the end of its timeline (the occurrences' `_f3d.placements`, else its last captured position, else its stored transform); else an `as_built_joint` of the placements with a warning ([core/import/README.md](../../../import/README.md#joints)) |
| `AsBuiltJoint` | `as_built_joint` | `occurrenceOne`/`Two` → `a`/`b`, `relative` from the placements at its point of the timeline (`_f3d.placements` checked against them, a warning where they differ), `jointMotion` → `kind`, its motion frame `origin` the recorded frame (on geometry of `b`'s component, else of `a`'s, else fixed on `b`), its limits where the recorded placement is the file's and they hold 0 (a rest value other than 0 → `position` 0) |
| `RigidGroup` (an as-built joint of motion type 11) | `rigid_group` | `occurrences` → the occurrences of its component they are or are in (members below the top level and inside components of other documents move with them); with the component's own geometry among them, rigid `as_built_joint`s of each to it |
| `JointOrigin` | `joint_origin` | in the owning component: `geometry` → geometry that gives the frame before `angle` and `offsetZ` (`"origin"` for the component's origin point, a face or edge, else a fixed plane), `angle` → `angle`, `offsetZ` → `offset`; checked against the file's frame, else that frame fixed |
| `GroundOccurrence` | | `occurrence` → the occurrence is grounded |
| `Snapshot` (captured position) | `capture_position` | one per component whose occurrences it places: `positions[]` (`occurrence` paths from the item's component, `transform` there; for a captured position of joints' values, the occurrences whose `_f3d.placements` name it) → each occurrence at its placement in its parent; positions inside components of other documents and of grounded occurrences are left out (`partial`) |
| Items the stream decoder names by their class only (`ComponentInsert`, `GeometricRelationship`, `Group`, `FlangeFeature`, ...) | | those without geometry are skipped with what they are, the others are not translated (fallback) with the reason ([core/import/README.md](../../../import/README.md#items-without-a-translation)) |

### FreeCAD import (.FCStd)

`{"cmd": "import_fcstd", "path": "part.FCStd", …}` (C++
`Document::command`, or `Document::import_fcstd(path, json)` for the
report alone; Rust `mitcad_import::freecad::import_fcstd(doc, path,
options)`, `import_fcstd_file` with a loader for linked documents)
imports a FreeCAD document's bodies in its structure, with its sketches,
history and parameters (`core/import/src/freecad/`; the file is read by
`core/freecad`). One undo step (`Import part.FCStd`). How the import
works (stages, checks, readings tried, what falls back):
[core/import/README.md](../../../import/README.md#freecad-import-fcstd).

| Field | Meaning |
|---|---|
| `bodies_only` | the bodies only (FreeCAD's results), without the history, the sketches and the parameters |
| `text` | `import_fcstd` returns a readable report |
| `report_path` | also write the JSON report to this file |
| `reference` | a dump of the document by FreeCAD (`tools/freecad-export/dump.py`) to compare the import with |
| `set_parameters` | `{"Width": "60 mm"}`: parameters changed after the import, before the report measures it (to compare with a dump of FreeCAD's document after the same change) |

What the document gets:

- Components: `App::Part` and `Assembly::AssemblyObject`, placed by their
  placements (nested ones: subcomponents; hidden ones: hidden
  occurrences). A Body with a placement of its own, and an object a link
  shows, gets a component of its own (its origin the component's) with
  an occurrence at its placement. `App::Link` → an occurrence of the
  linked object's component per array element (`ElementCount`: the
  element objects' placements, else `PlacementList`; `VisibilityList`),
  placed by the link's placement in place of the object's own
  (`LinkTransform` false) or on top of it (true); a link to another
  document reads that file next to the imported one (missing: reported,
  left out); a scaled link → a scaled copy of the body. An assembly's
  grounded joint grounds its part's occurrence; other joints are left
  out.
- Stored bodies: `PartDesign::Body` and the Part workbench's leaf objects
  (with a stored shape that no other shape-bearing object consumes) not
  rebuilt by the history (all of them with `bodies_only`) are one base
  feature per component before the timeline (`FreeCAD bodies`, `FreeCAD
  bodies of <component>`), bodies named after the objects' labels, faces
  `<feature>:import(j)` in the shapes' face order. Hidden objects →
  hidden bodies; shape colours other than FreeCAD's defaults (0.21
  `ShapeColor`, 1.0 `ShapeAppearance`) → body colours (not when the faces
  have colours of their own). Objects without faces are left out with the
  reason.
- The document's `UnitSystem` (1.0 and later) sets the length unit.
- Sketches: a `sketch` feature per `Sketcher::SketchObject`, named after
  its label, in the component of its Body or part, with FreeCAD's
  visibility. Plane: an origin plane it is attached to, a datum plane the
  history made (`"F4"`), a planar face of a body it is attached to
  (`{"face": "F1:import(5)", "body": "F1.b0"}`), else the parallel origin
  plane, else a hidden fixed construction plane; `frame` keeps the stored
  placement. The root point is a fixed point at the origin, the H and V
  axes fixed construction lines when used.
  - Geometry: points, lines, circles, arcs, ellipses and elliptical
    arcs, B-splines (poles as control points; periodic and unclamped
    ones clamped exactly), Bézier curves; arcs of hyperbolas and
    parabolas as exact rational quadratic splines; construction and
    blocked (`fixed`) flags. Internal geometry: a B-spline's control
    point circles are its control points, its end knots its ends; an
    ellipse's axis lines stay construction lines held on it (`midpoint`
    at its centre, the major one ending at its major point, the minor one
    `perpendicular` with an end on the ellipse); foci, interior knots and
    the parts of hyperbolas and parabolas are left out.
  - Constraints: horizontal, vertical (lines, two points), parallel,
    perpendicular (a line and a circle or arc: its centre on the line),
    tangent (two lines: `collinear`), equal, point on object
    (`coincident`; on an axis `horizontal_points`/`vertical_points` with
    the root point), symmetric (about a point: `midpoint` of a
    construction line between the points), block (`fixed`).
  - Dimensions (driven for FreeCAD's reference ones): distance (`length`,
    `arc_length`, `distance`, `point_line_distance`, `line_distance`),
    DistanceX/Y (`horizontal_distance`/`vertical_distance` in the order
    that makes FreeCAD's signed value positive), angle (`angle` in 0..π,
    one line against the H axis, an arc's sweep below 180° between
    construction lines from its centre), radius, diameter.
  - External geometry on a body the import made in the sketch's
    component is a linked projection by its Mitcad name; other elements
    are fixed reference geometry; non-defining external geometry is
    construction geometry.
- History: every PartDesign Body and every Part workbench primitive,
  extrusion or revolution of a sketch, or operation is rebuilt as
  features in dependency order (ties in document order), each checked
  against FreeCAD's stored shape of it; one that cannot be rebuilt is a
  base feature of the stored shape that `replaces` the Body's body
  (`fallback`), and one whose stored shape is its base's is `skipped`. A
  Body's `Tip` before its last feature: the features after it are
  replayed, then suppressed; a suppressed feature (1.0's `Suppressed`):
  replayed and suppressed (`partial`). Bodies are named after FreeCAD's
  objects and hidden as there; their colours are not kept.

| FreeCAD | Mitcad | Readings tried |
|---|---|---|
| Pad, Pocket | `extrude` join / cut (the Body's first solid: `new_body`; participants the Body's bodies) on the profile sketch's material regions; Length → `distance`, ThroughAll → `through_all`, UpToFirst / UpToLast → `to_object` body (`through` for the last), UpToFace → `to_object` face or plane, UpToShape (1.0) → `to_object` of its one face or plane, of a whole object's body, or without shapes of the Body's body; `Offset` → `offset`; Midplane / SideType Symmetric → `symmetric` (`full_length`), TwoLengths / SideType Two sides → `two_sides` (1.1's `Type2`); TaperAngle(2) → `taper`; a custom direction (`UseCustomVector`, `ReferenceAxis`: FreeCAD's `Direction`) along the normal; off the normal → `sweep` of the regions along a fixed line through the sketch's origin, `parallel` (the length along the direction, or along the normal with `AlongSketchNormal`; one length, two or symmetric; no taper) | the regions inside an even number of wires (1.1), else the outer ones with their holes (1.0); the pocket against the normal (`Reversed` turns it), else the other way; the taper's and the offset's signs |
| Revolution, Groove | `revolve` join / cut; the axis: a sketch's H or V axis (its axis line), `Axis<n>` (its n-th construction line), `Edge<n>` (that line), an origin axis, a datum line, a straight edge; 360° → `full`, Midplane → `symmetric` (half each way), TwoAngles → `two_sides`, UpToFace, UpToShape (one face, a plane, a body) and UpToFirst → `to_object` | the angle's sign; up to an object also about FreeCAD's axis (`Base`, `Axis`) either way |
| Fillet | `fillet`, constant radius, on the edges and faces (`UseAllEdges`: every edge) | |
| Chamfer | `chamfer` `equal_distance`, `two_distances` (Size, Size2), `distance_angle` (Size, Angle) | `flip` both ways |
| Hole | `hole` at the profile sketch's circles' and arcs' centres (`sketch_points`), Diameter, depth (DrillForDepth taken off) or through all, flat or angled point, counterbore, countersink, counterdrill, Tapered → `taper` (90° less TaperedAngle; FreeCAD's drill point is as high as the straight hole's: the tip angle from the wall's end that gives it); a cosmetic ISO metric, Unified (UNC, UNF, UNEF: `1/4-20 UNC`), Whitworth (BSW, BSF: `1/4-20 BSW`; BSP: `G 1/4`), NPT (`1/4-18 NPT`) or tyre valve (ISOTyre, `5V1`) thread over the hole's depth → a cosmetic `thread` on the walls (the size from the version's list for the thread type, `ThreadSize[ISOMetricProfile]` in `data/enums.json`; the hole keeps FreeCAD's diameter), other threads left out (`partial`); a modelled ISO metric or Unified thread → first as FreeCAD cuts it: per hole a fixed `construction_plane` through the axis, a `sketch` of the groove's section (60° flanks, the root P/8 wide at the major diameter plus the class's clearance; 1.1's quadrilateral, the older hexagon) and a `helix` cutting it from a pitch above the top as deep as FreeCAD's thread runs (through all: two pitches past the body), then the `hole`; then a modelled `thread` (Mitcad's basic profile cut from FreeCAD's bore); both stay 1.3e-6 to 2e-6 (the groove) off FreeCAD's thread, so these fall back; Whitworth and NPT ones and those FreeCAD did not model fall back at once, tyre valve ones (FreeCAD rounds their crests) after the thread is tried | `flip` both ways |
| Mirrored, LinearPattern, PolarPattern | `mirror`, `rectangular_pattern` (1.1's second direction too), `circular_pattern` of the originals' Mitcad features (`identical`), or of the body (TransformMode whole shape: `combine` for the mirror); planes and axes as above (a sketch's V axis as a mirror plane: the plane through it along the normal); extent or spacing | the distance's or angle's sign |
| MultiTransform | of the originals' features: one transformation as that feature; two LinearPatterns → one `rectangular_pattern` of two directions; two Mirrored → both `mirror`s, a `construction_axis` where their planes meet (`two_planes`) and a `circular_pattern` of 2 about it (the half turn); otherwise (any sequence of Mirrored, LinearPattern and PolarPattern) each transformation a `mirror` or pattern of the one before (patterns of patterns); a LinearPattern or PolarPattern and then a Scaled of as many occurrences → that pattern with `scale` (Factor); its transformations are no features of their own; other Scaled ones and the whole body fall back | the signs of the directions |
| Boolean | `combine` join / cut / intersect of the Body's body with the other Bodies' bodies | |
| Draft | `draft` of the faces about the neutral plane (a pull direction taken as its normal) | `flip`, the angle's sign |
| Thickness | `shell` (Skin; Pipe and RectoVerso as Skin, which FreeCAD's solids give) | inside or outside, rounded |
| AdditiveLoft, SubtractiveLoft | `loft` of the profile and section sketches' regions, Ruled, Closed | |
| AdditivePipe, SubtractivePipe | `sweep` along the spine sketch's edges, Standard / Frenet → `perpendicular`, Fixed → `parallel`; with sections (Transformation Multisection) → `loft` of the profile and the sections with the spine as its `centerline`; auxiliary spines fall back | |
| AdditiveHelix, SubtractiveHelix | `helix` of the profile's regions about the reference axis: the pitch and the turns as the Mode gives them (Height / Pitch, Height / Turns), LeftHanded → `left_handed`, Reversed → `flip`; a cone's angle → `growth` `Pitch * tan(Angle)`, height-turns-growth's Growth → `growth`, both with `"construction": "freecad"`; a subtraction outside the profile (Outside) → `intersect` | the hand and the direction both ways |
| Additive / Subtractive Box, Cylinder, Sphere, Torus, Cone, Prism, Wedge, Ellipsoid | `box`, `cylinder`, `sphere`, `torus` on a fixed plane at the placement; a cone and parts of a turn (Cylinder's and Cone's Angle, Sphere's latitudes and Angle3, Torus's section sector and Angle3): a section in a fixed plane through the axis turned about it (a construction plane, a sketch and a `revolve`); a prism: its polygon `extrude`d from a sketch on a fixed plane; skewed (FirstAngle, SecondAngle; a cylinder too) → `sweep` along the skew, `parallel`; a wedge: a ruled `loft` from its rectangle at Ymin to the one at Ymax (or a point); an ellipsoid: a `sphere` of Radius2 at the origin (a part of one: its section in the XZ plane turned about the z axis, Angle1 to Angle2 and Angle3 round), `scale` non-uniform (Radius3 / Radius2, Radius1 / Radius2), `move` to the placement and `combine` with the Body's body | |
| Plane, Line, Point (datums) | `construction_plane` (`offset` from an origin plane or a planar face when attached flat with only a normal offset; attached flat to an origin plane and turned a quarter or not at all, `offset` from the origin plane parallel to it by the offset's coordinate along its normal; turned about a line in its support's plane through its origin (its x or y axis or between) by a bound angle, `angle` from the support about that line; else `fixed`), `construction_axis`, `construction_point` (`fixed` at the placement); coordinate systems are skipped | |
| Part Box, Cylinder, Sphere, Torus, Cone, Prism, Wedge, Ellipsoid | the primitives as above, new bodies | |
| Part Extrusion of a sketch | `extrude` new body: LengthFwd (else the direction's length), LengthRev → `two_sides`, Symmetric, Reversed, TaperAngle(Rev); along the normal or a custom direction along it; a custom direction or an edge's (DirMode Custom, Edge: `Dir`) off the normal → `sweep` along a fixed line, `parallel` (no taper) | the direction, the taper's sign |
| Part Revolution of a sketch | `revolve` new body about a fixed axis (Base, Axis); 360° → `full`, Symmetric | the angle's sign |
| Part Cut, Fuse, MultiFuse, Common, MultiCommon | `combine` (the first operand the target) | |
| Part Fillet, Chamfer | `fillet` / `chamfer` of the edge list (constant sizes; unequal chamfer sizes `two_distances`) | `flip` both ways |
| Part Mirroring | `mirror` of the source's body (plane Base, Normal or 1.0's MirrorPlane), then a base feature without bodies that removes the source's body | |
| others (binders, Draft objects, plain shapes, …) | a base feature of the stored shape | |

- An operand of the Part workbench that a later object uses too is
  copied (`move` with `copy`) for each earlier use.
- The features of one FreeCAD object are named after its label, those
  that help the main one with a suffix (hidden when they are
  construction planes or sketches): `<label> (plane)` and `<label>
  (section)` of a cone, a prism or a part of a turn, `<label> (bottom
  plane)`, `(bottom)`, `(top plane)`, `(top)` of a wedge, `<label>
  (sphere)`, `(scale)`, `(placement)` of an ellipsoid (a part of one also
  `(plane)` and `(section)`), `<label> (mirror 1)`, `(mirror 2)`,
  `(axis)` of a MultiTransform's mirrors, `<label> (transformation 1)`,
  `(transformation 2)`, … of its chain of patterns and mirrors, `<label>
  (thread)` of a hole, `<label> (source removed)` of a mirroring, `<label>
  plane` of a sketch on an offset or turned construction plane.

Parameters and expressions (`freecad/params.rs`), before the timeline:

- User parameters: spreadsheet cells with an alias (named after it),
  cells an expression names by address (`<sheet>_B3`), VarSets' numeric
  properties (1.0), the history's feature properties that expressions
  refer to (`<label>_<property>`: `Pad_Length`), and sketches' named
  constraints. A name Mitcad cannot take, or one several share, gets its
  owner's label in front. Each has FreeCAD's expression translated, else
  FreeCAD's value (a cell without a value that does not translate is
  `skipped`).
- Bound expressions become the expressions of the values the import
  carries over (pads' and pockets' lengths, offsets and tapers,
  revolutions' angles, fillets', chamfers' and holes' sizes, patterns'
  distances, angles and counts, draft angles, thicknesses, primitives'
  sizes, Part extrusions' lengths and tapers, sketches' dimensions), using
  the parameter made of the property where there is one (the feature, or
  the sketch for a named constraint, adopts it). Attachment offsets of
  sketches and datum planes: bound along the normal → a construction
  plane `offset` from the support; a bound turn about the support's x or
  y axis → an `angle` construction plane; turned a quarter (by a value)
  so that a bound side offset moves it along its normal → the parallel
  origin plane `offset` by that expression. A cone's and a partial
  cylinder's radii and height are length dimensions of their section
  sketches. Side offsets that only move a sketch or plane within itself,
  and turns about other axes or with shifts, keep FreeCAD's values.
- Translation: references (`Spreadsheet.Width`, `<<Sheet>>.B3`,
  `.Constraints.width`, `Sketch.Constraints.width`, `VarSet.Depth`,
  `Pad.Length`, `.Height.Value` as a plain number), units (`°` as `deg`,
  `gon` as `grad`, `thou` as `mil`, `"` and `'` as `in` and `ft`, `dm`,
  `nm`, `′`, `″` by their factors), `pi`, `e` as `PI`, `E`, `c ? a : b`
  as `if(c; a; b)`, `log`, `log10` as `ln`, `log`, `mod(a; b)` as `a %
  b`, `hypot`, `cath`, `cbrt`, `trunc`, `sum` and `average` (over ranges
  too) written out, `min` and `max` of several; FreeCAD's signs bind
  tighter than `^` and its `^` is left-associative (`-2^2` = `(-2)^2`). A
  unitless result where FreeCAD has a length or an angle gets FreeCAD's
  unit (`B3 * 1 mm`).
- Every translated expression is compared with FreeCAD's stored value
  (1e-9 relative; an integer property's expression is rounded as FreeCAD
  rounds it, `round(Count / 2)`, when that gives FreeCAD's value); one
  that differs (`mismatched`) or does not translate keeps FreeCAD's
  value, with the reason in the report.

The result: `file`, `design` (the document's label), `items` (objects),
`bodies`, `counts` (objects per outcome: `body`, `component`,
`occurrence`, `parametric`, `partial`, `fallback`, `included`,
`skipped`) and `report`:

- `program_version`, `schema_version`, `units`, `objects_by_type`;
- `items`: per object `index`, `name`, `label`, `type`, `outcome`,
  `note`, `bodies`, `component`, `stale` when its stored shape may be out
  of date;
- `bodies`: each with its `object`, `file` when from another document,
  `uid`, `name`, `component`, `kind`, `volume`, `area`, `center` in its
  component, `color`, `visible`;
- `placed`: where each body, part, link and array element of the
  document shows its geometry: `path`, `bodies`, world `volume`, `area`
  and `center`;
- `components`, `occurrences`, `files` and `missing_files` (linked
  documents), `stale`, `warnings`;
- `sketches`: per sketch `object`, `label`, `outcome`, `feature`,
  `component`, `plane`, FreeCAD's `geometry` and `constraints` counts,
  Mitcad's `entities`, `mitcad_constraints`, `dimensions`, `error` of the
  solved points, `dropped`, `notes`, `dimension_sources`, `expressions`,
  `shape` against the stored shape, and the profile curves' `edges`,
  `length` and world `center`;
- `features`: the history in timeline order, per object `object`,
  `label`, `type`, `body` (its PartDesign Body), `outcome`, `features`
  (the Mitcad features made), `mitcad` (their type), `check` against the
  stored shape (`volume`, `area`, `center`, `solids` and FreeCAD's,
  `distance`, `pass`) and `notes` (which reading was kept, or why it fell
  back);
- `parameters`: per parameter `name`, `source`, `object` and its `cell`,
  `property` or `constraint`, `describe`, FreeCAD's expression
  `freecad`, Mitcad's `expression`, `value`, `freecad_value`, `outcome`
  (`parameter`, `value`, `mismatched`, `skipped`) and `note`;
- `expressions`: every expression FreeCAD binds to a property: `object`,
  `path`, `expression`, `mitcad`, `outcome` (`parameter`, `expression`,
  `value`, `mismatched`, `unused`) and `note`;
- with `reference`, `reference`: `checked`, `differences`, `pass`
  (volumes of solids and areas within 1e-9 relative, world centres of
  mass within 1e-6 mm, places with bodies of the history
  (`placed[].replayed`) within the replay's 1e-6; sketches' edge counts,
  lengths and centres within 1e-6 relative; parameters' values against
  FreeCAD's values of the cells, properties and constraints within 1e-9
  relative). A place of stored shapes also has `placed[].fixed`: their
  `volume`, `area` and `center` with the kernel's plain fixed-point
  integration, as FreeCAD measures; where FreeCAD's measures are those,
  a difference is listed as not compared (Mitcad integrates faces bounded
  by B-splines of many spans more exactly, mitcad#139).

Tests: `core/freecad/tests/parse.rs`, `core/freecad/tests/sketch_corpus.rs`,
`core/import/src/freecad/tests.rs` (mock kernel), `cli.import_fcstd`
(`tools/cli/fcstd-test.cmake`), `freecad.corpus` (`MITCAD_FCSTD_CORPUS`;
[core/import/README.md](../../../import/README.md#tests)) and
`tools/ui-import-test.sh`.

### .ipt import

`{"cmd": "import_ipt", "path": "part.ipt", …}` (C++ `Document::command`,
or `Document::import_ipt(path, json)` for the report alone) imports an
`.ipt` part file (mitcad#60; `core/ffi/src/ipt_import.rs`, the reader is
[core/ipt](../../../ipt/README.md)):

- **With its history** (the default when the file's definitions segment
  has features or parameters; stages 2 and 3): the part's parameters with
  their expressions, its sketches, work planes and features, decoded into
  the import's dump IR (`mitcad_ipt::design`) and replayed by the same
  importer as the `.f3d` import ([.f3d import with the
  timeline](#f3d-import-with-the-timeline)): each feature is checked
  against the state of the ASM history stored in the B-rep record that its
  operation made, and comes in as the bodies of that state where it
  cannot be replayed (`fallback`); the final bodies are compared with the
  stored ones. The report adds `history` (true), `parameters` (the
  definitions segment's parameter records: `records`, `model`, `user`,
  `outside` (not the part's: annotations, reference dimensions, features'
  own tables), `internal` (`RDxVar<n>`), `unread`), `expressions`
  (`translated`, `agree`: evaluated to the stored value, `differ`,
  `not_translated`, `computed` (parameters whose value the model computes,
  not their expression: the stored value comes in), each `name: why`),
  `features` (items with a history
  state), `features_with_states` (of them, those whose state the history
  has), `counts` (items per outcome), `design` (the importer's report:
  `items`, `parameters`, `history`, `bodies`, `warnings`, as for
  `import_f3d`), `design_text` (its text) and `seconds` {`read`,
  `import`}; `imported` lists the document's bodies after the replay
  (`feature` is the feature that made the body).
- **Bodies only** (`bodies_only`, and files without definitions; stage
  1): one base feature per body (solids and sheet bodies), built from the
  file's B-rep record with OCCT.

Either way the triangles of the part's mesh features come in as mesh
bodies (`source` `PmGraphicsSegment#<record>`, `mesh` true, `closed`,
`triangles`; after the replay with the history), and a part without
bodies and meshes opens empty with a warning (bodies that cannot be built
are still an error).

Either way the document's length unit becomes the part's when the
document has no features yet (else a warning says so), and the bodies get
the part's material when Mitcad's library has one of that name or id (case
does not matter). One undo step (`Import part.ipt`). A job does not cancel
it.

| Field | Meaning |
|---|---|
| `text` | `import_ipt` returns a readable report |
| `report_path` | also write the JSON report to this file |
| `reference` | a STEP file of the same part: each of its solids is matched with the imported solid of the closest volume and their volumes and areas compared |
| `max_relative` | the largest relative difference of a volume or area that passes (default 1e-6) |
| `deviation` | with `reference`, also the sampled surface deviation (as `compare_step`) |
| `bodies_only` | the bodies only, as base features (stage 1) |
| `no_verify`, `no_fallback`, `no_compare` | as for `import_f3d`: do not check the replay against the history; leave out features that cannot be replayed; compare the final bodies by volume only |
| `time_limit` | seconds after which the remaining features take the file's bodies |
| `hang_limit` | seconds without progress after which a geometry kernel call counts as hung (an empty document only): as for `import_f3d`, the replay runs again on its own thread with the item it hung on taking the file's bodies, a warning saying so |
| `dump_path` | also write the decoded design (the dump IR, `core/import/SCHEMA.md`) to this file |
| `design_path` | replay this dump (as `dump_path` writes it) instead of the decoded design |

The command's result: `file`, `design` (the part number), `bodies` (the
number imported) and `report`: `format` (`ipt`), `part_number`,
`material`, `mitcad_material` (the material set, or null), `release` (the
release that saved the file, as it says), `units` (the document's new
length unit, or null) and `unit_code`; `records` (each B-rep record:
`source`, `asm_version`, `bodies`, `history_states` (the states of the
ASM history the record carries), `truncated`); `imported`
(each body: `source` `<segment>#<record>/<body>`, `solid`, `valid`
(OCCT's checker), `volume`, `raw_volume` (before the solids were
oriented), `area`, `faces`, `issues`, `messages` (the builder's, with what
the checker finds wrong), `feature`,
`bodies` with `uid`, `name`, `kind`); `skipped` (bodies not built:
`source`, `error`); `warnings`; `seconds` {`read`, `build`}; and with
`reference`, `reference`: `file`, `step_solids`, `solids` (pairs:
`step_body`, `step_volume`, `step_area`, `body`, `name`, `volume`,
`area`, `volume_difference`, `area_difference`), `unmatched_step`,
`unmatched_bodies`, `max_difference`, `max_relative`, `valid`, with
`deviation` `max_deviation`, and `pass` (every STEP solid and every
imported solid matched, all within `max_relative`, every imported solid
valid).

Tests: `core/ipt` (`core.ipt`, no OCCT; with the design replayed on the
mock kernel, `tests/design_import.rs`), `core.exchange`
(`core/tests/test_exchange.cpp`), `cli.import_ipt*`, `cli.ipt_design_file`,
`ipt.corpus`
(`MITCAD_IPT_CORPUS`, [core/import/README.md](../../../import/README.md#ipt-import))
and `tools/ui-import-test.sh`.

### .iam import

`{"cmd": "import_iam", "path": "assembly.iam", …}` (also `import_ipt` with
an `.iam` file; C++ `Document::command`, or `Document::import_iam(path,
json)` for the report alone) imports an `.iam` assembly (mitcad#60, stage
4; `core/ffi/src/iam_import.rs`, the reader is
[core/ipt](../../../ipt/README.md#assemblies)):

- Every distinct part file is imported once with the `.ipt` import, in a
  document of its own: its stored bodies, which become one base feature
  of a component of this document (the shapes shared, not copied), or
  with `history` its parameters and features replayed, that design copied
  into a component (`Document::add_component_copy`); each of its
  occurrences is an
  occurrence of that component, named after the file and the instance
  number (`part:2`) or the label the file stores.
- A sub-assembly is a component whose occurrences are placed in it the
  same way; each sub-assembly file comes in once, however often it is
  placed.
- Placements are the file's (cm become mm); hidden occurrences are hidden,
  grounded ones grounded, suppressed ones left out (counted).
- Referenced files are found as saved, else relative to the assembly as it
  was when saved (the saved paths of both), else by the saved path's tail
  under the assembly's folder and the folders above it, else by name in
  the assembly's folder tree and `search` folders. A part found there is
  checked to be the referenced document: its own file list names the
  version id the occurrence stores (`identity`). A file not found is an
  empty component with the occurrences placed, and the report names it.
- Checks against the file: each occurrence's placement in the document
  against the transform the file displays it with (where the file keeps
  one), and each placed part's bodies (their bounding box in the
  component) against the range box the file stores for the document:
  they must lie within it, to 1 % of its diagonal (at least 0.05 mm); the
  stored box is not tight (curved faces make it larger), so how much
  larger it is (`box_slack`) is information only. The stored centre of the
  placed range box is compared too, for information only: the file does
  not always keep it up to date.

The document's length unit becomes the assembly's when the document has no
features yet. One undo step (`Import assembly.iam`); a job does not cancel
it.

| Field | Meaning |
|---|---|
| `text` | `import_iam` returns a readable report |
| `report_path` | also write the JSON report to this file |
| `search` | more folders searched for referenced files by name (with their subfolders) |
| `history` | each part with its design replayed (as `import_ipt` without `bodies_only`), not only its stored bodies: much longer and much more memory |
| `no_verify`, `no_fallback`, `no_compare`, `time_limit`, `hang_limit` | with `history`, each part's import, as for `import_ipt` |

The command's result: `file`, `occurrences` (placed) and `report`:
`format` (`iam`), `release`, `units`, `unit_code`; `occurrences` (counts:
`placed`, `suppressed`, `hidden`, `of_missing_files`, `not_placed`,
`without_file`, `without_placement`, `not_rigid`); `parts` (part files:
`files`, `imported`, `missing`, `failed`, `identity_confirmed`,
`identity_differs`); `assemblies` (sub-assembly files); `resolution`
(files per way they were found); `files` (each: `saved`, `path`, `found`,
`kind`, `status`, `identity`, `occurrences`, `component`, and for parts
`part`: the part import's `bodies`, `solids`, `valid`, `not_built`,
`history`, `counts`, `features`, `warnings`, `units`, `material`);
`display`, `boxes`, `centers` (each `compared`, `agree`, `worst` in mm,
`failures`); `pass` (every display and range box check agrees);
`occurrence_list` (each placed or suppressed occurrence with its path
name, `file` index, `occurrence` uid, `grounded`, `hidden`, `checks`);
`warnings`; `seconds`.

Tests: `core/ipt` (`core.ipt`: the reader on assemblies made by
`mitcad_ipt::testassembly`), `core.model` (`add_component_copy`),
`cli.iam_files`, `cli.import_iam`, `cli.info_iam`, `cli.import_ipt_iam`
(a test project of those assemblies: a sub-assembly, parts found relative
to the assembly and by name, a missing one; hidden, grounded and
suppressed occurrences), and `iam.corpus`
([tools/cli/iam-corpus.cmake](../../../../tools/cli/iam-corpus.cmake),
every `.iam` under `MITCAD_IPT_CORPUS`).

## Files and version history

### Project files

The file format (version 2 single files, version 3 in projects, the
B-rep store, the per-user display state) is in
[docs/architecture.md](../../../../docs/architecture.md#project-file).
What callers see:

- In a project (a folder with `.mitcad/project.json`, the nearest one
  above the file) a base feature body's `brep` refers to its data in the
  project's store: `{"format": "occt", "compression": "zlib", "size":
  6146, "sha256": "3fa9…e1"}`, the data in `.mitcad/brep/3f/3fa9…e1.brep.zlib`.
  A `brep` with `sha256` in a command is such a reference too.
- A reference whose data is missing or damaged stays a reference: its
  base feature fails (`body 0: its B-rep data … is missing from the
  project store (.mitcad/brep)`) and the `document` query's `warnings`
  say so. Saving a document with an unresolved reference keeps it, so
  even a single file is then version 3.
- The display state is written to `.mitcad/local/display/<path in the
  project>.json` in a project, inside the file otherwise.

No JSON commands; Rust and the bridge (names in brackets):

| Function | Meaning |
|---|---|
| `Document::save_file(path, format)` (`save_project(path, "auto" \| "single" \| "project")`) | auto is version 3 in a project and version 2 elsewhere; returns the text written |
| `Document::load_project(path)`, `from_json_at(json, path)` (`load_project`, `load_document_at`) | read a file resolving its project's store |
| `from_json_in(json, store)`, `to_project_json(store)` | with any `BlobStore` |
| `Project::find`, `open`, `init` (`init_project(dir)`) | `init` writes the marker, `.gitattributes` and `.gitignore` |
| `load_warnings` (bridge) | what loading changed |
| `to_json` | always the single file (autosave, the import's process) |
| `find_project(path)` (bridge) | the folder of the project a file is in, or `""` |

`mitcad-cli project init <folder> [--author A] [--no-history]` and
`mitcad-cli convert <in> <out> [--format v2|v3|auto]`; `--save` of the
other commands writes as auto does. Tests:
`core/model/src/file/project_tests.rs`, ctest `cli.project_v3`
(`tools/cli/project-test.cmake`).

### Version history

A project whose folder is the root of a git repository records versions
(`core/vcs`, `mitcad-vcs`, with gitoxide); Mitcad commits only to such a
repository. How versions are built:
[docs/architecture.md](../../../../docs/architecture.md#version-history).
These are not document commands:

- The bridge's `open_project(path)` (a file, which need not exist, or the
  project's folder; the error says why there is no history) gives a
  `Project` whose `command(json)` answers in JSON and `command_text(json)`
  in text. A `Project` is used by one thread at a time.
- `init_project_history(dir, author)` makes a project with history (the
  marker, `.gitattributes`, `.gitignore`, `git init` with the branch
  `main` unless the folder is a repository's root, and a first version;
  `author` `"Name <email>"` or `""` for git's configured one) and returns
  `{"root", "branch", "commit", "written", ...}`.
- `create_project_repository(dir)` makes the folder a project with a git
  repository (`main`) but no version yet, so that the author git's
  configuration gives there can be shown before `init_project_history`
  records the first version.
- `git_repository_root(path)`: the work tree of the git repository a
  folder (which need not exist) is in, or `""`.
- `load_version(project, rev, path)` reads the project file now at
  `path` as it was in a version (under the path it had there, through
  the renames its history follows), its B-rep data from that version's
  tree, as an untitled document (nothing computed, links not followed).
- Paths are file paths (absolute, or relative to the current folder) in
  the project. A version (`rev`) is a commit id, a unique prefix of one
  (4 or more hex digits) of a commit reachable from HEAD, `HEAD`, or
  `HEAD~n`; the full id of any other commit the repository has (a
  remote's version a fetch brought) is one too.

| Command | Fields | Result |
|---|---|---|
| `commit` | `paths` (the saved files; a missing one is removed), `message` (default `Save <paths>`), `author` (`"Name <email>"`, optional), `fallback_author` (used when neither `author` nor git's configuration gives one) | `commit` (id or null), `skipped` (why not: HEAD detached, a merge or rebase in progress), `written`, `removed`, `deleted` (B-rep files deleted from the folder), `warnings`, `files` |
| `history` | `path`, `summaries` (default false) | `path` (relative to the project), `versions`, newest first: `id`, `short_id`, `time` (seconds, UTC), `offset` (seconds), `date` (`2026-10-05 14:03:12 +0300`), `author` {`name`, `email`}, `summary`, `message` (without Mitcad's trailers), `path` (in that version), `blob` (null: the version deleted it), `renamed_from`; with `summaries` also `changes` (below) |
| `read_version` | `path`, `rev` | `id`, `path`, `text` |
| `restore` | `path`, `rev`, `message` (default `Restore <path> from <id>`), `author`, `fallback_author` | as `commit`; the version is read under the path the file had there and written to its path now |
| `changes` | `from`, `to` (default HEAD) | `from`, `to` (ids), `changes`: `kind` (`added`, `deleted`, `modified`, `renamed`), `path`, `from` (renamed) |
| `diff` | `path`, `from`, `to` | see [Comparison of versions](#comparison-of-versions) |
| `status` | `path` | `path`, `head` and `file` (blob ids or null), `modified`, `renamed_from` (a project file not in HEAD whose content is that of a path of HEAD gone from the folder, renamed or moved outside Mitcad; else null) |
| `follow_rename` | `path` | such a file takes the display state of the path it had along: `renamed_from` (null when not renamed), `display_moved` |
| `identity` | `fallback_author` (optional) | `name`, `email`: git's configured author, else the fallback; an error without either |
| `resolve` | `rev` | `id` |
| `info` | | `root`, `branch` (null when detached), `head` |

- A version holds only the given paths (as `git commit --only`), with the
  B-rep files their project files refer to, minus the B-rep files no
  project file of the version refers to (those leave the folder too
  unless a changed project file there refers to them; none is removed
  when a project file of the version cannot be read). The user's staged
  changes stay and `git status` is clean. Nothing changed: no commit.
- Messages get the trailers `Mitcad-Version` and `Mitcad-Format: 3`; the
  author is also the committer.
- The history lists the commits on HEAD's first-parent chain that changed
  the file (`git log --first-parent -- <path>`), following renames that
  kept the content.
- `summaries`: each version's `changes` is the summary of its comparison
  with the version after it in the list (the one before in time): `d3
  20 mm -> 25 mm, 1 feature modified`, `no changes` for a rename, null for
  the oldest and next to a version that deleted the file;
  `changes_error` when a version could not be read. Each version is read
  once from its blob (`ProjectRepo::summaries`), so callers ask for the
  plain list first.

Rust: `Document::changes_since_saved` (the `changes_since_saved` query),
`Project::move_local_state(from, to)` and `remove_local_state(file)` (the
per-user display state follows a renamed project file and goes with a
deleted one; a commit that removes a project file removes it),
`file::rename_retrying` (used by `write_atomically`: on Windows a rename
that fails with access denied or a sharing or lock violation is tried
again for up to 2 s, waiting 10 ms to 200 ms), `ProjectRepo::create`,
`follow_rename`, `mitcad_vcs::repository_root`.

`mitcad-cli history <file> [--json]`, `version save <files> [-m M]
[--author A]`, `version show <file> <rev> [--save out]`, `version changes
<file-or-folder> <rev> [<rev>]`, `version restore <file> <rev> [-m M]`.

Tests: `core/vcs/src/tests.rs` (ctest `core.vcs`, with the system's git
where installed; `a_project_is_created_before_its_first_version`,
`a_file_renamed_outside_mitcad_takes_its_display_state_along`,
`versions_summarise_their_changes_and_follow_renames`), ctest
`cli.version_history` (`tools/cli/history-test.cmake`),
`core/model/src/api/tests.rs`
(`the_changes_since_saved_are_the_undo_steps_since`),
`core/model/src/file/project_tests.rs`
(`the_display_state_follows_a_renamed_file`,
`a_rename_is_tried_again_while_another_program_holds_the_file`),
`tools/ui-version-test.sh` and the Windows workflow test.

### Comparison of versions

What differs between two designs, `from` and `to` (two project files, two
versions, or a version and the open document), in the model's terms
(`core/model/src/diff/`, `mitcad_model::diff`). Both are snapshots of
whole documents; nothing is computed unless the geometry is asked for.

- **Parameters** by name: added, deleted, renamed (the same value slot of
  a feature uses the other name, or a user parameter has the same
  expression, unit and comment under another name), and changed
  expression, value (also one that follows other parameters), unit,
  comment, kind, owner, favourite. Expressions are compared with renamed
  references under their new names; a dimension name that went with a
  deleted feature and came again with a new one (`d6`) is two
  parameters.
- **Timeline** by feature uid: added, deleted, moved (the features
  outside a longest run that keeps its order in both), modified: the
  entry (name, suppressed, component, visible), the definition field by
  field in its file form (`extent.distance d3 -> d7`; lists of objects by
  their `id`, else index by index; B-rep data by SHA-256 and size) and
  the values of its parameter slots (`distance 10 mm -> 15 mm`). A
  sketch's entities, constraints and dimensions are compared by id:
  counts, added, deleted and changed ids, and the entities of which only
  the solved position or size changed (`moved`); where a dimension's
  value is shown is ignored.
- **Document:** `units.length`, `units.angle`, `marker` (`{"position",
  "after", "end"}`), `active_component`.
- **Components**, **occurrences** (placements as `moved by [x, y, z] mm
  and turned a deg`), **bodies** (names, visibility, material,
  appearance), **timeline groups**, **named views** and **analyses**.
- **Geometry** (optional): the volume and area of each body at the
  marker of the two computed documents, and their sums.
- The display state is not compared (it is per user).

| Field | Content |
|---|---|
| `identical`, `summary` | nothing differs; one line, `d3 20 mm -> 25 mm, +1 feature, 2 features modified` |
| `document` | field changes: `field`, `from`, `to` (null: not there), `text` |
| `parameters` | `kind` (`added`, `deleted`, `modified`), `name`, `renamed_from`, `owner`, `owner_name`, `from` and `to` (`expression`, `unit`, `value`, `text`, `comment`, `kind`, `owner`, `favorite`), `fields` (what differs), `text` |
| `features` | in `to`'s order, deleted ones where they were: `kind` (also `moved`), `uid`, `name`, `type`, `from_index`, `to_index`, `moved`, `fields` (field changes), `values` (`slot`, `label`, `from_parameter`, `to_parameter`, `from`, `to` in mm or rad, `from_text`, `to_text`, `text`), `sketch` (`entities`, `constraints`, `dimensions`: `from`, `to` counts and `added`, `deleted`, `modified` ids; `moved`), `text` |
| `components`, `occurrences`, `bodies`, `groups`, `views`, `analyses` | `kind`, `uid` (not for groups, views and analyses), `name`, `fields`, `text` |
| `geometry` | with the geometry: `bodies` (those that differ: `kind`, `uid`, `name`, `volume` and `area` as `from`, `to`, `change`, `relative`, `text`), `volume`, `area` (sums), `errors` |

Texts use `->` and ASCII units (`mm^3`). Entry points:

- The version history's `diff` command: `path`, `from` (a version), `to`
  (a version; left out: the file as saved in the folder); the answer adds
  `path`, `from` and `to` (ids; `to` null for the saved file) and `text`
  (`command_text`: `Changes in part.mitcad from abc1234 to def5678:` and
  the text). An older version is read under the path the file had there.
  B-rep data is compared by SHA-256, so no store is read.
- The bridge's `diff_documents(from, to, options)` for two documents (the
  open one and a version from `load_version`): options `text`,
  `geometry` (the bodies as last computed; compute the versions first,
  as a job), and `from`, `to`, `path` to name them in the answer.
- Rust: `diff_documents`, `diff_files`, `diff_states` with
  `read_design`, `DesignDiff::add_geometry`, `to_json`, `to_text`.
- `mitcad-cli diff <a.mitcad> <b.mitcad>` and `mitcad-cli diff <file>
  <version> [<version>]`, with `--json` and `--geometry` (both
  recomputed, linked components not followed).

Tests: `core/model/src/diff/tests.rs`, `core/vcs/src/tests.rs`
(`versions_compare_their_designs`), ctest `cli.diff`
(`tools/cli/diff-test.cmake`).

## Remote repositories

A project's version history shared through a git server (Forgejo, Gitea,
GitHub, GitLab, any host) or a repository in a folder
(`core/vcs/src/remote/`; design in
[docs/architecture.md](../../../../docs/architecture.md#remote-repositories)).
The network goes through the system's git program, 2.34 or newer:
`MITCAD_GIT` when set, else the first `git` on `PATH` (absolute folders
only), on Windows also Git for Windows where it installs itself. Objects,
commits and the counts of versions ahead and behind are read with gix
from the local references, without the network; local history works
without git.

- git runs without prompts and without a terminal, with English messages
  (`LC_ALL=C`), only over ssh, https, http and local paths
  (`GIT_ALLOW_PROTOCOL`), and with `-c core.autocrlf=false -c
  core.eol=lf` (files checked out as recorded; the project's
  `.gitattributes` apply). A cancel ends git's process group; git showing
  no sign of life for 5 minutes is stopped.
- Signing in is git's: SSH keys and the agent, or a credential helper
  (Git Credential Manager) for HTTPS. Mitcad never asks for, keeps or
  passes passwords or tokens: a URL with a password or a token
  (`https://user:token@…`, `https://ghp_…@…`), a remote helper (`ext::`,
  `fd::`), `git://`, other schemes and a URL starting with `-` are refused
  before git runs, and every text shown or logged has URLs' user
  information and access tokens (`ghp_…`, `github_pat_…`, `glpat-…`)
  hidden.

These are commands of the version history's `Project` (`command(json)`,
`command_text(json)`); `command_with(json, control)` runs one with a
`SyncControl` (bridge: `new_sync_control()`, `cancel()`, `is_cancelled()`,
`progress()` → `SyncProgress` {`text`: git's phase such as `Receiving
objects`, `percent` (-1: none), `cancelled`}) whose cancel ends git.

Every answer has `error`: null, or {`class`, `message` (for people, with
what to do), `detail` (git's error output)}, and `log`: the git commands
run and how they ended (`git push --porcelain … origin
refs/heads/main:refs/heads/main: ok`), credentials hidden. A failure of
git, the network, signing in or the remote is such an answer, not a
failed command; `command_text` makes it an error (`mitcad-cli` exits with
1). Classes: `git_missing`, `invalid_url`, `no_remote`, `unsupported`
(HEAD detached, no version yet, versions on both sides), `auth_failed`,
`host_key_unknown`, `network`, `not_found`, `rejected` (the remote has
versions the project lacks), `too_large` (GitHub's 100 MiB limit),
`lfs_missing`, `not_a_project`, `unrelated` (another history),
`timed_out`, `cancelled`, `conflict` (a sync needs a choice for files
changed on both sides), `local_changes` (a sync would change files that
have changes no version holds), `other`.

| Command | Fields | Result |
|---|---|---|
| `git_info` | | `path`, `version`, `lfs` (git-lfs's version or null), `supported` (2.34 or newer), `minimum`, `error` (`git_missing`) |
| `remote_info` | `links` (default false) | `name` (the remote the branch follows, else `origin` when there is one; null: none), `url` (credentials hidden), `branch`, `upstream` (`origin/main`), `ahead` and `behind` (versions the remote lacks and the project lacks, by the remote-tracking reference; null before the first fetch or push), `last_fetch`, `last_push` and `last_sync` ({`time`, `date`, `error`}, kept in `.mitcad/local/remote.json`), `remotes` (every remote of the repository, sorted: {`name`, `url` (credentials hidden)}; with several and `name` null, none is followed: [Following a remote](#following-a-remote)), with `links` `external_links`: linked components whose files are outside the project ({`file`, `component`, `path`}), which other copies of the project do not have |
| `remote_check` | `url` | `url`, `reachable`, `empty`, `default_branch` (what the remote's HEAD names, else `main`, `master` or the only branch), `head` (its commit), `has_project` (`.mitcad/project.json` at its root), `related` (a version in common with the project's history; null when empty). A branch whose commit the project lacks is fetched without a reference to read it, then `git gc --auto` |
| `remote_set` | `url`, `name` (default `origin`), `author`, `fallback_author` | `name`, `url`, `branch`, `upstream`, `commit` (the version `Configure project for sync`, or null), `written`: the remote added or its URL changed, the branch following its branch of the same name (unless it follows one of that remote), and the recommended `.gitattributes` (`*.mitcad text eol=lf merge=binary`) recorded when the project lacks it. No network |
| `remote_remove` | `name` (default `origin`) | `name`, `removed` (false: there was none); git drops the remote-tracking references and the upstream of branches that followed it |
| `connect` | `url`, `name`, `author`, `fallback_author`, `push` (default true) | `check` (as `remote_check`), `set` (as `remote_set`), `fetch`, `push` (null when not done), `ahead`, `behind`: an empty remote gets the project's versions; one that shares the history is fetched, and pushed to when the project only has versions to add; another history (`unrelated`) or an unreachable remote changes nothing. A failure after the remote was set keeps it set |
| `fetch` | | `remote`, `updated` (remote-tracking references that moved: `name`, `old`, `new`), `ahead`, `behind`; `git fetch --prune` of the remote the branch follows, the folder untouched |
| `push` | | `remote`, `branch` (on the remote), `pushed`, `versions` (pushed, when the remote's branch was known), `head`, `warnings`, `rejected`: the versions the remote lacks, never forced; nothing to push (the remote-tracking reference has HEAD): `pushed` false without the network; the upstream is set when it was not. A file of those versions over 100 MiB holds the push back (`too_large`, no network), one over 50 MiB is a warning |
| `sync_plan` | `fetch` (default true) | what `sync` would do, below; nothing changes but the fetch |
| `sync` | `resolutions` ({path: `mine` \| `theirs` \| `copy`}), `resolve_all`, `fetch` and `push` (default true), `author`, `fallback_author` | fetch, then push, fast-forward or replay, then push; below |
| `incoming` | `path` (optional) | `upstream`, `ahead`, `behind`, `versions` (the newest 100 of the remote's versions the project lacks, as in `sync_plan`), and with `path`: `path` (relative to the project), `file_versions` (those of them that changed the file, newest first), `differs` (the remote's file is not the project's latest version of it), `changed_here` (the project's versions the remote lacks changed it too: a sync asks what to keep). From the local references as the last fetch left them, no network |

Without a project: the bridge's `git_info(text)` (JSON as the command,
or text), `git_info_at(program)` (the same for a given git program, `""`
for the one found) and `clone_project(url, dir, control, text)`: the
project opened from `url` into folder `dir` (new, or empty) with `git
clone` of the remote's default branch, the remote named `origin`; JSON
{`root`, `url`, `branch`, `head`, `files` (the project files of its
latest version), `error`, `log`}. An empty remote, or one whose root
holds no project, is `not_a_project`, and a failed or cancelled clone
leaves no folder (an empty one that was there stays, empty).

Rust also has `ProjectRepo::fast_forward` (the remote's newer versions
when the project has none of its own: `git merge --ff-only`, the folder's
files follow; git refuses to overwrite a file with changes no version
holds), `remote::clone_project` with a `GitCli`, `check_url`, `redact`,
`classify` and `ProjectRepo::set_git`. Versions that could not be pushed
wait as `ahead` until a push succeeds.

`mitcad-cli remote add <folder> <url> [--name N] [--author A]`
(`connect`), `remote check <folder> <url>`, `remote show <folder>` (git's
version and `remote_info` with links), `remote remove <folder>`, `fetch
<folder>`, `push <folder>`, `clone <url> <folder>`, each with `--json`.
Tests: `core/vcs/src/tests/remote.rs` (a bare repository as the remote,
an SSH path through a fake `ssh` in `GIT_SSH_COMMAND`, cancellation, the
error classes; skipped without git), ctest `cli.remote`
(`tools/cli/remote-test.cmake`, where git is installed) and the bridge's
`test_remote_bridge`.

### Sync and conflicts

`sync` (`core/vcs/src/remote/sync.rs`) fetches (unless `fetch` is false),
then compares the branch (L) with its remote-tracking reference (R) and
their last common version (B):

- `up_to_date` (L = R): nothing to do. `push` (R = B, or no remote branch
  yet): the project's versions are pushed. `fast_forward` (L = B): the
  remote's are taken with `git merge --ff-only`.
- `replay` (both have versions the other lacks): the project's
  unpublished versions are replayed after R, oldest first, file by file,
  each a new commit with the version's author, author time and message,
  by the committer (`author`, else git's configured user; only a replay
  needs one). The history stays one line; the shared history is never
  rewritten, only the unpublished versions get new ids. A version whose
  changes were all taken from the remote is left out. B-rep files are
  never conflicts: each replayed version gets the B-rep files its project
  files refer to and drops those none refers to.
- `unsupported` (`reason`): a merge, rebase or other git operation in
  progress, a merge made with git among the unpublished versions (sync
  with git instead), or a remote branch of another history. Nothing
  changes.

A path that the project's versions and the remote's versions both
change, whose contents in L and R differ, is a **conflict** of the whole
file (files are never merged line by line, nor designs feature by
feature). Kinds: `modified`, `deleted_theirs` (the remote deleted it, the
project changed it), `deleted_mine`, `added_both`. Choices:

- `mine`: the project's versions of the file come after the remote's;
- `theirs`: the remote's file; the project's versions of it are left
  out and kept only in the backup;
- `copy`: the remote's file, and the project's versions of it replayed
  under `<name> (conflict copy <author> <yyyy-mm-dd HH.mm>)<.ext>` in the
  same folder, after the author and time of the project's latest version
  of it; the display state moves to the copy. Files of `.mitcad/` have no
  `copy`.

Nothing changes before every conflict has a choice (`resolutions` by path
as the plan names them, or by a file's path absolute or relative to the
current folder; `resolve_all` for the others where it is a choice) and
the files the replay would change have no changes that no version holds
(else `local_changes`). Then the old history is kept in
`refs/mitcad/sync-backup/<yyyymmdd-HHMMSS>` (UTC; those older than 30
days go at a later sync, the newest 5 always stay), and `git reset
--keep` moves the branch and the folder together (other changes in the
folder stay). A version recorded meanwhile starts the round again. Then a
push (unless `push` is false); the remote moving meanwhile (`rejected`)
starts the round again with a fetch, up to 3 times, and a round after
the first asks again for choices of new conflicts. Afterwards `git gc
--auto`. A cancel stops git (fetch, push) or the replay; until the backup
reference is written nothing changes.

`sync_plan` answers `case`, `branch`, `upstream`, `head`, `remote_head`,
`base`, `ahead` and `behind` (counts), `local` and `remote` (the newest
100 versions of each side: `id`, `short_id`, `time`, `offset`, `date`,
`author`, `summary`), `conflicts`, `uncommitted` (paths with changes no
version holds, the B-rep store left out), `reason`, `fetch` (the fetch's
answer, or null), `error`, `log`. A conflict: `path`, `kind`, `mine` and
`theirs` ({`blob` (null: deleted), `version` (the side's latest version
that changed the file), `versions` (how many did)}), `choices`, `copy`
(the copy's path).

`sync` answers `case` (of the first round), `branch`, `upstream`,
`fetch`, `head`, `replayed` ({`from`, `to` (null: left out), `summary`}),
`resolved` ({path: choice}), `copies` ({`path`, `copy`}), `backup` (the
reference, or null), `push` (the push's answer, or null), `pushed`,
`retries`, `changed_paths` (in the folder), `conflicts` (those without a
choice: `error` `conflict`, nothing changed), `uncommitted` (after the
sync), `reason`, `warnings` (choices for paths that are not conflicts, a
`theirs` whose versions are only in the backup, B-rep files missing),
`ahead`, `behind`, `error`, `log`. A failure after the fetch is in
`error` with what was done before it. The bridge's
`describe_remote(command, answer)` turns such an answer into text, a
failure too.

`mitcad-cli sync <folder> [--resolve <path>=mine|theirs|copy]...
[--resolve-all C] [--author A] [--no-push] [--dry-run] [--json]`
(`--dry-run`: `sync_plan`); conflicts without a choice list the files
with both sides' versions and the copy's name, and exit with 3. Tests:
`core/vcs/src/tests/sync.rs` (other files on both sides, the same file
with each choice, deletions against changes, files added on both sides,
the B-rep store, changes in the folder in the way, a push refused
meanwhile, a merge made with git, cancellation, files too large, backups,
the JSON options, `incoming`, a remote's version compared by its id;
after each `git fsck`, a clean `git status` and `git log
--first-parent` of each file as its history) and ctest `cli.remote`.

### Following a remote

A repository can have several remotes; the project syncs with the one its
branch follows: the branch's upstream, else `origin`, else the only remote
(`upstream_for` in `core/vcs/src/remote/mod.rs`). With several, none of
them `origin` and no upstream, none is followed (mitcad#89):
`inspect_folder`'s `remote` and `remote_info`'s `name` are null, the
remote commands answer `no_remote`, and the application's Project
Settings asks which one to follow.

| Command | Fields | Result |
|---|---|---|
| `remote_follow` | `name` (a remote of the repository), `branch` (the remote's branch; default the current branch's name) | `name`, `url` (credentials hidden), `branch` (the current branch), `upstream` (`second/main`), `changed` (false: it followed that branch already), `ahead` and `behind` (by the remote-tracking reference; null before that remote is fetched). The current branch follows the remote's branch from now on (`branch.<b>.remote`, `branch.<b>.merge` in the repository's configuration); nothing is fetched or sent. A remote that is not there is `no_remote`, a detached HEAD `unsupported`; a name that is no remote's name or a branch that is no branch's name is a failed command |

`mitcad-cli remote follow <folder> <name> [--branch <branch>] [--json]`;
`remote show` lists the remotes when none is followed. Tests:
`core/vcs/src/tests/projects.rs`
(`a_folder_is_told_for_what_it_is`: several remotes, none followed,
then each followed) and ctest `cli.remote`.

## Projects

Local and Cloud projects (mitcad#89; `core/vcs/src/projects/`, design in
[docs/architecture.md](../../../../docs/architecture.md#projects)). A
project is a folder with `.mitcad/project.json`; it is **Local** when the
folder is the root of a git repository without a remote and **Cloud** when
the repository has one. The kind is never stored: it is read from the
folder, so a project cloned with any git tool is Cloud when opened.

Commands without a project go through `mitcad_vcs::api::projects_command`
(bridge: `projects_command(json, control)` and `projects_command_text(json,
control)`, with a `SyncControl` for progress and cancellation, as
`library_command`). Every answer has `error` ({`class`, `message`,
`detail`} or null) and `log` (the programs run, credentials hidden), as
the remote commands: a failure of git, the network, the remote or a
folder's state is an answer, not a failed command (`projects_command_text`
makes it an error, `mitcad-cli` exits with 1). A command that is not
understood, a path that is a file where a folder is needed, a host that is
no host name and values that are no settings are failed commands. New
error classes: `has_project` (the remote holds a Mitcad project where a
new one was to go), `not_empty` (a folder that must be new or empty is
not), `inside_project` (a folder that is a project, or inside one, where a
new project was to be made).

| Command | Fields | Result |
|---|---|---|
| `inspect_folder` | `dir` | what the folder is, no network and no writes; below |
| `check_remote` | `url` | `remote_check` without a project: `url`, `reachable`, `empty`, `default_branch`, `head`, `has_project`, `files` (names at the root of the default branch's latest version, at most 100, cleaned for display), `latest` ({`time` (Unix seconds), `date` (RFC 3339, UTC), `author` (the name, cleaned)} or null), `versions` (commits on the default branch, counted up to 10 000; null when empty). A remote with branches is cloned without file contents (`--filter=blob:none` where the server allows it) into a temporary bare repository, which is removed: nothing is left behind |
| `create_project` | `dir`, `author` (`Name <email>`), `design` ({`path` (relative, `/`), `text`}, optional), `include_designs` (default true), `url` (optional: Cloud), `push` (default true), `shared` (optional: the shared settings, as `set_project_settings`' `shared`, merged into the defaults and checked strictly) | below |
| `clone_project` | `url`, `dir` (new or empty: `not_empty`; not in a project: `inside_project`), `adopt` (default false), `author` (optional) | the bridge's `clone_project` answer (`root`, `url`, `branch`, `head`, `files`), and `adopted`, `commit` (the first version as a project, or null) and `push` (`push`'s answer, or null). With `adopt` a remote with files but no project is cloned, made a project (marker, `.gitattributes`, `.gitignore`; first version `Make this repository a Mitcad project` by `author`, else git's configured author) and pushed; without, it is `not_a_project` and nothing stays. An empty remote is always `not_a_project` (`create_project` with `url` makes a project there). `author`, when given, is written to the repository's own configuration. A failure before the first version leaves no folder (an empty one that was there stays, empty); a failed push is `error`, the project made |
| `init_bare` | `dir` | `dir`, `created` (false: it was a bare repository already): a missing or empty folder (files the system leaves count as empty) made a bare repository with the branch `main`, for a shared folder as a remote; a folder with anything else (a work tree too) is `not_empty` |
| `host_keys` | `host`, `port` (default 22) | `host` (lowercase), `port`, `keys` ([{`type`, `key` (base64), `fingerprint` (`SHA256:…`)}]), `service` (`GitHub`, `GitLab` or `Codeberg` for github.com, gitlab.com and codeberg.org, else null), `published` (`verified`: every key is one the service publishes; `mismatch`: one is not; null for other servers), `known` (a key of the host is in `~/.ssh/known_hosts`, also under a hashed name), `changed` (`known_hosts` has another key of the same type for the host), `known_hosts` (its path), `dropped` (lines of the output not read). Below |
| `trust_host_key` | `host`, `port`, `type`, `key` | `path`, `written` (false: it was there): the key appended to `~/.ssh/known_hosts` (the folder and the file made, readable by the user only, when missing) as `host`, or `[host]:port` for another port than 22, only when a new scan still gives it (else `host_key_unknown`) and it is one the service publishes (a `mismatch` is `host_key_unknown`, never written). Without `type` and `key`, every key a new scan gives (none when one is a mismatch) |
| `ssh_public_key` | | `path` and `text` of the first of `~/.ssh/id_ed25519.pub`, `id_ecdsa.pub`, `id_rsa.pub` that holds a public key (at most 16 KiB), or both null |
| `git_info` | | as the bridge's `git_info`, and `credential_helper` ({`configured`, `name`}: the `credential.helper` of git's system and user configuration that applies, credentials hidden) and `author` ({`name`, `email`}: git's configured `user.name` and `user.email`, which New Project shows first; null when git has none) |

`inspect_folder` answers `dir` (absolute) and `kind`, the first that
applies:

- `missing`: the folder does not exist;
- `project`: it has `.mitcad/project.json`;
- `inside_project`: a folder above it has (`project_root` names it);
- `repository`: it is the root of a git repository's work tree;
- `empty`: nothing in it (`.DS_Store`, `desktop.ini` and `Thumbs.db`
  do not count);
- `designs`: `.mitcad` files in it or below;
- `other`: anything else;

and `project_root` (the project the folder is or is in, also for a missing
folder inside a project; else null), `git_root` (the work tree the folder,
or its nearest existing folder upwards, is in; or null), `outer_repository`
(that work tree when it is not the project's or the folder's own),
`history_blocked` (why a project inside another repository, not at its
root, has no versions; null otherwise: a project made in a folder inside
another repository gets a repository of its own, which the other one does
not record), `has_history` (the project's folder is the root of its
repository), `remote` ({`name`, `url` (credentials hidden), `branch`,
`upstream`} or null: the remote the branch follows, else `origin`, else
the only one; null with several and none of these, which Project Settings
asks about: `remote_follow`), `remotes` (all names, of the project's repository or the
folder's own), `cloud` (`has_history` and a remote), `designs` ([{`path`
(relative to the project, else to the folder, with `/`), `modified` (RFC
3339, UTC)}], newest first, at most 1000, `.git` and `.mitcad` folders and
links left out, at most 50 000 entries looked at), `designs_truncated`
(more than listed), `last_design` (from `.mitcad/local/recent.json`
while that design is there, else null), `settings` (the project's shared
settings, checked, as `project_settings`' `shared`; null outside
projects) and `problems` (what of the settings was not used).

`create_project` makes the project in `dir` (missing, empty, a folder of
designs or other files, or a git repository's root; not a project and not
inside one: `inside_project`):

1. With `url`, the remote is checked first and nothing changes on a
   failure (an unreachable remote too): a remote with a project is
   `has_project`; an empty one becomes the project's remote; one with
   files but no project is cloned into `dir`, which must then be missing
   or empty (`not_empty`), and the project is made beside its files
   (`adopted`), or, when `dir` is the root of a repository that shares
   the remote's history (a clone of it), the project is made there
   (`adopted`; the remote's newer versions are synced in before the
   push).
2. The folder, the repository (branch `main`, unless the folder is the
   root of one), `.mitcad/project.json` (with `shared` when given: New
   Project's live updates), `.gitattributes` and `.gitignore` (lines added
   to those there).
3. `author` in the repository's own configuration (`user.name`,
   `user.email`).
4. The first version, `Create project <folder name>`: the project's files,
   `design` written to its `path` (it must not be there yet) when given,
   and with `include_designs` the `.mitcad` files already in the folder
   (other files stay as they are, not versioned).
5. With `url`: the remote set as `remote_set` does (`origin`), then a push
   unless `push` is false.

The answer: `root`, `branch`, `commit` (the first version's id),
`files` (the paths it recorded, sorted), `warnings` (as `commit`'s),
`remote` (as in `inspect_folder`, or null), `adopted` (the remote's files
were taken in), `push` (as `push`'s answer, or null), `sync` (the sync of
a clone with newer versions on the remote, or null), `ahead`, `behind`,
`error`, `log`. A cancel or a failure before the first version leaves the
folder as it was (a folder made for the project is removed; the marker,
`.mitcad`, `.git`, the design, and the `.gitattributes`, `.gitignore`
and repository configuration as they were); a failure after it (the
push) leaves the project made, with the error in `error` and its versions
`ahead`.

`host_keys` runs `ssh-keyscan` (`MITCAD_SSH_KEYSCAN` when set, else the
one of the git installation, as Git for Windows' `usr/bin`, else `PATH`'s,
else Windows' OpenSSH) as git runs: without a terminal, stdin closed, at
most 20 s (10 s per connection), its output at most 64 KiB (more stops
it). The output is untrusted: only lines for the host asked about (`host`,
or `[host]:port`) with a key type `ssh-ed25519`, `ecdsa-sha2-nistp256`,
`-nistp384`, `-nistp521` or `ssh-rsa` and a key that is base64 of a key
blob of that type (at most 8 KiB) are read, at most 16 keys, repeats
dropped; fingerprints are computed in Rust. The fingerprints GitHub,
GitLab and Codeberg publish are built in (`hostkeys.rs`, with their
sources). A server that sends no key is `network`.

Commands of a project (`Project::command`, `command_with` for those that
use the network):

| Command | Fields | Result |
|---|---|---|
| `project_settings` | | `kind` (`local`, `cloud`: the repository has a remote), `shared` ({`edit_locks`: {`enabled`, `idle_minutes`, `poll_seconds`}, `live_updates`: {`broker`, `prefix`} or null}), `local` ({`live`: {`mode`: `project`, `off` or `broker`, `broker`, `prefix`}, `sync`: {`send_at_once`, `check_minutes`}, null meaning the application's default}), `live_updates` (what this computer uses, or null), `problems` (`mitcad_model::file::settings`), `authors` (who made the versions on HEAD's history, what sharing the project publishes: [{`name` (cleaned), `email`, `versions`}], most versions first, up to 10 000 versions read) |
| `set_project_settings` | `shared` (optional), `local` (optional), `author`, `fallback_author` | `shared`, `local`, `written_local`, `commit` (null when the shared settings did not change), `skipped` (why no version was recorded: HEAD detached, a merge in progress; else null). `shared` is merged into the settings as they are, object by object (`{"edit_locks": {"enabled": false}}` keeps the times; `live_updates` is replaced whole, null: none); `local`'s `live` is replaced whole and its `sync` merged. Values are checked strictly (an error, nothing written). A change of the shared ones is written into the marker (its other fields kept) and recorded as a version, `Change project settings: <what changed>` (`edit locks off, idle time 15 min, live updates through mqtts://broker.example.com:8883`), by `author`, else git's configured author, else `fallback_author`, which the application then sends as a saved version |
| `set_identity` | `name`, `email` (both empty: removed) | `name`, `email` (null when removed): the repository's own `user.name` and `user.email` in `.git/config` (a name and an email address without `<`, `>` or control characters, at most 200 characters each). `identity` and the author of versions then come from them first, before git's other configuration and `GIT_AUTHOR_*` |
| `remember_design` | `path` | `path` (relative to the project): written to `.mitcad/local/recent.json` as the design opened last (`inspect_folder`'s `last_design`) |
| `remote_check` | `url` | as before, and `files`, `latest`, `versions` as `check_remote` |
| `connect` | as before, and `onto_files` (default false), `resolutions` and `resolve_all` (as `sync`'s) | as before, and `sync` and `undone`. With `onto_files`, a remote with files but no project and no version in common is set and fetched, and the project's versions (all of them) are replayed after the remote's latest one as `sync` replays unpublished versions: `.gitignore` and `.gitattributes` on both sides are merged by lines (the remote's, then the project's missing ones), another path on both sides is a conflict answered with `resolutions` (or `resolve_all`), then a push unless `push` is false. The branch then follows the remote's default branch (`set`'s `upstream`). `sync` is the sync's answer; when it stops before it changed the project (a conflict without a choice, changes in the folder, a cancel, the network) the remote is removed again (`undone` true) and nothing stays changed. A remote with a project and another history stays `unrelated` |

A branch without an upstream follows `origin`'s branch of the same name,
else (mitcad#89) the only remote's; a push then sets it (all remote
commands).

`mitcad-cli project inspect <folder>`, `project create <folder> --author A
[--url U] [--no-push] [--design <name.mitcad>]` (`--design`: a new empty
design in the first version), `project settings <folder> [--set JSON]
[--author A]` (`--set` takes `{"shared": …, "local": …}`), `remote check
<url>` without a folder (`check_remote`), `remote share <folder> <url>
[--author A] [--resolve <path>=mine|theirs|copy]... [--resolve-all C]
[--no-push]` (`connect` with `onto_files`; conflicts without a choice exit
with 3), `clone <url> <folder> --adopt [--author A]`, `host-keys <host>
[--port P] [--trust]` (`--trust`: `trust_host_key` of every key), each
with `--json` (`tools/cli/projects.cpp`).

Tests: `core/vcs/src/tests/projects.rs` (every kind of folder, a project
inside another repository, several remotes and a branch without upstream;
projects made Local, on an empty remote, beside a remote's README and
`.gitignore`, in a clone of the remote; a remote with a project, an
unreachable one, a cancel and a failure leaving the folder as it was; a
repository adopted; a Local project shared onto a README, with a
`.gitignore` merged, a file on both sides as a conflict and with a choice,
and a remote with another project refused; the project's author and
settings; shared folders; host keys parsed from good, malformed, oversized
and mutated output and for the wrong host, compared with the published
ones, trusted and found under hashed names, with a fake `ssh-keyscan`;
the user's public key; after each `git fsck` and a clean `git status`),
`core/vcs/src/projects/` (the services' keys as their servers send them,
base64, HMAC-SHA1, RFC 3339 times, the cleaning of text) and ctest
`cli.remote`.

## Edit locks

Advisory edit locks of the designs of Cloud projects (mitcad#89), kept on
the remote as refs outside the branches: `refs/mitcad/locks/<id>`, a
parentless commit whose tree holds `lock.json` (authored by the holder),
and `refs/mitcad/lock-requests/<id>/<session>` with `request.json`, so a
requester never races the holder; `<id>` is the SHA-256 (hex) of the
file's path in the project. Mitcad honours them, other git tools do not
see them (`core/vcs/src/remote/locks.rs`; the files and their checks in
`locks/format.rs`; design in
[docs/architecture.md](../../../../docs/architecture.md#edit-locks)).

- **Writes** are compare-and-swap pushes, `git push --porcelain
  --no-verify --no-signed --no-recurse-submodules
  --force-with-lease=<ref>:<expected> <remote> <commit>:<ref>`
  (`<expected>` empty: the ref must not exist); a release is a deletion
  with the same lease. A ref the remote rejects is listed again: changed
  means someone else wrote it first (`changed`), unchanged means the
  remote refuses lock refs (`error` class `unsupported`, "This remote
  does not accept Mitcad's lock references").
- **Reading**: one `git ls-remote <remote> refs/mitcad/locks/*
  refs/mitcad/lock-requests/* refs/heads/<branch>` per command; only when
  a ref points to an object the project lacks, `git fetch --prune
  <remote> +refs/mitcad/locks/*:refs/mitcad/remote-locks/locks/*
  +refs/mitcad/lock-requests/*:refs/mitcad/remote-locks/lock-requests/*`.
  `fetch`, `push`, `sync` and clones name only branches, so they never
  fetch or push lock refs, and lock commits are in no version's history.
- **Untrusted contents**: a lock or request commit is read only when it is
  at most 64 KiB, without parents, with a tree of at most 4 KiB and its
  file a blob of at most 16 KiB; then `format` (`mitcad-lock`,
  `mitcad-lock-request`) and `version` (1; another version is listed as
  unreadable, not guessed at), ids and commit ids as hex of the right
  length, sessions as lowercase UUIDs, times as RFC 3339 from 2000 to
  2200, paths relative without `..` and matching the ref's id, names at
  most 100 characters, messages at most 200, texts cleaned (control,
  bidirectional and other invisible characters removed, whitespace
  collapsed; shown as plain text only), `idle_minutes` clamped to 1–120
  and `poll_seconds` to 5–600. A malformed ref is listed (`readable`
  false, `problem`) and counted once (`dropped`), never an error; a lock
  that cannot be read holds its file until it is stale by the project's
  settings.
- **Stale** locks are judged on this process's monotonic clock, never by
  comparing another computer's clock (the times in locks are only shown):
  `unchanged`, the ref seen unchanged for `idle_minutes` × 60 + 2 ×
  `poll_seconds`; `no_receipt`, this session's request missing from the
  holder's `requests_seen` 2 × `poll_seconds` after it was asked (or the
  holder took the lock; the project's poll interval when both listen to
  the broker); `unanswered`, a receipt without an answer for the idle
  time and two polls (after a `keep`, from the end of the keep). What each
  listing saw is kept per project folder in the process and shared by
  every `Project` of it (the application opens one per task).
  `MITCAD_LOCK_TIME_SCALE` (a factor) speeds this clock up for the
  application's tests.
- **Requests go stale** as locks do (mitcad#89), so that a requester whose
  Mitcad was killed is not asked about for ever: while a request waits
  (no answer, or a `keep`; not declined, not granted), the requester's
  `lock_poll` writes it again every half the project's idle time, as a
  refresh of itself (`refresh_of` its id, `refreshed_at`, and the
  requester's `poll_seconds`: one compare-and-swap push, the request's
  id, place in the order, receipt and answer kept). Another session's
  request whose ref this process has seen unchanged for the project's
  idle time and two of the requester's polls (`poll_seconds` of the
  request, else the project's) is stale: it gets no receipt
  (`requests_seen`), a hand-over without `to` passes it by, it is listed
  with `stale` `unchanged`, and the next `lock_poll` of any session
  removes it with a compare-and-swap deletion (a refresh in between keeps
  it). A declined request is not refreshed, so it goes the same way. A
  session never judges its own requests.

`lock.json` (`"format": "mitcad-lock", "version": 1`): `path`, `owner`
({`name`, `email`}), `session`, `application_version`, `taken_at`,
`refreshed_at`, `active_at` (RFC 3339, UTC), `idle_minutes`,
`poll_seconds`, `mqtt`, `base` (the commit of the version the holder
edits), `state` (`active`, `idle`), `idle_since`, `requests_seen`
(request ids: the receipts), `answers` ({request id: {`answer`:
`declined` | `keep`, `until`, `message`}}), `handed_over_from` and
`taken_from` ({`name`, `email`, `session`}, `taken_from` with `reason`:
`unchanged`, `no_receipt`, `unanswered` or `take_over`). A lock Mitcad
writes always fits in 16 KiB (the oldest answers and request ids are
left out). `request.json` (`"format": "mitcad-lock-request", "version":
1`): `path`, `requester`, `session`, `asked_at`, `message`, and since
mitcad#89's stale requests `poll_seconds` (the requester's git poll
interval, clamped to 5–600) and, in a refresh, `refresh_of` (the
request's id) and `refreshed_at` (shown only); a reader takes a request
without them as before. A request's id is the commit it was first
written as (a refresh names it), so asking again is a new request.

The commands are `Project` commands, run with `command_with` (their git
runs show progress and stop on a cancel); each answers `error` and `log`
as the remote commands do, a failure of git or the network being an
answer. Common fields: `path` (a design, absolute or relative to the
current folder), `session` (the application's UUID for this run; without
it the command line's session, a UUID made from the author's email, the
same in every run), `author` and `fallback_author` (the holder or
requester, as for versions), `mqtt` (this session listens to the
project's broker).

| Command | Fields | Result |
|---|---|---|
| `lock_poll` | `session`, `receipts` (default true), `mqtt`, `poll_seconds` (this session's poll interval, written into its requests' refreshes; default the request's), `tidy` (default true) | one listing; with `tidy` this session's waiting requests refreshed when due and other sessions' stale requests removed (one push); receipts written for the requests to this session's locks; the view below, and `receipts` (the paths whose locks got them), `lost` ([{`id`, `path`, `lock` (as it is now, or null)}]: locks this session held in this process that are not its own any more: taken, or removed), `refreshed_requests` (the paths of this session's requests refreshed) and `removed_requests` ([{`id`, `file_id`, `session`, `path`, `requester`}]: stale requests removed) |
| `lock_status` | `path` (optional), `session`, `network` (default true; false: what the last listing saw, no git), `mqtt` | the view (of one file with `path`, and `path`, `file_id`, `lock` or null). Nothing is written |
| `lock_take` | `path`, `session`, `author`, `idle_minutes` and `poll_seconds` (default the project's), `mqtt`, `base` (default HEAD), `take_over` (a lock's `commit`, as shown when the user confirmed) | `outcome`: `taken` (free, stale, or `take_over` still names it: another session of the same person, or a holder whose application went offline), `refreshed` (this session's already, one handed over to it too: written with these values, this session's request withdrawn), `held` (another session's; nothing written), `changed` (someone wrote it between the listing and the push); `path`, `file_id`, `session`, `lock` (as it is now), `taken_from`, and with `taken` `previous` (the lock as it was, for "Alex's edit lock had expired (last active 14:02)"; null when it was free) |
| `lock_refresh` | `path`, `session`, `active_at` (RFC 3339, or `now`), `state` (`active`, `idle`: `idle_since` follows), `idle_minutes`, `poll_seconds`, `mqtt`, `base` | `outcome` `refreshed` (with the receipts of the requests seen that are not stale; answers of requests that are gone dropped) or `lost`; `lock` |
| `lock_release` | `path`, or `all` true; `session`; `force` | `outcome` (`released`, `not_held`, `free`, `changed`), `released` and `withdrawn` (paths of the locks and requests removed), `changed`, `lock`. With `force` any lock of the file and the file's requests; `all`: this session's locks and requests, with `force` every lock and request ref of the project |
| `lock_hand_over` | `path`, `session`, `to` (a requester's session; default the first request served that is not declined; never a stale one), `base` (default HEAD) | `outcome`: `handed_over` (one compare-and-swap writes the requester as the owner, `handed_over_from` this session, so nobody else can take it in between), `no_request`, `lost`, `free`, `changed`; `to` ({`name`, `email`, `session`}), `lock` |
| `lock_request` | `path`, `session`, `author`, `message` (at most 200 characters), `mqtt`, `poll_seconds` (the requester's poll interval; default the project's) | `outcome`: `requested`, `free` (nothing written: take it), `mine`, `changed`; `request` (its id), `my_request`, `receipt_due_seconds` |
| `lock_withdraw` | `path`, `session` | `outcome` `withdrawn` or `none` (the requester's window closed, or it got the lock) |
| `lock_answer` | `path`, `session`, `request` (its id), `answer` (`declined`, `keep`), `minutes` (keep: 1–120, default 15), `message` | `outcome` `answered` (written into the lock with `until` for a keep), `no_request`, `lost`; `lock` |
| `lock_probe` | `session` | `remote`, `accepted`, `message` ("This remote does not accept Mitcad's lock references" or null), `reason`, `left` (a probe ref the remote did not let go): `refs/mitcad/locks/probe-<hex>` created, listed and deleted |

The **view**: `remote`, `session`, `polled`, `head` (the remote branch's
commit, from the same listing), `tracking` (the remote-tracking
reference's), `newer` (the two differ: newer versions to fetch), `locks`,
`requests_without_lock`, `my_requests` (this session's requests),
`dropped` (malformed lock and request refs seen by this process),
`settings` (the project's `enabled`, `idle_minutes`, `poll_seconds`).

- A **lock**: `id`, `commit`, `readable`, `problem`, the fields of
  `lock.json` (null when it cannot be read; `owner` {`name`, `email`}),
  `mine`, `same_owner` (another session of the same email: "You have this
  design open elsewhere"), `unchanged_seconds`, `stale` (null,
  `unchanged`, `no_receipt`, `unanswered`; never for this session's),
  `stale_in_seconds` (until it is, by the earliest rule), `requests` (in
  the order they are served: the order this process first saw them) and
  `my_request`.
- A **request**: `id`, `commit` (its ref's commit: the id, or the latest
  refresh), `file_id`, `path`, `session`, `requester`, `asked_at`,
  `refreshed_at`, `message`, `mine`, `order`, `waiting_seconds` (since
  this process first saw it), `unchanged_seconds` (since its ref last
  changed), `stale` (null, or `unchanged` for another session's request
  not refreshed in time), `stale_in_seconds`, `seen` (the holder's
  receipt), `answer` ({`answer`, `until`, `message`} or null),
  `readable`, `problem`.
- **my_request**: `id`, `commit`, `file_id`, `path`, `state` (`waiting` for the
  receipt, `seen`, `kept`, `declined`, `granted`: the lock was handed
  over, `free`: there is no lock), `answer`, `stale`, `stale_in_seconds`,
  `tracked` (asked in this process).

The application's lock controller polls every `poll_seconds` (2 minutes
while live updates are connected) with `lock_poll` (with its
`poll_seconds`, so a request it waits on stays alive), takes a design's
lock when it opens (`lock_take`; `held` opens it read-only), refreshes
it every half idle time and when its state changes (`lock_refresh`),
answers requests (`lock_answer`, or `lock_hand_over` after saving and
syncing), releases at idle time or on closing (`lock_release`) and turns a
window read-only when a lock is `lost`; turning locking off or Cloud →
Local releases this session's locks and requests (`lock_release` with
`all`). `sync_plan` takes `session` and `locks` (`last`: the last
listing, no git) and answers `locked` ([{`path`, `lock`}]: the files its
push would send whose lock another session holds, for "Send Anyway";
empty when locks are off or nothing is sent) and `locked_error` (null,
or why the locks could not be read: `locked` is then null).

`mitcad-cli lock status <project-or-file>`, `lock take <file>`, `lock
release <project-or-file> [--force]`, each with `--author` and `--json`
(a lock someone else holds is the exit status 1). Tests:
`core/vcs/src/tests/locks.rs` (two clones of a bare repository and a
`file://` remote: two takers at once, refresh, release, hand-over,
requests, receipts and answers, each stale rule on an injected clock,
a killed requester's request going stale and removed, a waiting
request refreshed with its id, receipt and answer kept, a hand-over
passing declined requests by, a
takeover that fails because the holder refreshed, take-over from another
session, release with `force` and `all`, Sync and fetch never carrying
lock refs, `sync_plan`'s `locked`, a remote whose hook refuses them,
malformed and oversized refs, the readers' checks and a seeded random
run of mutated files; `git fsck` clean), ctest `cli.remote`, and the
fuzz targets in `core/vcs/fuzz` (by hand: `cargo +nightly fuzz run
lock_json`, `request_json`).

## Live updates

The optional MQTT client of Cloud projects (mitcad#89), all in Rust
(`core/vcs/src/remote/mqtt.rs` and `mqtt/`; design in
[docs/architecture.md](../../../../docs/architecture.md#live-updates)):
the socket, TLS (rustls with ring, the broker's certificate checked by
rustls-webpki against the system's root certificates), MQTT 3.1.1 and the
checking of everything received. It is only an accelerator: a message
makes the application poll git at once, and only the lock refs grant a
lock. One connection per broker, user and prefix is shared by the open
projects on it; each project subscribes to `<prefix>/<project>/#`, the
project id being the repository's first commit.

The bridge has a `LiveHub` (`new_live_hub()`) with `command(json)` (an
error only for a command that is wrong; what happens on the network comes
as events) and `events(timeout_ms)`, a blocking read of the checked events
run on a worker thread. Both may run at once on two threads. The
application never sees MQTT's bytes.

### Topics and payloads

| Topic | Payload (`format`) | |
|---|---|---|
| `<prefix>/<project>/locks/<file>` | `mitcad-live-lock` | retained; empty when the lock is released |
| `<prefix>/<project>/requests/<file>` | `mitcad-live-request` | QoS 1, not retained: requests, receipts, answers, withdrawals |
| `<prefix>/<project>/versions` | `mitcad-live-version` | QoS 1, not retained: a branch and commit after a push |
| `<prefix>/<project>/open/<file>/<session>` | `mitcad-live-open` | retained; empty when the window closes |
| `<prefix>/sessions/<session>` | `mitcad-live-session` | retained `online` after each connect; the connection's will `offline`; empty after a clean disconnect |

`<file>` is the SHA-256 (hex) of the file's path in the project (`/`
between folders), `<session>` the application's session (a UUID in
lowercase). A session's presence is not under a project, since one
connection has one will and serves every project on the broker (which is
also why the connection is per prefix). Payloads are JSON with `format`
and `version` (1); a reader takes only the topic's format and version 1
and ignores fields it does not know. No email address is sent (the broker
may be more public than the repository).

| Format | Fields |
|---|---|
| `mitcad-live-lock` | `path`, `owner` ({`name`}), `session` (the holder's), `state` (`active`, `idle`), `taken_at`, `active_at`, `idle_since`, `idle_minutes`, `poll_seconds`, `commit` (of the lock ref) |
| `mitcad-live-request` | `kind` (`request`, `receipt`, `answer`, `withdrawn`), `session` (the sender's), `name`, `at`, `for` (the requester's session; receipts and answers), `answer` (`released`, `keep`, `declined`), `until`, `message` |
| `mitcad-live-version` | `branch`, `commit`, `session`, `name`, `at` |
| `mitcad-live-open` | `name`, `mode` (`editing`, `read-only`), `since` |
| `mitcad-live-session` | `state` (`online`, `offline`), `since` |

Everything received is checked (mitcad#89, "Untrusted input"): a packet's
remaining length at most 64 KiB, read only after that check (larger: the
connection ends, class `protocol`); more than 200 messages in a second
end the connection (`flood`), which connects again after the back-off
(the retained copies a broker sends at once after a subscribe count
apart, up to 2000 a second, so that a project with many designs open can
subscribe); topics only under the connection's prefix, of the forms
above, of a subscribed project or a followed session; no retained copies
of requests and versions; payloads at most 16 KiB of
UTF-8 JSON of the right format and version; ids as lowercase hex of their
length, sessions as UUIDs, times RFC 3339 between 2020 and 2100 (given
back in UTC), paths relative without `..`, a lock summary's path the one
its topic's id is of, branch names as git allows them (stricter),
`idle_minutes` and `poll_seconds` clamped to the project settings' bounds,
names 1 to 100 characters and messages at most 200 after cleaning (control
characters, bidirectional overrides and other invisible format characters
removed, whitespace collapsed); at most 64 retained messages per file and
10 000 per project. A message that fails is dropped and counted, never an
error.

### Commands

| Command | Fields | Answer |
|---|---|---|
| `connect` | `broker` (`mqtts://host[:port]`, port 8883, or `mqtt://host[:port]`, 1883: plain text), `prefix` (default `mitcad`; letters, digits, `-`, `_`, `/`), `session` (the application's; one per hub), `user`, `password` (only with `mqtts://` and a user: else the class `insecure`, before anything is sent), `ca` (a PEM file of certificate authorities trusted besides the system's) | `connection` (`c1`), `broker`, `prefix`, `user`, `tls`, `state`, `warning` (for `mqtt://`: the messages are plain text). Opens the connection of the broker, user and prefix, or gives the open one; with a new password, or when it is `offline` or `failed`, it connects again at once (with these `password` and `ca`) |
| `subscribe` | `connection`, or the fields of `connect`; `project` (the first commit id, 40 or 64 hex) | `connection`, `project`, `topic`, `state`; a `subscribed` event follows |
| `unsubscribe` | `connection`, `project` | `subscribed` (it was), `closed`: the project's open entries of this session are cleared first; the connection's last project closes the connection |
| `watch` | `connection`, `session` | follows a session's presence (`session` events); sessions named in a project's messages are followed without it, at most 512 per connection |
| `publish` | `connection`, `project` (subscribed on it), `message` (below) | `topic`, `qos`, `retain`, `connected`; the message as others will read it (checked, cleaned, times in UTC, numbers clamped), sent at once or after the reconnect |
| `disconnect` | `connection` (none: all), `wait_ms` (at most 10 000, default 0) | `closed` (the ids), `finished` (all disconnected within `wait_ms`): this session's open entries and presence cleared, DISCONNECT (no will), in the connection's thread |
| `status` | | `session`, `closed`, `connections` ([{`connection`, `broker`, `prefix`, `user`, `projects`, `state`, `error`, `retry_in_ms`, `connected_since`, `tls`, `connects`, `received`, `dropped`, `dropped_reasons` ({reason: count}), `inflight`, `queued`, `watched_sessions`}]): the lock details' MQTT state |
| `test` | the fields of `connect` but `session`; `timeout_ms` (per step, default 10 000), `text` | `ok`, `broker`, `prefix`, `user`, `tls`, `topic`, `steps` ([{`step`: `connect`, `sign_in`, `subscribe`, `publish`, `receive`; `ok`, `ms`}], up to the one that failed), `error`, `warning`; with `text` the same as text for people. Project Settings' Test: a connection of its own subscribes to `<prefix>/test/<random>`, publishes once and waits for the message. Blocks: run it on a worker thread |
| `close` | `wait_ms` | as `disconnect` of all (quitting: `wait_ms` lets the others see a clean leave rather than the will); then `events` answers at once with `closed` (the reading thread ends) and `connect` fails (`closed`) |

The messages of `publish` (`type` and its fields):

| `type` | Fields | Sent |
|---|---|---|
| `lock` | `path`, `lock`: {`owner` ({`name`}; lock.json's owner may be given, its `email` is not sent), `session` (default this session), `state` (default `active`), `taken_at`, `active_at` (default now), `idle_since`, `idle_minutes`, `poll_seconds` (default the settings' defaults), `commit`}, or null: released | retained, QoS 1 |
| `request` | `path`, `name`, `message` (optional) | QoS 1 |
| `receipt` | `path`, `name`, `for` | QoS 1 |
| `answer` | `path`, `name`, `for`, `answer`, `until` (with `keep`), `message` | QoS 1 |
| `withdrawn` | `path` | QoS 1 |
| `version` | `branch`, `commit`, `name` | QoS 1 |
| `open` | `path`, `name`, `mode`, `since` (default now) | retained, QoS 1 |
| `closed` | `path` | the open entry cleared |

The session keeps its own retained state (its locks' summaries and its
open entries) and publishes it again after each reconnect, as the broker
had it last; a lock summary of another session (a hand-over) is published
once, and another session's summary received for one of this session's
locks (a takeover) ends that. QoS 1 messages without a PUBACK are sent
again after 20 s and after a reconnect; while offline up to 256 wait.

### Events

`events(timeout_ms)` answers `{"events": [...], "lost": n, "closed":
bool}`: at most 1000 events a read, `lost` those pushed out by the limit
of 10 000 waiting. Every event has `type` and `connection`; those of a
project `project`, `retained` (the broker's retained copy, after a
subscribe) and `own` (sent by this session).

| `type` | Fields |
|---|---|
| `state` | `broker`, `prefix`, `state`: `connecting`, `connected`, `offline` (connects again in `retry_in_ms`: 1 s doubling up to 5 min, ±20 %), `failed` (not again until a new `connect`: `auth_failed`, `certificate`, `refused`, `invalid`), `closed`; `error` ({`class`, `message`} or null) |
| `subscribed` | `project`, `error` (the broker refused): after each SUBACK, also after a reconnect: what was known of the project's retained topics is to be forgotten, their retained copies follow |
| `lock` | `file`, `lock` (the summary, checked) or null (released) |
| `request` | `file`, `kind`, `session`, `name`, `at`, `for`, `answer`, `until`, `message` (fields not sent are absent) |
| `version` | `branch`, `commit`, `session`, `name`, `at` |
| `open` | `file`, `session`, `entry` ({`name`, `mode`, `since`} or null: closed), `cleared` (true: cleared on the broker by this session because that session went offline; given once the broker acknowledged it) |
| `session` | `session`, `state` (`online`; `offline`: its connection was lost, the will; `left`: it disconnected cleanly), `since` |
| `dropped` | `reason`, `topic` (cleaned, at most 200 characters), `count` (the connection's total; one event stands for a run of drops) |

Error classes: `invalid` (a wrong command or field), `insecure` (a
password without TLS), `network`, `timed_out`, `certificate` (not
trusted, another host name, expired; or no root certificates on the
computer), `tls`, `auth_failed` (the user name or password refused, not
authorised; in `test` also a topic the broker does not let the user
read or write), `refused`, `protocol` (the broker broke MQTT's rules or
sent a packet over 64 KiB), `flood`, `closed`, `internal`.

When a session goes offline (its will), the sessions that follow it
clear its open entries on the broker (they become `open` events with
`cleared` once the broker has acknowledged the clearing, so that a
session subscribing after that event does not get them); its lock
summaries stay, as its lock refs do. Another session's open entry is
given only while that session is known to be online: the retained copies
after a subscribe come before the session's presence, so an entry waits
until its presence says `online` (then it is given), `offline` (then it is
never given and is cleared on the broker, as above, without an event) or
nothing yet (a session that left, or whose presence has not come: it
waits). A closing is given only for an entry that was given. When the
broker refuses the subscription to a session's presence, or the limit of
followed sessions is reached, its entries are given as they come.

`mitcad-cli live test <broker> [--prefix P] [--user U] [--ca FILE]
[--timeout S] [--json]` runs `test`, the password from
`MITCAD_MQTT_PASSWORD`. Tests: `core.vcs` (`remote::mqtt`: packets,
limits, payloads, the cleaning of text, a seeded random run of mutated
input, and scenarios against a broker in the test and against mosquitto
when it is installed: TLS with a certificate authority made with openssl,
the will (seen by a follower and by a newcomer), retained messages,
reconnecting, one connection for two
projects, a flood, a password refused over plain text), `core.bridge`,
`cli.live_*`; fuzz targets in `core/vcs/fuzz`.

### The project's id

| Command | Fields | Answer |
|---|---|---|
| `project_id` (a `Project` command) | | `project`: the first commit of HEAD's first-parent chain (hex), the same in every clone; null before the first version |

The application subscribes a Cloud project under it
(`app/files/LiveController.cpp`, [app/COMMANDS.md](../../../../app/COMMANDS.md#live-updates)).
Test: `core/vcs/src/tests.rs`
(`the_project_id_is_the_first_commit_of_the_history`).

## Component libraries

Component libraries and the community library (mitcad#64, mitcad#63) are
git repositories of Mitcad designs with a manifest; their format is in
[docs/libraries.md](../../../../docs/libraries.md). The model knows no
git: it reads a library's component at a version through a
`LinkResolver` (`core/model/src/library.rs`), which `mitcad-vcs`
implements over the cache of fetched libraries
(`core/vcs/src/library/`). The bridge's `configure_libraries(json)` sets
the cache (`{"root"}`, default `MITCAD_LIBRARIES_DIR`, else the user's
data folder) and installs the resolver for every document; Rust code can
give a document its own (`Document::set_link_resolver`).

### Configuration tables

A design's table of sizes (`configurations` in the project file, left out
when empty): `selectors` (the cascaded choices, may be empty),
`parameters` (names of the design's parameters the rows set), `default`
(a row's name; the first row when left out) and `rows`: `name` (unique),
`select` ({selector: value}, a unique combination), `values` ({parameter:
expression}; a parameter left out keeps its own) and an optional
`designation`.

| Command | Fields | Result |
|---|---|---|
| `set_configurations` | `configurations` (the table; null or an empty table removes it) | `rows`; refused with the first problem; undo step `Edit Configurations`, nothing recomputed |
| `apply_configuration` | `name` | `changed` (parameters whose values changed); undo step `Apply M5x16` |

| Query | Result |
|---|---|
| `configurations` | `configurations` (the table or null), `selector_values` ({selector: values in natural order: `M2.5`, `M3`, `M10`}), `current` (the row whose expressions the parameters have, or null), `problems` |

Problems: a parameter that does not exist or is listed twice, a row
without a name, a value for a selector or parameter not in the table, a
missing selector value, two rows with the same name or the same
selection, a value its parameter refuses (unit, cycle, syntax), a default
that is no row. Opening a file with such a table fails with the first.

### Library parts

`insert_component` with `library` instead of `path` inserts a library's
component: `{"id": "<library id>", "url": "<where it was fetched
from>", "rev": "<commit or tag>", "component": "<component id>",
"config": "<row>"}` (`config` left out: the table's default), with
`link`, `transform` and `name` as for files. The design is read at that
version, the row applied to a copy of its parameters, and:

- linked (default): its visible bodies in a base feature of a new,
  read-only component (one body is named after the designation), its link
  `path` the design's path in the library;
- a copy (`"link": false`): the design with its history in the row (the
  table is not copied), editable.

Either way the component records `library` (in the project file too):
`library` (id), `url`, `rev` (the commit, also for a tag), `label` (the
commit's tag), `component`, `path`, `config`, `license`, `authors` and
`designation` (`ISO 4762 M5x16`). The component is named after the
designation unless `name` is given; the undo step is `Insert <name>`.

- **Versions stay.** `update_links` (when a design opens) reads every
  linked library part at its recorded commit and takes new bodies only
  when that design or row differs from what was saved; a library or
  version that is not on this computer keeps the saved bodies with a
  message (`... library mitcad-fasteners v1.0.0 (3f9a2c1) (url) cannot
  be read (...); it keeps the bodies saved with the design`). Nothing
  follows a library's newest version on its own.
- **Updates are explicit.** `update_library_parts` with `changes`:
  [{`component` (uid or name), `rev` (another version), `config`
  (another row)}] changes linked parts as one undo step (`Update <name>
  to v1.1.0, M5x20` or `Update Library Parts`) and returns `changes`
  (`ISO 4762 M5x16: version v1.0.0 (3f9a2c1) -> v1.1.0 (7d0e5b2)`, `...
  size M5x16 -> M6x20`). A part named after its designation, and its
  body, take the new designation. A copied part is refused (it does not
  follow its library).
- The comparison of versions (`diff`) shows a part's changes field by
  field (`library.rev`, `library.config`, ...) and a table's changes row
  by row (`configuration M5x16: dk 8.5 mm -> 8.7 mm`).

| Query | Result |
|---|---|
| `library_parts` | `parts`: `component`, `name`, `linked`, `occurrences`, `library` (as recorded), `version` (`v1.0.0 (3f9a2c1)`) |
| `parts_list` | `rows`, one per component other than the root in the order they were made: `component`, `name`, `designation` (a library part's, else the name), `quantity` (how many times the design places it, through every occurrence of the components it is in), `linked`, `library`, `version`, `config`, `license`, `authors`, `url` |

### Library commands

Commands of the cache of libraries, not of a document: the bridge's
`library_command(json, control)` (a fetch reports its progress to the
`SyncControl` and stops when it is cancelled) and `describe_library` (the
answer as text); `mitcad-cli library`. A fetch's failure is an answer
with `error` (`class`, `message`, `detail`, as the remote commands') and
`log`; other failures are errors.

| Command | Fields | Result |
|---|---|---|
| `library_fetch` | `url` (https, ssh, a file:// URL or a folder; never with credentials) | `kind` (`library`, `index`), `id`, `name`, `url`, `dir`, `cloned`, `head`, `versions`, `problems` (`errors`, `warnings`), `error`, `log`: a bare clone into the cache (`git clone --bare`), or new branches and tags fetched into it; a repository that is neither, or larger than 200 MB, is refused and leaves nothing |
| `library_list` | | `root`, `libraries`: `dir`, `url`, `kind`, `versions`, and a library's `id`, `name`, `description`, `license`, `authors`, `components` (count), `problems`, an index's `name`, `entries` |
| `library_show` | `url` or `id`, `rev` (default: the newest version) | `dir`, `url`, `rev`, `labels`, `versions`; a library's `manifest`, `problems` and `components` (`id`, `name`, `path`, `category`, `standard`, `description`, `keywords`, `license`, `license_note`, `preview`, `designation`, `configurations`: `selectors`, `selector_values`, `default`, `rows` with `name` and `select`); an index's `name`, `description`, `entries` |
| `library_search` | `text` (words that must all match name, id, standard, description, category, keywords, the library), `licenses` (SPDX ids an item may have; left out: any), `unlicensed` (default false: items without a licence are hidden), `category`, `libraries` (ids or URLs) | `components` (of fetched libraries at their newest versions: as `library_show`'s with `library`, `library_name`, `url`, `rev`, `version`, `authors`), `libraries` (index entries with `index`, `index_url`, `fetched`, `license_note`), `hidden` (left out for their licence) |
| `library_preview` | `url` or `id`, `rev`, `component` | `png` (base64) or null |
| `library_diff` | `id`, `url`, `from`, `to`, `parts` ([{`component`, `config`}]) | `from`, `to` (commits), `from_labels`, `to_labels`, `library` (licence and version changes), `parts`: `changed`, `missing` (the component or row is not in `to`), `lines` (the row's value changes), `text`, `design` (the comparison of the design without its table) |
| `library_check` | `dir` (a library folder) | `id`, `components`, `ok`, `errors`, `warnings`: the manifest, the files it names (there, no symbolic links, designs that open and link no other files, PNG previews of at most 512 KB), a licence file |
| `library_init` | `dir`, `id`, `name`, `description`, `license`, `authors`, `homepage` | `dir`, `id`, `written`: the manifest, a `LICENSE` note naming the licence, a README, and a Mitcad project with a git repository (no version yet) |
| `library_add` | `dir`, `text` (a design as a single project file), `id`, `name`, `category`, `category_name`, `standard`, `description`, `keywords`, `license`, `designation`, `preview` (a PNG, base64), `path` (default `<category>/<id>.mitcad`) | `component`, `written`, `errors`, `warnings` (a check of the folder); a component of the same id is replaced |
| `index_entry` | `dir`, `url` (where the library is published), `rev` (default HEAD) | `path` (`libraries/<id>.json`), `rev`, `labels`, `entry`, `text` |
| `licenses` | | `licenses`: the SPDX ids a community index accepts with a `note` for users |

`mitcad-cli library fetch|list|show|search|diff|init|add|check|index-entry`
and `mitcad-cli parts <file>` (the parts list); inserting and updating
parts are the commands above in a script (`run --open --save`). The
metric fastener library is made by
`tools/libraries/make-fastener-library.py`.

Tests: `core/model/src/library_tests.rs` (tables, parts with a resolver
in memory: versions kept, explicit updates, copies, the parts list),
`core/model/src/configurations.rs`, `core/vcs/src/tests/library.rs`
(libraries made, tagged and fetched from folders and file:// URLs,
versions, the resolver, comparisons, checks, an index; skipped without
git), ctest `cli.library` (`tools/cli/library-test.cmake`: the generator,
fetch, insert, parts list, diff, update; needs git and Python) and
`tools/ui-library-test.sh`.

## Progress and cancellation

A document can be given a `RecomputeMonitor` (`Document::set_monitor`,
`core/model/src/monitor.rs`), shared with another thread that shows the
progress and may cancel. Every recompute then reports to it and checks
for a cancel before each feature before the marker. The bridge gives it
to C++ as a `JobControl` (`core/ffi/src/jobs.rs`). Threading and the
application's worker:
[docs/architecture.md](../../../../docs/architecture.md#application).

| Rust | Meaning |
|---|---|
| `RecomputeMonitor::cancel()`, `is_cancelled()` | ask the computation to stop; it stops before the next feature, or when the evaluation running now returns, which the kernel's long operations cut short. A monitor stays cancelled: a new job takes a new one |
| `progress()` | `position` (the features before the one computed, shown as `position + 1` of `total`; `total` when done), `total` (features before the marker), `evaluated` (evaluations kept in the cache so far, in all recomputes of the job), `feature` (the name of the feature evaluated last; a cache hit leaves it), `cancelled` |
| `set_test_delay(ms)` | for tests: each evaluation takes `ms` longer, in 10 ms steps that end on a cancel |
| `cancel_after(n)` | for tests: cancel once `n` evaluations were kept, as between two features |
| `Document::try_recompute`, `try_undo`, `try_redo` | fail with `ModelError::Cancelled`; `recompute`, `undo` and `redo` stay for callers without a monitor (a cancelled `undo` gives None, a cancelled `recompute` keeps the last results) |

| C++ (`mitcad_bridge/lib.h`) | Meaning |
|---|---|
| `new_job_control()` | a `rust::Box<JobControl>`, one per job |
| `JobControl::cancel()`, `is_cancelled()`, `set_test_delay(ms)` | as the monitor's; both threads may call them at once |
| `JobControl::progress()` | a `JobProgress {position, total, evaluated, feature, cancelled}` |
| `Document::attach_job(control)`, `detach_job()` | the document's recomputes report to the control until detached |

- A cancelled computation rejects what ran it with `the computation was
  cancelled` (`Cancelled::MESSAGE`, `ModelError::Cancelled`): a command
  (`recompute`, `undo` and `redo` too), a `preview`, the `update_links`
  of an opened project. The document stays as it was: the definition
  state, the undo and redo steps, the revision, the results of the last
  recompute and `recompute_count`. The exception is `import_f3d` with the
  timeline, which a cancel stops early with what it imported ([.f3d
  import with the timeline](#f3d-import-with-the-timeline)).
- A cancelled preview leaves no preview (`preview_body_shape` is null).
- An array of commands stops at the cancelled one as at any error
  (`commands[2]: the computation was cancelled`); the commands before it
  stay done.
- Results evaluated before the cancel stay in the cache, so trying again
  continues where it stopped. An evaluation that returns after the
  cancel request is not cached.
- Inside an evaluation: a recompute with a monitor runs each evaluation
  through `Kernel::interruptible(monitor, ...)`. The OCCT kernel
  (`core/ffi/src/kernel/cancel.rs`) gives the geometry the monitor as its
  `CancelSource` (`geometry/include/mitcad/geometry/cancel.hpp`) for the
  evaluation's thread; booleans, fillets and chamfers, sweeps, lofts,
  shells and offsets, drafts, splits and sewing run with a progress range
  whose user break asks it, and the geometry also asks between its own
  steps (`throw_if_cancelled`); a stopped operation throws `Cancelled`
  ("the operation was cancelled"). OCCT's checker (`BRepCheck_Analyzer`)
  cannot be stopped. Without a monitor nothing stops early.
- Commands that compute nothing (renames of parameters, comments, named
  views) are not cancelled.
- A document is used by one thread at a time; the other thread may hold
  shapes of earlier results meanwhile but must not read them (OCCT's
  algorithms set flags on the sub-shapes of their inputs, which earlier
  results share).
- The .f3d imports (`import_f3d` with or without the timeline) run
  without the control: they build the document in several steps and are
  not undone as a whole.
- Tests: `core/model/src/monitor_tests.rs`; `core/tests/test_bridge.cpp`
  (a cancel from another thread, jobs handed between two threads, a
  modelled thread cancelled inside OCCT); `geometry/tests/test_cancel.cpp`.
  A ThreadSanitizer build of `test_bridge` (by hand, not in ctest)
  reports the progress read of the feature name: the Rust mutex is not
  instrumented.

## Caches

How the recompute cache and the result store work:
[docs/architecture.md](../../../../docs/architecture.md#the-model).

### Result store

Results of costly evaluations can be kept on disk, so that opening a
design again (or a copy, or an older version of it) does not evaluate
them again. A result's version is a fingerprint of the feature's uid, its
definition and what it read (`core/model/src/fingerprint.rs`), the same
in every document and process. Store code and file format:
`core/model/src/store.rs` (module comment).

| Rust | Meaning |
|---|---|
| `ResultStore::new(dir, build_id).with_label(name)` | a store folder; results stored by another `build_id` are not used. The label (the project's file name) is kept with the results written |
| `Document::set_result_store(Some(store))` | recomputes take results from it when the memory cache has none; None (the default) for none |
| `Document::persist_results(min_time)` | writes the results of the last recompute that are not in the store yet: features that succeeded, took at least `min_time`, and whose results the store takes; returns `PersistReport {results, shapes, bytes, failed, errors, time}`. A preview's results are not written |
| `store::gc(dir, budget)`, `store::clear(dir)` | `gc` removes the files used longest ago until 80 % of `budget` bytes are used (when more is), and temporary files more than an hour old; `clear` removes all. `GcReport {files, bytes, removed, removed_bytes}` |
| `RecomputeStats::restored`, `restore_times` | the features whose results came from the store in the last recompute, and how long reading each took |

| C++ (`mitcad_bridge/lib.h`; `core/ffi/src/result_store.rs`) | Meaning |
|---|---|
| `Document::set_result_store(dir, build_id, label)` | as above; an empty `dir` for none. `build_id`: `mitcad::geometry::kernel_build_id()` (the shape format, OCCT's and Mitcad's versions, the size and time of the running program's file) |
| `Document::persist_results(min_ms)` | JSON `{"results", "shapes", "bytes", "failed", "errors", "ms"}` |
| `result_store_gc(dir, budget_mb)`, `result_store_clear(dir)` | JSON `{"files", "bytes", "removed", "removed_bytes"}`; any thread may call them |

- Not stored: sketches and construction features, features that move
  occurrences, base features that only bring in bodies, failed features,
  results evaluated with a parameter renamed since (their versions hold
  the old name). A result read from the store is in the memory cache
  afterwards and is not written again.
- The store keeps up to four variants per feature uid and definition,
  for different inputs. A file that cannot be read (damaged, cut short, a
  shape the kernel cannot read) is removed and the feature evaluated; a
  file of another build is passed over and replaced when the feature is
  stored again. A hit touches the file's time, for `gc`. Files are
  written to a temporary file and renamed, so programs sharing the folder
  see whole files.
- Kernel: `Kernel::shape_bytes` and `Kernel::shape_from_bytes` (the shape
  with its face names, notes and measured properties; OCCT:
  `geometry::serialize_shape`).
- `mitcad-cli info <file> --result-store DIR [--persist-min-ms MS]` (also
  `run --open`) opens the project with the store, writes the results
  that took at least `MS` (default 100) and prints `Result store:
  restored from store: N, evaluated: M; stored: K (B bytes)`.
- Tests: `core/model/src/store_tests.rs`, `version_tests.rs`;
  `geometry/tests/test_persist.cpp`; ctest `cli.store_fill`,
  `cli.store_hit`, `cli.store_edit` (the perf model with a store).

### Memory cache and diagnostics

`Document::set_memory_budget(Some(bytes))` (the bridge's
`set_memory_cache_budget(megabytes)`, 0 for none) limits the estimated
size of the memory cache's results: `Kernel::shape_memory` of each
result's shapes (OCCT: `geometry::memory_estimate`; parts shared with
other results are counted with each), a sketch's regions and what it
read. Beyond it the results used longest ago go, except those the
document's results and its preview hold; with a result store they come
back from there. Each feature keeps at most eight results whatever the
budget; without a budget (the default) nothing else goes.

| Command / query | Fields | Result |
|---|---|---|
| `clear_cache` | | drops the cached results the document's results and preview do not hold: `results`, `bytes`. No undo step, the revision stays |
| `cache` query | `disk` (default false: also read what the store's files hold) | `memory`: `bytes`, `budget` (null for none), `used` (bytes / budget), `results`, `features`, `lookups`, `hits`, `misses`, `restored` (from the store), `evicted`, `evicted_bytes`, `by_feature` (`uid`, `name`, `type`, `results`, `bytes`; the largest first), `by_type` (`type`, `features`, `results`, `bytes`); `store` (null without one): `dir`, `build_id` and what the stores of this process did: `lookups`, `hits`, `misses`, `damaged`, `other_build`, `read_bytes`, `read_ms`, `writes`, `write_bytes`, `write_ms`, `write_errors`, and with `disk` its `disk`: `files`, `bytes`, `other_builds` {`files`, `bytes`}, `damaged`, `by_document` (`document`, `features`, `results`, `bytes`), `by_feature` (`document`, `uid`, `name`, `type`, `results`, `bytes`), `by_type`, the largest first; `last_recompute`: `evaluated`, `restored`, `cached`, `evaluate_ms`, `restore_ms`, `features` in timeline order (`uid`, `name`, `type`, `source`: `evaluated`, `store`, `cache`, `suppressed` or `rolled_back`; `ms`: how long it took now; `evaluated_ms`: how long its evaluation took) |

The bridge's `query` adds to the `cache` answer `process`: `resident`,
`peak_resident` and `heap` bytes (`geometry::process_memory`, OCCT's
`OSD_MemInfo`). `mitcad-cli info <file> --diagnostics` prints it
(`Cache: {...}`, with `disk`). Tests: `core/model/src/cache_tests.rs`,
ctest `cli.store_diagnostics`.

## Geometry of other components

A sketch belongs to its component and lies in that component's
coordinates, but its plane and its projections may be geometry of
another component (mitcad#100, `core/model/src/links.rs`): a planar face,
a construction plane or an origin plane, and edges, faces and vertices to
project. Such geometry is named with where it was picked: `occurrence`,
the occurrence path from the root to its component (`"O1"`, `"O1/O4"`,
`""` for the root), and optionally `context`, the path the sketch's own
component is seen at (default: the first path to it). The model keeps
both as an `OccurrenceLink` (`{"source": "O1", "target": "O2"}`):

- `sketch.create` with `occurrence` stores the plane's link as the
  sketch's `plane_link`; `sketch.project` with `occurrence` moves the
  picked geometry into the sketch, and a linked projection keeps the link
  in its record (`projections[i].link`). Geometry in the sketch's own
  component needs no link, and none is stored for it.
- Evaluation finds the geometry in its component at the sketch's point of
  the timeline and moves it by `placement(target)⁻¹ ·
  placement(source)`, the occurrences' placements there. The sketch
  follows when the geometry changes or either occurrence moves; the
  cache keys on those placements and on the other component's bodies the
  evaluation read. A face's frame is the frame of every planar face,
  taken in the sketch's coordinates; a construction or origin plane keeps
  its own axes.
- A target that no longer leads to the sketch's component (a copy, an
  occurrence deleted) is replaced by the first path to it; a source path
  that leads nowhere fails the sketch with the reason. The sketch must
  come after the features that make the geometry, as with any reference.
- Without `occurrence`, another component's body fails the sketch as
  before (`body F3.b0 (from Extrude2) belongs to A; …`), a construction
  plane of another component is refused, and `sketch.project` refuses a
  body of another component (`… is a body of A, not of B; pick it where
  an occurrence of A shows it`).

The `sketch` query shows `plane_link`. A combine's tools may be bodies of
another component the same way (`tool_links`, [combine](#combine),
mitcad#104). Tests: `core/model/src/link_tests.rs`,
`core/model/src/links.rs`.

## Adding to the API

- A feature type: a module in `core/model/src/features/` with its
  definition struct (generic over the value form `P`), `map_params`,
  `FeatureInfo` and `Evaluate`, and one line in `feature_types!`
  (`features/mod.rs`). Its serde form is then its command and file form;
  the dispatch, the cache, undo, preview and the project file need no
  change. Add its section under [Feature definitions](#feature-definitions).
- Patterns and mirrors of features: `FeatureInfo::tool_use()` returns the
  operation and participants of the tool body the feature leaves in
  `FeatureOutput::tool`; without it a feature cannot be patterned
  (extrude, revolve, hole, sweep, loft, pipe, coil, helix, rib, web and
  the primitives have it). `Evaluate::placed_tool(ctx, feature,
  placement)` optionally rebuilds the tool with the feature's own
  placement moved by a rigid transform, faces named after `feature`, for
  `adjust` (the primitives); without it the stored tool is moved. Reading
  another feature's tool goes through `EvalContext::feature_tool`, which
  the recompute cache tracks by the version of that feature's result.
- A kernel operation: a `Kernel` method with a default
  `Err(KernelError::Unsupported(…))` (`kernel.rs`), a family bridge in
  `core/ffi/src/kernel/` with its C++ half in `core/cpp/bridge/`, and the
  geometry in `geometry/src/<family>.cpp`.
- A command or query: a variant of `Command` or `Query` in `mod.rs` and a
  line in the tables above.

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
  exact size it takes a face away, which OCCT cannot build), a sketch
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
  follow the source when the model changes (`sketch.project`).
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
separate regions. A region reference whose key no longer exists means the
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
  `whitworth`, `npt`), `designation`, `class`, `right_handed` (default
  true), `modeled` (ISO metric and Unified only), and `length`, `offset`
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
  `npt`), `designation` (`M10x1.5`, `M10` for the coarse pitch, `1/4-20
  UNC`; Whitworth `1/4-20 BSW`, `1/4-26 BSF`, the parallel pipe threads
  `G 1/4`; NPT `1/4-18 NPT`), `class` (`6g` external, `6H` internal;
  Whitworth `Close`, `Medium`, `Free`, `A`, `B` external, `Medium`,
  `Normal` internal; NPT `Standard`; checked against the face),
  `right_handed`. Sizes: `core/model/data/threads.json` (the
  `thread_sizes` query). An NPT size is listed at its pipe's outside
  diameter; its 1:16 taper is not kept.
- `modeled`: cut the basic 60° profile into the faces (an external crest
  on the cylinder, an internal one at the bore; ISO metric and Unified
  only, the others are cosmetic only); otherwise cosmetic: no geometry,
  listed by the `threads` query.
- `length` and `offset`: part of each face, from the end its cylinder's
  axis points to (`location`: `high_end`, default) or from the other
  (`low_end`); the whole face when left out, running out through both
  ends.

A modelled thread's groove faces are `thread(<face>)`.

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
  direction and only moves).
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
`unsupported: …`). Sharp corners of a plain sweep are mitred.

Faces: `side(<segment>)` from each profile segment, `#k` pieces where the
path has several curves; `start(<region>)` and `end(<region>)`: with the
profile at an end of the swept part `start` is at the profile, otherwise
`start` ends side one (beyond the profile) and `end` side two, as for
extrusions. A sweep round a whole closed path has no caps.

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
  features that leave a tool, see [Adding to the API](#adding-to-the-api))
  or `faces` of one body (with planar caps where they meet the body they
  bound a boss, joined, or a pocket, cut). A pattern or mirror of
  features among the `features` (a pattern of patterns) stands for its
  originals at each of its elements (the original's too, not the
  suppressed ones): the copies go to every product of the inner and the
  outer elements, the inner one applied first. Patterns of bodies or
  faces cannot be patterned again.
- Quantities include the original. `distance_type`: `spacing` between
  neighbours or `extent` from the first element to the last. A negative
  distance or angle goes the other way. A full turn spaces a circular
  pattern by angle / quantity, a partial angle by angle / (quantity − 1).
  `symmetric`: the quantity (and extent or angle) on each side of the
  original. Path distances are arc lengths along the path; `start` (0…1)
  is where the original sits on the path; `along_path` turns the copies
  with the path.
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
- Names: copied faces are `<pattern>:inst<element>(<original face>)`,
  e.g. `F5:inst4(F4:side(c1))`; their edges follow. A pattern of a
  pattern names its copies of the inner copies around the inner names
  (`F6:inst1(F5:inst2(F4:side(c1)))`). Copies of bodies are
  bodies `<pattern>.b<k>`, k = (element − 1) · (number of bodies) +
  (position of the body), so suppressing an element keeps the other ids.
- The `preview` of a pattern lists `elements` (see [Preview](#preview)).

### mirror

```json
{"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
 "plane": "yz", "combine": false, "compute": "adjust"}
```

A pattern of one copy, element 1 (`<mirror>:inst1(<face>)`). Mirrored
bodies are new bodies; `combine` joins each to its original when they
touch. Mirrored features reflect their finished tool (never a rebuilt
one).

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
| `set_body_appearance` | `uid`, `appearance` (an id, or null) | |
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
| `bodies` | `properties`, `volumes` | `uid`, `name`, `component` (when not the root's), `visible`, `material`, `appearance` (when set); with `"properties": true` also `kind` (`solid`, `sheet`: faces or open shells, `mesh`, `empty`), `volume`, `area`, `center`, `bbox` {`min`, `max`} (in the component's coordinates), `faces`, `edges` (counts); with `"volumes": true` only `volume` (cheaper) |
| `profiles` | `hashes` | `sketch`, `sketch_name`, `region`, `consumed` (an extrude uses it); with `"hashes": true` also `hash` (16 hex digits of the region's geometry and sketch frame: the same while the profile's face would be the same) |
| `faces` | `body` | per face: `names`, `surface` (`plane`, `cylinder`, …), `area` |
| `edges` | `body` | per edge: `name`, `curve` (`line`, `circle`, …), `length` |
| `dependents` | `uid` | `dependents`: `uid`, `name` of the features that refer to it, directly or through others, in timeline order (what `delete_feature` with `dependents` deletes too) |
| `can_reorder` | `uid`, `index` | `ok`, and `error` (the reason `reorder_feature` would give) when not |
| `sketch` | `uid` | see [Sketch commands](#sketch-commands) |
| `threads` | | threads on the bodies (thread features and tapped holes): `feature`, `name`, `body`, `face`, `standard`, `designation`, `class`, `right_handed`, `modeled`, `major_diameter`, `minor_diameter`, `pitch`, and from the face `internal`, `radius`, `start`, `end` (points on the axis where the thread begins and ends) |
| `thread_sizes` | `standard` (`iso_metric`, `unified`, `whitworth` or `npt`; all when left out) | `standards`, ISO metric first, then Unified, Whitworth (bolt sizes, then the pipe sizes `G 1/4`) and NPT: `standard`, `title` (the thread type's name), `default` (true for ISO metric), `sizes` (smallest first: `size` as the table names it, `10`, `1/4`, `#10`; `major_diameter` mm; `designations`, the coarse pitch or UNC first, each one `thread` and `hole` accept), `classes_external`, `classes_internal`, `default_class_external` (6g, 2A, Medium, Standard), `default_class_internal` (6H, 2B, Medium, Standard) |
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
| `sketch.create` | `plane` (default `xy`; `xz`, `yz`, a construction plane `"F5"` or `{"face": …, "body": …}`), `frame`, `name` (optional) | result `uid`, `name` |
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
| `sketch.project` | `source`: an edge, a face (its boundary) or a vertex of the bodies before the sketch, `body` (optional), `linked` (default false) | fixed reference entities; a linked projection follows its source when the model changes |
| `sketch.import_dxf` | `path` (.dxf), `unit` (of a drawing that names none: `mm` default, `cm`, `m`, `in`, `ft`), `at` (the sketch point the drawing's origin goes to, default [0, 0]), `layers` (only the entities on these layers; all when left out; a layer with nothing on it is refused) | as other edits, and `curves`, `points` (shared points made), `text_count`, `warnings`; undo step `Insert DXF into Sketch1`; see [DXF](#dxf-into-sketches) |

The `sketch` query (`{"query": "sketch", "uid": "F1"}`) returns:

- `uid`, `name`, `plane`, `frame` (`origin`, `x_axis`, `y_axis`, `normal`
  in model coordinates, or null when the sketch did not evaluate),
  `solved`, `error`;
- `entities`: the definition's, with solved `at` for points, `geometry`
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
| `insert_component` | `path`, `link` (default true), `transform`, `name`, `base` (directory of relative paths) | `component`, `occurrence`, `name` |
| `update_links` | `base` (optional) | `messages` |

| Query | Fields | Result |
|---|---|---|
| `components` | | `active`; `components` (the root first): `uid`, `name`, `created_by`, `link` (path), `features`, `bodies`, `occurrences` (count); `occurrences`: the tree from the root, each `uid`, `name`, `path`, `component`, `transform` (rows, in the parent, at the marker), `world` (4x4 into the design), `grounded`, `visible`, `children` |
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
its component (the design's new-component operations are new bodies
there). One undo step (`Import part.f3d`). The bridge handles the command
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
| `hang_limit` | seconds without progress after which the import is taken to hang in the geometry kernel (a new document only): the import runs on a thread of its own and is run again with the item it hung on taking the file's bodies (hung comparing the final bodies: by volume only; elsewhere: the stored bodies without the timeline); the report warns. The thread left behind keeps running until the program ends |
| `text` | `import_f3d_timeline` returns a readable report |
| `report_path` | also write the JSON report to this file |
| `list` | `import_f3d_timeline` only lists the designs: `{"designs": [{"label", "items"}]}` |

The result: `file`, `design`, `items` (timeline items), `counts` (items
per outcome: `parametric`, `partial`, `fallback`, `skipped`) and
`report`: per design `parameters` (imported, as values, renamed,
skipped, mismatched), `items` (`index`, `name`, `type`, `outcome`,
`features`, `verified`, `note`, `component` when not the root),
`history` (`states`, `built`, `matched`, `reached_end`), `bodies` (each
of the file's stored solids with `file_volume`, the replayed body it
matched, `volume_difference`, `max_deviation`, `relative_difference`),
`extra_bodies`, `warnings`, `components` (`components` made,
`occurrences` placed, `external` occurrences of other documents left
out, `items` per component) and `stopped` when the import was stopped.

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
"Fallback": the item takes the file's bodies.

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
| `CombineFeature` | `combine` | `targetBody` → `target`, `toolBodies` → `tools`, `operation` (Join/Cut/IntersectFeatureOperation), `isKeepToolBodies` → `keep_tools`; `isNewComponent`: the combine goes into the component the import puts the item in |
| `MirrorFeature` | `mirror` | `patternEntityType` + `inputEntities` → `objects` (Faces → faces of one body, Features, Bodies; not Occurrences), `mirrorPlane` (origin `XY`/`XZ`/`YZ` → origin plane; other construction planes → a fixed plane from `geometry`; face fingerprint → face), `isCombine` → `combine`, `patternComputeOption` → `compute` |
| `RectangularPatternFeature` | `rectangular_pattern` | `directionOne/TwoEntity` → axes (origin X/Y/Z, edge, fixed from `directionOne/Two` when the entity is null or a sketch line), `quantityOne/Two`, `distanceOne/Two`, `isSymmetricInDirectionOne/Two`, `patternDistanceType` (Extent/Spacing), `patternComputeOption`, `suppressedElementsIds` → `suppressed_elements` matched by `outputs.patternElements[].transform` |
| `CircularPatternFeature` | `circular_pattern` | `axis`, `quantity`, `totalAngle` → `angle`, `isSymmetric`, `patternComputeOption`, `suppressedElementsIds` |
| `PathPatternFeature` | `path_pattern` | `path` (a sketch line, arc or circle → `{"sketch", "curve"}`; else a fixed line), `quantity`, `distance`, `patternDistanceType`, `startPoint` → `start`, `isFlipDirection` → `flip`, `isOrientationAlongPath` → `along_path`, `isSymmetric` |
| `MoveFeature` | `move` | bodies of `inputEntities`; `transform` (rigid, cm) → `free` matrix, or `moveFeatureDefinition` (`TranslateXYZ`, `TranslateAlongEntity`, `Rotate`, `PointToPoint`, `PointToPosition`) to keep it editable; moves of faces are unsupported |
| `CopyPasteBody` | `move` with `copy: true` | `props.sourceBody`, `translate_xyz` 0, 0, 0 |
| Align (not in the API) | `move` (`free`) or `align` | |
| `ScaleFeature` | `scale` | `inputEntities` (bodies), `point` (origin point → origin, vertex, else fixed), `isUniform`, `scaleFactor`, `xScale`, `yScale`, `zScale` |
| `Box/Cylinder/Sphere/TorusFeature` | `box`, `cylinder`, `sphere`, `torus` | sizes from `parameters.model` by `createdBy`; the placement (not in the API) from the body: a fixed plane |
| `SweepFeature` | `sweep` | `profile` → `profiles` (as for extrude), `path` → `path`, `guideRail` → `guide_rail`, `orientation` (Perpendicular/ParallelOrientationType), `twistAngle` → `twist_angle`, `taperAngle` → `taper_angle`, `profileScaling` (SweepProfileScale/Stretch/NoScalingOption) → `profile_scaling`, `distanceOne`/`distanceTwo` → `extent` `partial` unless both 1, `isDirectionFlipped` → `flip`, `operation`, `participantBodies`; `guideSurfaces`, `extent` FullExtents and `isSolid` false → fallback |
| `LoftFeature` | `loft` | `loftSections[]` by `index`: `entity` a profile → `profile`, a face → `face`, a path of a body's edges (`PathEntity` items of `BRepEdge`s, or one edge; the section of tangent and smooth conditions) → `face`, the face of that body the edges go round (each of them borders it and it has no others), a sketch point, a construction point or a vertex → `point`; first and last `endCondition`: `LoftFreeEndCondition` → none, `LoftPointSharpEndCondition` → `point_sharp`, `LoftDirectionEndCondition` → `direction` (`angle`, `weight`), `LoftTangentEndCondition` → `tangent`, `LoftSmoothEndCondition` → `smooth`, `LoftPointTangentEndCondition` → `point_tangent` (`weight`); `centerLineOrRails` with `isCenterLine` → `centerline`, else `rails`; `isClosed` → `closed`; `operation`, `participantBodies` (the IR has no ruled loft). A loft with end conditions or rails is kept when it matches the history within 0.5 % (see [loft](#loft)) |
| `PipeFeature` | `pipe` | `path`, `sectionType` (Circular/Square/TriangularPipeSectionType) → `section`, `sectionSize` → `size`, `isHollow` with `sectionThickness` → `thickness`, `distanceOne`/`distanceTwo` → `extent`, `operation`, `participantBodies` |
| `CoilFeature` | `coil` | no API inputs: `parameters.model` with `createdBy` the coil gives diameter, revolutions, height, pitch, angle and section size by `role`; the coil type, section and placement come from the body |
| `RibFeature`, `WebFeature` | `rib`, `web` | no API inputs: thickness and depth from `parameters.model` by `role`; the curves are the sketch before the item |

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
| Hole | `hole` at the profile sketch's circles' and arcs' centres (`sketch_points`), Diameter, depth (DrillForDepth taken off) or through all, flat or angled point, counterbore, countersink, counterdrill, Tapered → `taper` (90° less TaperedAngle; FreeCAD's drill point is as high as the straight hole's: the tip angle from the wall's end that gives it); a cosmetic ISO metric, Unified (UNC, UNF, UNEF: `1/4-20 UNC`), Whitworth (BSW, BSF: `1/4-20 BSW`; BSP: `G 1/4`) or NPT (`1/4-18 NPT`) thread over the hole's depth → a cosmetic `thread` on the walls (the size from the version's list for the thread type, `ThreadSize[ISOMetricProfile]` in `data/enums.json`; the hole keeps FreeCAD's diameter), other threads left out (`partial`); a modelled ISO metric or Unified thread → a modelled `thread` (Mitcad's basic profile cut from FreeCAD's bore, which the check finds 1e-2 off FreeCAD's thread, so these fall back; Whitworth and NPT ones and those FreeCAD did not model fall back at once) | `flip` both ways |
| Mirrored, LinearPattern, PolarPattern | `mirror`, `rectangular_pattern` (1.1's second direction too), `circular_pattern` of the originals' Mitcad features (`identical`), or of the body (TransformMode whole shape: `combine` for the mirror); planes and axes as above (a sketch's V axis as a mirror plane: the plane through it along the normal); extent or spacing | the distance's or angle's sign |
| MultiTransform | of the originals' features: one transformation as that feature; two LinearPatterns → one `rectangular_pattern` of two directions; two Mirrored → both `mirror`s, a `construction_axis` where their planes meet (`two_planes`) and a `circular_pattern` of 2 about it (the half turn); otherwise (any sequence of Mirrored, LinearPattern and PolarPattern) each transformation a `mirror` or pattern of the one before (patterns of patterns); its transformations are no features of their own; Scaled and the whole body fall back | the signs of the directions |
| Boolean | `combine` join / cut / intersect of the Body's body with the other Bodies' bodies | |
| Draft | `draft` of the faces about the neutral plane (a pull direction taken as its normal) | `flip`, the angle's sign |
| Thickness | `shell` (Skin; Pipe and RectoVerso as Skin, which FreeCAD's solids give) | inside or outside, rounded |
| AdditiveLoft, SubtractiveLoft | `loft` of the profile and section sketches' regions, Ruled, Closed | |
| AdditivePipe, SubtractivePipe | `sweep` along the spine sketch's edges, Standard / Frenet → `perpendicular`, Fixed → `parallel`; with sections (Transformation Multisection) → `loft` of the profile and the sections with the spine as its `centerline`; auxiliary spines fall back | |
| AdditiveHelix, SubtractiveHelix | `helix` of the profile's regions about the reference axis: the pitch and the turns as the Mode gives them (Height / Pitch, Height / Turns), LeftHanded → `left_handed`, Reversed → `flip`; a cone's angle, a growth and a subtraction outside fall back | the hand and the direction both ways |
| Additive / Subtractive Box, Cylinder, Sphere, Torus, Cone, Prism, Wedge, Ellipsoid | `box`, `cylinder`, `sphere`, `torus` on a fixed plane at the placement; a cone and parts of a turn (Cylinder's and Cone's Angle, Sphere's latitudes and Angle3, Torus's section sector and Angle3): a section in a fixed plane through the axis turned about it (a construction plane, a sketch and a `revolve`); a prism: its polygon `extrude`d from a sketch on a fixed plane; skewed (FirstAngle, SecondAngle; a cylinder too) → `sweep` along the skew, `parallel`; a wedge: a ruled `loft` from its rectangle at Ymin to the one at Ymax (or a point); an ellipsoid: a `sphere` of Radius2 at the origin (a part of one: its section in the XZ plane turned about the z axis, Angle1 to Angle2 and Angle3 round), `scale` non-uniform (Radius3 / Radius2, Radius1 / Radius2), `move` to the placement and `combine` with the Body's body | |
| Plane, Line, Point (datums) | `construction_plane` (`offset` from an origin plane or a planar face when attached flat with only a normal offset; attached flat to an origin plane and turned a quarter or not at all, `offset` from the origin plane parallel to it by the offset's coordinate along its normal; turned about its support's x or y axis by a bound angle, `angle` from the support; else `fixed`), `construction_axis`, `construction_point` (`fixed` at the placement); coordinate systems are skipped | |
| Part Box, Cylinder, Sphere, Torus, Cone, Prism, Wedge, Ellipsoid | the primitives as above, new bodies | |
| Part Extrusion of a sketch | `extrude` new body: LengthFwd (else the direction's length), LengthRev → `two_sides`, Symmetric, Reversed, TaperAngle(Rev); along the normal or a custom direction along it; a custom direction or an edge's (DirMode Custom, Edge: `Dir`) off the normal → `sweep` along a fixed line, `parallel` (no taper) | the direction, the taper's sign |
| Part Revolution of a sketch | `revolve` new body about a fixed axis (Base, Axis); 360° → `full`, Symmetric | the angle's sign |
| Part Cut, Fuse, MultiFuse, Common, MultiCommon | `combine` (the first operand the target) | |
| Part Fillet, Chamfer | `fillet` / `chamfer` of the edge list (constant sizes; unequal chamfer sizes `two_distances`) | `flip` both ways |
| Part Mirroring | `mirror` of the source's body (plane Base, Normal or 1.0's MirrorPlane), then a base feature without bodies that removes the source's body | |
| others (Scaled, binders, Draft objects, plain shapes, …) | a base feature of the stored shape | |

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
  relative).

Tests: `core/freecad/tests/parse.rs`, `core/freecad/tests/sketch_corpus.rs`,
`core/import/src/freecad/tests.rs` (mock kernel), `cli.import_fcstd`
(`tools/cli/fcstd-test.cmake`), `freecad.corpus` (`MITCAD_FCSTD_CORPUS`;
[core/import/README.md](../../../import/README.md#tests)) and
`tools/ui-import-test.sh`.

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
  appearance), **timeline groups** and **named views**.
- **Geometry** (optional): the volume and area of each body at the
  marker of the two computed documents, and their sums.
- The display state is not compared (it is per user).

| Field | Content |
|---|---|
| `identical`, `summary` | nothing differs; one line, `d3 20 mm -> 25 mm, +1 feature, 2 features modified` |
| `document` | field changes: `field`, `from`, `to` (null: not there), `text` |
| `parameters` | `kind` (`added`, `deleted`, `modified`), `name`, `renamed_from`, `owner`, `owner_name`, `from` and `to` (`expression`, `unit`, `value`, `text`, `comment`, `kind`, `owner`, `favorite`), `fields` (what differs), `text` |
| `features` | in `to`'s order, deleted ones where they were: `kind` (also `moved`), `uid`, `name`, `type`, `from_index`, `to_index`, `moved`, `fields` (field changes), `values` (`slot`, `label`, `from_parameter`, `to_parameter`, `from`, `to` in mm or rad, `from_text`, `to_text`, `text`), `sketch` (`entities`, `constraints`, `dimensions`: `from`, `to` counts and `added`, `deleted`, `modified` ids; `moved`), `text` |
| `components`, `occurrences`, `bodies`, `groups`, `views` | `kind`, `uid` (not for groups and views), `name`, `fields`, `text` |
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
| `remote_info` | `links` (default false) | `name` (the remote the branch follows, else `origin` when there is one; null: none), `url` (credentials hidden), `branch`, `upstream` (`origin/main`), `ahead` and `behind` (versions the remote lacks and the project lacks, by the remote-tracking reference; null before the first fetch or push), `last_fetch`, `last_push` and `last_sync` ({`time`, `date`, `error`}, kept in `.mitcad/local/remote.json`), with `links` `external_links`: linked components whose files are outside the project ({`file`, `component`, `path`}), which other copies of the project do not have |
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

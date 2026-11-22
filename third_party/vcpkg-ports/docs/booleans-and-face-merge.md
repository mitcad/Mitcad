# Booleans and the face merge: 0110, 0220, 0360, 0590, 0680, 1040, 1285, 1695, 2170

OCCT's face merge after booleans (`ShapeUpgrade_UnifySameDomain`) and the
boolean's wire splitter. The face merge's speed-ups are 0006 in
[repeated work](repeated-work.md) and 0592 in
[multi-turn faces](multi-turn-faces.md). Back to the
[port's README](../README.md).

## Overview

- **0110** keeps a chain of B-spline edges whose curves cannot be
  concatenated as its edges instead of ending the whole merge with an
  exception.
- **0220** lets the face merge after booleans merge two faces of one
  cylinder whose seams lie half a turn apart (mitcad#284).
- **0360** orders the edges that leave a vertex tangentially to each other
  about it when a boolean splits a face (mitcad#235).
- **0590** gives up a merged face's wire that repeats itself over the same
  seams instead of building it without end (mitcad#235).
- **0680** merges a ring of cylinder pieces cut through from end to end,
  which reaches round the whole period without winding round it, into one
  face without a seam (a spiral stair's slabs, treads and helical
  stringers, mitcad#400).
- **1040** lets the face merge after booleans move the faces of a cylinder
  whose axis points the other way onto the reference face's parameters by
  the signed angle between their X directions, which left a bin's corner cut
  by an extrusion going down in faces built separately with seams used
  once, an invalid result (mitcad#642).
- **1285** classifies a point along fixed lines when the solid classifier's
  segments to the faces have all run out, instead of counting it as
  outside; the checker took a cavity of a face-merged union for lying
  outside another, and the union was kept unmerged (mitcad#1280).
- **1695** stops the decomposition of a walking intersection line at the
  faces' boundaries from reading a point past the line's ends, which
  crashed a boolean in a release build (mitcad#1819).
- **2170** lets the wire splitter tell apart the two points of a vertex
  that a face's edges meet a period apart, with no closed edge there:
  the union of a reported design's finned rods with a ring and
  triangles left faces closed in space but not in the face's parameters,
  an invalid union (mitcad#2170).

## 0110: a chain that cannot be concatenated

The patches 0110 and later change OCCT's face merge after booleans
(`ShapeUpgrade_UnifySameDomain`).

### 0110-shapeupgrade-unifysamedomain-chain-not-concatenated

A chain of B-spline edges whose curves cannot be concatenated (the end of
one lies farther from the start of the next than the tolerance of the vertex
between them, which `GeomConvert::ConcatC1` refuses with "Courbes non
jointives") stays as its edges, as chains of other curves that cannot be
merged do (`MergeSubSeq` returns false). Before, the exception ended the
whole merge, and a join whose boolean had given a valid result failed
(`BRepAlgoAPI_BuilderAlgo::SimplifyResult`).

## 0220: seams half a period apart

The patch 0220 makes the `.f3d` import's replay keep one face where the
files have one (mitcad#284).

### 0220-shapeupgrade-unifysamedomain-seams-half-a-period-apart

A join of two coaxial cylinders of one radius, one on the other, leaves two
faces of one cylindrical surface with their own seams. The face merge after
the boolean removes the seams and the circle between the faces and builds
the merged face's seam again from the removed edges
(`ReconstructMissedSeam`); a removed edge that ends at the current vertex
was judged by its far end, taken when that end lay more than half a period
from the current point and dropped when less. Where the seams lie half a
turn apart, the half circle between them ends exactly half a period away:
neither test took it, and the faces stayed apart (a revolution joined onto
an extruded cylinder; the files' bodies have one face). On a surface
periodic in one direction the edge is now turned to start at the current
vertex first and its own end there is compared.

## 0360: the wire splitter at tangent edges

The patch 0360 lets a boolean split a face where two of its new edges
leave a vertex tangentially to each other (mitcad#235).

### 0360-bopalgo-wiresplitter-tangent-edges-measured-at-one-distance

The wire splitter (`BOPAlgo_WireSplitter`) orders the edges at each vertex
by their directions, each measured at a step of its own from the vertex: on
a circle about sqrt(2 R tol) away, on a B-spline a step of its parameter,
often a tenth of that distance. A counterbore cut through an internal thread
whose crest lies on the hole's circle splits its end face by the circle and
by the thread's section, which leaves the circle tangentially: the circle,
measured farther out, had turned more than the section beside it though the
section turns faster, the two came out in the wrong order, and the face
between them came out with a negative area; the cut left the body empty (a
part's counterbores through threaded holes). Edges within 0.1 radians of
each other at a vertex are now measured again by the chord to their points
at one distance from it (`RefineTangentAngles`).

## 0590: a wire repeating itself

The patch 0590 comes from threads whose turns meet in one edge, as the
bodies stored in files have them (mitcad#235):

### 0590-shapeupgrade-unifysamedomain-wire-repeating-itself-given-up

The face merge builds a merged face's wires edge by edge and, where no edge
left goes on from the current vertex, takes a removed seam there
(`ReconstructMissedSeam`). The seven pieces of a cylinder cut by the helix
and the lines along the axis where a thread's turns meet in one edge (the
threaded part intersected with the thread's crest cylinder past a blind
bore's bottom) sent the wire back and forth over the same removed edges
without end, until the memory was gone. A wire runs along each edge left at
most once and each removed edge at most twice; past that count it is given
up as when no next edge is found, and the faces stay unmerged (mitcad#235).

## 0680: faces round a period without winding

The patch 0680 comes from a spiral stair's join of its slabs, treads and
helical stringers (mitcad#400):

### 0680-shapeupgrade-unifysamedomain-faces-round-a-period-without-winding

The face merge takes faces on a surface periodic in u, none of them with a
seam, whose u reach round the whole period to wind round the surface, and
builds their union's wires with seams reconstructed from removed edges
(`ReconstructMissedSeam`). The rim of the stair's slabs, split by the
sketch's lines into 80 pieces each, is cut through from its bottom to its
top by the treads' inner sides rising on it step by step, or by a helical
stringer's band along it: what is left reaches from one side of the cut
round to the other, the whole period, without winding round the cylinder.
Built with seams, the merged face used an edge with two pcurves on its
surface once (`BRepCheck_InvalidMultiConnexity`) and left pieces of the rim
under it; the result was invalid, the union was computed again without the
merge (twice the time), and kept a thousand pieces of the rim, which every
later boolean met. `IntUnifyFaces` now first follows the contours of the
merged face's edges, moving each pcurve by whole periods onto the end of the
one before it (`RelocateContoursNotWinding`); when each contour closes in 2d
and each hole lies inside the outer contour at one place, the face is built
without a seam, its pcurves spanning more than the period, on a copy of the
surface (as faces covering less than the period are built on a trimmed one).
A contour that winds round the surface, a vertex with more than one way on,
a hole that cannot be placed or a surface periodic in both directions take
the seams as before.

## 1040: the signed angle to a reversed axis

The patch 1040 comes from mitcad#642 (the open time of a design of a bin
with a pattern of 372 ribs): an intersection of the bin's body (8 191
faces) with an extrusion going down, whose face merge was invalid, so the
boolean was computed again without it (8 s more) and left 8 208 faces
where the stored body after the item has 6 721; a rounding of seven edges
on that body then failed after the item's 60 s and the item fell back.

The patch 1050, from the same design, is in
[repeated work](repeated-work.md#1050-shapefix-self-intersecting-wire-edges-read-once).

### 1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis

The pieces of the bin's rounded corner (radius 2.75) lie on one cylinder
with two parametrisations: the body's, axis up with its X direction along
+y, and the tool's, axis down in an indirect frame with its X direction
along +x, a quarter turn behind. `TransformPCurves` mirrors the pcurves of a
face whose axis or handedness is reversed against the reference face's and
then turns them by the angle between the X directions, which for such faces
was the unsigned angle: they came out half a turn away from where the faces
lie. The edges between the pieces got a seam's two pcurves on the reference
surface, the union seemed to fall into four intervals of u, and each piece's
wire was built as a face of its own with such an edge used once
(`BRepCheck_UnorientableShape`). With the signed angle (about the
reference's X direction crossed with its Y direction) the six pieces lie in
one interval of u and are merged into one face: the intersection is valid
with 6 732 faces at the first try (0.8 s for the merge), the volume within
0.06 mm³ (1.2e-6 of it) of the unmerged result's.

## 1285: the solid classifier where its segments run out

The patch 1285 comes from mitcad#1280 (found by mitcad#1279's profiling of
the reported design of GitHub #133): the union of a machine's 257 bodies
was made with its faces merged (7 458 faces), and the check of the merged
union found a cavity lying outside another (`BRepCheck_SubshapeNotInShape`),
so the union was taken as OCCT left it (8 721 faces). The checker
(`BRepCheck_Solid`) classifies a point on an edge of each closed shell
against the solid of every other one; the point of one cavity lay in the
plane of the bottom faces of two cavities some 300 mm away, and their
classifier gave up on it.

### 1285-brepclass3d-point-classified-along-fixed-lines-once-the-segments-run-out

`BRepClass3d_SClassifier::Perform` classifies a point along a segment from
it to a point of a face, the faces taken one after another from the next
one on (`BRepClass3d_SolidExplorer::OtherSegment`). A segment through an
edge or a vertex, or one whose nearest face it meets on its boundary, is
dropped for the next. A face whose surface holds the point (outside the
face) gives no segment, and a face seen almost edge-on (every segment to it
less than about 12° off its plane) only a weak one, which such a face
later in the shell discards. When the faces ran out (`OtherSegment` started
again from the first face), the classifier ended with its "rejected" state,
which `State()` gives as `TopAbs_OUT`. A point in the plane of a far face
of a shell whose other faces it sees almost edge-on so came out outside
the shell's solid, whatever side it lay on.

With the patch the point is then classified along fixed lines: from it
towards 32 points spread over the shape's box (an additive sequence of
three irrational steps starting at the box's centre, kept a tenth of the
box from its sides, the same on every call), each reaching past the box's
farthest corner. The intersections are those of the segments; a line that
meets no face (it may pass by the shape), touches a face where it meets it
first, ends on a face's boundary or passes through an edge or a vertex is
passed over, and only when every one is the point rejected as before. The
box is computed only then; classifications whose segments did not run out
are unchanged.

A fixed line through an edge is passed over even where the transition
across the edge's two faces is clear, which decides a segment
(mitcad#1881). A flat shell, every face in one plane and its volume 0,
gives a point in that plane no segment (every face's surface holds it),
and its box being flat, every fixed line runs in the plane too; the faces'
transition across an edge met there counted points outside the shell
inside it. OCCT's TKBO GTests BCutSimpleTest1 K1, K5, K7 and L1 cut a
block by such a shell (their translation of the DRAW cases' plane
`p 0 0 -1 1 0 0` is a plane of normal x, so the swept tool lies in its own
plane), and the cut came out empty, the block classified inside the tool;
it is now the block unchanged, as without the patch, which is within the
GTests' area tolerance of the DRAW cases' value.

## 1695: no point read past a walking line's ends

The patch 1695 comes from mitcad#1819: the table of dressups that take a
face away (`geometry/tests/test_dressup_consumes_face.cpp`, its chains
part) builds the reference of a concave two-distance chamfer along a
tangent chain over a cylinder strip by a boolean of the chamfered body with
the body it was made on, and on Linux that boolean crashed with a
segmentation fault in a release build of OCCT; a debug build raised
`Standard_OutOfRange` there instead and the row passed.

### 1695-inttools-wline-decomposition-reads-no-point-past-the-line-ends

`IntTools_WLineTool::DecompositionOfWLine` splits a walking line of a
face/face intersection where it leaves the faces' domains, and moves the
end point of a piece onto a natural boundary or away from a tangent zone.
To do so it looks along the line, from the end's neighbour on, for a point
distinct from that neighbour (more than `gp::Resolution` apart in the
face's parameters): two `while` loops step the index one point on and read
`theWLine->Point(index)` before their condition checks the index. Where
every point up to the line's end lies on the neighbour, the last step read
`Point(0)` or `Point(NbPnts + 1)`: in a release build, without the
sequence's range check, a null node (valgrind: an invalid read at address
0x28 in `DecompositionOfWLine` under `IntTools_FaceFace::MakeCurve`). Each
loop now stops before an index outside `1..NbPnts`, as it ends when it
finds no distinct point within the piece; every case that read only points
of the line keeps its result.

## 2170: a vertex met a period apart

The patch 2170 comes from a reported design (mitcad#2170, GitHub
mitcad/Mitcad#463): a Combine joining a hub with 30 blades, triangles,
rods and a ring, read from a STEP file. Six of the bodies are each a rod
with three thin fins. The rod's side is one cylindrical face without a
seam: its boundary runs down a line of the cylinder at u = 0, round the
rod, and back up beside the fins just short of u = 2 pi; it reaches
u = 2 pi only at the vertex where two fins meet the rod (and at the
rod's end circle), and passes that vertex a second time on the line at
u = 0. A two-edge inner wire, where the third fin's knife edge grazes the
rod, touches the boundary at that vertex, at u = 0. Six edge uses meet
there, at two points of the face's parameters a period apart, and none
of the edges is closed on the face (so the face's vertex is not marked
as one on a seam).

### 2170-bopalgo-wiresplitter-vertex-met-a-period-apart

A union of such a rod with the ring or a triangle that trims its side
face sends the face to the boolean's face builder (a vertex used more
than twice keeps BuildDraftFace from rebuilding the face's wires as they
were). Its wire splitter (`BOPAlgo_WireSplitter`, `Path`) walks the
edges vertex by vertex and closes a loop where it comes back to a vertex
it passed; it compares the points in the face's parameters only at a
vertex with a closed or degenerated edge (`aVertMap`). At the rod's
vertex it closed a loop on coming to its point at u = 2 pi after leaving
it at u = 0, and chose the edge to go on along among those leaving both
points, by angles measured each at its own point: two of the five loops
of the trimmed face were closed in space but a period apart in 2d, and
their faces could not be oriented (`BRepCheck_NotClosed`,
`BRepCheck_UnorientableShape`). The union of the design's 26 bodies
that do not lie inside others was invalid (the other five, bare rods
each inside a finned rod, are left out of the union by mitcad#2053).

`SplitBlock` now marks a vertex used by more than two edges, none of
them closed on the face or degenerated, whose edges meet it at points
more than half a period apart on a surface periodic or closed in u or v
(`aVertCopies`), where the face's own wires (the face given to the face
builder) also meet it at points a period apart. A vertex the boundary
passes once, where an edge whose curve on the face the boolean made (a
section, a split shared with another face's edge, on a face spanning
more than the period) lies a period from the boundary, is not marked:
the patch's first form marked such a vertex in the join of a pad flush
with a body's merged cylindrical walls (mitcad#964's case in
`geometry.suite.boolean`), and the join came out without the pad. The
face's own vertex is matched with the vertex in the split where it is
that vertex or lies within their tolerances of it: where another
argument's vertex coincides with it, the pave filler puts a new vertex in
its place (`MakeSDVertices`), as in a finned rod's common with a union
holding it; matched by identity alone, the reported union of all 31
bodies still missed 24 of them and the finned rods' commons with the
union came out 2 % short. At a marked vertex `Path` closes a loop only at the visit
that left from the point it comes to, and goes on only along edges
leaving that point, as it does at a vertex on a seam with the 2d
tolerance. Planar faces, vertices whose edges meet them at one point and
vertices on a closed edge are split as before.

## Reports and tests

- 0220: mitcad#284 and the coaxial cylinders' join in
  `geometry/tests/test_boolean.cpp`, which fails without it.
- 0360: mitcad#235 and the split by tangent curves in
  `geometry/tests/test_boolean.cpp`, which fails without it.
- 0590: mitcad#235 and the nut threaded past its bore's bottom, intersected
  with the cylinder inside the bore, in `geometry/tests/test_sweeps.cpp`.
- 0680: mitcad#400 and two slabs of four quarters with treads on their rim
  in `geometry/tests/test_helical_joins.cpp` (the rim merged into one face;
  the slabs alone, a ring with a seam, as before).
- 1040: mitcad#642 and a quarter of a cylinder fused with a sector before or
  after it mirrored top to bottom (its axis down, its frame indirect),
  either first, in `geometry/tests/test_merge_reversed_axis.cpp` (the merge
  valid, one cylindrical face, the volume kept; without the patch the two
  faces stay apart wherever the other face's X direction lies behind the
  reference face's: the sector behind with the quarter first, the sector
  ahead with the sector first).
- 1285: mitcad#1280 and a block with two cavities in
  `geometry/tests/test_classifier_far_plane.cpp`: a thin pointed cavity
  whose bottom face, last in its shell, lies in the plane of a box-shaped
  cavity's bottom beyond its point, every other face of it seen almost
  edge-on from there. Points of the box-shaped cavity's bottom classify IN
  the pointed cavity's solid (without the patch they were rejected, OUT),
  points inside it OUT, and the checker accepts the block (without the
  patch `BRepCheck_SubshapeNotInShape`). The join of the reported design's
  257 bodies in `geometry/tests/test_join_many_bodies.cpp` keeps its merged
  union. mitcad#1881 and a flat shell (a rectangle swept within its own
  plane) in the same test file: points in its plane outside it classify
  OUT, and a pocketed block cut by it keeps its volume (with the patch's
  earlier form three of the points came out IN and the cut empty, as in
  OCCT's GTests BCutSimpleTest1 K1, K5, K7 and L1).
- 1695: mitcad#1819 and the chains part of
  `geometry/tests/test_dressup_consumes_face.cpp` (ctest
  `geometry.dressup_consumes_face.chains`): its third row, a concave
  two-distance chamfer of 0.99 of the strip's width along a tangent chain
  over a cylinder strip, whose reference boolean crashed in a release
  build.
- 2170: mitcad#2170 and `geometry/tests/port/p2170-vertex-met-a-period-apart.cpp`
  (ctest `geometry.port.2170`): such a face built from scratch on a
  cylinder, split by the face builder from its own edges and with a
  section line across it, in every order of the edges; and the reported
  join of 31 bodies in `geometry/tests/test_join_holds_inputs.cpp`.

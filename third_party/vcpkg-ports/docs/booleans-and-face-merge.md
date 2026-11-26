# Booleans and the face merge: 0110, 0220, 0360, 0590, 0680, 1040, 1285, 1695, 2170, 2284, 2384

OCCT's face merge after booleans (`ShapeUpgrade_UnifySameDomain`) and the
boolean's wire splitter, and the volume integration that measures a
boolean's result (2284). The face merge's speed-ups are 0006 in
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
- **2284** lets the volume and area integration (`BRepGProp_Gauss`)
  integrate across the joints of each face's loop of pcurves, where two
  pcurves meet at their vertex within its tolerance but apart along v,
  and the volume integration integrate the ribbon from each edge's curve
  on a face to its 3D curve: a reported ring's common with a union
  measured a volume that moved by 4.1e-6 relative with the point it was
  integrated about (mitcad#2284).
- **2384** puts a walking intersection line's end on a surface's boundary
  within a room that grows with a parameter range longer than a full turn:
  the section of a loft's side parametrized by arc length with its mirror
  image stopped 4.2 micrometres below the top, where the two sides are
  tangent, and the union of the loft with its mirror image kept the loft
  alone (mitcad#2384, GitHub mitcad/Mitcad#641).

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

### The ring's common: its volume, not material, is loose (mitcad#2262)

The join test checks every reported body by its volume in common with the
union (mitcad#2217). The ring's common came out 1.09e-6 relative over the
ring (21135.163943 against 21135.140975 mm³), with nothing of it outside
the ring (the common's cut by the ring empty). Its 40 faces all lie on the
ring's own surfaces, cut where the other bodies cross the torus; edges of
1851 mm carry tolerances above 1e-7 mm, up to 1e-6 mm.

- The excess is not material. Integrated about the middle, a diagonal
  along X and one along Z, the common measured 21135.163838, 21135.164173
  and 21135.077405 mm³, the ring 21135.140975 mm³ about each: the common's
  volume moves by 4.1e-6 relative with the point it is integrated about,
  from 1.1e-6 over the ring to 3.0e-6 short of it, and the 1.09e-6 lies
  inside that spread. A shell that closes has one volume about every
  point.
- The 1e-6 mm tolerances need not measure a gap. Where a body's plane cuts
  the torus, OCCT's face-face
  intersection (`IntTools_FaceFace::MakeCurve`) walks such a section,
  approximates it in the plane's parameters, lifts the 3D curve from
  there, and then raises the curve's tolerance to at least 1e-6 mm for any
  plane–torus pair, whatever the approximation reached
  (`ComputeTolReached3d` then raises it only where the 3D curve lies
  farther from a pcurve). No port patch touches that step.
- The volume integration (`BRepGProp_Vinert`, through `BRepGProp_Gauss`)
  reads no 3D curve: each face is integrated over the domain its pcurves
  bound, from the face's lowest u to the boundary. Two faces on one
  surface share one pcurve of their edge, so their domains meet exactly
  whatever the edge's tolerance; a sliver can only open where the faces
  beside an edge lie on two surfaces. Each face's integral also carries an
  error, relative to the face's own integral, which grows with the point's
  distance from the shape.

A lower tolerance for those sections (a port patch of the 1e-6 mm floor)
would therefore change the edges' numbers, not the volume, and is not
made. The test writes, for the ring, each kind of loose edge (a section
or a piece of the ring's own edge, its curve type, the union's surface
beside it), how far its 3D curve really lies from its pcurves, how many
of them lie between faces on two surfaces and how far apart those faces'
curves are, and the integration's own error bound about each of the three
points (each face's integral times the relative error the integration
reports for it) to `common-volumes.txt` in its build folder: that record
tells whether the spread is the integration's error or slivers between
faces on two surfaces. The ring's limit was then the measured spread
rounded up, 5e-6 relative, until the cause was found and patched (2284,
below); since then the ring is checked within 1e-6 both ways like every
other body.

#### Neither: the joints of a face's pcurves (mitcad#2267)

The record ruled out both causes above. About the three points the
integration's own error bound came to at most 6.5e-10 of the ring (one
face's relative error up to 5.4e-10), some 6000 times less than the
spread; the 8 loose edges between faces on two surfaces (or a face's
seam), pieces of the ring's own circular edges, leave no gap between the
two faces' curves, and the 133 loose section edges (B-splines, tolerance
up to 1e-6 mm, their 3D curves within 8.5e-8 mm of their pcurves) lie
between faces on one surface. So the common's shell does not close for a
reason inside its faces, not between them.

`BRepGProp_Gauss` integrates a face by Green's theorem in its
parameters: along each of the forward face's edges, its pcurve in the
edge's orientation, the integrand is first integrated in u from the
face's lowest u (its UV bounds) to the pcurve's point at the pcurve's v,
and that is integrated along the pcurve against dv. This is exact only
for a loop of pcurves that closes in the parameters. Where one pcurve
ends at (u, v) and the next one starts at (u', v'), the two points within
the vertex's tolerance in space but not equal, the integral leaves out a
strip from the lowest u to u, v' - v wide (a gap along u leaves out
nothing). The strip's flux moves the face's volume with the point it is
integrated about, and its width counts against the strip's whole length:
on a torus face reaching round the ring a joint 1e-6 mm apart along v
leaves out about 1e-6 mm times up to the ring's circumference, of the
order of 1e-4 mm², the size of the net normal integral the spread along
the ring's axis implies. The strips run round the axis, so their parts
across it turn and largely cancel while their parts along it add up, as
in the record, where the spread about the point along Z is 260 times the
one along X. The record confirmed it (the cause found, below, 2284).

The test writes, for each face of the ring's common and of the ring
(`shell_flux`), the vector integral of its normal over its area as the
integration gives it (its volume about the middle and about points along
X, Y and Z), its share of the ring's surface from its edges' 3D curves
(half the integral of (P − O) × dP round them; the shares sum to zero),
the same through its pcurves with chords across the joints, the joints'
largest gaps along u and v and in space, the strips they leave out, and
how much closing them changes its volume; the sums over the faces, the
spread the summed vector gives against the one measured, and the volumes
about the three points with every face's loop closed at its joints. If
the strips account for the spread, the fix is in `BRepGProp_Gauss` (a
port patch integrating across each joint so the loop closes, the shape
left as it is) and the ring's limit returns to 1e-6; if not, the faces
whose integrated vector is off their share of the ring's surface by more
than their strips carry the open part, and the record names them.

## 2284: the joints of a face's pcurves integrated

The record of mitcad#2267 found the cause. For the ring's common:

```
the common's 40 faces summed: their normals over their areas integrated
  (-1.53e-06, 0.000108, 0.000396) mm2, the ring's surface within their
  edges' 3D curves (2.31e-14, -1.87e-14, 4.12e-11), through their pcurves
  with the joints closed (-6.38e-12, -6.66e-14, -4.24e-11), the strips the
  joints leave out (-1.53e-06, 0.000108, 0.000396)
the common's summed vector moves its volume from the middle to the points
  along X and Z by +1.58e-08, -4.09e-06 relative to the body (measured
  +1.58e-08, -4.09e-06); with every face's loop closed at its joints,
  about the three points: common 21135.140974536, 21135.140974536,
  21135.140974539, body 21135.140974537 (each)
the common's face 10 (torus, forward, 12 wires, 159 edges, u 0 to 2pi,
  v pi to 3pi): 159 joints of its pcurves, apart by up to 1.26e-10 along
  u, 4.07e-08 along v, 9.17e-08 mm in space; the strips they leave out
  (-1.57e-06, 0.000108, 0.000397) mm2
```

The common's shell closes in space (its faces' shares of the ring's
surface sum to zero within 4e-11 mm²) and in the parameters once each
loop is closed at its joints; the strips the joints leave out are the
whole of the integrated normals' sum, almost all of it on the one torus
face reaching round the ring, and with them integrated the common equals
the ring within 1.2e-13 about every point. The pcurves meet at their
vertices within the vertices' tolerance (up to 1e-6 mm), up to 9.2e-8 mm
apart in space: each edge's pcurve ends at its own point of the vertex,
and a section curve on the torus is an approximation the face-face
intersection made, so two curves meeting at one vertex end within the
vertex's tolerance of it, not at one point. A valid shape allows that (the
common is valid; the checker accepts pcurves meeting within the vertex's
tolerance), and making them meet would mean moving the curves of a valid
result after the boolean, at every vertex of every such face. Which step
of the boolean put each of the 159 joints apart (the section's
approximation, the split of an edge at a vertex, the pcurve of a split
piece of the ring's own edge) the record does not tell apart; the
integration is fixed instead, which needs none of that.

### 2284-brepgprop-gauss-joints-of-a-faces-pcurves-integrated

`BRepGProp_Gauss` integrates across the joints of each face's loop of
pcurves (`JointNodes`). In each wire of the forward face, each edge use's
end (its pcurve's point at its end, in the edge's orientation) is paired
with the start of a use there at the same vertex not yet paired, the
nearest in the parameters. A joint whose two points and middle lie on the
surface within the vertex's tolerance of it (so not a period apart, not
across the domain) and whose points differ along v by more than 1e-14 of
the face's v range (at least 1e-14; below that the gap is the parameters'
rounding) is integrated as the straight segment from the end to the
start: four Gauss points along the segment and, at each, the integrand
integrated in u from the face's lowest u over the surface's u knot spans
(at least eight points each), the parameters kept within the face's
bounds as the pcurves' points are.
The three `Compute` methods that integrate over a domain (the adaptive
one `BRepGProp::VolumeProperties` and `SurfaceProperties` use with an
epsilon, and the non-adaptive volume and area ones) add those nodes'
integrand to the face's sums after the domain's edges, so every loop
closes in the parameters. A joint apart only along u adds nothing, and
faces whose pcurves meet exactly at their vertices are integrated as
before.

Closing the loops alone measures each face over the domain its pcurves
bound. Where a pcurve lies off its edge's 3D curve, within the edge's
tolerance (an approximated section curve's pcurve on a blend), the face
reaches past the edge or stops short of it, and the face beside it on
another surface, bounded by its own curve of the edge, does not meet it.
The integration along the pcurves alone happened to hide such an offset
where the pcurve runs along u (a v offset of a pcurve along u reads
nothing, and the strips its joints left out took it back). The first form
of this patch closed the joints only, and in the bundle check two
roundings of radius 3 mitred at a short edge
(`geometry.suite.fillet_corners`, `test_two_roundings_mitred_at_a_short_edge`)
measured 16.0707129 mm³ removed against 16.0693324 from booleans of the
analytic shapes, past the test's 5e-5 relative; the port's own test of a
pcurve moved 1e-6 off its arc (`geometry.port.2284`) moved by 4.2e-7 with
the point. So for the volume (and its moments) `RibbonNodes` also
integrates the ribbon from each edge's curve on the face, A(t), to its 3D
curve, C(t): R(t, s) = A(t) + s (C(t) − A(t)), oriented with the face (the
face's orientation in the shape, the edge's in the face), at 8 Gauss
points on each of the 3D curve's C2 intervals (at least 4 pieces, at most
32) and 2 across (exact across: the integrand is of degree 2 in s). Faces
and ribbons together close along the edges' 3D curves, the remaining
holes at the vertices of the order of the tolerance squared, so a shell
closed in space has one volume about every point. Two faces on one
surface sharing their edge's pcurve (the ring's common) have opposite
ribbons that cancel. Only edges whose tolerance is above the confusion
tolerance (1e-7 mm) are looked at, since an edge's curves lie within its
tolerance of each other, and of those only edges whose curve on the face
lies off the 3D curve by more than 1e-7 mm at one of five points;
degenerated edges, edges without a 3D curve and edges not SameParameter
have none; the area gets no ribbons. A shape whose edges all lie within
1e-7 mm costs one tolerance lookup per edge: the patch's second form
looked at every edge, built both curves' adaptors and asked the curve on
the face for its C2 intervals (on a spline face, its crossings with the
surface's knot lines), and a sheet-metal board's script round trip
(`cli.script_round_trip.sheet_board_0427`) ran past its 900 s in the
bundle check. The shape is left as it is: no curve is moved, no tolerance
changed, nothing is healed. The Gauss–Kronrod integration
(`BRepGProp_VinertGK`, `BRepGProp::VolumePropertiesGK`) is a separate
integrator and is not changed.

In the OCCT lab (the cases `2284-tube-rim-volumes` and
`2284-stored-bodies-closure`, each body's volume about its rough
barycentre and about points 100 mm away), stored bodies whose volume the
patch changes most moved with the point by 2.07 mm³ (a wall's run-on),
0.148 (a convex mitre), 0.144, 0.132 and 0.087 (a tube's rim before and
after its rounding, and OCCT's rounding of it), 0.097 (a three-face
corner) and 0.024 mm³ (a tube's end) without the patch, and by 4e-6,
0.0026, 3e-5 each, 1e-6 and 4e-5 mm³ with it: what the patch changes is
the part of the volume that depended on the point it was integrated
about. Over the 33 shared cases only volumes change (by up to 4.7e-5
relative, on the wall's run-on); no case's validity, faces or success.

## 2384: a walk's end put on the boundary at any parameter scale

The patch 2384 comes from mitcad#2384 (GitHub mitcad/Mitcad#641): a
three-way pipe made of a loft from a circle of radius 170 at z = 315 to
one of radius 85 at z = 0 centred at x = 225, mirrored about x = 0 and
joined. Mirror3 said ok, but the body was the loft alone (its volume and
box), and the reporter's rounding of the crotch and Shell found no
mirrored half. The loft's side is an oblique cone, a B-spline of degree
8 x 1 parametrized by arc length (u 0 .. 1068.14 round the top circle,
v 0 .. 387.10 along the line at u = 0). It crosses its mirror image in the
plane x = 0 along a curve from (0, 170, 315) down to (0, 0, 142.26) and up
to (0, -170, 315); at both ends the two sides share their tangent plane.

The section of the two sides came out short: the walk from (0, 170, 315)
stopped at (0, -169.9989, 314.9959), 4.288 micrometres from the top
circle's point. The top circle was then not split there, the mirror
image's side was not cut, the boolean classified it inside the loft as a
whole, and the union was a valid solid of the loft's volume. The kernel's
own through-sections loft of the same circles (v over [0, 1]) joined
right, its section ending on the boundary.

### 2384-intwalk-walk-end-put-on-a-boundary-within-a-room-scaled-to-the-parameter-range

`IntPatch_PrmPrmIntersection` calls `IntWalk_PWalking::PutToBoundary`
after each walk: an end of the line that lies within `aTol` of a
surface's boundary in one of its parameters is taken there, and
`SeekPointOnBoundary` looks for the surfaces' common point on the
boundary from it (by distance minimisation, within its own tolerance, or
adds none). `aTol` is a thousandth of a parameter (1e-3 times the
smallest range, at most 1). On a surface over [0, 1] or an angle that is a
thousandth of its size, 0.3 mm on the kernel's own loft; on the loft
parametrized by arc length it was a micrometre, and the walk had stopped
5e-3 in v below the top. The patch gives each parameter of each surface a
room of its own: `aTol` for a range up to 2 pi, as before (analytic
surfaces and those over [0, 1] are unchanged), and `aTol * range / (2 pi)`
for a longer one; `IsParallel` keeps `aTol`. The point put on the
boundary is still the surfaces' common point that `SeekPointOnBoundary`
finds.

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
  join of 31 bodies in `geometry/tests/test_join_holds_inputs.cpp`. With
  the patch a bare rod's common with a finned rod holding it is the rod
  (mitcad#2217: the test's rods built from scratch, fins beside and over
  the rod's seam, its faces merged or not, joined with a plate, within
  1e-6 both ways; the reported design's five bare rods no more than 1e-6
  short in the union), so no patch of the common was needed. The
  reported ring's common with the union was checked within 5e-6, the
  spread of its volume about different points (mitcad#2262, above), with
  each face's vector integral of its normal and the joints of its pcurves
  recorded (mitcad#2267, above), until 2284.
- 2284: mitcad#2284 and
  `geometry/tests/port/p2284-joints-of-a-faces-pcurves-integrated.cpp`
  (ctest `geometry.port.2284`): half a cylinder built from scratch, one of
  whose edges' pcurves on its side face is changed by 1e-6 mm (the edge's
  and its vertices' tolerances 1e-5 mm), its volume the same within 1e-9
  about its middle and points 100 mm away along X, Y and Z and equal to
  pi R² H / 2 within 1e-9: the side line's pcurve ending 1e-6 short of the
  top (the joint; without the patch the volume moves by 4.2e-7 relative
  along Y), the top arc's pcurve moved 1e-6 above its 3D curve (the
  ribbon; with the joints closed alone it moves by 4.2e-7), and the half
  cylinder as built. `geometry.suite.fillet_corners`,
  `test_two_roundings_mitred_at_a_short_edge`: the removed volume within
  5e-5 of the booleans' (it failed with the joints closed alone). In
  `geometry/tests/test_join_holds_inputs.cpp` (ctest
  `geometry.join_holds_inputs`, from `MITCAD_MITCAD_REPORTED`) the
  reported ring's common with the union is checked within 1e-6 both ways
  like every body, and its volume about each of three points within 1e-9
  of the ring's; the record keeps the strips the joints leave out of an
  integration along the pcurves alone and the volumes that integration
  gave.
- 2384: mitcad#2384 and `geometry/tests/test_mirror_loft_join.cpp` (ctest
  `geometry.mirror_loft_join`): the reporter's loft, alone and with its two
  end extrusions, joined to its mirror image is one valid solid reaching
  both arms' ends with the union's volume (twice the loft's half at
  x >= 0, integrated over the height); without the patch the join refuses
  in words (geometry/src/boolean.cpp, a union that does not reach as far
  as a body it joins). The reporter's design in
  `tools/cli/tests/shell_reported641.json` (ctest `cli.shell_reported`,
  from `MITCAD_MITCAD_REPORTED`). In the OCCT lab, the case
  2384-loft-mirror-join (MitCad's loft and its mirror image as B-rep
  files, written by `test_mirror_loft_join --write`): without the patch the
  section of the two sides has 4 edges and ends 0.004288 mm from
  (0, -170, 315), and the fuse has 3 faces, the loft's volume and x from
  -171.9; with it 5 edges ending on both points, and the fuse 5 faces,
  27 047 300 mm3 (6 digits), x -310.9 .. 310.9, valid. The 44 other shared
  cases are unchanged; 2384-loft-mirror-join-proxy (the same cone as a
  rational B-spline made in DRAW, by arc length and over [0, 1]) joins
  right with and without the patch.

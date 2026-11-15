# Repeated work removed: 0006–0009, 0060, 0120, 0121, 1050, 1270, 1280–1284, 1680, 1690, 1691

These patches change no result. Each removes repeated work that made an
OCCT algorithm quadratic (or worse) in the edges or wires of a face or a
wire, which stored bodies with thousands of holes or long wires reach.
The same kind of bound for booleans on faces that run many turns is in
[multi-turn faces](multi-turn-faces.md) (0591 to 0593). Back to the
[port's README](../README.md).

## Overview

- **0006 to 0009** change no result: each removes repeated work that made
  an algorithm quadratic in the edges or wires of a face, which bodies with
  thousands of holes reach (mitcad#39, mitcad#72, mitcad#77, mitcad#79).
- **0060** changes no result either: it removes repeated work that made
  ShapeFix's repair of intersecting edges quadratic in the wires of a face
  (mitcad#60).
- **0120 and 0121** remove repeated work that made the checker quadratic in
  the wires or edges of a face (mitcad#119).
- **1050** reads a wire's edges into an array once in ShapeFix's search for
  intersecting edges of one wire instead of by number from the wire's
  sequence for every pair, which made healing a body with long wires grow
  with the cube of their edges (mitcad#642).
- **1270** keeps the open edges of a shell ShapeFix builds face by face
  instead of going through all of its edges after every face added, which
  made the face merge after a boolean quadratic in the faces of a large
  shell (mitcad#1273).
- **1280 to 1284** take the repeated work out of the union of many bodies:
  the stick vertices of each pair of faces found by an index instead of a
  scan of all interferences, the solid classifier's map of edge faces made
  only when it is read, a solid's face intersectors made beside each other,
  a face intersector classifying only the points in its range, and a new
  solid's edges mapped once for all its holes (mitcad#1279).
- **1680** sets up the classifier of each side face once for the hundreds
  of points the join of two stripes across a corner (0772, 0820) asks it
  about, instead of once per point (mitcad#1806).
- **1690** reads a blend's line by index in constant time; the
  approximation of a blend's surface walked the line's linked list at every
  evaluation (mitcad#1823).
- **1691** keeps the polyhedra of a face's surface for all the edges a
  boolean or an offset intersects with the face, instead of sampling the
  surface again for every edge (mitcad#1823).

## 0006 to 0009: booleans and the checker

### 0006-shapeupgrade-unifysamedomain-seam-search-by-vertex

The face merge after booleans (`ShapeUpgrade_UnifySameDomain`) looks up the
edges it removed by vertex instead of going through all of them at each step
of a wire across a seam.

### 0007-brepcheck-analyzer-no-search-in-face

The checker (`BRepCheck_Analyzer`) no longer searches a face for each of its
own vertices and edges (`InContextOfAncestor`).

### 0008-brepcheck-wire-seam-edges-once-per-face

The checker finds the seam edges of a face once instead of once per wire for
the 2d closure check, and no longer searches the face for each of its wires
(`BRepCheck_Wire::InContextOfFace`).

### 0009-bopalgo-face-face-adaptors-once-per-face

The face/face intersections of a boolean build the surface adaptor of each
face (its UV bounds go through all its edges) once instead of once per pair
of faces (`IntTools_Context::SetSurfaceAdaptor`).

## 0060: ShapeFix's repair of intersecting edges

### 0060-shapefix-intersecting-edges-once-per-face

The repair of a wire's intersecting edges
(`ShapeFix_IntersectionTool::FixSelfIntersectWire`, once for each wire of a
face) applied the healing context to the whole face and went through all its
vertices for their largest tolerance. The face is applied with
`ShapeBuild_ReShape::ApplyAgain`, which gives the previous call's result and
status back while no replacement has been recorded since (that call recorded
none either: it rebuilt nothing), and the largest tolerance is looked up
only for a wire with edges that may cross, before anything changes. The
repair of intersecting wires (`FixIntersectingWires`) lists the edges of the
second wire of a pair only when the boxes of the two wires meet. The healing
context's class gets members (code built against the port's headers must be
rebuilt with them).

## 0120 and 0121: the checker on bodies with thousands of holes

The patches 0120 and 0121 change no result either: they remove repeated
work that made the checker (`BRepCheck_Analyzer`) quadratic in the wires
or edges of a face, on stored bodies of thousands of holes (mitcad#119).

### 0120-brepcheck-face-wire-boxes-of-conics-past-their-range

The check of how a face's wires lie inside one another
(`BRepCheck_Face::ClassifyWires`) skips pairs of wires whose UV boxes do not
meet, but gave up a wire's box when an edge's range reached past its
p-curve's own parameters, which circles and ellipses of stored bodies do by
a rounding error (an arc ending at 2π + 4 · 10⁻¹⁵): every pair of such wires
was classified (8.6 million on a face of 2 929 holes, 3.5 s). A circle's or
an ellipse's box is now the conic's over the edge's range.

### 0121-brepcheck-shell-orientation-edges-listed-once-per-face

The shell's orientation check (`BRepCheck_Shell::Orientation`) searched each
face at each edge for the edge; a face of 64 edges or more now lists the
orientations of its edges once (9.6 s → 0.1 s on a shell whose two faces
hold 23 428 edges each).

## 1050: ShapeFix's search for intersecting edges of one wire

The patch 1050 also comes from mitcad#642: after 1040 the larger part of
healing the bin's large stored bodies went to the search for intersecting
edges of one wire.

The patch 1040 is in [booleans and the face merge](booleans-and-face-merge.md#1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis).

### 1050-shapefix-self-intersecting-wire-edges-read-once

`ShapeFix_IntersectionTool::FixSelfIntersectWire` reads edge num1 and edge
num2 of the wire for every pair of numbers, by number from
`ShapeExtend_WireData`'s `NCollection_Sequence`, whose access walks node by
node from the place read last: each pair walked about twice the distance
between the two numbers, and half the stack samples of healing the bodies
stood in that walk. The edges are read into an array in one walk and taken
from it; the fixes change the wire only for a pair whose edges intersect,
and the array is read again after such a pair, so the fixes see the same
edges as before.

## 1270: ShapeFix's orientation of a large shell's faces

### 1270-shapefix-shell-closed-shell-found-as-faces-are-added

`ShapeFix_Shell::FixFaceOrientation` (its `GetShells`) builds a shell
face by face. Where more than two faces share an edge (its multi-connex
mode) it tested after every face added whether the shell had closed, with
`BRep_Tool::IsClosed`, which goes through every edge of the shell. The face
merge after a boolean (`ShapeUpgrade_UnifySameDomain::UnifyEdges`) runs it
on each shell whose faces it changed: on the union of a machine's 257
bodies, one shell of 8 800 faces, 22 of 25 stack samples of the 14 s merge
stood in those tests. The edges used an odd number of times (as `IsClosed`
toggles them) are kept in a map as the faces are added; the shell is closed
when it has an edge that counts and none is left over, the same test, and
the map is emptied with each new shell.

## 1280 to 1284: the union of many bodies

The union of a machine's 257 bodies (mitcad#1279: after mitcad#1278 one
union of all, 17 s in `geometry.join_many_bodies` on a 32-core
workstation) spent most of its single-threaded time in three places,
which stack samples of every thread showed: the stick vertices of each
pair of intersected faces (about 1.4 s), the placement of the result's
cavities in `BOPAlgo_BuilderSolid` (about 3.5 s) and the checker's test of
the solid's shells in `BRepCheck_Solid` (about 3.5 s). The last two were
mostly the solid classifier, on a solid of 8 800 faces: the map of its
edges' faces made for every point, and the loading of the solid. With the
five patches the union takes 12.9 s, with the same result (the same
solids, shells, faces and volume); the design's Combine in `mitcad-cli
info --timings` 13.7 s instead of 19.4 s, its peak 1.48 GB instead of
1.73 GB.

### 1280-bopalgo-stick-vertices-of-face-pairs-by-an-index

`BOPAlgo_PaveFiller::GetStickVertices` gives the vertices of the VV, VE,
EE, VF and EF interferences between the sub-shapes of two intersected
faces. It went through every such interference of the operation for every
pair of faces, in `MakeBlocks` and again in `PostTreatFF`: quadratic in
the interferences. The two loops now read an index of the interferences
by the shapes they join, made once (and made again when the interferences
or the shapes have grown since), and find a pair's interferences from the
sub-shapes of its two faces. Their vertices are added in the order of the
scan (by kind, then by index), so the maps filled, whose order of
iteration later steps follow, are the same. `GetStickVertices` itself is
unchanged.

### 1281-brepclass3d-edge-faces-mapped-only-where-the-line-meets-an-edge

`BRepClass3d_SClassifier::Perform`, which classifies a point against a
solid, mapped every edge of the solid to its faces for every point, though
the map is read only where the line through the point passes through an
edge. Against the union's solid, making and freeing that map was most of
each classification. It is now made the first time it is read.

### 1282-brepclass3d-face-intersectors-made-beside-each-other

`BRepClass3d_SolidExplorer::InitShape` (a classifier's loading of a solid)
makes an intersector for each face, which samples the face's surface; the
union's solid is loaded several times (the builder's classification of its
shells, its placement of the cavities, the checker). The intersectors are
now made beside each other (`OSD_Parallel`, from 64 faces on) and bound to
the faces in the faces' order, as before.

### 1283-intcurvesface-only-points-in-range-classified

`IntCurvesFace_Intersector::InternalCall` classified every point where a
line meets a face's surface in the face and only then kept those in the
segment's range. A classifier's segment meets the surfaces of most faces
of a large solid outside its range. Only the points in the range are now
classified, and a face with none is left at once; the points kept are the
same.

### 1284-bopalgo-buildersolid-solid-edges-mapped-once-for-its-holes

`BOPAlgo_BuilderSolid::PerformAreas` tests each hole shell whose box meets
a new solid's box for whether it lies in the solid (`IsInside`), and
mapped all the solid's edges for every such hole. They are now mapped once
for all the holes tried in the solid. 1283 and 1284 together took 0.4 s
off the union.

Not patched: a box test of a hole's shell before the checker classifies
another shell's point against it (`BRepCheck_Solid`; a point outside the
box of a cavity is outside the cavity) changes a result. OCCT's classifier
fails (it reports a rejected line, which counts as out) on a point of one
of the union's cavities that lies in the plane of the bottom faces of two
other cavities 300 mm away, so the checker finds the union with merged
faces invalid ("a sub-shape not in the shape") and the union is taken as
OCCT left it (8 721 faces). With the box test the merged union is valid
and taken (7 458 faces, 0.26 mm³ less), and the union took 1.5 s less,
mostly because the union as OCCT left it was then not checked.

## 1680: the join of two stripes across a corner

### 1680-chfi3d-corner-join-face-classifier-set-up-once-per-face

`ChFi3d_Builder::JoinAcrossCorner` (0772, 0820) walks one blend over a
corner of the solid and finds where its contact on the side faces leaves
the one face and comes onto the other: it classifies the contact's points
in 400 steps from the middle of the first stripe's piece, then bisects
each change in 50 steps. Every point was classified with a new
`BRepClass_FaceClassifier(F, P, Tol)`, which computed the face's parameter
bounds again from its edges' p-curves, set up the projection on its
surface again (`Extrema_ExtPS`: for a B-spline surface the grid of its
samples, up to a thousand parameters a direction on a thread's face since
patch 0593, evaluated at the first point) and made the explorer of its
edges again. Each face now has its bounds, its projection and its
explorer set up once (`gapFaceStates`), and each point is projected and
classified with them: the same bounds and tolerances, the nearest
projection classified against the face's edges by the same classifier, a
point without a projection out, so the answers are those of a classifier
made for the point.

On the threaded design of mitcad#1801, whose edit (a chamfer's distance)
recomputes a fillet that joins two stripes across a corner beside the
thread, the fillet took 2.7-3.4 s instead of 3.1-3.9 s and the edit
4.8-6.2 s instead of 5.3-6.7 s (four pairs of runs, the old and the new
library at the same time). What is left of the join is the classification
of each point against the face's edges itself (the line from the point
intersected with the thread's helical p-curves), about a sixth of the
edit.

## 1690 and 1691: a blend's line, a face's polyhedra

Both from the three slowest geometry tests (mitcad#1823), profiled with
stack samples.

### 1690-brepblend-line-points-read-by-index-in-constant-time

A blend's line (`BRepBlend_Line`, the points a fillet's walk found) held
its points in an `NCollection_Sequence`, whose access by index walks its
linked list from the place read last. The approximation of the blend's
surface (`BRepBlend_AppFuncRoot::SearchPoint`, under
`Approx_SweepApproximation`) finds the points around each parameter it
evaluates by a bisection over the line, reads the line's middle point for
a ball that stays put (mitcad#1412) and inserts the points that took long
to solve, so every evaluation walked the line several times over. Rounding
the window sills of a ring wall away (`geometry.fillet_consumed_strip`),
whose blends the approximation cuts into many pieces, spent 90 % of its
stack samples in `NCollection_BaseSequence::Find`. The line now holds its
points in a `std::deque` of their own allocations: a point is read by index
in constant time, keeps its address while others are added or removed
elsewhere (as a sequence's node did), and the line's order, contents and
range checks are those of the sequence.

The fillet of that test took 23 s instead of 514 s on a Linux workstation
(the old and the new library at the same time), its result written as
BRep the same byte for byte. What is left of it is the approximation's
own work: each evaluated section solved by Newton's method
(`math_FunctionSetRoot` on `BlendFunc_ConstRad`), at the many points of a
blend whose two contacts meet at the wall's middle.

### 1691-intcurvesurface-surface-polyhedra-made-once-per-face

The intersection of an edge with a face in a boolean
(`IntTools_BeanFaceIntersector::ComputeAroundExactIntersection`) asks
`IntCurveSurface_HInter` where the edge's curve meets the face's surface;
for a surface that is neither a plane nor a quadric, HInter samples each
piece of the surface into a polyhedron (up to 40 × 40 points and their
derivatives, with its deflection; a B-spline surface's own samples) for
every curve. A face meets many edges in a boolean or an offset, so its
polyhedra were made again for each. `IntCurveSurface_HInter` gets
`Perform(Curve, Surface, Polyhedra)`: the same steps as `Perform(Curve,
Surface)`, with the polyhedron of each piece taken from `Polyhedra` where
it was made before for the same bounds and kept there where it is made
now; `IntTools_Context` keeps each face's polyhedra
(`CurveSurfacePolyhedra`) for the bean intersector. The same polyhedra
give the same intersections.

The 28 closed offsets of lofts of `geometry.offset_loft_circle_seam_lean_in`
(each a boolean of its offset faces per pair) took 68 s instead of 92 s
(the old and the new library at the same time), all 28 results the same
byte for byte as BRep. What is left of an offset is OCCT's own work:
the face/face intersections and, for the edge merged across a tangent end
(patch 1190), its curves projected on both faces and their tolerance
found by a global search (`GeomLib_CheckCurveOnSurface`).

## Reports and tests

- 0006 to 0009: the reports mitcad#84 (0006), mitcad#90 (0007), mitcad#91
  (0008) and mitcad#92 (0009).
- 0060: mitcad#60 and `geometry/tests/test_shapefix.cpp`, whose expected
  results are OCCT's without the patch.
- 0120 and 0121: mitcad#119 and `geometry/tests/test_large_bodies.cpp`,
  whose statuses are OCCT's without them.
- 1270: mitcad#1273, the union of the reported design of GitHub #133 in
  `geometry/tests/test_join_many_bodies.cpp` (its results are the same
  without the patch; only its time changes).
- 1280 to 1284: mitcad#1279, the same test (the same results, 17 s → 12.9 s
  for the union), and the boolean, fillet and chamfer tests of `geometry`
  and `core.model.features-solid`, whose results are OCCT's without them.
- 1680: mitcad#1806, the reported threaded design of mitcad#1801 (its
  bodies and timeline the same, `mitcad-cli info --json` byte for byte),
  and the fillet and chamfer tests of `geometry`, whose results are OCCT's
  without it.
- 1690 and 1691: mitcad#1823, `geometry/tests/test_kernel_repeated_work.cpp`
  (the line's order and addresses through appends, prepends, inserts and
  removals; the curve/surface intersections with and without kept
  polyhedra on a B-spline surface and a surface of revolution), and the
  ctests `geometry.fillet_consumed_strip` and
  `geometry.offset_loft_circle_seam_lean_in`, whose results are OCCT's
  without them.

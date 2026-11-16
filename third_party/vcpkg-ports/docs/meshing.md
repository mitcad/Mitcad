# Meshing: 1070, 1080, 1081, 1082

Patches to the triangulation BRepMesh takes a plane's base triangulation
from with its Delabella algorithms, and to the check of a face's wires
before it is triangulated. Back to the [port's README](../README.md).

## Overview

- **1070** makes Delabella's in-circle and orientation signs exact, so
  that nearly cocircular boundaries no longer crash it or loop it forever,
  and ends a hull that still comes out inconsistent as a failed
  triangulation (mitcad#846).
- **1080** evaluates 1070's exact signs without the heap, and from the
  differences directly where they are exact: the same triangulations in
  less than half of Delabella's time on a perforated plate (mitcad#884).
- **1081** skips, in the healer's search for crossing segments of a
  face's wires, the pairs of wires whose boxes are apart: the same
  crossings, a tenth of the time on a plate with many holes (mitcad#884).
- **1082** takes, at Delabella's start, a cocircular point beyond any edge
  of the contour, not only beyond the two at the point added last: a
  rectangular plane, whose corners 53-bit rounding could order so, is
  triangulated instead of failing (mitcad#1919).

## 1070: Delabella on nearly cocircular points

### 1070-brepmesh-delabella-exact-signs

Delabella (`delabella.cpp`, the sweep-hull Delaunay triangulation of
`BRepMesh_DelabellaBaseMeshAlgo`) lifts a plane's boundary points, with
four corners of their enlarged box, onto the paraboloid z = x² + y² and
builds their convex hull a point at a time: the faces the new point sees
are deleted, and new faces join the point to the silhouette of the deleted
ones. MitCad's view meshes planes through it in the interactive tier and,
checked, in `mitcad-cli mesh-compare`'s `checked` variant (mitcad#635,
mitcad#644).

Whether a face sees a point is the sign of an in-circle determinant, which
Delabella computed in `long double`: 64 bits of mantissa with GCC and
Clang on x86-64, 53 with MSVC and on ARM. A building's plate, a square of
1 650 mm with a concave arc of 1 200 mm radius cut from a corner, never
finished meshing on Windows: on the arc's nearly cocircular points the
rounded signs contradicted each other, and the faces a point sees were no
longer one disk (a vertex twice on the silhouette, an unseen face inside
it). The silhouette then had fewer edges than the new faces need, the face
taken as the last one added had not been written in that step, and the
walk round the silhouette followed `next` and `sew` links left from earlier
steps or never set: a null or stray pointer, or a loop that never came back
to its start (`while (pr != entry)`). With Delabella's `long double` as
`double`, the plate's points crashed it in three of the plate's four
copies; with 64 bits, a sweep of the radius from 400 to 1 550 mm at
0.01 mm deviation crashed on 12 of 230 radii and looped on 2, through
BRepMesh alone and through MitCad's meshing alike.

- `incircleSign` and `orientSign` give the determinants' signs exactly: the
  floating-point value when it exceeds Shewchuk's error bound for it
  (doubled, for fused multiply-adds), else the exact value as an expansion
  of non-overlapping doubles (J. R. Shewchuk, "Adaptive Precision
  Floating-Point Arithmetic and Fast Robust Geometric Predicates", 1997),
  with `std::fma` for the products' errors. `Face::dot` is the in-circle
  sign of the input coordinates (the lifted z no longer rounded), and the
  z of `Face::cross` the orientation's sign, which picks the Delaunay
  faces. With exact signs every point is a vertex of the lifted hull and
  the faces it sees are a disk.
- Guards, for a hull that still comes out inconsistent: a silhouette vertex
  met twice in one step, new faces other than the deleted ones plus two, a
  silhouette that is not one loop through every new face's first vertex
  (checked before anything is sewn), a missing back link, and no face that
  sees a point end the triangulation with an `[ERR]` message and 0, as
  when out of memory. The walk round the last point is bounded by the
  number of faces, the fall-back search runs over the faces on the hull,
  and vertices and faces are allocated zeroed.
- All points on one circle: the point retracted to become the cone's top is
  unlinked from the start's contour (the cone took more faces than there
  are; BRepMesh's box corners keep its input from this case).
- `BRepMesh_DelabellaBaseMeshAlgo::buildBaseTriangulation` throws
  `Standard_Failure` when Delabella returns 0: the face fails (MitCad's
  checked planes then triangulate it by the default insertion) instead of
  going on without a base triangulation.

## 1080, 1081: perforated plates

A plate with 1,187 round holes (the repo's `perf_holes` design: two such
planes, 172,924 boundary nodes each) took 2.8 s per plane in BRepMesh
alone (one thread): 1.0 s in the healer's check of its wires and 1.7 s in
its triangulation, 1.1 s of that in Delabella. With both patches 1.1 s
(the check 0.1 s, the triangulation 0.9 s, Delabella 0.49 s), the same
triangulations; in the application the design's display meshing on a cold
open fell from 2.7–2.9 s to 1.3 s, the open from 4.6–5.1 s to 3.1–3.3 s.

### 1080-brepmesh-delabella-exact-signs-in-place

Of the plate's 14 million in-circle tests, 334,000 (the points of one hole
are nearly cocircular) are too close to call in floating point and are
evaluated exactly (1070). Each such evaluation built its expansions in
`std::vector`s, dozens of heap allocations a test.

- The expansions are held in place (`Expansion<N>`, N the most components
  the operation can give), with the same operations in the same order.
- The exact evaluations are functions of their own, not inlined, so that
  their frames (the expansions in place, tens of kilobytes) are set up
  only when the floating-point filter cannot decide.
- When the six differences of the points are exact in floating point (as
  between near points), the determinant is evaluated from them directly,
  (adx² + ady²) bc + (bdx² + bdy²) ca + (cdx² + cdy²) ab with every product
  exact, far fewer components than from the differences' expansions.
- The faces made once the hull is no longer flat get only the sign of
  their normal's z (`Face::turn`), all that is read of them.

The signs are exact before and after, so the triangulations are the same;
on the plate's planes, every triangle's nodes the same, Delabella took
0.49 s instead of 1.1 s.

### 1081-brepmesh-face-checker-wire-boxes

`BRepMesh_FaceChecker` (the healer's search for crossing segments of a
face's wires) searched every segment of a wire in the box tree of every
later wire of the face: on the plate 1,187 × 593 pairs of wires, about
145 searches each, nearly all rejected at the tree's root. A wire's tree is
now searched only when the box of the wire's own segments (its tree's
root) meets it. A segment's box lies in its wire's box, and both have at
least its gap, so the searches left out would have been rejected at the
root anyway: the crossings found are the same. The plate's check took
0.1 s instead of 1.0 s.

## 1082: a rectangle's corners at Delabella's start

### 1082-brepmesh-delabella-cocircular-start-beyond-any-contour-edge

Delabella sorts its points by their lifted z = x² + y² (BRepMesh scales
each coordinate into -0.5 to 0.5, so that z is the square of the distance
from the middle of the points' box) and, while the points so far are
cocircular, grows their contour, expecting each next point beyond one of
the two edges at the point added last. A rectangular plane's four boundary
points are cocircular round the middle (an axis-aligned rectangle stays one
when scaled), so their z's are equal but for their rounding, and the
rounding orders them. With 64-bit `long double` (GCC and Clang on x86-64)
the order is that of the exact values, in which the third corner is never
opposite the fourth; with 53 bits (MSVC, ARM) it can be, the fourth corner
then lies beyond the contour's first edge, and 1070's guard ended the
triangulation as failed ("a cocircular point on neither side"). The plane
was left without a triangulation, and MitCad's checked planes made it
again by the default insertion: `cli.perf_holes_mesh` reported five planes
redone on Windows (the five square notch walls between holes centred on
the notch's sides), and of the 10 mm cube `mitcad-cli mesh-compare` meshes
first, all six. Of a sweep of 9,415 squares and as many rectangles, 287
squares and 91 rectangles failed with 53 bits, none with 64.

- A cocircular point the two edges at the last point do not take is added
  on the edge a -> b of the contour it lies beyond (the orientation sign of
  a, the point and b equal to the contour's), found by walking the
  contour; the contour goes on from it as from a point added at the last
  point (tail a, head b, the face (b, a, point) in the contour's
  orientation). The contour of cocircular points is convex, so the point
  lies beyond exactly one edge; when it does not, the guard ends the
  triangulation as before.
- Only the points the old start refused reach the walk: every
  triangulation Delabella made before is the same.

## Reports and tests

- 1070: mitcad#846 and `test_nearly_cocircular_planes` in
  `geometry/io/tests/test_io.cpp`: the plate with discs of 1 200 mm and
  three radii that crashed or looped on Linux, meshed by both of the
  view's Delabella tiers, each plane's triangles covering its boundary and
  none made again. Without the patch the test crashes on Linux at the
  second radius. The patch's own validation (not committed): random,
  integer-grid and arc point sets, each triangulation checked for its area
  against the convex hull's and for empty circumcircles, with 64- and
  53-bit `long double`.
- 1080, 1081: mitcad#884. `test_face_checker_wire_boxes` in
  `geometry/io/tests/test_io.cpp`: a square with 64 holes apart has no
  crossing segments, and with two holes that overlap each other exactly
  their two edges cross. `cli.perf_holes_mesh` meshes the `perf_holes`
  design's body by the view's final tier, every face triangulated and no
  plane made again by the default insertion. The patches' own validation
  (not committed): the plate's two planes meshed through BRepMesh before
  and after, the triangulations compared (every triangle's nodes and
  every node's coordinates), and Delabella's triangulation of their nodes
  compared triangle by triangle.
- 1082: mitcad#1919. `test_rectangular_planes` in
  `geometry/io/tests/test_io.cpp`: a 10 mm cube, a 5 mm cube placed as
  `perf_holes`' notch walls and four rectangles of the sweep that failed,
  meshed by both of the view's Delabella tiers, every plane triangulated,
  its triangles covering its boundary, and none made again;
  `cli.perf_holes_mesh` expects no plane of the design redone. Without the
  patch the test fails where `long double` has 53 bits (MSVC, ARM) and
  passes with 64 (GCC and Clang on x86-64). The patch's own validation
  (not committed): Delabella alone on the sweep's squares and rectangles
  with their box corners as BRepMesh gives them, with 64- and 53-bit
  `long double` (`-mlong-double-64`), each triangulation 10 triangles
  covering the box; and on every subset of at least four of twelve exactly
  cocircular points with a box, the same results before and after.

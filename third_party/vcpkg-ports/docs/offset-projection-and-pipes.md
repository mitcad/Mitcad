# Offset, projection and pipes: 0100, 0960, 1030, 1130, 1160, 1180, 1190

OCCT's offset (`TKOffset`), the projection of a curve onto a surface
(`ProjLib`) and the pipe shell's guide trihedron (`GeomFill`). Back to the
[port's README](../README.md).

## Overview

- **0100** drops a face whose offset vanishes within the offset distance
  and intersects the offsets of the faces on both sides of it instead
  (mitcad#172).
- **0960** stops the projection's boundary search once its bracket's
  endpoints are adjacent floating-point values, where it tried the same
  projection for ever (mitcad#504).
- **1030** corrects the second derivative of the binormal of the guide
  trihedron by curvilinear equivalence (`GeomFill_GuideTrihedronAC::D2`),
  which bent a pipe shell's surfaces along an auxiliary spine between its
  sections (mitcad#539).
- **1130** keeps only the branch along a convex crease of the intersection
  of its faces' offsets where they become tangent past the solid's end (a
  loft from a polygon to a circle shelled or offset inwards), which
  failed or came out invalid at every thickness (mitcad#1184).
- **1160** no longer intersects, on the offset of a face, the images of
  two of its edges whose faces beyond meet along a tangential edge (a
  loft's side split by the circle's seam): their common vertex is where
  each crosses that edge's offset; the closed offsets were invalid
  (mitcad#1199).
- **1180** keeps, when an edge is extended for the intersections, a curve
  on a face that lies on the edge all along its own range where
  reprojecting it fails, instead of an approximation that was 0.13 mm off
  (the same loft's shared tangential edge; mitcad#1201).
- **1190** makes one intersection edge of the branch along the crease
  (1130's) and its continuation where the offsets become tangent inside
  the solid (the walls leaning in towards the circle), and widens a
  substituted vertex's tolerance to the points it takes over
  (mitcad#1193).

## 0100: faces vanishing within the offset

The patches 0100 and later change OCCT's offset (`TKOffset`, mitcad#172);
`geometry/tests/test_shell_vanishing.cpp` rebuilds the cases with exact
volumes, partly with OCCT alone.

### 0100-brepoffset-faces-vanishing-within-the-offset

A face whose offset vanishes within the offset distance (offset inside: a
chamfer strip narrower than what the offsets of the faces beside it take
from it, a convex rounding of a radius smaller than the wall; outside, the
same concave) is dropped, and the offsets of the faces on both sides of it
are intersected with each other instead, as if they were neighbours
(`FindVanishingFaces`, `BRepOffset_Inter3d::ConnexIntAcrossVanished` and
`ConnexIntByArcAcrossVanished`); the walks round the faces' wires skip its
edges (`BRepOffset_Inter2d::ConnexIntByInt`). It finds faces of four edges
whose offset turns inside out between two opposite edges all along them,
also several in a row (a strip and the cones at its corners); a face that
vanishes only in parts stays, and with the arc join one with a tube along an
edge. Before, the shell failed (the thick solid had fewer faces than the
body), a closed offset had no solid, or the solid was wrong: the inverted
rounding kept, or with the arc join the body itself, unshelled. A thick
solid to which the offset contributes no face is no longer done. The private
methods `BuildOffsetByArc`, `BuildOffsetByInter`, `Intersection3D`,
`IntersectEdges` and `ToContext` of `BRepOffset_MakeOffset` and three
methods of `BRepOffset_Inter2d` and `BRepOffset_Inter3d` get parameters; no
class gets members.

## 0960: the projection's boundary search

The patch 0960 comes from mitcad#504: a rounding's corner of more than
three edges hung while its boundary curve was projected onto the initial
surface of a plate. A stack sample of the import of `eecf90bd` stopped in
`DichExactBound` inside `ProjLib_CompProjectedCurve`.

On the replayed body before item 50 (Fillet7, radius 1 on ten edges),
OCCT's four fillet contours reach that projection with a boundary made
from a straight 2D line on a B-spline blend surface. The boundary extends
far beyond the surface's native parameter bounds, so evaluating it
extrapolates the surface and produces plane coordinates of order 1e11
and a projection derivative of order 1e10. The boundary search's last
accepted parameter is 102.90382719900337, the failed one
102.90382719900339: adjacent doubles, 1.4210854715202004e-14 apart. Its
UV-derived stopping tolerance is 1.1242033789204225e-14, smaller than that
gap, and their midpoint rounds to the failed endpoint. A second projection
onto a bounded initial plane reaches the same condition with a tolerance
of 1.1343993885480033e-15.

The replayed input has 98 faces. On the file's stored input (84 faces),
the same ten edges match within 1.1e-14 mm and the same four contours fail
promptly, before and after 0960. On the replayed input, 0960 makes the
boundary search return too; the projection cannot cover the whole corner
curve, and `ProjLib_CompProjectedCurve::D0` rejects a parameter outside its
continuous parts. The OCCT-only probe then reports that the corner failed
instead of hanging. It uses OCCT's normal signal-to-exception support,
as the import already does.

In the diagnostic and final production macOS builds, importing `eecf90bd`
without `--hang-limit` returns: item 50 fails promptly and takes the stored state through the
existing import error handling. Its final report names a zero-length
curve where the rounding's last pieces lie on different faces. All 120
item outcomes stay as before (101 parametric, 7 fallback, 12 skipped),
all other item notes are unchanged, and the five final bodies' volume
differences are identical to the baseline. The watchdog warning is gone.

### 0960-projlib-projection-boundary-bisection-stops-at-adjacent-parameters

The boundary search bisects between a curve parameter that projects and one
that does not. Its stopping tolerance is the UV tolerance divided by the
projected curve's derivative; it can be smaller than the spacing of the
parameters' floating-point values. Once the bracket's endpoints are adjacent
values, their midpoint is one of the endpoints and the same projection is
tried for ever. Stop then, retaining the last successfully projected point.
The projection's tolerances and success criteria stay as they were; no
failed point is accepted and no iteration limit cuts a search short while it
can still make progress. The OCCT-only test `geometry.projection_boundary`
projects a straight B-spline segment with knots from 1e6 to 1e6 + 1e-6 onto
a bounded plane, where its crossings have no representable parameters.
Without 0960 the constructor hangs in `DichExactBound`. The test checks that
the boundary points project correctly to 1e-6 mm and that their adjacent
parameters lie outside the plane's bounds, instead of demanding a curve
point at an unrepresentable parameter. A segment with the same knot span
near 1 is the control.

## 1030: the guide trihedron's second derivative

The patch 1030 comes from mitcad#539 (the FreeCAD import's pipes along an
auxiliary spine): an 8 x 2 bar swept 50 along a line and turned a quarter
turn by an auxiliary spine 10 above it, crossing from one side to the
other, the spine's points at the same fraction of the lengths as the
line's (the sweep's auxiliary `frame` with `curvilinear`; the reference
model `pd_pipe_modes`, whose stored pipe holds 800 mm³: every section is
the bar).

### 1030-geomfill-guide-trihedron-ac-second-derivative-of-the-binormal

The trihedron's values and first derivatives were right (checked against
differences of D0 and D1), and so were the pipe's sections (`Simulate`) and
its surfaces at the approximation's sections, but its side faces lay up to
0.27 off the turned bar between them: the pipe held 784.9 mm³ (775.9 at a
tolerance of 1e-7). `D2` took the binormal's second derivative as a unit
vector's with the square of the binormal itself (1) where the square of its
first derivative belongs, which added `-(1 - |d|^2) b`; the binormal's
second derivative was off by about the binormal. With the square of the
first derivative (as the normal's second derivative has it) the pipe holds
800 mm³, as by the plane rule.

## 1130: offsets tangent past the solid's end

### 1130-brepoffset-offsets-tangent-past-the-end-keep-the-branch-along-the-crease

A loft from an octagon to a circle (`BRepOffsetAPI_ThruSections`), shelled
3 mm inwards with both its ends removed, or offset inwards closed. At the
octagon's corners the loft's ruled faces meet at convex creases that
flatten to tangency at the circle. Where the walls lean out towards the
circle, the inward offsets of the two faces at a corner become tangent a
little past the circle's end (0.16 mm past it for the 3 mm wall) and cross
there. `BRepOffset_Inter3d::ConnexIntByInt` intersects the enlarged offset
faces: the branch along the corner and five to seven more pieces about and
past the tangency, up to the enlarged faces' ends, all connected, so that
`CheckIntersFF` keeps them all. From them `MakeLoops` rebuilt no inner face
for most walls: the thick solid was not done (`BRepOffset_UnknownError`) or
came out invalid (the body with offset faces of no area), a
`Geom_OffsetSurface` evaluation failed on a vertex of such a piece
(`UpdateTolerance`), and the closed offset was null or invalid, at every
thickness from 0.5 mm. The seam the circle splits an octagon's side at is
no part of it (a seam on a corner fails the same way), nor is the
thickness; the faces' enlargement does not reach where the ruled surfaces
narrow to their apex lines (1.8 of their height below the octagon). A
square to a circle that it surrounds failed the same way.

`BRepOffset_Tool::Inter3D` now keeps only the branch along the edge there
(the new `DropCrossingsPastTangentEnd`, after `CheckIntersFF`): of the
edges nearest to the edge's middle (a curve the pave filler gives twice,
once run on past the tangency, the one that runs least far past the end)
and the edges it goes on into towards the edge's other end where no other
edge meets them. It acts only where the edge's two faces have parallel
normals (within 1e-4) at one end of it and not at its middle, the
intersection gave more than one edge, and that branch runs past the plane
of the solid's end there, the osculating plane of the face's boundary at
that end. Where the offsets become tangent before the end (walls leaning
in, the end's own offset cutting the branch first) and for every other
edge, the intersection is as before: a square to a smaller circle (walls
leaning in) fails as before when shelled, and its closed offset is valid as
before. The shells are valid, both ends open, the inner walls the
thickness from the outer ones within 1e-6 mm.

## 1160: images meeting along a tangential edge

### 1160-brepoffset-images-meeting-along-a-tangential-edge-meet-where-they-cross-its-offset

A loft from a polygon to a circle (`BRepOffsetAPI_ThruSections`) whose
circle's seam falls on a side of the polygon, offset inwards closed. The
loft splits that side at its middle, so it becomes two faces tangent all
along the edge between them (their normals agree there within 1e-14), and
their offsets share that edge's offset. At each end of the edge,
`BRepOffset_Inter2d::ConnexIntByInt` walks the wire of the end face and
intersects the images of its consecutive edges on the end face's offset;
at the split those are the curves where the two halves' offsets cut it,
tangent where they meet. Their 2D intersection (`RefEdgeInter`, two
B-splines) is ill-conditioned: 2.7 µm off along them for a square to a
circle of radius 15 at 1 mm, 7 µm at the circle's end. The walks of the two
halves find the same vertex where each image crosses the offset of the
tangential edge, transversally and exactly; `FuseVertices` merged them into
one whose tolerance covered the points but kept an edge's parameter from
the ill-conditioned intersection, so the checker found the vertex off its
curve (`BRepCheck_InvalidPointOnCurveOnSurface`). The closed offsets were
invalid at most thicknesses (a square to a circle of radius 15 at 1 to
3 mm, radius 14.142 at 2 mm, the octagon of 1130 to a circle of radius 21.7
at 0.5 to 3 mm), and up to 2 mm³ off in volume where they passed.

`ConnexIntByInt` now skips that intersection (the new `MeetAlongTangentEdge`
and `IsAnalyticOn`): only where the two edges' common vertex has three
edges, the third tangential all along it (the analysis's single
`ChFiDS_Tangential` interval) with the faces beyond the two edges on its
sides, both edges have images, and the images' curves on the offset face
are not both lines or conics (whose tangent intersections the 2D
intersector computes exactly: a rounding between planes cut by a plane is
as before). Every other intersection is as before. The offsets are valid
single solids, the inner walls the thickness from the outer ones within
1e-6 mm, and of the volume of the same loft with the seam on a corner (no
side split) within 3e-5 of it.

Left as they were, at the polygon's corners, not at the split side: the
closed offsets where the corner offsets become tangent inside the solid or
on the plane of its end (a square to a circle of radius 6, 8 or 10, or 14.142
at 0.5 mm, the octagon to a circle of radius 14 or 12), the crease branches
of mitcad#1193 (patch 1190). A shared tangential edge whose curve on one
half's offset was 0.13 mm off (a square to a circle of radius 20) is patch
1180's.

## 1180: an extended edge's curves where reprojection fails

### 1180-brepoffset-extended-edge-keeps-a-pcurve-on-its-curve-where-reprojection-fails

The loft of 1160, a square to a circle of radius 20, the seam on a side,
offset inwards closed. The shared offset of the tangential edge between
the two halves has a line u = 10 on each half's offset surface, its
parameter v the edge's. `BRepOffset_Inter2d::ExtentEdge` extends it for
the intersections: each curve is clipped to its surface's bounds, which
differ (the halves' offset surfaces are extended by different lengths, v
to 1.245 and 1.250), the 3D curve is rebuilt on the narrower one, and the
other curve is reprojected onto its surface (`GeomProjLib::Curve2d`). That
projection failed, and `SameParameter` then replaced the exact line by an
approximation 0.2 mm off in parameter (v = 1.0065 t - 0.0033 where the
surfaces agree). The edge kept it on one half, its tolerance raised to
0.14 by `UpdateTolerance`: the offset valid, but that half 3.27 mm² short
and the volume 12.7 mm³ short at 0.5 mm (8.1 mm³ at 3 mm).

Where the projection fails, a curve that lies on the rebuilt 3D curve
within its approximation's deviation plus 10 `Precision::Confusion()` at
33 parameters over the edge's own range (the original edge's, within the
extension) is now kept as it is (the new `PCurveFollowsCurve`); only
otherwise `SameParameter` runs as before. Where the projection succeeds,
and for every other curve, nothing changes. The offsets' edges are within
0.001 mm at 0.5 to 2 mm, the volume that of the same loft with the seam on
a corner within 1e-5. At 3 mm the curves where the halves' offsets cut the
circle's end come out to 1.06e-3 mm, and the volume is 1.6e-5 of it off
(those intersections, not this curve).

## 1190: offsets tangent inside the solid

### 1190-brepoffset-offsets-tangent-inside-keep-the-branch-and-its-continuation

A loft from a 20 mm square to a circle inside it or through its corners
(radius 8 to 14.142), whose walls lean in towards the circle, shelled
inwards with both its ends removed, with one, or offset inwards closed.
The inward offsets of a corner's two faces become tangent inside the solid
(0.1 mm below the circle's end for a 1 mm wall at radius 10, on it where
the circle runs through the corners). Past the tangency the walls, extended
to a removed end, cross along the branch's continuation and two branches
turning sideways, all starting tangent there; the exact extended walls
would hold crescent slivers up to 0.06 mm wide between them. The pave
filler gives the branches as pieces joined by pieces of a few microns, not
always connected, sometimes the branch along the crease and a sideways one
as one curve. 1130 left them all (the branch ends before the end plane),
and `MakeLoops` could not rebuild the inner walls: the thick solid was not
done or invalid, the closed offset invalid or empty, for most thicknesses
from 0.5 to 3 mm. Keeping the branch and its continuation as those pieces
does not help: `AssembleEdge` glues them into an edge of a tolerance of
50 mm, which `Inter3D` drops.

`DropCrossingsPastTangentEnd` now finds where the offsets become tangent
(the tangent end's nearest point on the offset face) and, where that lies
inside the solid or within 1 % of the offset past its end, makes one edge
that `Inter3D` gives as the intersection, with its curves on both offset
faces: the branch along the crease up to 1 % of the offset before the
tangency, of all the pieces of the intersection the one from within 5 % of
the offset of the tangency that goes on most nearly in the branch's
direction from 1 % past it (and on into the pieces that go on from its far
end in that direction, where it is short: the loft the geometry facade
builds, its circle's seam splitting a side), and a curve tangent to both
between them, each parametrised by its
length. The sideways crossings, slivers in the walls' extension up to a
removed end, are left out. A single edge that runs on along the branch
itself is left as it is; where the tangency lies further past the end, 1130
is unchanged.

`BRepOffset_MakeLoops` substitutes vertices and gives the new one the old
one's points on the edges' curves, which could lie further from it than
either's tolerance: in the closed offset and the one-end shells the vertex
where a corner's edge meets the end's offset lay 6.4e-5 mm off one of them
against a tolerance of 5.7e-5 mm (`BRepCheck_InvalidPointOnCurve`), and
so in the closed offset of a square to a circle of radius 20 (walls leaning
out) at 2 mm. Its tolerance now covers the points it takes over, with a
margin of 0.1 % (`CoverPoints`); where they lay within it, it keeps it but
for the margin.

The shells and offsets are valid, both ends open, the inner walls the
thickness from the outer ones within 1e-6 mm, the volume of the shell open
at both ends the one the one-end shells and the closed offset give (within
0.05 mm³). OCCT's default volume integration (without an accuracy) is off
by up to 0.3 % on these bodies; with an accuracy, and on a mesh, it agrees.

## Reports and tests

- 0100: mitcad#172 and `geometry/tests/test_shell_vanishing.cpp`, which
  rebuilds the cases with exact volumes, partly with OCCT alone.
- 0960: mitcad#504 and the OCCT-only test `geometry.projection_boundary`
  (described with the patch above).
- 1130: mitcad#1184 and `geometry/tests/test_shell_loft_to_circle.cpp`
  (octagon-to-circle lofts of radii 24.5 and 21.7, the seam on a side and
  on a corner, shelled 0.5 to 3 mm with both ends removed, a square to a
  circle of radius 20 shelled 0.5 to 2 mm, and the closed inward offset,
  with OCCT alone; the reported loft shelled through the geometry facade:
  valid single solids, both ends open, the walls the thickness within
  1e-6 mm; every OCCT-only case fails without the patch).
- 1160: mitcad#1199 and `geometry/tests/test_offset_loft_circle_seam.cpp`
  (a square to a circle of radius 15 offset 0.5 to 3 mm and of radius
  14.142 offset 2 mm, the octagon to a circle of radius 21.7 offset 0.5 to
  3 mm, the seam on a side, closed inward offsets with OCCT alone: valid
  single solids, the walls the thickness within 1e-6 mm, the volume within
  bounds from the areas and of the seam-on-a-corner loft's offset within
  3e-5; every case but radius 15 at 0.5 mm fails without the patch).
- 1180: mitcad#1201 and the same test (a square to a circle of radius 20
  offset 0.5, 1 and 2 mm, the seam on a side: valid, edges within
  0.001 mm, the volume that of the seam-on-a-corner loft's offset within
  1e-5 where that one is valid; all three fail without the patch on the
  edges, at 0.5 and 1 mm on the volume).
- 1190: mitcad#1193 and `geometry/tests/test_shell_loft_to_circle.cpp`: a
  square to circles of radii 8, 10 and 14.1 shelled 0.5 to 3 mm with both
  ends removed, the volume of the shell open at both ends against the
  one-end shells and the closed offset for three of them, and a loft
  leaning in shelled through the geometry facade.

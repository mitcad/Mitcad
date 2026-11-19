# A corner's set-back walked over a vertex, a walk starting on an edge: 1801 to 1803

mitcad#2018, a follow-up of mitcad#2009 (port patch 1800): the bent guide
imported from a FreeCAD document, its pads' ends and its ribs rounded with
0.5, made smaller through its bend radius. Back to the
[port's README](../README.md).

## Overview

- **1801** lets the set-back of a corner of more than three edges walk
  over the far vertex of a sharp edge shorter than it, onto the edge that
  goes on along it: the plate's point lies on that edge, and its border on
  the other side crosses the tangent edge at the vertex. Before, the point
  lay past the edge's end and the result was invalid or open.
- **1802** ends a walk whose first point lies on a face's edge within the
  guide's tolerance on that edge, as a walk ends there from any later
  point. Before, it ended at its first point on no edge, and ChFi3d took
  that for a walk that drags: the rounding failed.
- **1803** lets the plate's border leave the point on the edge walked onto
  at no set angle. Before, it left back along that edge over the crossing
  of the tangent edge, a sliver narrower than the plate's tolerance with a
  vertex in it, and the plate's face could not be triangulated.

## The case

The guide is a round tube (radius 2.2) with a flat top 1.1 above its axis,
swept through a quarter turn about the vertical axis, a straight pad 2
long at each end. The flat is one face over the bend and the pads; the
tube goes on into each pad's cylinder across the bend's end edge, a
circle's arc, tangentially. Ribs stand across the flat, their sloped sides
going on tangentially into the tube at the flat's two rims; the rib
nearest each end spans 1 to 5 degrees of the bend. The design rounds the
pads' ends and each rib's four sloped edges with 0.5.

Below a bend radius of about 31 the fillet failed: "invalid shape" from 27
to 30.5, "would leave the body open" from 26 down, and from 23.5 to 24.5
the rounding of the outer sloped edge of the rib 1 degree from each end
failed ("stops part of the way: the walk between the two faces fails").

## 1801: the set-back walks over the vertex

### 1801-chfi3d-corner-set-back-past-a-short-sharp-edge-walks-over-its-vertex

Where the rounding of a rib's sloped edge ends on the flat's rim,
`PerformMoreThreeCorner` fills the corner with a plate: one stripe and
three sharp edges, the rim under the rib (tube and sloped side, tangent),
the rim beyond the rib (flat and tube) and the edge between the flat and
the rib's face. The plate's points on the sharp edges are set back by the
largest distance of the stripe's ends from the vertex, 0.508 at the inner
rim and 0.496 at the outer. The rim from the rib to the bend's end is 1
degree long: 0.49 at the inner rim at a radius of 30 (0.42 at 26), and
0.45 at the outer at 24. The point lay past the rim's end, on the rim's
line beyond the edge: an invalid shape or a body left open.

At the rim's end vertex three edges meet: the rim, the pad's rim going on
along it (the flat is one face over both), and the bend's end edge across
which the tube goes on tangentially into the pad's cylinder.

- `WalkOverVertex` (new): whether a sharp edge E between faces Fa and Fb
  ends at a vertex V2 with three edges: E, an edge E2 of Fa going on along
  E (at pi), and an edge Eg between Fb and E2's other face Fc, across which
  Fb goes on tangentially.
- `PerformMoreThreeCorner`, after the parse, where no two stripes are next
  to each other, so that the set-back is the largest distance of the
  stripes' ends from the vertex (no intersection between stripes), and the
  corner has no free border, seam or two tangent stripes: a sharp edge
  between two sharp edges, shorter than the set-back, that `WalkOverVertex`
  accepts, with E2 longer than the rest of the set-back, is replaced in the
  corner's edges by Eg and E2 (Eg on the side of Fb, Fc between them).
  Their near vertex is V2: their parameters at the vertex, their
  directions, the curves' orientation along them and the battens' end
  angles are taken there instead of at the corner's vertex. E2's point is
  the rest of the set-back along it from V2.
- The plate's border on the side of Fb crosses Eg between the nearest
  points of Eg to the border's two ends, weighted by their distances.
  Here both ends lie on the rims, so that the crossing is V2 itself; it is
  taken 1e-4 (the corner's approximation tolerance) along Eg from V2, as
  wherever it falls within ten times that: a border through V2 runs along
  E, which the builder then keeps in the tube's face as a dangling edge
  (an invalid wire). With the crossing 1e-3 away the plate's border folded
  within its approximation's error and the plate's wire crossed itself.
- Elsewhere nothing changes. No catch is added.

## 1802: a walk starting on its face's edge

### 1802-brepblend-walk-whose-first-point-lies-on-a-restriction-ends-there

From 23.5 to 24.5 the rounding of the outer sloped edge starts its walk
0.0024 from the spine's end, where the first section, reframed onto the
sloped side's edge with the tube, lies within 6e-5 of that edge. The walk
back towards the spine's end leaves the side at its first step.
`BRepBlend_Walking::InternalPerform` takes any first step that leaves a
face for a step too large and halves it down to the guide's tolerance
(6.4e-5), then ends the walk at the first point with no point on the edge.
`ComputeData` took that for a walk that drags (its first point more than
ten times the guide's tolerance from the spine's end, on no edge), and the
rounding failed. At 23 and at 25 a halved step happened to land inside the
face, and the next step found the edge.

- `InternalPerform`: the last first step the guide's tolerance allows (a
  step under twice that tolerance) that leaves one face, the other's point
  inside its face, goes on to the reframing on that face's edge, as a step
  from any later point does; where that gives a point on the one edge past
  the first point, the walk ends there. Otherwise the step is halved as
  before: where the reframing fails, where both points leave their faces,
  and where the edge's point is the first point itself (a walk starting
  where an earlier one ended on that edge). Taken there, the walk looped
  without end at the first point in OCCT's case blend/encoderegularity/A1
  and in `geometry.fillet_still_ball_corners`, and a vault's end rounded
  into the face it runs into (`geometry.suite.fillet_corners`) failed.
- No catch is added.

## 1803: the border from the point walked onto

### 1803-chfi3d-border-from-a-point-walked-onto-leaves-it-at-no-set-angle

With 1801 and 1802 the guide built at 24.5 and 25, but an STL export at a
deviation of 0.01 and an angle of 10 degrees (25), or 0.05 (24.5), failed:
"a face could not be triangulated", the plate of the corner whose set-back
walks over the inner rim's end (the app would draw the body with a hole
there). BRepMesh's triangulation of that face came out empty.

The plate's border on the flat runs from the point on the pad's rim, 0.11
past the rim's end vertex, to the point on the rib face's edge with the
flat. It is a batten whose ends keep the angle of the sharp edges'
directions there (`CalculBatten`), so that it left the point back along
the pad's rim, over the vertex where the border on the tube's side crosses
the bend's end edge: a sliver of the plate 0.01 wide and 0.1 long, that
vertex in it. The plate fits its borders within 0.012 (the G1 border along
the rim, where the rib's sloped side goes on into the tube, against the
flat's G0 border; the corners' plates fit within 0.002 to 0.014 with or
without the walk), so that its surface is approximated within 0.04, its
edges' tolerances 0.05 and its vertices' 0.08: the mesher took the
sliver's sides as one within those tolerances. With all its tolerances set
to 0.03 the face meshed.

- `PerformMoreThreeCorner`, in its connecting curves: where the corner
  walked over a vertex, the batten from the point on the edge walked onto
  towards the next sharp edge (not towards the tangent edge) has no angle
  at that point (`CalculBatten`'s constraint there off, as for a free
  border); its other end keeps its angle. The border leaves the point
  towards its other end, and the sliver with the vertex in it is gone.
- Elsewhere nothing changes. No catch is added.

The walked corners' plates then mesh at every setting tried (deviations
0.0005 to 0.05, angles 5 to 20 degrees) from 20 to 32. At 25.5 and below,
the plates of the corners at the ribs' end faces on the outer rim (no walk)
still fail at the finest settings (a deviation of 0.002 with an angle of 5
degrees; from 23 down also 10 degrees), their own G1 border along the rim
against the flat's border folding back along it at its point on the rim.

## Results

The guide imported with its bend radius set from 20 to 45: every feature
rebuilds, the body valid and within its pads' ends, and its volume falls
by 26.22 mm3 for each millimetre of radius, within hundredths of a cubic
millimetre of that line across the radii where the corners walk over the
vertex (inner rims below 31, outer rims below 26.5) and where they do not.
Each radius still notes the plates' bulge of about 0.02 out of the body
that 45 noted before (the plates' approximation; mitcad#2009).

## Reports and tests

- `geometry.fillet_corner_walk_over_vertex`
  (`geometry/tests/test_fillet_corner_walk_over_vertex.cpp`): with
  `MITCAD_GEOMETRY_CASES`, the case `fillet-corner-walk-over-vertex`, the
  bodies before the rounding at bend radii 30 and 24: the inner sloped edge
  (30) and the outer (24) of the rib 1 degree from the end rounded with 0.5
  by OCCT's fillet alone build valid, within the body's box, taking less
  than three times the rounding's section along the edge. Without the
  patches: an invalid result (30), the fillet fails (24).
- `cli.fcstd_bend_reported`: the reported design (from
  `MITCAD_MITCAD_REPORTED`, never committed) imported with the bend radius
  set to 30, 25, 24 and 20 as well: every feature rebuilds, the body stays
  within its pads, its volume on the line through 32 and 45 within 0.1
  mm3; at 24.5 and 25 the body exports as STL at deviations of 0.0005,
  0.002, 0.01 and 0.05 with an angle of 10 degrees (1803).
- A DRAW reproduction: `restore` the case's body, `explode b e`, `blend r b
  0.5 b_28` (30) or `b_26` (24): without the patches `checkshape` finds the
  result invalid (30) and the blend fails (24); with them both are valid.
- OCCT's own blend and chamfer groups (the private suite's `run.sh`, a DRAW
  build of the port with 1801 and 1802 against the same build without
  them, and with 1803 against the port without it): 754 cases each, no
  case changed.
- No body built from OCCT's primitives alone was tried for 1802: its walk
  starts within 6e-5 of the edge, a matter of the input's last bits.

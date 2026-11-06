# The blend rolling on an edge as a true rolling ball: 0170–0172, 0390, 0391, 0500–0503, 1230, 1370

The blend OCCT's fillet rolls on an edge, where a rounding's face is
narrower than the radius, made a true rolling ball, as
[0075](fillet-edge-overflow.md#0075-blendfunc-constrad-rolling-ball-section)
did for the blend between two faces (mitcad#237, mitcad#378, mitcad#1215,
mitcad#1370).
`geometry/tests/test_fillet_rolling_ball.cpp` checks the cases with OCCT
alone. Back to the [port's README](../README.md).

## Overview

- **0170 to 0179** make the blend rolling on an edge a true rolling ball
  (mitcad#237).
- **0390 and 0391** let a blend rolling on an edge pass an edge turning
  towards the ball and let the ball rest on the edges of both faces
  (mitcad#237).
- **0500 to 0502** make the rounding between a cylinder and a cone about one
  axis a torus, cut it round flats under an overhanging rim it passes below,
  and cut a rounding whose contact runs off its face over a gap by the face
  at its end (mitcad#237).
- **0503** joins two roundings at a corner along their intersection where
  their ends meet on an edge within its tolerance (mitcad#378).
- **1230** lets a rounding whose contact runs off a narrow face onto an
  earlier blend of its radius roll on that face's far edges, the ball
  passing over slivers there (mitcad#1215).
- **1370** lets that ball's turn over a sliver carry its other contact past
  the seam of a face closed round its periodic surface (mitcad#1370).

## 0170 to 0172: the rolling ball and its pivot

The patches 0170 and later make the blend that rolls on an edge (where a
rounding's face is narrower than the radius) a true rolling ball, as 0075
did for the blend between two faces, and let it turn about the edge's ends
(mitcad#237); `geometry/tests/test_fillet_rolling_ball.cpp` checks the
cases with OCCT alone against the ball's centre curves.

### 0170-brepblend-surfrst-rolling-ball-and-pivot

The surface/edge blend (`BRepBlend_SurfRstConstRad`, and the inverse
functions for its ends) puts the ball's centre at the radius along the
surface's unit normal and at the radius from the edge, the edge tangent to
the ball; the guide only picks the ball (the contact points' mid-point in
its normal plane, as 0075), and each section is the ball's arc in the plane
holding both contact directions. Before, both contacts lay in the guide's
normal plane (6.5 · 10⁻⁴ off on a rib whose far side tapers, radius 3). The
ball leaves the edge for the face beside it where it reaches into that face.
Where the edge ends and the edge after it turns away from the ball, the walk
holds the contact at the end and turns the ball about it until it can roll
on (`StartPivot`). Where a circle on the axis of a surface of revolution is
the edge, the contact lies in the plane through the axis (also where the
ball's centre is on the axis).

### 0171-chfi3d-rolling-ball-turns-about-a-vertex

Where a blend rolling on an edge reaches a vertex whose next edge runs away
from the ball, the next blend is the ball's turn about the vertex, on the
old edge (`StartSol`), and where the contact stayed at the vertex the same
edge goes on; a side reduced to a point becomes a degenerated edge
(`ChFi3d_FilDS`).

### 0172-chfi3d-rolled-blend-cut-by-the-face-at-the-end

0074 for a blend rolling on an edge: where its contact on its face leaves
the face into the face beyond the edge before the spine's end (a pocket
whose floor is a lens narrower than the radius between its wall and a post's
rim), the blend is walked on, still rolling on the edge, until the ball
reaches that face, and cut by it (`RolledCutToEnd`); the stripe ends in a
point on the edge. Before, the corner at the lens's tip was invalid.

## 0390 and 0391: the next edge and two edges

The patches 0390 and 0391 go on with 0170 to 0172 (mitcad#237): a ball
rolling on an edge that meets an edge turning towards it, and a ball
resting on the edges of both faces; `geometry/tests/test_fillet_rolling_ball.cpp`
checks them with OCCT alone.

### 0390-chfi3d-rolling-ball-touches-the-next-edge-before-the-vertex

Where the edge E1 the ball rolls on ends at a vertex whose next edge E2 of
the reference face turns towards the ball (a rib's far side that thickens
towards the ball, a thin rim's inner edge turning into a spoke), the ball
touches E2 before the vertex. The walk rolled on through E2 into the solid
up to the vertex, and the next blend started on E2 at the vertex (a rib,
radius 3: 2.6 · 10⁻³ off the ball). Now the walk stops where the ball
touches E2 (`NextEdgeGap`, `FindConcaveTurn`, with the next edges' curves
from `ChFi3d_NextEdges`), `StartSol` goes on rolling on E2 from the ball's
contact there (`ChFi3d_ConcaveTurnEdge`) with the same ball, and between the
two blends the rounding is that ball: pieces of its sphere whose side on the
other face is the shared contact (a degenerated edge, as 0171) and whose
other side runs where the sphere cuts the faces beyond E1 and E2, across
their common edge (`ChFi3d_ConcaveTurnWedge`). E1 and E2 end at the blends'
contacts. Where the edges' contacts lie more than a right angle round the
ball from the other contact (the sections rising over the ball's top), the
sphere cuts neither face between the sections and the stripe fails.

### 0391-brepblend-rstrst-rolling-ball-on-two-edges

Where both faces of the rounded edge are narrower than the ball's contacts
(a ridge whose roof faces end at sharp eaves), the ball rests on the far
edges of both. The overflow start refused it and the fillet failed or became
another rounding. `StartSolOverflow` now lets both sides roll where their
contacts lie beyond sharp convex edges, on closed contours only (the corners
where such a blend meets the faces at an open contour's ends are not made:
an open contour fails as before), and the blend between two edges
(`BRepBlend_RstRstConstRad`, `BRepBlend_CurvPointRadInv` at an edge's end)
is the ball tangent to both (`BallOnTwoCurves`: its centre in both edges'
normal planes at the radius from both; the guide only picks the ball, as
0075), its sections the ball's arcs; on a closed guide the walk's period
starts where it ended to the left, as for the other walks.

## 0500 to 0503: coaxial surfaces, flats and gaps

The patches 0500 to 0502 go on with 0170 to 0172 and 0390 to 0391
(mitcad#237): a rounding between a cylinder and a cone about one axis whose
contact passes below a ledge over flats cut into the cylinder, which in the
files is not a ball rolling on the ledge's edge but the torus of the
cylinder and the cone, cut round each flat; and a thin rim's outer edge
rounded wider than the rim, which in the files is the outer wall's rounding
cut by the rim's round end caps; `geometry/tests/test_fillet_rolling_ball.cpp`
checks all three with OCCT alone, and each check fails without its patch.
0503 (mitcad#378) mends the corners where 0500's torus meets the next
edge's analytic rounding; its check there fails without it.

### 0500-chfikpart-fillet-between-coaxial-surfaces-of-revolution

The constant-radius fillet between a cylinder and a cone, or two cones,
about one axis, along a circle about it, is an analytic blend, the torus its
ball sweeps (`ChFiKPart_MakeFilletCoaxial`, `ChFi3d_KParticular`). OCCT had
analytic blends only with a plane among the faces; this one was walked as a
B-spline, and where its contact passed over a cut-out of its face the walk
rolled on the cut-out's edge and failed. In the meridian half-plane the
ball's centre lies at the radius from both profiles; the section runs from
the contact on the first face to the one on the second with the torus's v
rising, as the other analytic fillets have it. The convex and concave
roundings of a post's conical top or flare and of two cones, whole and part
revolutions, agree with the walked ones within the walk's approximation (2 ·
10⁻⁵ of volume).

### 0501-chfi3d-kpart-cut-round-a-cut-out-under-an-overhanging-rim

An analytic blend whose contact passes below a ledge over a flat cut into
its face (a post's rim overhanging ten flats, rounded with 0.9 where it
meets the flare above: the contact 0.073 below the ledge) is cut round the
flat in turn: up the flat's plane, along the ledge (grown out to the cut)
and down the flat again (`ChFi3d_KPartCutChain` round from the flat back to
it, crossings on the prolongations of the edge between the flat and the
ledge beyond both its ends, 0142). Those crossings lie just beyond the gap
(0.01 along the spine): the pieces are kept within the gap, the ones between
its ends and the crossings of no length along the spine, their cut on the
flat their only side; the cut's arc of the ledge's circle is the one round
the piece, and a gap across a closed spine's start is split there, its
closing section the torus's seam. The same across the spine's edges (the
circle in two edges, its vertices at a flat's corner and mid-flat:
`ChFi3d_FillKPartRoundGapsAcrossEdges`, the pieces at the edges' ends given
the arcs SplitKPart leaves them without), and the torus is one face round
the whole circle: the edges' tori are parametrized alike and the pieces that
are one face (0073) join across the edges
(`ChFi3d_JoinKPartRunsAcrossEdges`). Before, the stripe failed ("the ball's
piece at a concave turn not made", 0390's rolling on the ledge's edge); now
the result is the post rounded without the flats, the flats cut from it
after, as the bodies stored in the files are (MitCad's dressup built that
with the flats cleared and cut again, mitcad#311).

### 0502-chfi3d-kpart-cut-by-the-end-face-over-a-gap

An analytic blend near an open spine's end whose contact runs off its face N
over a gap and comes back into N only where the face at the end already cuts
the blend (a thin rim between an outer wall and a pocket, its ends round
caps, the outer top edge rounded with 1.2, wider than the rim: the contact
on the top lies over the pocket and comes back into the top inside the cap)
is cut by the end face from where the other contact reaches the end face's
edge to where the blend, the gap's wall and the end face meet, and by the
gap's wall from there to the first piece whose contact comes back
(`ChFi3d_KPartCutToEndOverGap`, where 0059's cut at the end does not apply);
the end face grows up to its cut (its edge with N prolonged beyond the
spine's end vertex), a new edge bounds it and the gap's wall, and the pieces
are one face with the rest. Before, the stripe failed; now the result is the
outer wall rounded alone, the ring it loses taken off the rim outside the
caps, as the files' bodies are.

### 0503-chfi3d-two-roundings-meeting-on-an-edge-within-its-tolerance

Ticket: mitcad#378, a regression of 0500.

Two roundings meeting at a vertex of four sharp edges whose ends lie on the
same two points are joined along their intersection
(`PerformTwoCornerSameExt`); the many-edge corner took that path only where
the ends agreed within 1e-7, and now also where both ends are common points
on one edge within that edge's tolerance (`SameEndPoint`). A slot through a
round boss, its rim chamfered and the chamfer's lower edges rounded with 1:
at each corner the wall's rounding (a cylinder) meets 0500's torus round the
boss, and both end on the crease between the chamfer's plane and cone. On a
stored body that crease's curves on the two faces differ within its
tolerance (1e-4) and the ends lay 1.6e-7 apart: the corner was a plate of no
width whose edges reached tolerances of 0.4, and MitCad's check found a
self-intersecting wire once the vertex tolerances were cut down. Walked
roundings end further apart there, so 0500 exposed it; now the corner is the
two roundings' intersection, as in the files' bodies.

## 1230: rolling on the far edges of a face before an earlier blend

### 1230-chfi3d-rolls-before-a-blend-curving-away-as-fast-as-the-ball

Ticket: mitcad#1215, a follow-up of mitcad#1204.

A ring's top, 5 wide between an inner cylinder of radius 88.5 and an outer
rim that an earlier rounding of 2.5 took (its contact on the top at radius
90.978, 2.478 from the inner rim), with fins at eight mouths whose crests
meet the top there; the inner rim, a closed chain of 17 edges, is rounded
with 2.5. The ball's contact on the top would lie at radius 91.0, past the
top face; the face beyond its tangent far edge is the earlier blend, which
curves away from the ball as fast as the ball's own sphere, so the ball
touches it along a whole section and the walk that overflowed onto it
failed at once. The stored body after the rounding has the ball passing
through the top's far edge all round, the top face gone, the blend a torus
through that edge meeting the earlier blend there. At some mouths the far
edge has a sliver edge of 2 to 7 µm where a fin's crest meets the top, and
at one the inner cylinder's seam lies under the crest.

- `ChFi3d_Builder::Compute` computes a stripe whose walk or start failed
  again where a side reaches, within twice the radius, an edge of a face of
  its edges across which the face beyond continues it tangentially and
  curves away from the ball as fast as its sphere or faster (normal
  curvature across the edge at least 1/(1.01 R), `RollsBeforeCurvedAway`):
  that side rolls on the edge before that face, as `RollsInsteadOfOverflow`
  makes it before another stripe's blend, the overflow start rolling there
  although the faces are tangent and the obstacle chaining at a vertex
  going on along the next such edge; as before where that fails too.
- A sliver edge between two such edges (shorter than 100 tolerances,
  `PassesOverEdge`): the ball turns about the end of the edge it rolled on
  until it first touches the edge after the sliver
  (`BRepBlend_SurfRstLineBuilder::FindTouchOverSliver`), or rolls on from
  that edge's start where it reaches it already; the piece of the ball
  between (`ChFi3d_ConcaveTurnWedge`, as 0390's concave turn) runs along the
  sliver and the edge after it, so that the face rolled on goes all round.
- Where the other contact lies within a hair of an edge of its face across
  which that face goes on tangentially (a seam) at the end of the edge
  rolled on, it changes face there; the ball turns about the next edge's
  start, its other contact a hair outside its face at most.
- `ChFi3d_FilDS`: a closed stripe rolling on an edge at its start, where its
  last piece ends on the same edge, cuts that edge with the last piece's
  transition only; a piece whose contact on its first side is a point takes
  the transition of its second side.

On item 36's bodies of two versions of the design: valid, 343 and 358
faces, the faces of each body round the rim on the other's within 2 µm both
ways (OCCT's walked blends against the file's); each rounding takes 0.2 s.

## 1370: the turn over a sliver past a seam

### 1370-chfi3d-roll-over-a-sliver-past-the-seam-of-the-other-contacts-face

Ticket: mitcad#1370, a regression of mitcad#1215.

1230's case after mitcad#1261, which joins the faces on one surface after
each feature: the body before the rounding has the ring's top as one face,
the inner cylinder as one face closed round its axis, its seam under a
crest at one of the eight mouths, and the inner rim as one closed edge
whose vertex lies at that mouth. Before, the cylinder was cut into faces of
their own at that mouth, and 1230 moved the other contact onto the next
face at the edge between them. Now the ball reaches the end of the far edge
it rolled on with its contact on the cylinder 0.15 µm short of the seam,
and its turn about that end over the 6.5 µm sliver carries that contact
across the seam: the surface/edge walk found the contact outside its
face's parameters past the seam, refused each step, and the rounding
failed. Taking that contact past the seam, a second fault showed: the
walk of the turn gave two points, so `ChFi3d_Builder::ComputeData` walked
it again with a quarter of the step, and that walk kept the turn the first
one had found without solving its start again with the ball pinned at the
edge's end. It began from the start's free section, tens of nanometres off
the turn's, and each of its first steps was refused as too large.

- `PerformSetOfSurfOnElSpine`: 1230's change of face for the other contact
  takes the seam of a face closed round its periodic surface too (an edge
  used both ways in the face; two faces on one surface also share an edge
  with two curves on it, each using it once). The contact goes on in the
  same face on the seam's other curve, where the point a little ahead lies
  nearer that curve than the contact does; the piece before ends on the
  seam, its use on the side the contact leaves, with the walk's direction
  as its vector (`ChangeTransition` takes the next piece's start by it).
  The walk then starts there as on its face, with nothing to find on the
  face's boundary.
- `BRepBlend_SurfRstLineBuilder::Perform`: a turn about the restriction's
  end found by an earlier `Perform` of the same walk is found again from
  the start; one found by `PerformFirstSection` is kept as before.

On item 36's bodies of the two versions of the design, as the replay
builds them: valid, 244 and 261 faces; 1230's bodies as before.

## Reports and tests

- 0170 to 0179, 0390 to 0399 and 0500 to 0509: mitcad#237 (0503:
  mitcad#378) and `geometry/tests/test_fillet_rolling_ball.cpp`, which fails
  without them.
- 1230: mitcad#1215 and the cases `fillet-roll-over-blend` and
  `fillet-roll-over-blend-2` of `MITCAD_GEOMETRY_CASES` (`body.brep`, the
  reported design's body before the rounding in two versions;
  `after.brep`, the stored body after it) in
  `geometry/tests/test_fillet_roll_over_blend.cpp`: the inner rim rounded
  with OCCT alone, valid, of the expected volume and faces, within 2 µm of
  the stored body round the rim (failing without the patch).
- 1370: mitcad#1370 and the cases `fillet-roll-over-seam` and
  `fillet-roll-over-seam-2` of `MITCAD_GEOMETRY_CASES` (`body.brep`, the
  replay's body before the rounding, its faces on one surface joined, in
  the design's two versions; `after.brep`, the stored body after it) in
  the same test (failing without the patch).

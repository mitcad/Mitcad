# Fillets of stored designs: 0870–0911

The eleventh and twelfth rounds of mitcad#343: a closed chain whose contact
runs off a narrow band, a strip the ball cannot touch, a sliver at a vertex,
a disc's rim over a hole's mouth, a bar's rim and notch walls, a thread's
chamfer cone and a pin's foot. The earlier rounds are in
[0790 to 0822](fillet-replay-0790-0822.md), the later ones in
[0930 to 0942](fillet-replay-0930-0942.md). The radius law 0920 is in
[walks and contours](fillet-walks-and-contours.md#0920-chfi3d-radius-law-of-the-callers-along-a-contour).
Back to the [port's README](../README.md).

## Overview

- **0870** keeps a rounding's ball on the surface of a narrow face of its
  edge where the contact runs off that face across a tangent edge and comes
  back, the face beyond cutting the rounding, as the stored designs'
  roundings are (a closed chain round a part, mitcad#343).
- **0880** starts a rounding along an edge of a face the ball touches at no
  point (a strip of an earlier, smaller rounding, convex) across that face,
  the strip consumed (an endcap's notch corner; mitcad#343).
- **0890** ends a rounding at a vertex where a sliver lies between its faces
  and the face beyond the edge its contact left its face by as that face's
  OnSame end, the sliver consumed (a wall's foot rounded up to an earlier
  rounding of its radius past a step's end face; mitcad#343), and **0891**
  walks a closed rounding over a hole's rounded mouth where the piece over
  the gap runs on round the spine or the gap lies across the spine's start
  (a disc's rim; mitcad#343).
- **0900 to 0909** round the rim and the notch walls of a bar's end
  (mitcad#343):
  - 0900 names the face of a piece walked again past an OnSame end after
    the spine's edges;
  - 0901 ends a walked intersection with the face at the end where the walk
    first passes the end point;
  - 0902 ends a rounding that vanishes where three faces are tangent at its
    spine's end vertex;
  - 0903 lets a blend rolling on an edge go on where its other contact
    crosses onto the face of the spine's next edge;
  - 0904 rolls a blend on along the line or circle of the edge it rolls on
    past that edge's vertex, cut by the face at the end (or ends it in a
    point where the narrow face ends in the spine's vertex);
  - 0905 turns a ball rolling on an edge about that edge's vertex onto the
    next edge;
  - 0906 takes the next start found as an edge overflow where the first
    walk from one fails;
  - 0907 releases such a turn where a step lands on the point where it
    ends;
  - 0908 samples the spine more finely where no overflow start is found;
  - 0909 ends a blend rolling into the spine's vertex where its section lost
    its width a hair short of it.
- **0910** starts an OnSame end's cap at the opposite contact's crossing
  with the prolonged edge itself, not where the contact's approximated curve
  crosses the face at the end (a rounding along a thread's chamfer cone
  ending on the thread's root cylinder, mitcad#343).
- **0911** makes an end where the face at the end goes on tangentially from
  a face of the spine's edge, behind that edge, an OnSame end: the face at
  the end cuts the blend and grows up to it (a pin's foot rounded up to the
  slot whose sides are tangent to the pin, mitcad#343).

## 0870: a contact off a narrow face across its tangent edge

The patch 0870 comes from the eleventh round of mitcad#343: a closed chain
of 36 edges round a part rounded with 5, the face on one side of three of
its edges a cylinder band about 2.47 wide, tangent to a torus of radius 10
beyond it, the contact on the band 2.59 from the edge.

### 0870-chfi3d-contact-off-a-narrow-face-across-its-tangent-edge-keeps-its-surface

The rounding's contact on the band N leaves N across its tangent edge with
the torus C and comes back where N widens. The walk took C for N in between,
the ball rolling on over C, which curves away from N's surface: up to 0.18
deeper, 7.1 mm3 more taken away at each of the three places. The stored
designs keep the ball on N's surface run on (its centre 5 from N's cylinder
all along), and C cuts the rounding along their intersection, a crease of up
to 0.19 rad, the rest of N left as strips at the crossings.
`KeepFacesAcrossTangentEdges` (in `Compute` after `JoinAcrossCorners`,
stripes of constant radius) finds a stripe's run of pieces on N and Fo, on C
(across N's tangent edge where the contact crossed it) and on N and Fo
again, and walks it again from its first section to its last with N's
surface unbounded, along a copy of the stripe's elementary spine, or the
spine interpolated over the run where analytic pieces at its ends have none.
Where the walk reaches both sections, its contact leaves N once and comes
back, and the contact lies off C's surface by a tenth of the radius at most
(deeper overflows are left as they were), the run becomes three pieces on
the walked surface, one face: up to where the contact leaves N, the cut by
C, and from where it comes back. The cut is found section by section
(`ChFi3d_SectionCut`: C meets the blend tangentially at the crossings, where
an intersection of the surfaces leaves gaps), its curves interpolated by
chord length. The spine's edges generate the walked face (`Generated`). Only
runs whose pieces on C roll on it (the blend tangent to C along their
contact) are taken: a blend walked over a rounded hole's mouth and already
cut by its rim at a crease (patch 0160) is the same construction, exact, and
is left as it is (`geometry` test "rounding over a hole with a rounded
rim").

### 0871-chfi3d-stripe-end-curve-follows-a-run-walked-again

From mitcad#1858: a reported nut design whose closed chain of 38 edges,
rounded with 5, closed on itself along an edge of tolerance 4.79 mm.
`ChFi3d_MakeExtremities` makes a periodic stripe's end curve from its first
piece's first section, and its parameter curve on the last piece's surface
as a line between that piece's contacts at its end. 0870's
`KeepFacesAcrossTangentEdges` runs later in `Compute`; where a run ends at
the stripe's last piece (or starts at its first), the pieces are replaced
by three on a newly walked surface with parameters of its own (the
section's parameter running to 28.7 where it ran to 1.22), and the old line
on it lay off the curve by up to the section's length. Where the run holds
an end piece, `KeepFaceAcrossTangentEdge` now makes that end's parameter
curve again on the new piece's surface (`endPCurveOnPiece`), the line
between its contacts as `ChFi3d_MakeExtremities` makes it, and raises the
curve's tolerance to what it reached; the 3D curve stays, the run being
walked again between the same sections. Runs inside a stripe are
unchanged. Test: `geometry.port.0871`, a stadium-shaped block whose top
is a strip 2 wide in front of and behind a hole with a rounded rim, its
top's outer rim rounded with 3, the chain started from either side of a
run (before: tolerances of 4.33 and 174 mm, the second solid not valid).

## 0880: a start across a strip the ball cannot touch

The patch 0880 starts a rounding along an edge of a strip the ball cannot
touch (an endcap's notch corners, round 11 of mitcad#343).

### 0880-chfi3d-start-across-a-strip-the-ball-cannot-touch

A notch cut into a ring's outer cylinder left at one corner a strip 5 µm
wide of an earlier rounding of radius 0.4 between the notch and the
cylinder, and the strip's edge with the notch is rounded with 1. The strip
is convex and bends more sharply than the ball, so no ball inside the solid
touches it, and the section on the edge's faces has no solution even on
their surfaces. `StartSolOverflow` (0011, 0013) went on across a face the
ball touches at no point only after a side had already moved, and gave up
where the spine's own face is such a face: the rounding had no start ("no
ball of the radius touches both faces"). Now, at the spine, the side whose
face is convex there and bends towards the solid more sharply than the ball
(`ballTouchesNoPointOf`) goes on across the face's edge the section crosses,
within the rounding's size, as such a side does further on: the ball touches
the notch and the cylinder, the strip consumed, as at the corner without the
strip.

## 0890 and 0891: a sliver at the vertex and a bridge round a closed spine

The patches 0890 and 0891 come from the eleventh round of mitcad#343:
roundings whose contacts run off a narrow face (the edge overflow of
mitcad#121), one ending at a sliver, one closed round a disc's rim over a
hole's rounded mouth.

### 0890-chfi3d-onsame-end-across-a-sliver-at-the-vertex

A wall's foot rounded with the radius of an earlier rounding round the foot
of the wall's end, whose ball the new one's becomes where the contact on the
wall leaves the wall across their common edge (0540's case at one end of the
foot). At the other end a step's end face is left between the floor, the
wall and the earlier rounding (a triangle 0.16 by 0.02, the wall narrowing
to an edge 0.02 long), and the foot edge ends at that sliver, not at the
earlier rounding. The vertex's three edges are all concave (AllSame); the
corner found no face holding both contacts' ends and went round the vertex
(`PerformIntersectionAtEnd`), and the result was invalid.
`ChFi3d_SliverFaceAtEnd` recognises such an end: one contact left its face
across an edge not at the vertex (any edge of that face at the contact's end
where that end is a vertex) whose face beyond Fv closes the third face at
the vertex, a face smaller than the radius whose other edges all lie on Fv,
its edges with the blend's faces at the vertex running to a vertex of that
edge and to a vertex of an edge of Fv and the opposite face.
`PerformOneCorner` then takes the end as Fv's OnSame end: Fv is the face at
the end, that edge of Fv and the opposite face is prolonged from its vertex
at the sliver, the contact's point at a vertex is put on the edge left
across, and the sliver is consumed (its edges cut at their vertices, as for
a face consumed at an end along an edge).

### 0891-chfi3d-bridge-over-a-gap-round-a-closed-spine-and-across-its-start

A disc's bottom rim rounded with 1 over a narrow rounding of the rim, the
contact on the bottom face running over the mouth of a blind hole, rounded
with 0.5, which the ball cannot touch: 0160 walks the blend on over the gap
and cuts it by the mouth's rounding, in two pieces where the cut crosses
that rounding's seam. Three faults there. The piece over the gap was walked
on round the rest of the closed spine, and `ChFi3d_KPartLineParam` took the
section through the gap's start (its other contact line's own start) at the
projection's foot on the far side of the disc, the line's ends being no
feet: the seam's crossing was not found and the stripe failed. A bounded
line's end nearer than any foot is taken now. With the gap across the
spine's start, the intersection of the walked blend with the mouth's
rounding lost the cut's last stretch at the surface's end, where the contact
comes back (`ChFi3d_KPartCutCurve` intersects a walked blend extended by a
quarter of its length past both ends along the spine), and the stretch past
the start took the vertex where the cut crossed the seam, just before it,
for a vertex of the mouth's rounding under the blend (`ChFi3d_KPartCutPiece`
passes over a vertex whose nearest point on the contact's line is no foot).

## 0900 to 0909: a bar's rim and notch walls

The patches 0900 to 0906 come from the same round: the roundings of a
bar's end with radius 0.86, the rim chain round a band vanishing at a
vertex where three faces are tangent, and the walls of a notch 1 wide,
strips about 0.06 high between the notch's floor (a cone) and an earlier
rounding, whose balls touch the floor and roll on the walls' far edges.

### 0900-chfi3d-walked-onsame-piece-generated-by-the-spine-edges

The face of a piece walked again past an OnSame end
(`WalkedOnSameToEndFace`, 0600) is generated by the spine's edges: the
edges' map to their surfaces (`myEVIMap`) still named the old piece's
surface, so `Generated()` left the new face out and a caller naming the
result's faces after their edges found it unnamed.

### 0901-chfi3d-cap-intersection-ends-where-the-walk-first-passes-the-end

`ChFi3d_ComputeCurves` cuts the walked intersection of a blend with the face
at its end where the walk first passes each end point (a local minimum of
the distance no larger than the step), not at the points nearest to them: at
an OnDiff end where the blend touches that face, the walk passed onto the
other branch, wandered and came back, and the cap kept the wandering part
(the face's wire self-intersecting). The same blend walked the other way
round was valid.

### 0902-chfi3d-rounding-vanishing-where-its-contact-crosses-onto-a-tangent-face

A rounding vanishing at its spine's end vertex V where three faces are
tangent: its contact crossed onto a face tangent to the other face along an
edge into V a hundredth of the radius before V, and no ball fits between
those two. `ChFi3d_ContactOntoTangentFaceAtEnd` recognises such an end; the
walk ends the stripe there, `PerformFilletOnVertex` puts the end at V (the
blend ending in a point), and `ChFi3d_ComputeArete` returns without a
p-curve where its projection fails instead of using a null one.

### 0903-chfi3d-rolling-contact-crossing-onto-the-next-spine-face-goes-on

A blend rolling on an edge whose other contact crosses onto the face of the
spine's next edge goes on along the spine (`ChFi3d_ArcOntoSpineFaces`);
before, that face held the spine's end vertex and was taken for the face at
the end, the walk stopped with most of the spine unwalked, and
`ChFi3d_ExtendAtSpineEnds` extended the piece over the next one.

### 0904-chfi3d-rounding-rolled-on-past-one-end-along-a-line-or-circle

A blend rolling on an edge of a face narrower than its radius rolls on along
that edge's line or circle past the edge's vertex on the face at the end,
which cuts it (`ChFi3d_RolledToFaceAtEnd`, `RolledOnProlongedEdgeAtEnd`;
0342 took lines only and a piece without vertices at the spine's ends), at
one end or both; where the narrow face ends in the spine's vertex, its far
edge running into it, the blend ends in a point there
(`ChFi3d_RolledIntoEndVertex`).

### 0905-chfi3d-ball-turning-about-a-vertex-of-the-edges-it-rolls-on

The piece where the ball turns about the common vertex of two edges it rolls
on (one contact standing still, 0200) is walked back only over its turn (it
was completed forward over the piece before it), extended only at the end
its walk reached, and its side reduced to a point takes the transition of a
neighbouring piece on the same faces (its face came out reversed on its
wire).

### 0906-chfi3d-next-overflow-start-where-the-first-walk-fails

`StartSolOverflow` samples the spine from its start and takes the first
section it finds. On a notch wall narrowing to a point at the spine's first
vertex, that section (the ball touching the floor's side and the face beyond
the wall's far edge, an earlier rounding, with its contact inside it) made
the wall's side roll on that far edge; but the ball rolling on the edge and
touching that face touches its surface outside the face, so the walk failed
at its first section and the stripe with it. `PerformSetOfSurfOnElSpine` now
takes the next start past it (`StartSolOverflow`'s `theAfter`) when the
first walk from such a start finds no section there (`myStartSectionFailed`,
set by `ComputeData`); a walk that fails further on keeps its failure
(0821's lens is built after one). The same wall on the notch's other side,
its spine running the other way, built before.

### 0907-brepblend-pivot-released-where-a-step-lands-on-the-tangency

0170's ball turning about the end of the edge it rolls on is released where
its side of the next edge is within `Precision::Confusion()` of the release,
and the release point is compared with the walk's last point and the step
within `Precision::PConfusion()`. On the replayed body a step landed on the
release point (side -5e-14): it was not taken, the step past it solved the
release back at that point and the exact comparison rejected it, the steps
shrank to nothing, the piece ended on no edge and the stripe failed.

### 0908-chfi3d-overflow-start-sampled-finer-where-none-is-found

Where none of `StartSolOverflow`'s samples (eleven on a spine of one or two
edges) finds a start, the spine is sampled again between them at a quarter
of their spacing. A start of the far notch wall's rounding exists only along
about a millimetre of its 17.7 long spine; on the replayed body no sample
fell in it.

### 0909-chfi3d-rounding-vanishing-a-hair-short-of-the-vertex-it-rolls-into

A blend rolling on an edge that runs into the spine's end vertex
(`ChFi3d_RollsIntoVertex`) ends the stripe where the walk stopped with its
contacts within ten times the tolerance of each other and within a hundredth
of the radius of that vertex (`ContactsAtTangentEnd`), and
`ChFi3d_RolledIntoEndVertex` then takes that contact within a hundredth of
the radius, so that the end is put at the vertex (0904). On the replayed
body the contacts met 0.005 before the vertex and the next step, with no
edge to roll on, failed.

## 0910: an OnSame cap from the contact's point on the prolonged edge

The patch 0910 comes from mitcad#343 (round 11), a rounding of radius 0.5
along a thread's chamfer cone that ends where its contact on the cone runs
into the thread's root, an exact cylinder.

### 0910-chfi3d-onsame-cap-from-the-contacts-point-on-the-prolonged-edge

In an OnSame end whose faces are not tangent along the prolonged edge,
`IntersUpdateOnSame` takes the opposite contact's crossing with that edge's
curve on the face at the end (unbounded) where the point found by
intersecting the contact's 3D curve with the face's surface lies within the
tolerance of it, and `PerformOneCorner` makes the cap start there
(`ChFi3d_CapFromPoint`: the end poles of the cap's curve and of its curves
on the blend and on the face moved onto the point, by no more than ten times
the tolerance). The contact's approximated curve lay 2e-6 off the cone and
crossed the cylinder at a small angle, its point 1.3e-6 below the cone's
circle on the cylinder. The blend is tangent to the cone along its contact,
so the cap leaves the circle tangentially, and as it is nearly tangent to
the cylinder there it rises from it slowly: it crossed the circle 0.026 from
the vertex, a self-intersecting wire for the checker (the circle's tolerance
is 1e-7). On the thread swept a turn at a time, whose root was a B-spline,
the same end had passed only behind a cap whose 3D curve lay 1.4 off its
curves on the faces, with vertex tolerances of 2.

## 0911: an OnSame end where the face at the end goes on tangentially

The patch 0911 comes from mitcad#343 (round 12), the foot of a pin of
radius 4.5 standing out of a wall, rounded with 1 along its arc on the
wall, which ends where the wall ends in a slot whose sides are tangent to
the pin.

### 0911-chfi3d-onsame-end-where-the-face-at-the-end-goes-on-tangentially

`ChFi3d_EdgeState` decides an end's state from the sides of the faces at the
vertex's two other edges, sampled inside each edge; where an edge lies
between faces tangent to each other, those sides follow no rule. At the
pin's arc ends the slot's side goes on tangentially from the pin across
their common edge, and the wall's edge with the slot's side runs into the
vertex tangent to the arc (the wall a cusp there); the end came out OnDiff,
the walk past the wall's edge rolled on it into the vertex (a blend ending
in a point, 0770) or, with the end not taken as that edge's, ran round the
pin to its seam, and the corner walked round the vertex: the result was 0.14
mm off the file's or invalid. Where exactly one of the two edges lies
between tangent faces and leaves the vertex behind the spine's edge (on the
side away from the interior of the face the two edges share, whose corner
there is reflex, so that the blend's contact on that face does not meet it:
`ChFi3d_TangentEdgeBehindSpine`), the end is now OnSame: the face at the end
cuts the blend and grows up to it along the tangent edge prolonged
(`PerformOneCorner`'s OnSame end, whose tangent case seeks the contact's
crossing with that edge). Where such an edge leaves into that face (an
earlier rounding of another of its edges coming in at the vertex, its
contact line the tangent edge), the blend's contact runs onto the face at
the end before the vertex, and the state is decided as before (an OnSame end
failed in two such corners of another design, which build as before). On the
stored body before the rounding: the file's change of volume within 2.4e-4
mm³ and every new face on the file's within 1e-5 mm.

## Reports and tests

Each of these tests fails without its patch, except where noted.

- 0870: mitcad#343 and the block whose top, 2 wide in the middle and 8 at
  its ends, goes on tangentially into a rim rounded with 6, its front edge
  rounded with 3, in `geometry/tests/test_fillet_tangent_overflow.cpp`
  (valid, every point of the rounding's faces 3 from the ball's path on the
  front wall and the top's plane, 2.7e-15 off; 0.171 off without the
  patch).
- 0880: mitcad#343 and the ring's notch with a strip 5 µm wide at one
  corner, in `geometry/tests/test_fillet_strip_start.cpp` (the strip's edge
  with the notch rounded: valid, the strip gone, the rounding's face as
  large as the sharp corner's within 1e-6 and the material removed as there
  within 1e-4, both corners at once as each alone).
- 0890: mitcad#343 and the wall cut from a half-cylinder's rounded end whose
  foot is rounded, ending at a sliver the cut's end face leaves, in
  `geometry/tests/test_fillet_end_overflow.cpp` (valid, the sliver consumed,
  the volume the result without the sliver has, with the material beyond
  the cut, within 1e-7).
- 0891: mitcad#343 and the B-spline disc whose rim is rounded over a hole's
  rounded mouth, in `geometry/tests/test_fillet_end_overflow.cpp` (valid,
  the volume of the disc rounded without the hole less the hole within
  1e-7, with the gap away from the rim's vertex and across it on either
  side; it fails without the patch with the gap across the vertex: the
  first two of the patch's three changes are verified on the stored
  design's bodies only, as no synthetic body made the projection miss its
  line's end or the intersection lose its last stretch).
- 0900: mitcad#343 and the plate with an overhanging tongue whose left
  edge, rounded with 0.3, is rounded with 1 so that its last piece is walked
  again past its OnSame end, in `geometry/tests/test_fillet_corners.cpp`
  (every face of the result on a new surface is generated by the edge or
  its vertices).
- 0901 to 0909 are verified on the stored design's bodies only, see
  `core/import/docs/f3d-rounds-3.md`,
  [*A bar's rim and notch walls rounded with 0.86*](../../../core/import/docs/f3d-rounds-3.md#a-bars-rim-and-notch-walls-rounded-with-086-round-11-of-mitcad343).
- 0910 has no such test: its cap crosses the edge only through the contact
  curve's approximation lying on the wrong side of the face at the end by
  1.3e-6 mm, at a corner where the blend is nearly tangent to that face,
  verified on the design's replayed body; a bolt's thread cut by a chamfer
  cone and rounded there failed elsewhere first.
- 0911: mitcad#343 and a pin standing out of a wedge whose sides are
  tangent to it, its foot rounded along its arc on the wedge's top, in
  `geometry/tests/test_fillet_tangent_end_face.cpp` (valid, one torus, the
  material added the ring at the foot within the sides' planes within 1e-6;
  4.514 against 4.764 and invalid without the patch).

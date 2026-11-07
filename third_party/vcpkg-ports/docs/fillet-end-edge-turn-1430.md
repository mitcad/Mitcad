# A rounding rolled on a convex edge into the face at its end: 1430

mitcad#1389, the follow-up of mitcad#1349
([fillet-replay-1350-1352.md](fillet-replay-1350-1352.md)): a rounding
whose contact on its face leaves that face across a convex edge into the
face at the rounding's end, short of the end. Back to the
[port's README](../README.md).

## Overview

- **1430** rolls the ball on that edge where the walk over the face's
  surface past the edge does not close on the face at the end, and cuts
  the rolling blend by the face at the end from where its contact on the
  edge turns back (`ChFi3d_WalkedOnPastEndEdge`, `ChFi3d_RollsOnEndEdge`,
  `ChFi3d_RolledOnEndEdgeCutAtTurn`).

## The case

The bar of `5ef2dbd5` of mitcad#343 (item 94, radius 0.2), 14 wide: its
foot has a notch B for a tube (a concave cylinder of radius 13.5 about the
bar's axis), and at each side a narrow lip N (the plane y = -11.743, 0.34
wide) between the notch and the bar's side. The bar's underside is a plane
sloped 15 degrees whose edge with the side is rounded with 0.5 (R, a
cylinder, tangent to the underside along its other edge). The item rounds
each side's edges with 0.2 along their tangent chain; at its foot the
chain ends at the vertex V where N, R and B meet, its last edge the arc
between N and R. N's edge E with B (a line through V) is convex, 29.5
degrees; R's edge Eq with B runs from V too.

The rounding's contact on N reaches E (at z = 9.177) before the section
through V. The stored body after the item, matched face by face against
the item's input (the ball's centres of its faces sampled):

- the rounding between N and R ends at the section where its contact on N
  reaches E;
- from there the ball rolls on E, touching R (every centre 0.2 from E's
  line and 0.3 from R's axis within 5e-4 mm), its contact on E running
  down towards V to z = 9.1426 and back up; the face ends at the section
  of that turn T;
- past T the same rolling blend, cut by B from T's point on E down to the
  point Q (6.515, -11.824, 8.955) where its contact on R reaches Eq: a
  face ending in a point;
- the underside is untouched; E keeps its part above T, the wedge of B
  between E, Eq and the cut is gone, and V with it.

Three faces at each corner (0.125, 0.026 and 0.0068 mm²). At the chain's
other end (z = 19.95) the bar has the same three faces (a lip, a rounding
of 0.3, the notch), and there the stored body keeps the blend over the
lip's surface past its edge, cut by the notch: one face, as OCCT makes it.

## 1430: the ball rolled on the edge into the face at the end

### 1430-chfi3d-ball-rolled-on-a-convex-edge-into-the-end-face-cut-from-its-turn

`ChFi3d_ObstacleBeforeEnd` (0620) takes a contact leaving its face across
a convex edge before the spine's end for an obstacle the next piece rolls
on, but not an edge holding the end vertex nor one whose face beyond
holds it: there the walk goes on over the face's surface past the edge
and the face at the end cuts it. At the foot that walk's contact on R ran
past R's tangent edge onto the underside before reaching Eq, and the
corner cut that blend by B: two faces at each corner, the underside cut,
0.014 mm from the stored body (0.018 mm the other way). At the top the
walk's other contact reaches its face's edge with the notch: the walk
closes on the face at the end, and the stored body is that.

So the two ends differ by whether the walk past the edge closes on the
face at the end. `PerformSetOfSurf` now looks, after a stripe is walked,
at each end where a contact of a piece near the end reached a convex edge
E holding the end vertex V into a face B at the end
(`ChFi3d_ConvexEdgeIntoEndFace`) and the end piece's other contact ran on
from that piece's other face Fo onto another face, short of Fo's edge Eq
with B (`ChFi3d_WalkedOnPastEndEdge`). Such a stripe is walked again with
that end in `myRollEndEdgeSpines`:

- `ComputeData` takes the contact on E for an obstacle there
  (`ChFi3d_RollsOnEndEdge`: the same edge, more than 10 `tolesp` before
  the spine's end), so the walk ends on E;
- `StartSol` takes E for the edge the next piece rolls on, B its face (the
  arc holds the end vertex, which it otherwise excludes);
- the piece rolling on E (a surface/restriction blend) runs on until its
  other contact reaches Eq at Q.

`ChFi3d_RolledOnEndEdgeCutAtTurn` then finds the point T where that
piece's contact on E comes nearest V (its line sampled and refined; both
the piece's ends farther from V), and splits the piece there: the part
from the piece before up to T rolls on E (`ChFi3d_TrimWalkedPiece`, its
side on E from T); the part from T to Q is cut by B from T's point on E to
Q, where it ends in a point on Eq (`ChFi3d_NarrowCutPiece`), on a copy of
the blend's surface so that it is a face of its own. The piece's lines run
on past Q (the walk's approximation), so Q's parameter on the other
contact's line is the piece's end there. The stripe ends at Q with no
corner at V (a free boundary). Where any step fails, or no turn lies
inside the piece, the stripe is walked again as before.

On the design's body (both sides, 22 edges) the result is valid and has
the stored body's three faces at each corner; about the corners the result
and the stored body lie on each other within 0.0011 mm (areas within
0.0003 mm²); the underside's area is the stored one. With MitCad's dressup
the item gives the stored state's 26 new faces (24 before) and the bar's
final body all 229 of the stored body's faces.

## Tests

- 1430: mitcad#1389, in `geometry/tests/test_fillet_end_edge_turn.cpp`
  (`test_corners_roll_on_the_lip_edge`) on the case `notch-lip-corners` of
  `MITCAD_GEOMETRY_CASES`: the rounding of the 22 edges is closed and
  valid, has as many faces about each corner as the stored body, and the
  two lie on each other within 0.01 mm there. Without the patch it has two
  faces fewer about each corner and lies 0.014 mm (0.018 mm the other way)
  from the stored body. A synthetic body would need the notch beside a lip
  narrower than the ball and the underside's rounding going on into the
  underside; the design's body is the case.

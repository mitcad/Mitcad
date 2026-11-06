# A fin's side strips rolled on past their edges: 1420

mitcad#1388, the follow-up of mitcad#1349
([fillet-replay-1350-1352.md](fillet-replay-1350-1352.md)): a fin's side
strips, narrower than the ball, rounded against the fin's root chamfer on
the body MitCad builds, whose root chamfer is a spline a few micrometres off
the stored body's cone. Back to the [port's README](../README.md).

## Overview

- **1420** runs a ball that rolls on a strip's far edge on along that
  edge's circle past the edge's end, cut there by the transition piece of
  the chamfer beyond it, also where the walk went on rolling on that
  piece's edge or started beside the strip's point
  (`StartSolOverflow`, `RolledOnProlongedEdgeAtEnd`).

## The case

The fin of `5ef2dbd5` of mitcad#343 (item 90, radius 0.86), 1 thick: its
top's arcs are chamfered (cones of 45 degrees, mitcad#1341) and its root is
chamfered (a cone of 50 degrees in the stored body, a spline 0.003 mm off
it on MitCad's body). Between the two chamfers each side keeps a strip
(the plane x = ±0.5) about 0.05 high. Along each arc the strip's far edge
is its edge E with the arc's cone, a circle; at each end of it the far edge
goes on as the strip's edge with a transition piece W of the chamfer
(1350's piece C, tangent to the cone at E's vertex A) and narrows to the
strip's point V, the spine's end vertex. Beside the fin's end arc the
strip's other end is the fin's end face, which meets the cone at 45
degrees.

The stored body rounds the strip with a torus: the ball rolls on E and
touches the root's chamfer, past A on along E's circle, and W cuts it from
A down to the contact on the root's chamfer; W grows over the gap. Beside
the end face the ball rolls on the strip's edges with the end face (walked
pieces) down to the strip's point there. OCCT builds this (0904's
`RolledOnProlongedEdgeAtEnd`) where its walk, coming along E, fails past A
on W's edge, as on the file's stored state before the item.

## 1420: the strip's edge rolled on past its end

### 1420-chfi3d-strip-edge-rolled-on-past-its-end-cut-by-a-transition-piece

On MitCad's body the walk went otherwise, in either direction of the
spine:

- `StartSolOverflow` found its start beside the strip's point, rolling on
  W's edge, which runs into V: the walk from there rolled into V at once
  (a line stalling at the spine's end, whose approximation failed) or
  failed where its other contact crossed onto W. Such a start, a side
  rolling on an edge that runs into the spine's end vertex, is now taken
  only where no other start is found; the next sample, on E, starts it.
- Walking back from E, the walk went on past A rolling on W's edge into V.
  `RolledOnProlongedEdgeAtEnd` now also takes that end: the pieces there
  that roll on edges of a face W holding V, where W goes on tangentially
  from the face beyond E at A (outward normals within a tenth of a
  radian), or turn about A on E, are dropped, and the blend is rolled on
  along E's circle from the piece P before them, as where the walk failed
  past A; the dropped pieces generate no face. The end face, at 45 degrees
  to the cone, keeps its walked pieces.
- At the spine's first end the blend rolled on from P's other end back
  past the end was completed forward over P and the rest of the stripe:
  that walk is now not completed forward (`ChFi3d_SetStartsAtPieceBefore`),
  as a walk back from the start of a piece.

On MitCad's body, the eleven edges the stored body rounds (the strips
beside the middle arc on both sides, the strip beside the end arc on one
side, the rim round the fin's end) give the stored body's 152 faces,
valid; about the strips and the rim the result and the stored body lie on
each other within 0.003 mm, the body's own deviation there (the root's
spline). The strip beside the end arc alone builds with its edges in
either order: 149 faces, within 0.0006 mm. Before, that strip failed in
both orders, and so did the eleven edges. On the stored state before the
item the result is the same as before, 152 faces within 0.0007 mm.

MitCad's body is the one its dressup gives since mitcad#1388: the fin
top's chamfer (item 86) left the transition piece's edge with the strip
in two spline edges, which the dressup now joins (`join_at_turn_stops`
with `piece_vertices`, splines concatenated) as the stored body has it.
With the two edges, the end piece rolled on the short one and the corner
at the strip's point left the piece out.

## Tests

- 1420: mitcad#1388, in `geometry/tests/test_fillet_fin_end_strips.cpp` on
  the case `fin-end-strips` of `MITCAD_GEOMETRY_CASES`:
  `test_strips_rolled_on_past_their_edges` (the eleven edges on MitCad's
  body: the stored body's faces, valid, within 0.01 mm about the strips and
  the rim), `test_end_arc_strip_either_way` (the end arc's strip alone,
  both orders of its edges) and `test_stored_state` (the stored state
  before the item, as before). Without the patch the first two fail (one
  faulty contour each). A synthetic body would need the fin's spline
  profile with its chamfer's transition pieces; the design's body is the
  case.

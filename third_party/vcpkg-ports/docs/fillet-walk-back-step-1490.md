# A walk back to the spine's first end stepped by its own stretch: 1490

mitcad#1397, the follow-up of mitcad#1371
([fillet-three-face-corner-1380.md](fillet-three-face-corner-1380.md)):
roundings along a chain past the mitre where two earlier roundings of the
same radius meet, which built with the spine one way and failed the other.
Back to the [port's README](../README.md).

## Overview

- **1490** gives a walk back to the spine's first end the largest step of
  the stretch it walks, from its start to that end, as a walk forward to
  the last end gets (`ComputeData`); it got a fifth of the guide's
  extension before that end.

## The case

A block's top edges at a corner are rounded with 1, without the vertical
edge between them: their cylinders meet in a mitre, an arc of an ellipse
from the top of the vertical edge (to which it is tangent) up to the point
where both cylinders touch the top. Later the vertical edge is rounded with
1 too; its tangent chain takes the mitre. Over the mitre the ball does not
move (its centre lies on both cylinders' axes): its contacts run round the
cylinders' sections through that centre, and its section vanishes at the
top, where the corner is the ball's eighth. The vertical edge is an
analytic piece (a cylinder between two planes), the mitre a walked one.

The stored bodies have the ball's eighth at the corner. MitCad's rounding
failed ("the walk between the two faces fails") at one corner of the
reported design's block and built at the other, which differ only in the
way their vertical edges run: the spine follows the edge, from the bottom
up at one corner, from the top down at the other.

## 1490: the step of a walk back to the first end

### 1490-chfi3d-walk-back-to-the-spines-first-end-stepped-by-its-own-stretch

The mitre's piece is walked from the analytic piece's end at the top of the
vertical edge: forward to the top when the spine runs up, back to it (the
spine's first end, `intf`) when it runs down. `ComputeData` shortens the
largest step to a fifth of the stretch walked where that stretch is short,
the stretch taken as First to Last. For the forward walk those are its
start and the spine's last end (1.91 on the mitre's guide of 1.91, the step
0.096). For the walk back `FilBuilder::PerformSurf` puts First at the
spine's first end and leaves Last at the guide's extension before it, a
hundredth of the guide: the stretch came out as 0.019 and the step 0.0038.

The contacts leave the cylinders' edges tangentially (the mitre is tangent
to the vertical edge), so after a step of 0.0038 they lay on those edges
within the tolerance: the walker took the first step for one leaving the
faces, halved it, and stopped with a single section. Forward, the first
step of 0.096 put them 2.4e-4 into the faces and the walk went on.

Now a walk back to the spine's first end that does not start inside takes
its stretch from its start to First. The mitre's piece gets the forward
walk's step either way, and the corner is the ball's eighth: on the
reported design's body the material taken is 18.312845 mm³ against the
file's 18.3127, both corners alike. A third reported design's chain of the
same kind (a plate's rim rounded with 1 down a slot's corner past the
mitres of two earlier roundings of 1) builds too.

## Tests

- 1490: mitcad#1397, in `geometry/tests/test_fillet_mitre_same_radius.cpp`:
  `test_vertical_edge_through_the_mitre` (a box's top edges at each corner
  rounded with 1, then the vertical edge and the mitre with 1, each edge
  given first, each way round, on the box and on the box mirrored about
  its middle height so that its vertical edges run down: valid, the
  material taken the vertical edge's rounding and the mitre's corner less
  the ball's eighth within 1e-4). Without the patch three corners of the
  mirrored box fail.

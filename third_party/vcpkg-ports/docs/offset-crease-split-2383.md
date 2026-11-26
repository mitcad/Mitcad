<!-- SPDX-License-Identifier: MIT -->
# A crease split into several edges, trimmed once and oriented in each face: 2383

mitcad#2383, GitHub mitcad/Mitcad#642: a revolved cone with notches cut up
through its wall, shelled with Sharp corners. Back to the [port's
README](../README.md).

## Overview

- **2383** makes the offset of two faces that meet along a crease split
  into several edges (by notches cut through the wall along it and by a
  face's seam) come out right with the intersection join: the image of
  the crease, the faces' offsets intersected once for all its edges, is
  oriented in each face as its own edge is there, trimmed once, and cut
  where the gaps between its edges are. Before, Sharp failed ("the offset
  faces do not close up" in MitCad) at any thickness, or built a valid
  solid with a notch's walls missing; Rounded built.

## The case

The reporter's design: a cone (radius 52.7 at z = 8 to 23 at z = 80)
revolved on a ring (radius 59, z 0..8), with a step, a boss, a side boss,
and five notches 72 degrees apart cut up from a sketch on the ring's top
through the cone's wall (a round inner end of 9.5 and walls 13.4 degrees
off its tangent). Shelled 2 mm inside with the boss top and the foot
removed. The cone's foot, the crease between the cone and the ring's top
(the notches' floors are in that face), is split into five arcs by the
notches and once more by the cone's seam. A synthetic body of OCCT's
primitives (the cone, ring and boss, a box or the reported notch cut up
from the ring's top) gives the same faults; with the notch's floor 2 mm
above the ring's top (a face of its own) it built before.

`BRepOffset_Inter3d::ConnexIntByInt` intersects the offsets of the cone
and the ring's top once for all the arcs, in one circle, and its
post-treatment (each localized shared part) copies the circle for every
further run of connected arcs. `TrimEdges` and `TrimEdge` then trim each
copy to its run, a run across the circle's origin cut in two there (patch
2050). Five faults, each found by tracing the offset's loops on the
synthetic bodies and the reporter's (mitcad#2383's comments):

1. The copies went to the offsets of both faces with one orientation,
   though `Store` gives the circle each face's own: a copy ran against the
   wire of one of the faces.
2. `TrimEdge`'s second piece of a run cut at the origin went to the faces
   with the orientation of the edge where it was cut: in the cone's offset
   it ran against the wire, both foot arcs ending at the seam's vertex, and
   the cone's offset closed its foot loop alone (its seam and top left
   out); the offset shell fell in two (the box notch with the axes up).
3. `TrimEdges` trimmed a piece shared by the images of several edges (the
   arcs on both sides of the seam) again for each of them:
   `NCollection_IndexedMap::Add`, which it took for "is it new", gives the
   index. A second trim of a piece cut at the origin took the run the other
   way round the circle, across every other notch: the cone's inside ran
   across the notch beside the seam, a valid solid 3000 mm3 short on the
   reported design.
4. `IsOverOrigins` projected an arc's middle onto an original edge's curve
   stored only over its range: where the nearest point lies outside the
   range, the projection gives the farthest point, inside it, so the arc
   across a notch's gap counted as one over the arcs, no single run was
   found, and the circle was not trimmed (a notch of the pattern, not at
   the sketch's own place).
5. The image of the edge between a notch's round end and the cone comes in
   two branches meeting at an apex; `TrimEdge` trims a piece between the
   vertices its neighbours put on it, and a branch with one such vertex was
   not trimmed at all.

## 2383: the crease's image trimmed once and oriented in each face

### 2383-brepoffset-crease-split-in-several-edges-trimmed-once-and-oriented-in-each-face

- `BRepOffset_Inter3d::ConnexIntByInt`: each copy goes to each face with
  the orientation the circle has among that face's descendants.
- `TrimEdge`: the second piece of a run cut at the origin goes to each face
  with the orientation the edge has there.
- `TrimEdges`: each piece of an image in several pieces is trimmed once
  (`Contains` before `Add`); the vertices a piece shares with the other
  pieces of its chain count among its ends (`AddJointsOfPieces`).
- `IsOverOrigins`: an arc lies over an edge where a foot of the
  perpendicular from its middle inside the edge's range is a nearest point
  of the curve there (a local minimum of the distance), not a farthest one.

## Reports and tests

- `geometry.port.2383` (`test_box_notch_floor_in_the_ring_top`,
  `test_round_notches_one_and_five`), OCCT alone: one valid solid, the
  wall 2 mm behind each notch, five notches adding five times the wall one
  adds within 1e-5 (lab: 49807.6 mm3 without notches, 50456.9 with one,
  53054.3 with five).
- `geometry.shell_notched_cone`: the same bodies through MitCad's shell,
  without notes and with the inside of every face's wall.
- `cli.shell_sharp_reported`: the reported design's Shell, Inside 2 mm,
  Sharp, Tangent Chain on: one solid, the step's top 2 mm from its inside,
  every notch's round end with the inside of its wall (lab: 55678.2 mm3,
  Rounded 55359.1; 28276.6 at 1 mm).
- OCCT lab case `2383-cone-notch-on-ring-top`: the chain without the patch
  gives invalid solids for the box notch with the axes up and for five
  notches; the shared regression set (42 cases) shows no other change.
- Not covered by this patch (Sharp still fails, mitcad#2383's report): the
  same report's second design (a cylinder whose arm's sides are tangent to
  it, the arm's faces' offsets left untrimmed) and GitHub #645's loft cut
  to a slot and rounded as wide as the wall; see
  [patch 2415](offset-seam-and-intersection-chain-2415.md).

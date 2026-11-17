# A ride onto the corner at the other stripe's other end: 1727

mitcad#1926, GitHub #371's Fillet1: at the fin's inner arm ends the top's
rounding of 0.5 and the inner foot's rounding failed at their common end
vertex. Back to the [port's README](../README.md).

## The case

The design's body before Fillet1 (`fillet-wall-run-on` of
`MITCAD_GEOMETRY_CASES`, as in [1726](fillet-under-ball-1726.md)): a
U-shaped fin on a plate, its arms 2 wide, their tops sloping down into the
plate (z = -547) at the arms' ends; inside the U a pit whose edge with the
plate is rounded with 5, its sides' edges with the arms' inner walls with
1, and the inner walls' feet with 1 (one arm) and 0.8 (the other). The
top's edges are rounded with 0.5 (set 2).

At an inner arm end the top, the arm's inner wall and the plate meet at a
vertex V. The top's rounding of 0.5 (B, between the top and the wall) and
the foot's (C, between the wall and the plate) both end there. The wall
narrows to nothing before V: B's contact on the wall crosses, at Q (y
433.17), the contact of the corner patch K that OCCT's corner of three
blends makes where C meets the pit's roundings (a torus), and C's own
contact on the wall begins only beyond. OCCT's corner of two stripes at V
found no intersection of the two blends' curves on the wall and failed
("TwoCorner : No intersection pc pc"); so did Fillet1's whole rounding at
both inner arm ends.

The file's stored body has the shape: from Q the ball of 0.5 rolls on K's
torus, then on C's rounding, down to the plate, where the plate cuts it;
the top's contact runs on past the wall's line down to the plate (the top
grown past the wall's edge); C ends where the ball comes down to C's
contact with the plate (y 435.28).

## 1727: the ride over the corner patch and the other stripe

### 1727-chfi3d-ride-onto-the-corner-at-the-other-stripes-other-end

- `ChFi3d_Builder::PerformRideOnCornerAtEnd`, tried before
  `PerformTwoCorner` for a corner of two stripes (B constant, C of one
  SurfData whose other end is a corner patch K, a stripe with no spine
  whose end curve is C's there): B's contact on the face between crosses
  K's once, inside both, at Q.
- `RideOnCornerWalk`: B's ball walked along a guide built from B's spine
  (run on straight past its end) between B's other face, unbounded, and a
  support face made of a SurfData's surface. First on K's surface (bounded
  where K meets C) from Q's section to J, then on C's surface (bounded at
  C's contact with the plate) from J until its contact there reaches that
  contact (P_foot, where the walk stops on the support's bound).
- The data structure: B's SurfData is cut at Q's section and followed by
  the two walked pieces, whose side on the support lies on no face; K is
  split at Q's section and its piece towards C holds B's first line on
  K's surface; C holds B's second line from J to P_foot, its contact with
  the plate trimmed at P_foot, and ends in that point; the curve K and C
  share is trimmed at J.
- B's end: its last piece cut by the plate section by section
  (`ChFi3d_CutBySections`, now shared between the two files: the cut is
  tangent to the plate at P_foot), from P_top, where B's contact on the
  top meets the plate, to P_foot.
- The top grows past the wall's line: the top's edge with the plate is
  prolonged from V to P_top on both faces; the top's edge with the wall
  and the wall's edge with the plate are cut at V (they go).
- The stripes' points at Q, J and P_foot are made one once they are in the
  data structure (as a part ride's ends, 1721); B is put in after K and C
  (`myCornerRiders`); a line K or C holds against its own way is turned
  (`turnRidesAgainst`, `myCornerRides`). B's walked pieces' orientations
  are turned: the data structure orients their curves from their side on
  the top, their faces by the orientation found against the support's
  face, the opposite of the one their curves so need.

Results on the design's body:

- Both inner arm ends (`fillet1 body e12 e8 e9 e10 e42 2` in the ticket's
  scratch driver: the pit's edge, its sides' edges, the two inner feet and
  set 2) build together, valid, volume 6052.5531 mm3.
- C's end on the plate at y 435.283 on both arms, where the stored body
  has the foot's end (435.28). P_top lies 0.557 past the wall's plane on
  the arm with the foot of 1 (x -287.374; the stored body has 0.544 on
  the other arm); on the arm with the foot of 0.8 x -275.819, where the
  stored body has -275.975: there OCCT's K runs from radius 1 to 0.8, the
  stored body keeps the pit side's torus of 1.
- Fillet1's whole rounding (the twelve sets with sets 5, 6 and 2) now
  fails at two vertices instead of four: the outer arm ends ("an edge met
  twice", left for 1728).

## What is left

- The outer arm ends (`fillet1 body r3 2`): `PerformMoreThreeCorner`, "an
  edge met twice in the parsing of edges" (1711's parse), next.
- One inner arm with set 2 but without its foot's rounding beside the pit
  (`fillet1 body e12 e8 e9 2`, the other arm's foot not rounded): that
  other arm's end, made by OCCT's corner of one stripe, comes out invalid
  (the plate's wire not closed there) where before the corner at the
  rounded arm failed first. Fillet1 rounds both feet.
- The arm with the foot of 0.8 against the stored body (K's radius, see
  above).

## Reports and tests

- `geometry.fillet_top_chains`: both inner arm ends build together,
  valid, with B's ends on the plate at the foot's end and on the top's
  prolonged edge with the plate.
- OCCT's own blend and chamfer groups (DRAW, 754 cases, against the port
  with 1726): no case changed.
- valgrind: no errors on the inner arm ends.

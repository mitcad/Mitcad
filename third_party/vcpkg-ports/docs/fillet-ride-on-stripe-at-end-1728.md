# A ride on the other stripe at an end: 1728, 1729

mitcad#1926, GitHub #371's Fillet1: at the fin's outer arm ends the top's
rounding of 0.5 and the outer foot's rounding of 3 failed at their common
end vertex. Back to the [port's README](../README.md).

## The case

The design's body before Fillet1 (`fillet-wall-run-on` of
`MITCAD_GEOMETRY_CASES`, as in [1727](fillet-ride-onto-corner-1727.md)): a
U-shaped fin on a plate, its arms 2 wide, their tops sloping down into the
plate (z = -547) at the arms' ends. The fin's outer foot, a chain of eight
edges from one arm's end round the fin to the other's, is rounded with 3;
the top's edges with 0.5 (set 2).

At an outer arm end the top, the arm's outer wall and the plate meet at a
vertex V. The foot's rounding (A, between the wall and the plate) and the
top's (B, between the top and the wall) both end there. The arm's last
stretch of outer wall is flat and lower than 3: A runs on over its plane
and the top cuts it, ending in a point on the top's edge with the plate
run on 3 past V, the top growing out to it (`WalkedCutToEnd`,
[1700](fillet-wall-run-on-1700.md)). B's contact on the wall crosses A's
on the curved wall before the flat stretch, at W (y 429.485). The corner
of the two stripes at V went to `PerformMoreThreeCorner`, which met an
edge twice in its parse and failed ([1711](../README.md#index)).

The file's stored body has the shape: the wall ends in a point at W; from
W the ball of 0.5 rolls on between the top and A's rounding, down to the
plate, where the plate cuts it; A ends where the ball comes down to A's
contact with the plate (y 435.283); the top's edge with the plate runs on
past V to where B's contact on the top meets the plate.

## 1728: the ride over the other stripe's blend

### 1728-chfi3d-ride-on-the-other-stripe-at-an-end-where-the-face-between-narrows-to-nothing

- `ChFi3d_Builder::PerformRideOnStripeAtEnd`, tried at a corner of two
  stripes that is not regular (an end in a point) before
  `PerformMoreThreeCorner`'s last chance: B the smaller constant rounding,
  A a constant one ending at V in a point short of it (a free boundary),
  B ending at V with a width.
- W: where B's contact on a face F crosses A's contact on F once, inside
  both, in the SurfDatas within three of V (both on the curved wall here,
  the second SurfData of each).
- B's ball walked with `RideOnCornerWalk` (1727) along a guide built from
  B's spine, between B's other face (the top, unbounded) and A's blend as
  a support (1727's `cornerRideSupport`), SurfData by SurfData of A's from
  W towards V: each up to its end there (J, y 430.333, where A's run-on
  piece begins), the last until its contact comes down to A's contact with
  the plate (P_foot, the support bounded at that contact; the furthest
  section reached, by bisection).
- The data structure: A's SurfData through W is split there; its piece
  towards V and the SurfDatas after it hold B's lines on their side under
  B (that side on no face); A ends in the point P_foot, its contact with
  the plate trimmed there. B's SurfDatas between W and V go (their faces
  taken out of the edges' generated faces); its walked pieces follow its
  SurfData up to W, the last cut by the plate section by section
  (`ChFi3d_CutBySections`) from P_top, where B's contact on the top meets
  the plate, to P_foot.
- The top's edge with the plate, run on past V to A's old end point by
  `WalkedCutToEnd`, is run on to P_top instead (the old prolongation's
  curve taken off both faces); the top's edge with the flat wall and the
  wall's edge with the plate are cut at V (they go, as 1700 left them).
- As 1727's: the points at W, J and P_foot are made one once they are in
  the data structure (a part ride's ends); B goes in after A
  (`myCornerRiders`), a line held against a stripe's way is turned
  (`myCornerRides`), and B's walked pieces' orientations are turned.

Results on the design's body (`fillet1` in the ticket's scratch driver):

- Both outer arm ends (the foot's 3 and set 2): built, valid. W at
  (-289.869, 429.485, -544.153) (stored body: -289.870, 429.481, -544.150);
  J at (-289.971, 430.333, -544.560) (stored: -289.971, 430.330, -544.558);
  B's contact on the top at J's section within 0.003 of the stored body's.
- P_foot at (-292.931, 435.283) and P_top at x -292.249; the stored body
  has (-292.463, 435.283) and -291.790. A's end piece there is the ball
  rolling on the plate and the flat wall's plane (its contact with the
  plate at x -292.931 all along, 1700's); the stored body's piece turns in
  by 0.47 towards its end (not a ball of 3 on that plane), so OCCT's end
  holds about 0.5 mm3 more on each arm.
- The whole of Fillet1 (the twelve sets with sets 5, 6 and 2) now builds,
  valid, volume 6089.428 mm3; the stored body has 6092.117 (2.7 mm3 more:
  0.8-0.9 mm3 at each of the four corners of the tower's slots where the
  plate's roundings of 2 meet, the outer arm ends above, and 0.17 mm3 at
  the inner end of the arm with the foot of 0.8, see 1727).

### 1729-chfi3d-end-inside-the-stretch-another-blend-takes-away-fails

With one inner arm's foot rounded and the other's not, or the pit's edge
with set 2 alone (`e12 2` in the scratch driver), the arm end whose foot
is not rounded came out done but not valid (the plate's wire open there):
the top's 0.5 ends at that vertex with a corner of one stripe, its contact
on the wall meeting the wall's edge with the plate at y 434.417, inside the
stretch of that edge the pit's rounding of 5 takes away (its contact with
the plate meets the edge at y 434.98, from the pit's corner at 432.909).

- `PerformFilletOnVertex`, a regular end of one stripe: where one of its
  contacts meets an edge at the vertex on an arc, and another stripe
  ending at that edge's other vertex meets the same edge farther from that
  vertex with its contact on the edge's other face, the end fails there
  (`endInsideAnothersCut`), before any corner is made. Contacts on one
  face meeting the edge from either end (two roundings mitred at a short
  edge, 0210's `geometry.suite.fillet_corners` case) are left alone. The other stripe's end fails the same way (the pit's rounding's
  contact lies inside the 0.5's stretch). With both inner feet rounded the
  vertex is a corner of two stripes (1727), not checked.

Results: the single-arm cases and `e12 2` fail at those vertices (a
corner fault, "the end of a fillet meets an edge where another fillet has
taken it away"); every case above builds as before.

## What is left

- The tower slots' corners and the outer arm ends' last piece against the
  stored body (above).
- The single-arm case itself built: the 0.5 would have to ride on over the
  pit's rounding's end (as 1727 rides over the corner patch).

## Reports and tests

- `geometry.fillet_top_chains`: both outer arm ends (the foot's rounding
  and the top's edges) build, valid, with B's ends where the walk puts
  them; the whole of Fillet1 builds, valid, its volume within 3 mm3 of the
  stored body's; one inner arm's foot rounded, the other's not: never done
  and not valid (1729).
- OCCT's own blend and chamfer groups (DRAW, 754 cases, against the port
  with 1727): no case changed (670 OK, 27 BAD, 10 FAILED, 47 skipped
  without their data), with 1728 and with 1729.
- `geometry.suite.fillet_corners`: the two roundings mitred at a short
  edge (0210) still build (they are why 1729 looks at contacts on the
  edge's two faces only).
- valgrind: no errors on the outer arm ends and on the single-arm case.

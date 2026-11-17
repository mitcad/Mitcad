# A rounding riding tangent on an earlier rounding's blend: 1720

mitcad#1926, after [fillet-top-chains-1710.md](fillet-top-chains-1710.md)
("The whole rounding: what is left"): of two roundings of one fillet whose
blends overlap on a face narrower than both, the smaller ball rolls tangent
on the larger rounding's blend, which its contact there cuts, and the face
between them vanishes. Back to the [port's README](../README.md).

## Overview

- **1720** gives OCCT's fillet a ride on another blend's surface: where two
  overlapping blends cannot be one, the later stripe is walked between its
  other face and the earlier stripe's blend surface, for roundings of
  either kind; the earlier blend is cut by the later one's contact on it,
  and the corners at the faces at the ends cut both between their points
  on arcs and the point where the two meet.

## The case

The shape two roundings make one after the other, as the bodies the `.f3d`
files store have it for roundings of one fillet (GitHub #371's design, its
Fillet1, sets 2, 5 and 6): the later (smaller) ball touches its own other
face and the earlier rounding's blend, tangent to it, below the face
between the two edges, which goes. Synthetic prisms (sections in the
(y, z) plane, 20 long along x):

- a wall w wide (2 < w < 3) and 10 high, its top's edges rounded with 2
  (y = 0) and 1 (y = w): both convex; the ball of 1 lies inside the circle
  of the rounding of 2, touching it and the side y = w;
- a plate 5 thick behind a tower standing w from its back edge, the
  tower's foot rounded with 2 (concave) and the plate's back edge with 1
  (convex): the ball of 1, inside the plate, touches its back face and,
  from outside, the circle of the rounding of 2;
- a step h high (h < 1.5) on a plate, its foot rounded with 1 (concave) and
  its top's edge with 0.5 (convex): the ball of 0.5 touches the step's top
  and, from outside, the circle of the rounding of 1.

OCCT's fillet computes the two stripes overflowing the face between (0013);
no ball touches the faces beyond, and that fails. For two roundings of one
kind it then rolls the later one on the earlier one's contact line on the
face (0016): another shape (on the wall 2.5 wide, 4.49 mm3 more material
than the section of the two arcs; at the design's tower legs the contact
on the outer side lies about 1.2 higher than the stored one). Of the other
kind the fillet failed.

## 1720: the ride on the earlier blend

### 1720-chfi3d-rounding-rides-tangent-on-an-earlier-roundings-blend

- `ChFi3d_Builder::RideOnBlend`, from `Compute` for a pair whose overflow
  failed, before `RideOnOverlap`: the larger rounding is the earlier one
  (A), the other the rider (B). Only constant roundings, of the other kind
  or B the smaller of one kind (B's ball lies inside A's blend's circle),
  both open contours whose end vertices hold no other stripe.
- A is computed on the faces of its edge. A face is made of its blend's
  surface over its contacts' parameters, run on a quarter past its ends
  along its contact (within the surface's bounds), oriented as the face
  between where the blend touches it: the support.
- B is computed between its other face and the support
  (`StartSolOverflow`: its first section from the one on the faces of its
  edge, the contact on the face between projected onto the support as the
  start on it; no analytic blend, as for 0016's riders).
- Each end of A's contact on the face between lies on an arc of it; B's
  contact crosses the other face of that arc (the face at the end) there.
  B's side on the support lies on the face between, its curve projected
  there (as a rider of 0016); A's contact on that face becomes B's contact
  on A's blend, its p-curve on A's surface B's on the support; both
  stripes' points at those ends are mere points on that curve.
- `PerformOneCorner`: at an end of either stripe of such a ride whose point
  on the face between is a mere point, the face at the end is the other
  face of the arc of the other side's point (not an OnSame end); the cap is
  the blend's intersection with that face between the point on the arc and
  the mere point (projected on the face), oriented from that arc.
- `ApplyRides` then makes the two blends meet on B's contact (each side on
  its own surface, on no face), cuts the face between's other edges at their
  vertices and makes the stripes' points there one, as for 0016's rides.
- Where the two edges' spines run opposite ways round the face between, A
  holds B's contact reversed (a curve of its own) and B rides against it,
  as 0016's riders against a line do (`turnRidesAgainst`).
- `ChFi3d_Builder` keeps the riders (`myTangentRides`, `IsTangentRide`,
  `IsTangentRideOn`) beside `RideOf`'s lists.

Result: the three prisms build, valid, their volumes those of the sections
of the two arcs within a millionth, whichever order the edges are added in;
so does a step's riser rounded 0.3 at its top and foot (0.5 high, the two
of one radius, of the other kind), which OCCT's fillet failed before
(`geometry.suite.dressup`).

## What is left

[1721](fillet-part-ride-1721.md) makes the rides along a part of the
edges from an end to one crossing of the contacts (its *What is left* for
the rest), [1722](fillet-ride-end-corners-1722.md) the rides ending at a
corner with one more rounding. The text below is as 1720 left it.

Rides that end at a corner of more blends (an end vertex of either stripe
holding another stripe), rides along a part of the edges only (the face
between narrower than the two roundings along a part of its length, its
ends inside the face) are not made so: those pairs are computed as before (0016, or a failure). In
Fillet1 (GitHub #371's design) the three sets that fail are, as the
analysis of the design's body in
[fillet-top-chains-1710.md](fillet-top-chains-1710.md) describes them,
such cases (1720 has not been run on that design yet):

- set 2, the tower's top outer edge with 1 beside its top inner edge with
  2: the ride runs the whole length of the top, but its ends lie at the
  legs' fronts, where the rounding of 2 meets other roundings (a mitred end,
  `Corner_EndMitred`, and the legs' feet);
- set 5, the plate's back edge with 1 riding on the tower's foot rounded
  with 2: the plate's back edge runs past the tower, so the ride covers a
  part of it only, ending where the tower's foot turns round the tower's
  sides (a part ride with corners at its ends);
- set 6, the fin's top's 0.5 at the arms' ends on the inner sides' feet
  rounded with 1: short rides ending at corners of three blends
  (`Corner_Two`, `OneCorner` with the foot's rounding of 3).

These need the ride's ends at corners of more stripes (the corner's
section through the meeting point of the two blends) and part rides (the
earlier blend split where the ride ends, as 0016's part rides do).

## Reports and tests

- `geometry.fillet_tangent_ride`
  (`geometry/tests/test_fillet_tangent_ride.cpp`): the three prisms above
  (the wall 2.9, 2.5 and 2.127 wide; the tower 2.9, 2.5 and 2.2 from the
  plate's back edge; the step 1.45 and 1.3 high), each with the edges added
  in both orders: built, valid, the volume that of the section within a
  millionth. Without 1720 the wall's result has more material and the
  other two fail.
- OCCT's own blend and chamfer groups (the private suite's `run.sh`, a DRAW
  build of the port with and without 1720): 754 cases each, compared case
  by case.

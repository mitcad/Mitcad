# A tangent ride along a part of the edges: 1721

mitcad#1926, after [fillet-tangent-ride-1720.md](fillet-tangent-ride-1720.md)
("What is left"): of two roundings whose contacts on the face between their
edges cross once, the face narrower than the two roundings from the
crossing to an end only, the smaller ball rolls tangent on the larger
rounding's blend from that end to the crossing and runs on the face beyond
it. Back to the [port's README](../README.md).

## Overview

- **1721** gives OCCT's fillet 1720's ride along a part of the edges: the
  rider is the ride from the end to the crossing and its own rounding on the
  face beyond, the larger blend is split at the crossing, its piece of the
  ride cut by the rider's contact on it, and the face between keeps its part
  beyond the crossing, which ends in a point there.

## The case

Synthetic prisms and a step (`geometry.fillet_tangent_ride`):

- a wall 10 high whose top narrows from 3.5 to 2.5 along its 20, its top's
  edges rounded with 2 and 1, either edge slanting, narrow at either end;
- a step on a plate whose top falls from 2 to 1.3, its foot rounded with 1
  (concave) and its top edge with 0.5.

The face between is narrower than the two roundings over a part of its
length only. OCCT computed the two roundings on the faces of their edges:
their contacts on the face cross, and the blends ran through each other
past the crossing (the wall's result with 0.45 mm3 more material than the
shape the two roundings make one after the other, valid by the check).

## 1721: the ride from an end to the crossing

### 1721-chfi3d-rounding-rides-tangent-on-an-earlier-roundings-blend-part-of-the-way

- `ChFi3d_Builder::RideOnBlendPart`, from `Compute` once the stripes are
  computed (before the corners), for each pair of constant roundings of one
  SurfData each, sharing a face, of the kinds 1720 takes (the rider of the
  other kind, or the smaller of one kind), open contours whose ends meet no
  other stripe, whose contacts on the face cross once inside both.
- The ride's end: where the rider's contact lies nearer the larger
  rounding's edge than that rounding's contact. The larger contact there
  lies on an arc of the face; the face beyond it is the face at the end.
- The rider as computed is split at the crossing (`splitSurfDataAt`) and its
  piece beyond the ride kept.
- The support is the larger blend's surface as a face (`rideSupport`), run
  on a quarter along its contact and its whole width across it, so that
  the rider, walked between its other face and the support over its whole
  spine as 1720 does, rides on past the crossing. Its piece through the
  crossing is split there and the ride's piece kept; its points at the
  crossing are the other piece's. Its contact on the support ends where it
  crosses the face at the end and goes onto the face between with its
  curve projected there, as 1720's.
- The larger stripe is split at the crossing; its piece of the ride holds
  the rider's contact (reversed, a curve of its own, where the rider runs
  against it). The face between's edge at the ride's end, between the two
  edges, is cut at its vertices.
- `ApplyRides` then makes the two blends meet on that contact as 0016's
  part rides (the larger stripe's piece is not its first or last), and the
  points at the crossing are made one.
- `StartSolOverflow`: for a tangent ride, the contact on the face projected
  onto a periodic support is put in the period of the support's bounds (a
  support run on past the blend's contacts held it one period off, and no
  start was found).

Result: the five bodies build, valid, their volumes those of the two
roundings made one after the other within 5e-8, in both orders the edges
are added.

## What is left

[1722](fillet-ride-end-corners-1722.md) makes the rides ending at a corner
with one more rounding of another radius (its *What is left* for the
corners of three blends). The text below is as 1721 left it.

- Rides whose ends meet other stripes (an end vertex of either stripe
  holding another stripe: the corners of more blends of Fillet1's sets 2,
  5 and 6 in GitHub #371's design). With 1720's check relaxed for them the
  rides compute, but the corners fail: `PerformTwoCorner` expects the
  larger stripe's side on the face between to end on an arc there (the
  pivot), which the ride turns into a mere point, and goes to
  `PerformMoreThreeCorner`, whose result the blends' intersection check
  then refuses. Such a corner needs a construction of its own: the larger
  blend's end cut by the other stripe up to the face at the end and by
  that face from there to the ride's point. Synthetic cases: a wall 2.5
  wide (top edges 2 and 1) with an end's vertical edge rounded too.
- A face between narrower than the two roundings in its middle only (two
  crossings; the rider then needs two rides), and pairs whose blends OCCT
  already finds overlapping on the face before (a tower's foot rounded with
  2 beside a plate's back edge rounded with 1, the tower's back face
  running from 2.2 to 3.2 from that edge: made so there, the larger
  blend's wire at the ride's end was left open, so the pair is not made
  so; OCCT fails it as overlapping, as before).

## Reports and tests

- `geometry.fillet_tangent_ride`
  (`geometry/tests/test_fillet_tangent_ride.cpp`): the four narrowing walls
  and the falling step, each in both orders: built, valid, the volume that
  of the two roundings made one after the other (the larger first) within a
  millionth.
- OCCT's own blend and chamfer groups (DRAW): see the ticket for the run
  with 1721.

# A tangent ride ending at a corner with one more rounding: 1722

mitcad#1926, after [fillet-tangent-ride-1720.md](fillet-tangent-ride-1720.md)
and [fillet-part-ride-1721.md](fillet-part-ride-1721.md) ("What is left"):
a tangent ride whose end vertex holds one more rounding, and a ride all
along between two roundings that each fit on the face between alone. Back
to the [port's README](../README.md).

## Overview

- **1722** lets a tangent ride end at a vertex where one more rounding
  meets the ride's stripe: that rounding is cut by the ride's blend, the
  ride's blend by it and by the face at the end, as OCCT's bevel of a big
  and a small blend whose pivot points differ. It also finds rides all
  along where neither rounding overflows the face between (their contacts
  pass each other on it without crossing), and puts the ride's contact on
  a periodic blend surface in the period of that blend's own p-curves.

## The cases

- Synthetic (`geometry.fillet_tangent_ride`): a wall 2.5 wide and 10 high,
  its top's edges rounded with 2 (y = 0) and 1 (y = 2.5), with vertical
  edges at its ends rounded too: with 0.5 or 1 beside the rounding of 2,
  with 0.5 beside the rounding of 1, at one end or both, one or two at an
  end. 1720 made no ride where an end vertex held another stripe; the pair
  was rolled on the larger one's contact line (0016): a valid result with
  4.5 mm3 more material (or an invalid one, at the rounding of 1).
- GitHub #371's design (`geometry.fillet_top_chains`, the case
  `fillet-wall-run-on`): the tower's top, a horseshoe 2.127 wide at its
  legs and 3 (2 + 1) at its apex, rounded along its inner edge with 2 and
  along its outer edge with 1 (Fillet1's sets 4 and 5). Each rounding fits
  on the top alone, so neither overflows it and OCCT found no overlap;
  their contacts pass each other on the top, touching at the apex, and the
  mitred ends at the legs' fronts failed (the two blends crossing).

## 1722: corners at a ride's end, rides all along

### 1722-chfi3d-tangent-ride-ending-at-a-corner-with-one-more-rounding

- `ChFi3d_Builder::rideEndCornerAllowed` (RideOnBlend's check of the
  stripes' ends): an end vertex may hold one more stripe besides the ride's
  stripe, a constant rounding of another radius whose edge there does not
  bound the face between, at a vertex of three sharp edges. (Two roundings
  of one radius meeting square have a singular intersection, two curves
  crossing; an edge of the face between rounded too vanishes with that
  face. Neither is made so: as before.)
- `ChFi3d_Builder::PerformTwoCornerAtRideEnd`, from `PerformTwoCorner`
  first: at a vertex of two stripes where one (the big) is a tangent ride's
  stripe, its side on the face between ending at a mere point on the face
  at the end, which is the other stripe's (the small's) other face. As
  `PerformTwoCornerbyInter`'s bevel of a big and a small blend whose pivot
  points differ: the small is cut by the big's blend from the crossing of
  their contacts on their common face to the point where the small's
  contact on the face at the end meets the big's blend; the big is cut by
  the small's blend up to there and by the face at the end from there to
  the ride's point (no second curve where the two points are one). Both
  curves go into the data structure on the big's surface (the big's end
  marked in it), the second on the face at the end too, its transition
  there FORWARD where the vertex's side, which goes, lies on the curve's
  right about the surface's own normal.
- `Compute`, with 1721's part rides (once the stripes are computed, before
  the corners): a pair of constant roundings of one SurfData each, sharing
  a face, that 1721 does not ride, and whose contacts on that face pass
  each other all along without crossing (the rider's contact nowhere
  farther from the other edge than the other's contact, nearer somewhere,
  both spanning the face alike: `contactsPassAllAlong`) is ridden all along
  (`RideOnBlend`); where that fails, both are computed on the faces of
  their edges again.
- `RideOnBlend`, `RideOnBlendPart`: the larger stripe's contact on its
  blend (the rider's contact, from the rider's walk on the support) put in
  the period of the larger stripe's own p-curves (`inPeriodOf`). On the
  tower's torus the walk left it one period off and the blend face's wire
  was not closed.

Result: the wall's cases build, valid, their volumes those of the two top
roundings less what each end rounding takes away below the top's arcs
(within a millionth), in either order the edges are added; the tower's top
builds, valid, the outer rounding's contact on the outer side at the legs'
fronts where the file's stored body has it (z = -505.566).

## What is left

[1723](fillet-ride-three-blend-corner-1723.md) makes the corner of three
blends (the first item below). The text below is as 1722 left it.

- A corner the small rounding reaches past the ride's point: its contact
  on the face at the end meets the rider's blend, not the larger one's, so
  that the three blends meet at a point on the ride's line. The design's
  tower: the legs' inner front edges rounded with 0.5 (Fillet1's set 12)
  beside the inner rounding of 2, whose ride's point at the legs' fronts
  lies 0.285 from the inner side. `PerformTwoCornerAtRideEnd` does not
  take it (the small's contact misses the big's blend); the corner then
  goes to `PerformMoreThreeCorner` and the blends' intersection check
  fails it, as before 1722. It needs the small cut by both blends (the
  big's up to the ride's line, the rider's from there to the face at the
  end) and the rider's end at its own vertex cut by the small too.
- So Fillet1 as a whole still fails: the twelve sets with set 5 fail at
  that corner; sets 2 and 6 (part rides between two crossings, corners of
  three blends at the arms' ends) are as 1721 left them.

## Reports and tests

- `geometry.fillet_tangent_ride`
  (`geometry/tests/test_fillet_tangent_ride.cpp`): the wall's six cases,
  each in both orders: built, valid, the volume within a millionth.
- `geometry.fillet_top_chains`
  (`geometry/tests/test_fillet_top_chains.cpp`): the tower's top (sets 4
  and 5 alone), with `MITCAD_GEOMETRY_CASES`.
- OCCT's own blend and chamfer groups (the private suite's `run.sh`, a
  DRAW build of the port with 1722 against the same build without it): 754
  cases each, no case changed.

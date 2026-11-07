# A groove's rim rounded past a corner of the ball's radius: 1610, 1611

mitcad#1428, a follow-up of mitcad#1398 (found among the regenerated
rounding specs, core/import/docs/rounding-specs.md): the rim of a concave
groove (a cylinder of 12.5) cut across a leg, rounded with 0.5 along a
tangent chain that runs over an earlier convex rounding of 0.5, and up to
a wall's arc. The stored body makes the rounding over the corner of 0.5 a
piece of the ball's sphere (the ball stays put while its contact sweeps
round the corner, port patch 0670's construction) and cuts the roundings
that cross at the wall along their intersection (OCCT's own
`PerformTwoCornerbyInter`). Back to the [port's README](../README.md).

## Overview

- **1610** takes the section where the walk goes on from a standing ball's
  sphere only where the guide's normal planes pass the ball forwards:
  where the guide turns back round a later corner, a second section of the
  ball was taken and the walk from there came out twisted.
- **1611** cuts the larger of two crossing roundings with the smaller
  one's face at the end on the smaller one's periodic contact p-curve over
  its own parameters, not cut to the p-curve's first period.

## The case

A leg (a profile extruded along z, 29 long): a flange whose top y = 5
meets a slanted side (5 x + 3 y = 0) through a convex corner rounded with
0.5 (axis at x = -2.1169, y = 4.5), the slanted side meeting the leg's
right side x = 3 through a convex corner rounded with 2, and above the
flange a wall x = 0. A groove, a cylinder of 12.5 along x (axis y = -5,
z = 15), is cut across the leg; its face is concave. The design rounds the
groove's edges with the wall, the right side, the slanted sides and the
roundings of 2 with 0.5; the flange's top and its corner rounding join the
chain as tangent edges. Two such legs.

The ball's centre is 13 from the groove's axis. Over the corner rounded
with 0.5 it lies on the corner's axis (the corner's cylinder is a tube of
the ball's radius about its centre): the ball stays put at (-2.1169, 4.5,
6.1259) while its contact sweeps round the corner, and the stored body
has a piece of its sphere there (0.676 mm2), between the cylinder along
the flange's top and the walked rounding along the slanted side. At the
wall the rounding along its arc (a torus) and the one along the flange's
top (a cylinder) cross: the vertex's third edge, the wall with the
flange's top, is concave, and both roundings end OnSame on it at
different points; the stored body runs the cylinder on to x = 0.5 and cuts
the two along their intersection.

## 1610: the walk on from the sphere where the planes pass forwards

### 1610-chfi3d-standing-ball-section-where-the-normal-planes-pass-forwards

0670 puts the sphere's piece in the gap between the cylinder along the
flange's top (an analytic piece ending where the ball reaches the corner)
and the next analytic piece, the torus along the right side; the walk
takes the rest of the gap from the section where its own rule (the
contacts' mid-point in the guide's normal plane) puts the ball at the
sweep's far end. `ChFi3d_StandingBallSection` finds it as a root of the
mid-point's signed distance from the guide's normal plane, the root
nearest to the gap's far end for the sphere at the gap's start. Here the
gap runs on over the slanted side and the rounding of 2, round which the
guide turns from the slanted side's direction to the right side's: its
normal plane swings back through the ball there, a second root at spine
parameter 11.66 (the right one at 3.86, 7.8 along the guide before it).
The sphere's piece was made from 2.12 to 11.66, the walk over the slanted
side started at 11.66 with the ball's contacts at the corner, its
sections' tangents against the guide's (`TwistOnS1`, `TwistOnS2`), the
rounding of 2 got no piece, and the stripe failed as twisted
(`ChFiDS_TwistedSurface`, "the rounding's surface twists").

- `ChFi3d_StandingBallSection` takes only the roots where the distance
  decreases along the guide: the normal planes pass the mid-point
  forwards (ahead of the plane before, behind it after). Where it
  increases, the guide turns back past the ball, and the walk from there
  runs against the guide.

The sphere's piece now runs from 2.12 to 3.86, the walk over the slanted
side and the rounding of 2 from 3.86 to the torus; every walked face lies
on the rolling ball's envelope within 1e-5 mm.

## 1611: the crossing roundings' cut on a periodic p-curve

### 1611-chfi3d-two-corner-cut-on-a-periodic-pcurve-not-cut-to-its-period

At the wall, `PerformTwoCorner` tries `PerformTwoCornerbyInter` (both ends
OnSame): the two roundings' contacts on the pivot (the wall's edge with the
flange's top) lie at different points, so the smaller one (the torus,
whose contact meets the pivot nearer the vertex) is cut by the larger one
along their intersection, and the larger one by the smaller one's face at
the end (the wall) from the point where the torus's contact on the wall
crosses the cylinder to the cylinder's point on the pivot. That point is
found on the contact's 3D curve, then again on its p-curve on the wall,
over a twentieth of the contact on either side, cut to the p-curve's
range. The torus's contact on the wall is a circle whose p-curve is a
`Geom2d_Circle` of range [0, 2 pi]; the contact ran from -0.069 (the
crossing, past the torus's end on the pivot) to 1.34, and the range cut
from it, [0, 0.0009], did not hold the crossing: the second intersection
failed, and `PerformMoreThreeCorner` filled the corner with a plate 0.049
mm off the stored body's roundings (a face more at each such corner).

- `PerformTwoCornerbyInter`: a periodic p-curve is taken over the
  parameters about the crossing as they are; a non-periodic one is cut to
  its range as before.

The cylinder now runs on to x = 0.5 and both roundings are cut along their
intersection, as in the stored body (its faces there within 1e-6 mm of the
result's).

## Reports and tests

- 1610 and 1611: mitcad#1428, in
  `geometry/tests/test_fillet_groove_rim_corners.cpp`
  (`geometry.fillet_groove_rim_corners`): a synthetic leg, its groove's
  rim rounded with 0.5: valid and closed, every new face on the rolling
  ball's envelope within 1e-5 mm, each corner of 0.5 a piece of the
  ball's sphere about the point where the corner's axis meets the
  envelope, the flange's roundings run on to x = 0.5. With
  `MITCAD_GEOMETRY_CASES`, the case `groove-rim-corners`: the reported
  design's stored body before the item (`body.brep`) rounded as the
  design selects, against the stored body after it (`after.brep`): valid
  and closed, its 156 faces, both ways within 0.01 mm about each leg (0.0013
  mm), the change of volume within 5e-3 of the stored body's. Without
  1610 both roundings fail (twisted); without 1611 the synthetic leg's
  upper corner and the case's first leg are filled with a plate (0.37 mm
  off the envelope; 157 faces, 0.048 mm off).
- The stored body's roundings over the earlier roundings of 2 are
  approximations: the ball's centre seen from them lies up to 0.0006 mm
  off the envelope and their edges with the roundings of 2 have p-curves
  up to 0.0013 mm off their curves, which puts the stored body's volume
  integrals off by about 0.01 mm3; the change of volume is therefore
  compared by the faces' distance (the result's faces against the stored
  body's: 0.0020 mm3 for both legs, 2.8e-4 of the change).

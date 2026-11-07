# A convex rounding up into the mitre of tighter roundings: 1500

mitcad#1414, the follow-up of mitcad#1397
([fillet-walk-back-step-1490.md](fillet-walk-back-step-1490.md), the mitre
of roundings of the ball's own radius): a convex edge rounded up into the
mitre where two earlier convex roundings of a smaller radius meet. The
concave counterpart (a concave chain through a wall's corner tighter than
the ball) is 1380 in
[fillet-three-face-corner-1380.md](fillet-three-face-corner-1380.md). Back
to the [port's README](../README.md).

## Overview

- **1500** ends a convex contour of radius R at the foot of a mitre of two
  earlier convex roundings of radius rk < R and closes the corner with
  those roundings' own ball: the ball of rk rolled between the contour's
  end section and the top face, a torus about the rounding's axis.

## The case

A wedge (two planes A and B at 90 degrees) stands on a plate. Its foot
edges are rounded with 1.5 (concave), its top edges with rk = 2.5
(convex): the top roundings are cylinders tangent to A and B along their
edges at the top V of the wedge's vertical edge, and they meet in a mitre
from V (to which it is tangent) up to W on the top T. Then the vertical
edge is rounded with R = 3; its tangent chain takes both mitres, the foot's
and the top's (`7dab3af4` item 29 of mitcad#1371's list).

At the foot nothing is tight: the ball of 3 lies in the material outside
the concave cylinders of 1.5, and OCCT walks it over the mitre down to the
plate (a B-spline face of 9.2123 mm², the file's 9.2130). At the top the
ball of 3 would have to touch both convex cylinders of 2.5 from inside
(its centre 2.5 - 3 < 0 from their axes): there is no section, and the
walk over the mitre failed at once (`ChFiDS_WalkingFailure`), on the
replayed and on the stored body. On a synthetic block whose top edges are
rounded with 2.5, the vertical edge built at radii up to 2.5 and failed
from 2.6.

The stored body after the rounding:

- the vertical rounding is the cylinder of 3 between A and B from the
  foot's corner up to V's height, z = 40.5 (169.646 mm² = 36 x 3 x pi/2):
  it stops where the mitre starts;
- the corner is one torus about the same axis, major radius
  0.5 = R - rk, minor 2.5 = rk, its tube circle 2.5 below T (12.9017 mm²):
  the ball of rk rolling between the rounding's end (its contact the end
  section, the arc of 3 at z = 40.5) and T (its contact the circle of
  radius 0.5), from the section of one top rounding to the other's (its
  meridians through the vertical rounding's contacts with A and B, which
  are those cylinders' own sections there, tangent to them);
- the top roundings are cut along those sections (the strips up to the
  mitre gone), T along the circle;
- the material taken: the vertical piece 36 x 9 (1 - pi/4) = 69.531 mm³,
  the top corner (1 - pi/4)(rho² rk + rho pi rk² / 2 + 2 rk³ / 3) =
  3.423 mm³ (rho = R - rk; each horizontal section at h above V is the
  square of side rho + sqrt(rk² - h²) less its quarter disc), the foot's
  corner the rest (1.620 mm³; OCCT's walk 1.613).

## 1500: the corner's torus

### 1500-chfi3d-convex-chain-into-a-mitre-of-tighter-roundings-closed-by-their-balls-torus

`ChFi3d_ConvexMitre` (new) recognises the mitre Em at the end of a chain
past its edge Ex: Ex a line between the planes A and B; Em between two
cylinders KA and KB of one radius rk below R, KA tangent to A and KB to B
along their edges through V, both tangent to a plane T along their edges
through Em's other vertex W; T square to Ex; the cylinders' axes square to
Ex, rk inside their planes and T, and through the torus's tube circle
(radius R - rk about the axis of the ball of R between A and B, rk inside
T, at V's height).

`SplitAtConvexMitres` (new, as `Compute` starts, after 1380's
`SplitAtTightConcaveArcs`) leaves such a mitre out of a convex open
contour of constant radius whose first or last edge it is; the contour
ends at V with the status `ChFiDS_Tangent`, so its analytic piece (the
cylinder between A and B) ends at V's section.

`PerformConvexMitreCorners` (new, after `PerformTightConcaveCorners`,
before the corners at vertices) builds each such end's corner:

- the stripe ends on the arc of radius R in V's section, from PA on A to
  PB on B, its contacts' ends, which lie on the edges A and B share with
  KA and KB (the parts of those edges towards V taken away);
- the torus (exact, `Geom_ToroidalSurface`, its frame on the ball's axis
  rk below T, X towards A's contact) is a corner stripe on the stripe's
  blend (that arc, v = 0) and T (the circle of radius rho from QA to QB,
  v = pi/2, on the edges KA and KB share with T, the parts towards W taken
  away), from KA's meridian (PA to QA, u = 0) to KB's (PB to QB,
  u = phi, the angle between A's and B's normals);
- KA and KB are cut along the meridians (G1 with the torus, `myRegul`), the
  parts towards the mitre taken away; the torus is generated from the
  mitre's edge (the history's `Generated` of Em), so that the corner face
  has a name of its own.

0970's `PerformTightCorners` leaves such ends to it. Other ends, and mitres
whose corner is not that torus (a walked rounding, T not square to the
edge, cylinders off the torus), are left as before: the walk fails there.

On the design's stored body before the item: valid, the change of volume
74.568448 against the file's 74.574194 mm³ (the foot's walked corner), the
corner's torus 12.901728 mm², the top roundings 170.732442 mm² and T
453.439603 mm² as the file's (170.7324, 453.4396). On the synthetic block
at radii 2.6, 3, 4 and 6: valid; the top corner's material within 5e-5 mm³
of the formula above.

## Reports and tests

- `geometry.fillet_convex_mitre` (`geometry/tests/test_fillet_convex_mitre.cpp`):
  the block on the plate, its foot rounded with 1.5 and its top with 2.5,
  each vertical edge rounded with 3, 4 and 6, given each way round: valid,
  one torus of area rk phi (rho pi / 2 + rk), and the material taken at
  the top corner (against the block with its foot rounded only) the
  formula's within 1e-4; the block with only its top rounded, rounded with
  3: the straight piece and the corner; MitCad's fillet of the block's
  vertical edge: every face and edge named, the torus after the mitre's
  edge. With `MITCAD_GEOMETRY_CASES`: the
  case `convex-mitre-tighter-roundings`, the stored body before and after
  the rounding, the result on the stored body and back within 0.01 mm
  about the chain, the material taken the stored body's within 0.01 mm³.
  Each part fails without the patch.

# A ball beyond its support's centre of curvature: 1760

mitcad#1977: a cast part's top rim rounded with a ball twice as large as the
roundings of the drafted corners it runs round, which the walk could not
build after [0075](fillet-edge-overflow.md#0075-blendfunc-constrad-rolling-ball-section)
made the walked blend a true rolling ball. Back to the
[port's README](../README.md).

## Overview

- **1760** walks a constant-radius blend with its sections in the guide's
  normal plane, as OCCT 8.0.1 walks every one, where the ball would lie
  beyond a support's centre of curvature (`BlendFunc_ConstRad` and
  `BlendFunc_ConstRadInv`); a ball that fits both supports is 0075's
  rolling ball as before.

## The case

An `.ipt` part of a public test case (the NIST ftc_08 test case of the
`.ipt` corpus): a body whose sides are drafted by 2.5 degrees, its corner
edges rounded with 3.048, then the top's rim (a chain of 16 edges: lines
and the arcs where the top meets the corners' roundings, whose axes lean
with the draft) rounded with 6.096, one set of 40 edges with the rim of a
recess. OCCT failed on the rim's chain (a walking failure on the first
corner; only the eight analytic pieces along the lines were computed), and
the import fell back to the stored body after three definitions of about
20 s each in MitCad's paths past narrow faces. Bisected over the port's
patches, the first that fails is 0075: with 0074 the rounding builds,
valid, in 0.1 s. On 10.10.2026, before the port's patches, the import had
Fillet2 parametric.

## 1760: walked in the guide's normal plane where the ball cannot roll

### 1760-blendfunc-constrad-ball-beyond-its-support-walked-in-the-guides-normal-plane

0075 puts the ball's centre at the radius along both surfaces' unit
normals. Round a convex corner rounded with 3.048 the ball of 6.096 rounding
the rim lies inside the block, so its centre lies 6.096 from the corner's
rounding towards its axis: past the axis, 3.048 beyond it. No ball of that
radius touches the rounding from inside; the equations then hold for a ball
that holds the rounding's section and touches it from outside, and the
sections fold back as the guide runs round the corner. The walk failed, and
on a synthetic block (the test) every radius just above the corners'
(3.005 to 3.5) built an invalid result and 6 failed.

Before 0075 OCCT took the centre along each normal projected into the
guide's normal plane and the section in that plane; such a "ball" is no
rolling ball where the normals are not square to the guide, but it exists
round such a corner, and the rim's rounding built: on the case, valid, its
volume 559765.07 mm^3 against the file's stored 559765.68 (1.1e-6), the
corner pieces one B-spline face each (the file's corners have two).

1760 tells the case by the supports and the guide (`BallBeyondSupport`,
static): a cylinder, cone, sphere or torus whose normal curvature along the
guide (at the guide's middle, where the guide lies on it) turns towards the
ball's centre with a radius smaller than the ball's by more than 1e-6. The
direction matters: a foot rounding of 2 run round a corner torus whose
tube is 1.5 (patch 0460's case in `test_fillet_corners`) rolls round the
torus's axis, where the torus curves less, and stays 0075's rolling ball,
which builds it; taking the tube's radius instead broke that case.
`Set(Radius, Choix)` of both functions then
takes the guide's normal plane: `ComputeValues`, `IsSolution`, `Tangent`
and the four `Section`s of `BlendFunc_ConstRad`, and `Value` and `Values`
of `BlendFunc_ConstRadInv` are OCCT 8.0.1's own, renamed, in
`BlendFunc_ConstRad_GuidePlane.cxx` (their derivatives of `BlendFunc_ConstRadInv`
through `Values`, whose derivative of the guide's plane is the right one);
`StillSection` gives none (1520's tubes are of the ball's own radius, never
beyond it). The caches are cleared when the mode changes. The margin is
1e-6 and no wider: from 3.0001 to 3.005 the true ball's walk also left the
result invalid. Across the corners' radius the two ways meet: in steps of
0.01 from 2.985 to 3.015 the material taken away changes by 3.45, 4.28 and
3.51 mm^3.

## Tests

- 1760: mitcad#1977, in `geometry/tests/test_fillet_ball_beyond_support.cpp`:
  a block drafted by 2.5 degrees, its corners rounded with 3, its top's rim
  rounded by OCCT's fillet alone with 4, 6 and 8 (done, valid, more taken
  away with each; without 1760 6 fails and 4 is invalid) and from 2.985 to
  3.015 in steps of 0.01 (done, valid; without 1760 3.005 and 3.015 are
  invalid), the step over the corners' radius within 30 % of the steps
  beside it.

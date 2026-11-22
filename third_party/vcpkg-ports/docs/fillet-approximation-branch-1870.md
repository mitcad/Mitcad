# The approximation of a walked blend kept on the walked ball: 1870

mitcad#2161, found by mitcad#2121's proof runs: the foot of a wide, low
round step rounded higher than the step failed.
Back to the [port's README](../README.md).

## Overview

- **1870** solves the sections the approximation of a walked
  surface/restriction blend takes between the walk's points on the ball
  the walk followed: where the solution from the linear start lies
  farther from the contact the walk's tangents give than that start did,
  the section is solved again from that contact (`SearchPoint`).

## The case

A plate with a round step 0.2 high, the step's foot rounded with 0.4. The
rounding is higher than the step, so the ball touches the plate and rolls
on the step's top rim all round (patch 0011's overflow): a blend between
the plate and the rim, walked by `BRepBlend_SurfRstLineBuilder`, its
centre 0.4 above the plate and c = 0.346 outside the side.

With the step's radius 100 or more the fillet failed ("PerformSurf :
Failed approximation!"); with 50 it built. The limit grows with c: a
rounding of 0.6 failed from the radius 150, one of 1 from 200 (with a step
0.5 high, from 200 and 400). A rounding lower than the step (an analytic
torus) built at every radius.

## 1870: the sections between the walk's points

### 1870-brepblend-approximation-solves-sections-on-the-walked-ball

Every section of a surface/restriction blend has a second solution: the
ball on the restriction's other side, its contact on the surface 2c from
the walked one (here at 99.654 from the axis instead of 100.346). The walk
keeps to its branch. Its steps are 9.6 degrees at any radius: the
deflection allowance is a thousandth of the guide's length.

`BRepBlend_AppFuncRoot::SearchPoint` gives the approximation each section
between two walked points. It solves from their linear interpolation,
which on the surface lies on the chord, inside the contact circle by the
chord's sagitta, (R + c) theta^2 / 8. At the radius 100 that is 0.35 (the
start at 99.996), past the middle between the two balls (100.0). The
solver went over to the other ball. The section took more than three
iterations, so it was inserted into the line and drew the starts beside it
over too, and the approximation joined the two branches: its reached
tolerance was 0.326 mm (4e-6 at the radius 50). `StoreData` then found no
sample with a normal longer than that tolerance (the blend's U runs over
the guide's length, so its normals are short), and the fillet failed.

Now `BRepBlend_HermiteContact` takes the contact on the surface at the
section's parameter from the walked points about it, with the contact's
tangents the walk stores (dU/dt, dV/dt): a cubic Hermite interpolation,
whose error is of the fourth order in the step. Points the approximation
inserted carry no tangent and are passed over. Where the solution from the
linear start lies farther from that contact than the start did, and more
than ten times the variables' tolerance from it, the section is solved
again from that contact. The second solution is taken where it lies nearer
to the contact. Otherwise the first is kept, solved again so that the
function's state is its own. Elsewhere the first solution lies on the
walked contact, nearer to the Hermite contact than the chord, and nothing
changes. Blends between two surfaces and between two restrictions are not
changed.

## Reports and tests

- 1870: mitcad#2161, in
  `geometry/tests/port/p1870-approximation-keeps-the-walked-ball.cpp`
  (ctest `geometry.port.1870`): a plate with a round step 0.2 high, the
  foot rounded with 0.4 at the step's radii 100 and 200, and with 1 at
  200: one valid solid, its tolerances within 1e-4, the volume the rolling
  ball's (the section under its arc revolved) within 1e-5, the step's side
  gone. The radius 50 builds with and without the patch.

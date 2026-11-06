# A ball of twice a concave arc's radius: 1390

mitcad#1368: a pocket's floor rounded with exactly twice the radius of the
concave arcs at its corners, where the walk could not leave its first
section. The walked blend's section condition is
[0075's](fillet-edge-overflow.md#0075-blendfunc-constrad-rolling-ball-section).
Back to the [port's README](../README.md).

## Overview

- **1390** takes the walk's section condition on the ball's centre instead
  of the mid-point of its contact points where that mid-point lies on the
  guide's polar axis, in `BlendFunc_ConstRad` and `BlendFunc_ConstRadInv`.

## The case

A bracket's pocket (`5ef2dbd5` of mitcad#343, item 37), 3 deep, its side
walls at x = -7 and x = 7 meeting its near end through concave arcs of 0.4
(each turning 157 degrees) and convex arcs of 1.25 on either side of a long
concave arc of 12; every curve of the outline goes on tangentially into the
next, and the floor is square to the walls. The floor's outline is rounded
with 0.8, exactly twice the corner arcs' radius. OCCT failed at 0.8, also on
the file's stored state before the item, and built at 0.79999 and 0.80001;
MitCad's fillet took its 0.1 % smaller retry (0.7992).

## 1390: the ball's centre picks the ball where the mid-point cannot

### 1390-blendfunc-constrad-centre-picks-the-ball-where-the-mid-point-lies-on-the-guides-axis

Patch 0075 makes the walked blend a true rolling ball whose guide only
picks the ball: the mid-point of its contact points lies in the guide's
normal plane. Round a concave arc of radius R with the floor square to it,
a ball of radius r touches the floor r - R from the arc's axis on the far
side and the wall R from it on the near side, level with its centre; the
mid-point of the contacts lies (2 R - r) / 2 from the axis. At r = 2 R it
lies on the axis for every ball round the arc, and every normal plane of
the circular guide holds the axis: the condition holds for all of them and
picks none. The solver returned the start section at every step (the
Jacobian singular, the section's tangents not found, the point a tangency
point), and the walk halved its step to nothing. At 0.79999 the mid-point
lies 5e-6 from the axis and the condition still holds the ball, if weakly.
The inverse function, which frees the guide's parameter where a contact
runs onto a restriction, lost its hold on that parameter in the same way.

In general the mid-point's condition loses its hold where the mid-point M
lies on the guide's polar axis: the derivative of the condition by the
guide's arc length is k (M - P).n - 1 (P the guide's point, k its
curvature, n its principal normal), and it vanishes there. Both functions
now take the condition on the ball's centre (the mean of the centres seen
from both contacts, equal at a solution) where |1 - k (M - P).n| is below
1e-6, with its first and second derivatives (the centre moves by half of
each centre's move, which the equations of the centres already hold).
Where the ball's section lies in the guide's normal plane, as between
surfaces of revolution about a circular guide's axis, the centre and the
mid-point pick the same ball, so the walk goes on as it would beside the
singular radius; everywhere else the mid-point stays the condition and the
results are unchanged. The centre's own condition loses its hold where the
centre lies on the polar axis (r = R, the ball's centre on the arc's
axis), which the analytic blend of a plane and a cylinder builds as a
sphere before any walk.

On the case the rounding now builds at 0.8 on the body MitCad builds and
on the file's stored state before the item, without the 0.1 % smaller
retry. At each 0.4 arc OCCT ends the rounding in one of two ways, 0.04 mm
apart (the walked face 1.146 or 1.204 mm², 0.019 mm³ of material), and
which one tips with tiny differences, on either side of 0.8: at the arc
at x = -6.6 the body MitCad builds gives the first at 0.79999 and the
second at 0.8 and 0.80001, the stored state the other way round, and the
0.1 % smaller retry (0.7992) gave the second at the other arc. That choice
is OCCT's own and comes with the ball larger than the arc (the arc's
rounding overlaps the wall's beside it), not with 1390: at 0.8 the walk
gives what it gives at 0.80001, face for face. The file's own corner
faces are not the rolling ball's (their points moved by the radius along
their normals lie up to 0.2 mm from the radius above the floor); measured
from them, the second way lies within 0.024 mm and the first within
0.065 mm. At 0.8 the body MitCad builds now gives 0.024 mm at x = -6.6 and
0.065 mm at x = 6.6; the retry gave 0.064 and 0.024 mm. The two ways come
from the filling of the twisted piece past each arc, which read numerical
noise; [1410](fillet-ball-twice-arc-end-1410.md) makes them one, the
second (mitcad#1384).

## Tests

- 1390: mitcad#1368, in `geometry/tests/test_fillet_ball_twice_arc.cpp`: a
  pocket like the case's in a block, its floor's outline rounded with 0.8
  by OCCT's fillet alone (also at 0.79999 and 0.80001: built and valid,
  the material added at 0.8 within 1e-3 mm^3 of that at both) and by
  MitCad's fillet (built as asked, without the 0.1 % smaller retry, valid,
  its faces round the floor within 1e-4 mm of OCCT's at 0.79999 and
  0.80001 both ways; 6e-6 mm measured); and on the case
  `ball-twice-arc-rounding` of `MITCAD_GEOMETRY_CASES`, the body MitCad
  builds and the file's stored state before the item, the same roundings
  (the material added at 0.8 within 1e-3 mm^3 of that at one of the radii
  beside it, for the two ways above), with the stored body after the item
  measured against the result round the arcs. OCCT fails at 0.8 without
  1390 (not done, one faulty contour), and MitCad's fillet takes the
  0.1 % smaller retry.

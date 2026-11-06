# A ball of twice a concave arc's radius, its end at the arc: 1410

mitcad#1384, the follow-up of mitcad#1368
([fillet-ball-twice-arc-1390.md](fillet-ball-twice-arc-1390.md)): the
pocket's floor rounded with 0.8 now builds at 0.8, but at each concave arc
of 0.4 the rounding ended in one of two ways, 0.04 mm apart, as tiny
differences tipped it. Back to the [port's README](../README.md).

## Overview

- **1410** leaves the points where a constrained filling's tangent field
  vanishes out of the check that the field keeps its orientation
  (`GeomFill_ConstrainedFilling::CheckTgte`), so that numerical noise there
  no longer decides the filling's blending functions.

## The case

The pocket of 1390 (`5ef2dbd5` of mitcad#343, item 37): its floor's
outline rounded with 0.8 runs round each concave arc of 0.4 (157 degrees)
between the side wall and a convex arc of 1.25. The ball is larger than
the arc: along the arc the walk pivots about the point where the floor's
contact stands still, and its piece there is flagged twisted. ChFi3d then
fills it (`ChFi3d_Builder::PerformSetOfSurf`, the reprocessing of twisted
pieces) with a three-sided `GeomFill_ConstrainedFilling`: the contact on
the arc's cylinder, the sections of the pieces before and after, and the
standing point as the fourth, degenerate side.

On the body MitCad builds the arc at x = -6.6 came out one way at 0.79999
and the other at 0.8 and 0.80001, the file's stored state before the item
the other way round, and the 0.1 % smaller retry (0.7992) the second way at
the other arc: the filled face 1.146 or 1.204 mm², 0.019 mm³ of material.
The walked pieces, the contact on the arc and the neighbours' sections were
the same in every case.

## 1410: the filling's field is not judged where it vanishes

### 1410-geomfill-constrainedfilling-field-not-judged-where-it-vanishes

`GeomFill_ConstrainedFilling::Init` checks with `CheckTgte`, along each
side carrying a tangent field, that the trihedron of the side's tangent,
its surface's normal and the Coons patch's tangent field keeps its
orientation: it compares the mixed product at 13 points with the one at
the first. If it changes sign, the patch's blending functions are made
`Law::MixBnd` instead of linear, which moves the filling's inside by up to
0.04 mm here. The two sides that run into the degenerate corner have a
field that comes to nothing there: its mixed product at that end was
±1e-15, its sign numerical noise, and that noise switched the blending
functions. It changed with the radius by 1e-5, with a body 0.0003 mm³
apart, and between the two mirror-image arcs of one body.

`CheckTgte` now takes the 13 mixed products first and compares only those
above 1e-9 of the largest along the side: a field that vanishes at a
corner does not count as turning there. On the case both arcs now end one
way at 0.79999, 0.8 and 0.80001, on the body MitCad builds and on the
stored state: the linear blending, the filled face 1.204 mm². That is the
way nearer the file's own corner (whose faces are not the rolling ball's):
measured from it, the stored body after the item lies within 0.029 mm of
the result round both arcs. The other way gave 0.065 mm at its arc.

## Tests

- 1410: mitcad#1384, in `geometry/tests/test_fillet_ball_twice_arc.cpp` on
  the case `ball-twice-arc-rounding` of `MITCAD_GEOMETRY_CASES`, on the
  body MitCad builds and the stored state. At 0.79999, 0.8 and 0.80001 the
  area of the B-spline faces round each 0.4 arc is the same within
  1e-3 mm² (the two ways differ by 0.06), and the material added at 0.8 is
  within 1e-3 mm³ of that at both radii beside it. Without 1410 the area
  differs at one arc or the other.

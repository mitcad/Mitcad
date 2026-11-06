# A concave rounding past a corner tighter than its ball: 1380, 1381

mitcad#1371: a concave rounding whose chain runs through a wall's concave
corner rounded with less than the rounding's radius, where the ball would
touch three faces at once. The class mitcad#238 (port patch 0160, in
[fillet-walks-and-contours.md](fillet-walks-and-contours.md)) left open;
the convex counterpart is 0970 in
[fillet-replay-0950-0980.md](fillet-replay-0950-0980.md). Back to the
[port's README](../README.md).

## Overview

- **1380** splits a concave contour at a concave arc of its chain tighter
  than the ball, runs both roundings on to the point where the ball of
  the radius touches all three faces, and closes the corner with the
  corner's own rolling ball: the concave arc's cylinder is the ball of its
  radius rolling between the two walls, and that ball rolls on down,
  between a wall and a rounding and then between the two roundings, until
  the two roundings touch.
- **1381** unbounds 0940's surfaces through a trimmed surface and puts the
  curves projected on it in the face's own period: a later rounding of
  the same design (0940's key top over a narrow side up to a torus), which
  1380's corner now lets the replay reach, met the replay's torus face on a
  trimmed surface of the torus.

## The case

A rod (a cylinder of radius 10) and a wall standing on it, the wall's plan
arcs of 0.4 (convex), 1.25 (concave), 12 (convex), 1.25 and 0.4, tangent in
turn, rounded with 3 along their foot (`5ef2dbd5` item 30 and `99ef321f`
item 14 of mitcad#343). The ball of 3 cannot touch the concave arcs of
1.25: its centre would run round their axes at a radius of 1.25 - 3 < 0.
OCCT's walk went into the arc and failed 0.13 before its end, on the
replayed and on the stored body.

The stored body after the rounding:

- the rounding along the arc of 12 (A) runs on past the ball that touches
  the rod and both convex arcs (its contact with the rod P), its face on
  the wall up to height 45.414, just short of the concave arc; the
  rounding along the arc of 0.4 (B) runs on until its contact on the wall
  reaches the concave arc (height 45.667); both end on the rod at P;
- the concave arc is cut at that height;
- a strip (0.68 mm²) and a triangle (3.74 mm², its apex at P, a
  degenerated edge) close the corner.

Every section of the strip and of the triangle is a circle of radius 1.25
whose centre lies 1.75 = 3 - 1.25 from the balls of both roundings at its
contacts (within 5e-5 mm, the stored faces' own fitting): the strip is the
ball of 1.25 rolling between the wall of 12 and B, the triangle the same
ball rolling between A and B. At the top the strip's section is the
concave arc's own (the ball of 1.25 touching the wall of 12 and, at B's
end, the wall of 0.4); the strip ends where its contact on the wall of 12
crosses A's contact there (at height 45.414, which is where A's face ends
on the wall); the triangle ends where the two roundings touch, at P, where
the ball's section has no width left. The triangle's curves on the
roundings are that ball's contacts, which is why they are neither sections
of the roundings nor planar, and the circle between the strip and the
triangle is its section at the crossing: tangent to the wall of 12, tilted
to meet B.

## 1380: the corner's own ball

### 1380-chfi3d-concave-chain-through-a-corner-tighter-than-the-ball-closed-by-the-corner-s-own-ball

`SplitAtTightConcaveArcs` (new, as `Compute` starts, after 0970's
`SplitAtTightCorners`) splits a concave contour of constant radius R at an
edge of its chain between the chain's common face U and a cylinder K of
radius below R whose axis lies on the ball's side, K tangent along its
edges (lines along its axis from the edge's vertices) to the faces of the
chain's edges before and after it; the runs between such arcs become
contours of their own, ending at the arcs with the status
`ChFiDS_Tangent`. After `ExtentAnalyse`, `ExtendAtTightConcaveArcs` (new)
lets the ends at such an arc (two contours of one radius ending at its two
vertices) walk on past their vertex by two radii at least
(`ExtentOneCorner`'s half spine stopped the walk along the arc of 0.4 0.11
short of K's edge), so that each contact on its wall reaches K's edge,
where the walk stops. Other ends are left as they are: other concave
contours' tangent ends neither walk on nor make `PerformTightConcaveCorners`
fail.

`PerformTightConcaveCorners` (new, after `PerformTightCorners`, before the
corners at vertices) finds, for each such arc and the two stripes ending
at its vertices, where each stripe's contact on its wall crosses K's edge
(L the stripe whose crossing is the lower, H the other) and P on L's
contact with U where the ball's centre is R from H's wall. The sections of
the ball of K's radius rk are solved at heights along K's axis by Newton's
method on the supports' surfaces (the centre rk along each support's normal
on the ball's side, both centres one, at the height): from K's section at
H's crossing, on L's wall and H's blend, down to where the contact on L's
wall crosses L's contact (Q), then on both blends down to P, where the
ball's centre is rk along U's normal. The strip and the triangle are
B-spline surfaces through the sections' arcs (nine points each, the
triangle's last section the point P). L's blend ends on the triangle's
contact from Q to P, H's on the strip's and the triangle's contacts on it;
K is cut along its section; the strip and the triangle are corner stripes
generated from the arc's edge. Where both crossings lie at one height
there is no strip. 0970's `PerformTightCorners` leaves concave contours'
ends to it.

On the design's stored body before the item: valid, the result on the
stored body after it and back within 0.0012 mm about the chain, the change
of volume 15.038258 against the file's 15.038261 mm³; before, the fillet
failed. On the rod and wall of the case, OCCT alone: valid at radii 1.5,
2, 2.4, 3 and 4, no material of the body lost; at 3 the corner's points
within 6e-5 mm of the stored body's, the triangle 3.74149 and the strip
0.68399 mm² (the file's 3.74165 and 0.68392), the change of volume
15.0408 mm³. Before, at 3 and 4 the fillet failed; at 1.5 and 2 the walks
went on over the concave arc (0160's bridge), adding 2 % more and 0.4 %
less material; at 2.4 OCCT gave a result the checker passes that is not
the body rounded (its cut from the body is nearly the whole body).
mitcad#1368's synthetic pocket (the same profile as a pocket, its floor
rounded with 0.8 past corner arcs of 0.4 and 0.5) now has the arcs' own
balls at its corners: valid, 0.044 mm³ less material than OCCT's walk
round the arcs added; that test's checks hold.

The corner's curves on the faces and on the roundings' surfaces are
projected and kept only where they lie on them, else made point by point;
each is put in its face's period, or its first point's: a contact starting
on a torus's seam came out a period off, and so did a section starting on
a cylinder's seam (the pocket's corners gave unorientable faces).

## 1381: 0940 over a trimmed torus

### 1381-chfi3d-wide-surface-of-a-trimmed-surface-is-its-basis

The same design's item 38 (a key's top edge 13 long rounded with 2 over
its narrow side, up to the key's end roundings and a bore's wall, a
torus; 0940 in [fillet-replay-0930-0942.md](fillet-replay-0930-0942.md)).
With item 30 now built, the replay reaches item 38 on its own body, whose
torus face lies on a `Geom_RectangularTrimmedSurface` of the torus (the
stored body's on the torus itself). `ChFi3d_WideSurface` gave the trimmed
surface back as it was: 0940's run-on of the junction of the torus and an
end rounding past their vertex projected 48 mm off it, no run-on was
found, and the corners at both ends went to `PerformIntersectionAtEnd`,
which failed ("Failed to find edge"). With the basis taken, the run-on's
curve on the torus came out a period off the face's (the trimmed surface
is not periodic, so `ChFi3d_IntoPeriodOfFace` left it), and the torus faces
were unorientable. `ChFi3d_WideSurface` now unbounds a trimmed surface to
its basis, and `ChFi3d_IntoPeriodOfFace` takes a trimmed surface's basis's
period. On the replay's body before item 38: valid, as on the stored body.

## Reports and tests

- `geometry.fillet_three_face_corner`
  (`geometry/tests/test_fillet_three_face_corner.cpp`): the rod and the wall
  of the case, rounded with 3, 2 and 4; at 3 the corner's points, the
  strip's and the triangle's areas and the material added are the stored
  body's. With `MITCAD_GEOMETRY_CASES`: the case `three-face-corner`, the
  stored body before and after the rounding, the result on the stored body
  and back within 0.01 mm; the case `fillet-trimmed-torus`, the replay's
  body before item 38, its key's top rounded valid and closed (1381). Each
  part fails without its patch.

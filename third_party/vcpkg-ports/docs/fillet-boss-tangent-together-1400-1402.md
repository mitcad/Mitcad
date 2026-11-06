# A plate's top edge and a boss's foot rounded together: 1400 to 1402

mitcad#1374, the follow-up of mitcad#1359
([fillet-boss-tangent-1330-1332.md](fillet-boss-tangent-1330-1332.md)): the
plate's top edge along its side and the boss's foot, where the boss's
cylinder meets the side tangentially, rounded in one fillet. Back to the
[port's README](../README.md).

## Overview

- **1400** reports a convex and a concave rounding as overlapping
  (`Failure_Overlap`) instead of building them together, where they end at
  a common vertex at which every other edge is smooth and their contacts
  cross on a face of both their edges. MitCad's fillet then rounds the
  convex edge first and the concave one on its result.
- **1401** keeps a vanishing end moved to its vertex in the contact's
  period where the vertex lies on a periodic face's seam
  (`ChFi3d_ShrinkVanishingEnd`, 0130).
- **1402** takes a line walked into a vanishing end, and ended there
  narrower than 5e-4 of the radius (`ChFi3d_EndBeforeVanishing`, 0771), for
  that end at every radius (`ContactsAtTangentEnd`), instead of walking it
  once more into a sliver of its own.

## The case

The body of mitcad#1359: two circles of radius 25 whose centres are 85
apart, joined by the two lines tangent to both; the first circle and the
region between extruded 15 (the plate), the second circle 30 (the boss),
joined. The boss's cylinder is the plate's end too, and the plate's flat
sides go on tangentially into it along the vertical edges at x = 85,
y = +-25. Each of the plate's top edge along its side (a tangent chain
round the plate's other end to the other side) and the boss's foot on the
top rounds alone since 1330 to 1332, at 1, 3 and 6.

Rounded together, both end at the two vertices (85, +-25, 15). The side's
blend takes material away below the top, the foot's adds it above, and
their contacts on the top cross at
(85 - sqrt((25 + r)^2 - (25 - r)^2), +-(25 - r), 15), 67.68 at r = 3, short
of the vertices: past that point the foot's ball rolls off the top onto
the side's blend. The result needs the foot's rounding to follow the
side's blend down to the vertex where that blend vanishes, as rounding the
side first and the foot on its result gives: the foot's tangent chain then
runs on along the side's blend's cut on the cylinder, from the foot to
(85, +-25, 15 - r).

## 1400: overlap reported for a convex and a concave rounding

### 1400-chfi3d-convex-and-concave-roundings-crossing-before-a-smooth-common-end-overlap

OCCT computed both stripes and joined them at the vertices with its
corners, which do not see that the contacts cross before the vertex: the
result was invalid (self-intersecting wires at the vertex with the seam at
y = -25, an open body with it at y = 25). After the stripes are computed
and the passes for overlapping and crossing stripes
(`OverlappingStripes`, `contactsCross`) are done,
`convexConcaveCrossBeforeEnd` looks at each pair of open stripes: one
spine's edge convex and the other's concave on a face both stripes'
contacts lie on, an end vertex in common where every other edge is smooth
(its two faces tangent, `ChFi3d::DefineConnectType` giving tangential, or
one face across its seam), and their contacts on that face crossing inside
both (as `contactsCross` finds a crossing, not at either contact's end).
Both stripes then fail with `Failure_Overlap`, the two faulty contours,
and `round_edges` in `geometry/src/dressup.cpp` takes its existing route
(`concave_after_convex`, mitcad#343): the convex edges rounded first, the
concave one, found again by its name, on that result.

The smooth vertex is what tells this case from the one a corner builds: a
floor's convex top edge along its end and its concave edge along a wall
standing flush with that end meet at a vertex from which the wall's sharp
vertical edge goes on, and their contacts on the floor's top cross too;
there OCCT's corner joins the two bevels, bounded on that sharp edge
(1090, `geometry/tests/test_chamfer_sharp_corner.cpp`), and the stripes are
built together as before.

## 1401: a vanishing end on a periodic face's seam

### 1401-chfi3d-vanishing-end-on-a-periodic-faces-seam-kept-in-the-contacts-period

Rounded one after the other with 1, the foot's chain runs on along the
side's blend's cut on the cylinder to (85, 25, 14), where the side's blend
vanishes, the cylinder's seam passing through that vertex. The walk ended
there at u = pi/2 on the cylinder, its contact coming from u > pi/2.
`ChFi3d_ShrinkVanishingEnd` (0130) moves such an end to the vertex, the
contact's p-curve's last pole to the vertex's parameters on the face,
`BRep_Tool::Parameters`, which took u = 5 pi/2, the seam's other side: the
p-curve ran across the whole period in its last span (from 7.854 to
1.966) and the cylinder's wire crossed itself. The vertex's parameters are
now taken in the period of the contact's end (a whole number of periods
off the face's own, in u and in v). At 3 and 6 the results were valid
without it.

## 1402: the end before a vanishing end at radii above 2

### 1402-chfi3d-end-walked-into-a-vanishing-end-taken-for-it-at-any-radius

At 3 and 6 the same chain's blend had a face of 4e-6 mm2 at each of its
ends: `ChFi3d_EndBeforeVanishing` (0771) ends a line walked into a
vanishing end at its first section narrower than 5e-4 of the radius
(1.5e-3 at 3) within a hundredth of the radius of the vertex, and
`PerformSetOfSurfOnElSpine` takes that for the stripe's end
(`ContactsAtTangentEnd`, 0400 and 0771) only where the contacts are within
10 tolesp (1e-3) of each other. At radii above 2 the line's end may be
wider than that, and the stripe was walked on from there, 5.8e-3 along the
guide, into a piece of its own. `ContactsAtTangentEnd` now takes a width
up to `ChFi3d_EndBeforeVanishing`'s, a twentieth of its reach, where that
is more than 10 tolesp; the corner moves the end to the vertex as at 1.

## Not done here: the names of the chain's pieces

The foot's chain on the side's rounded body has three edges (the cut at
y = 25, the foot, the cut at y = -25) and three faces; the walked faces at
the two ends each lie partly along the foot. OCCT's history
(`PerformSetOfSurf`'s map of a walked piece to the spine's edges it runs
along) gives a walked piece whose guide runs over several edges the edges
from the one it starts on up to the last but one of the guide, never the
guide's last: the piece at y = 25 gets the cut there, the piece at
y = -25 the foot. So the faces are named
after the cut at y = 25, the foot (`#0`) and the foot (`#1`). Every face is
named; mapping each piece to every edge it runs along would also change the
names of faces in existing designs, so it is left as it is.

## Tests

- 1400, 1401 and 1402: mitcad#1374, in
  `geometry/tests/test_fillet_boss_tangent.cpp` (`check_together`): the
  plate's top edge and the boss's foot, and both with the boss's top edge,
  rounded together with 1, 3 and 6 on the body MitCad's sketch and
  extrusions build with the cylinder's seam at either vertex and on the
  case `boss-tangent-side-rounding` of `MITCAD_GEOMETRY_CASES`. OCCT's
  fillet alone reports `Failure_Overlap` with two faulty contours; MitCad's
  fillet gives a valid result, every face named, the roundings of both
  edges named after them (the foot's in numbered pieces), no face smaller
  than 1e-3 mm2, and the volume OCCT's fillet alone gives rounding the
  convex edges first and the foot on that result, within 1e-6 of it; that
  differs from the roundings' own material (the exact shapes of
  1330 to 1332) only within the boxes where the contacts cross short of
  the vertices. Without the three patches 78 checks fail, without 1401
  36 (every case at 1), without 1402 24 (the slivers at 3 and 6).
- 1400 with a sharp edge at the vertex:
  `geometry/tests/test_chamfer_sharp_corner.cpp`, still built together.

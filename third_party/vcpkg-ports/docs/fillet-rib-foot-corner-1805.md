# A rounding's corner beside a concave edge: 1805

mitcad#2038 (GitHub #456), a follow-up of mitcad#2009 and mitcad#2018
(port patches 1800 to 1803): the bent guide imported from a FreeCAD
document, its pads' ends and its ribs rounded with 0.5. Back to the
[port's README](../README.md).

## Overview

- **1805** sets the corner where one rounding ends at a vertex of more than
  three edges, one of them a concave sharp edge, back along the concave
  edges only, and lets its plate meet its borders G0. Before, it was set
  back along every sharp edge, and the plate turned the concave corner on
  its air side: it reached out of the body by 0.02 to 0.03, which MitCad's
  dressup check reported on the design as it opens.

## The case

The guide of [1801 to 1803](fillet-corner-walk-over-vertex-1801-1802.md#the-case):
a tube (radius 2.2) cut open by a flat 1.1 above its axis, swept through a
quarter turn, ribs bridging the channel above the flat. A rib's underside
goes on from the tube's wall below the flat tangentially (a face of its
own, a smooth edge between them along the rim under the rib); its end
faces stand on the flat. The Fillet rounds, among others, each rib's four
sloped edges, where an end face meets the underside, with 0.5.

Each of those roundings ends at its foot on the rim, a vertex of four
edges: the rounded edge, the end face's edge on the flat (concave: the rib
stands on the flat), the flat's rim past the rib (sharp) and the smooth
edge under the rib. `PerformIntersectionAtEnd` takes it to
`PerformMoreThreeCorner` as a corner of one stripe (the smooth edge beside
the rounded one). That corner set back 0.5 along every sharp edge from the
vertex: its point on the concave edge lay 0.5 out along the end face's
foot, its point on the rim 0.5 past the rib, and its plate's borders ran
from the rounding's end section down the end face to the first, along the
flat to the second and back along the rim under the rib. A smooth surface
whose border leaves a point of a concave edge on both of its faces lies on
the air side of both near that point: the plate rose above the flat past
the rib's end, up to 0.027 at the outer rim and 0.029 at the inner, at all
twelve feet and at every bend radius. Its tolerances (0.002 to 0.04)
cover that at some corners; the check of the result
(`geometry/src/dressup_check.hpp`) reported the two whose tolerance is the
smallest: "The corner of the fillet reaches 0.02 mm out of the body" when
the design opens at its own radius of 45, and at 17 and 18 (mitcad#2038).
FreeCAD's stored body has the same plates.

## 1805: set back along the concave edges only

### 1805-chfi3d-corner-of-one-rounding-sets-back-along-its-concave-edges-only

- `PerformMoreThreeCorner`, for one stripe (not two stripes on tangent
  edges, not OnSame + OnDiff, no free border, no seam), where a sharp edge
  at the vertex that is neither regular nor tangent to another edge is
  concave (`ChFi3d::DefineConnectType`): the set-back is taken along the
  concave edges only. Every other sharp edge keeps its point at the vertex
  (1801's walk over a vertex is not tried there: nothing is set back along
  those edges). Such a point is the vertex itself in the DS, as for two
  stripes on tangent edges, and its edge is not limited.
- The plate's border on the face beyond the concave edge (the flat) then
  runs from the vertex along that edge to its point, and the plate closes
  against the faces the rounding takes away: the end face, the rib's
  underside and the rim under it. The flat past the rib is not touched.
- The plate's constraints are all G0. At the vertex the faces meet at sharp
  angles and no tangency holds there (the corner's G1 error was 1.53 rad
  before as well, a crease of up to 84 degrees with the rounding). Held
  G1 to the rounding and to the faces, the plate fitted its borders within
  0.0006 at the outer rim but only within 0.05 at the inner (its plate's
  tolerance 0.08); G0 fits both within 0.0007.
- Elsewhere nothing changes. No catch is added.

## Results

On the guide at 45 the twelve feet's plates reach at most 0.0003 out of
the body, within their tolerances (0.0011 to 0.0013); the design recomputes
with every feature's status ok (no warning) at bend radii 16.8, 17, 18, 20,
24, 24.5, 25, 30, 32, 45 and 50, the body within its pads and its volume
on the line through 32 and 45. The rounding takes 0.06 mm3 more over the
twelve corners than before, about 5e-5 of the body; the import of the
FreeCAD document compares the replayed Fillet with FreeCAD's stored body,
whose plates bulge, and finds it 4.6e-5 off (its tolerance is 1e-5).

## Reports and tests

- `geometry.fillet_rib_foot_corner`
  (`geometry/tests/test_fillet_rib_foot_corner.cpp`): a block with an open
  channel and a rib bridging it, the rib's underside a cylinder tangent to
  the channel's along the rim; the edge of the rib's end with its underside
  rounded with 0.5 by OCCT's fillet alone builds valid, takes material
  away, its faces stay inside the body (less than 0.005 out and no farther
  than their tolerances) and the flat past the rib keeps its area. Without
  the patch its plate reaches 0.015 out and the flat loses a piece.
- `cli.fcstd_bend_reported`: the reported design (from
  `MITCAD_MITCAD_REPORTED`, never committed) at every radius it imports:
  every feature's status ok, not a warning.

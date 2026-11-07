# The plate's top edge picked at its other side: 1570 to 1572

mitcad#1422 (GitHub mitcad/Mitcad#173), after mitcad#1359
([fillet-boss-tangent-1330-1332.md](fillet-boss-tangent-1330-1332.md)): the
plate's top edge rounded up to a boss whose cylinder the plate's flat sides
go on into tangentially, picked at the side the earlier tests did not
pick. Back to the [port's README](../README.md).

## Overview

- **1570** runs a seam of the face at a spine's end on from the end vertex
  to the point where the face at the end cut the blend on an edge at that
  vertex (`ChFi3d_KPartCutToEnd`, the stripe ending in a point), as 1331
  does where the corner cuts it.
- **1571** takes that point's parameters on a periodic face at the end in
  the period of the cut's other end, not in the edge's own.
- **1572** makes the piece `ChFi3d_KPartCutToEnd` cuts on the spine's end
  edge run on the rest of that edge's analytic blend's surface, so the two
  are one face, as 1352 does for the piece past the end.

## The case

The part the report built in MitCad's UI: a Center to Center Slot 85 long
and 50 wide extruded 15 (the plate); in the same sketch a circle centred on
the slot's end centre through the end of its side, extruded 30 and joined
(the boss). The boss's cylinder is the plate's end too, one face from the
bottom up, its seam a line from the vertex (85, -25, 15) up (a B-spline
from the faces' join); the plate's flat sides go on into it tangentially
along the vertical edges at x = 85, y = +-25. The plate's top edge along
either side, with Tangent Chain, is one chain round the plate's far end,
ending at both vertices (85, +-25, 15).

Picked at y = 25 it rounds since 1330 to 1332. Picked at y = -25, the side
the report picked, it failed at every radius: `fillet produced an invalid
shape (face: BRepCheck_UnorientableShape)`. The stripe is the same, its
faces taken in the other order. The analytic blend's pieces are then split
with the contact on the top ending where it meets the boss's foot (x =
73.13 at 3), at that section's spine parameter, where the other order kept
the edge's own; so `ChFi3d_KPartCutToEnd` takes those ends: the boss's
cylinder cuts the blend from the foot down to the vertical edge at
(85, +-25, 15 - r), the stripe ending in a point there (FreeBoundary), and
no corner is made (`PerformOneCorner` makes the other order's ends, with
1331).

The two synthetic bodies of 1359 (the cylinder's seam at either vertex)
failed the same way picked at y = -25, through OCCT's fillet alone and
MitCad's; the case body of 1359, the sketch's circles split elsewhere,
built in the corner's order either way.

## 1570: the seam run on to a point end

### 1570-chfi3d-point-end-on-an-edge-at-the-vertex-runs-the-seam-on

Where the cylinder's seam runs up from the vertex, the cylinder grows down
across the seam's line from the vertex to the cut on the vertical edge (the
corner the blend removes lay between the side and the cylinder's other
side); at the corner, 1331's `ChFi3d_SeamRunOnAlongEdge` runs the seam on
to the cut and keeps it. A stripe cut to a point gets no corner, and the
seam stopped at the vertex: the cylinder's face was rebuilt without the
boss above the plate (removing 37388 mm3 at 3) and could not be oriented.

`PerformFilletOnVertex` now, for a lone stripe ending in a point whose two
end points lie on one edge at the vertex (`ChFi3d_SeamRunOnAtPointEnd`),
calls `ChFi3d_SeamRunOnAlongEdge` for each face of that edge; it does
nothing where no seam of the face ends at the vertex along the edge's line,
on the far side of the vertex from the cut.

## 1571: the cut's end in its other end's period

### 1571-chfi3d-kpart-cut-end-on-a-periodic-face-in-the-period-of-its-other-end

`ChFi3d_KPartCutToEnd` gives the cut's end on the face at the end the
parameters of the edge's curve there. With the seam running up at the
other vertex, the vertical edge's curve on the cylinder lies on the seam's
one side (u = 5 pi / 2) and the cut runs from it to the foot on the other
(u = 2.07 at 3, through pi / 2): its curve on the cylinder ran from 7.85 to
8.35, a period off the foot's, and the face's wire did not close in its
parameters (an unorientable face, adding 3471 mm3 at 3). The end's
parameters on a periodic face are now taken within half a period of the
cut's other end, the blend's contact on the edge it crossed into the face
(unless that edge is a seam of the face).

## 1572: the cut piece one face with the rest of the end edge's blend

### 1572-chfi3d-kpart-cut-piece-on-the-end-edge-one-face-with-its-rest

The piece cut on the spine's end edge had a copy of the blend's surface of
its own, so each side's cylinder was two faces where the other pick gives
one; MitCad's fillet joined them, the face named twice as numbered pieces
of its edge. Where the cut piece lies on the end edge itself and its rest
on the same edge runs on the same surface (the test 1352 makes for the
piece past the end), it now takes that surface, and the two are one face.

## Tests

- 1570 to 1572: mitcad#1422, in
  `geometry/tests/test_fillet_boss_tangent.cpp`: the plate's top edge
  picked at y = -25 rounded with 1, 3 and 6, alone and with the boss's top
  edge, through OCCT's fillet alone and MitCad's, on both synthetic bodies,
  the case body of 1359 and the case `slot-boss-tangent-rounding` of
  `MITCAD_GEOMETRY_CASES` (the body MitCad's UI builds from the slot and the
  circle, its fourth listed edge the one picked at y = -25): valid, the
  rounding of the picked edge one face named after it, and the material
  removed as the exact shapes give it within 1e-6 of it. Without 1570 every
  such rounding is invalid; without 1571 those on the synthetic body whose
  seam runs up at y = 25; without 1572 MitCad's results name each side's
  rounding as two pieces.
- The part through the UI: `tools/ui-fillet-boss-tangent-test.sh`
  (`app.ui-windows-fillet-boss-tangent` on Windows).

<!-- SPDX-License-Identifier: MIT -->
# A seam only where the face has it twice, an intersection chain kept whole: 2415

mitcad#2415, GitHub mitcad/Mitcad#642's second design and #645. Back to the
[port's README](../README.md).

## Overview

- **2415** makes two of the offset's choices right:
  1. An edge is a seam of a face only where the face has it twice
     (`BRepTools::IsReallyClosed`), not wherever it has two curves on the
     face's surface. A flat side tangent to a cylinder along the line of
     the cylinder's seam is so no seam, and its offset is trimmed where
     the offsets of the faces across it cut it. Before, Sharp failed ("the
     offset faces do not close up" in MitCad) or built a valid solid four
     times too heavy, at any thickness.
  2. Where the offsets of an edge's faces intersect in a chain of pieces
     on a closed face with a non-elementary offset, the piece nearest the
     edge's middle is kept with the pieces it runs on into, not alone.
     Before, the edge's image stopped short of its end, and the offsets of
     both faces had no face: the closed offset failed at any thickness.

## The cases

### A flat side tangent to a cylinder along its seam (GitHub #642's second design)

A disk (radius 33, z 50..90) with an arm (z 60..80) whose flat sides are
tangent to it, a boss on the arm's round end and the disk's rims rounded
3 mm; the disk's seam lies on one of the lines where an arm's side meets it.
The fuse splits the seam there: the piece between the arm's top and bottom
becomes the edge between the disk and the arm's side, and keeps both curves
of the seam on the cylinder though the disk's face has it once.

`BRepOffset_Inter2d::ConnexIntByInt` walks the cylinder's wire: where two
consecutive edges have no image of their own (a seam piece and that edge),
the offset of their common vertex goes onto both edges' extended offsets as
an end, except where a seam runs on from a tangential edge
(`SeamRunsOnFromTangentEdge`, patch 2050). That test took the tangential
edge for a seam too (`BRep_Tool::IsClosed` finds its two curves), found two
seams, and the vertex went onto the tangential edge's offset: its ends were
taken where the arm's top and bottom are, not where their offsets cut it
(2 mm in), and the offset of the arm's side had no face. Without the
disk's rims rounded the shell closed anyway, with the cavity cut short
(147950 mm3 instead of 36384.7 at 2 mm); with them it gave no result.

### A loft cut to a slot (GitHub #645)

A loft (its side one periodic B-spline surface) cut to a slot with round
ends, its top edges rounded 2 mm. The rounding along the slot's end is one
B-spline surface with a crease in its parametrisation, which MitCad's shell
splits there first (a sliver of 0.4 mm2 is left of it). The offsets of the
slot's round end (a cylinder, enlarged all round) and of the loft's face
below it intersect in three pieces meeting at vertices of their own: one
from the slot's side at +x to a point at x = -12.29, one from there past
the slot's side at -x, and one past the side at +x. `BRepOffset_Tool::Inter3D`
removes "excess edges that are out of range" where a reference face is
closed and an offset is not elementary: it kept the piece nearest the
edge's middle alone, the edge's image ended 4.7 mm before the slot's side,
the corner there was never cut, and the offsets of the slot's end and of
the loft's face got no face. The closed offset failed at any thickness,
Sharp and Rounded; the patch keeps the pieces the nearest one runs on into
(here all three), which the assembly after it joins into one edge.

The report's shell (the loft's top removed) still fails after the patch,
for causes outside it: the kernel's thick solid cannot open a face its
neighbours meet tangentially (the top and its roundings; a box with its top
edges rounded and its top removed fails the same way), and the cavity MitCad
cuts instead (the inward offset with the top moved out by a quarter of the
wall) comes out invalid. mitcad#2415's report names them; patch 2433
([offset-vanishing-beside-removed-face-2433.md](offset-vanishing-beside-removed-face-2433.md))
opens the box with a wall thicker than its rounding and says what the
report's 2 mm shell still lacks.

### Not covered: a crease that turns from concave to convex (GitHub #641, mitcad#2418)

GitHub #641's joined three-way pipe (a loft from a circle of 170 at z = 315
to one of 85 at x = 225, mirrored and joined, the crotch rounded 20) shelled
with its three end caps removed fails in the kernel's thick solid at 5, 2
and 1 mm, Sharp and Rounded, with this patch as without it. The edge where
each loft side meets the top cylinder (the top circle's arcs) is concave in
its middle and convex towards x = 0, the faces tangent at 67.8 degrees round
the circle. There the offsets of the two faces touch, and their
intersection comes as an X of branches at (62.3, +-152.8, 315) at 5 mm: 13
pieces for one arc, among them the concave run from (165, 0, 313.9) to the
touching point and two branches leaving it steeply (to z = 255 and z = 388),
but no run on along the convex part towards (0, 165); the removal of excess
edges then keeps a piece of the mirrored arc's (y < 0), and the offset shell
comes out with 2 faces. A crease that fades to tangency in the middle of its
edge, not at an end (patches 1130, 1190, 2140), is still to be handled.

## 2415: a seam only where the face has it twice, an intersection chain kept whole

### 2415-brepoffset-seam-only-where-the-face-has-it-twice-and-an-intersection-chain-kept-whole

- `BRepOffset_Inter2d.cxx`, `SeamRunsOnFromTangentEdge`: a seam is an edge
  the face has twice (`BRepTools::IsReallyClosed`).
- `BRepOffset_Tool::Inter3D`, the removal of excess edges: where the edge
  nearest the reference edge's middle does not reach past both ends of the
  reference edge (the nearest point of its curve to an end is one of its
  own ends), it is kept with the edges of the list it runs on into through
  common vertices, when they make one simple run (two free ends, no
  branch) along a non-periodic curve; that run alone is then assembled.
  Otherwise the nearest edge stays alone, as before: the line where a
  notch's wall cuts a cone's offset comes in two pieces split by the
  cone's seam plane, the nearest covering the edge, and keeping both hung
  the offset (the lab case 2383-cone-notch-on-ring-top, five notches); a
  circle's pieces are trimmed by patches 2050 and 2383.

## Reports and tests

- `geometry.port.2415` (`p2415-seam-only-where-the-face-has-it-twice.cpp`),
  OCCT alone: the disk with the arm, its seam on a tangent line, plain and
  with the boss and rims, Sharp 2 and 1 mm: one valid solid of the volume
  the same body gives with the seam at +x.
- `geometry.shell_seam_on_tangent_edge`: the same body through MitCad's
  shell, without notes and with the inside of every face's wall.
- `cli.shell_sharp_reported`: GitHub #642's second design, Sharp 2 and 1 mm
  with the boss top removed (43304.5 and 22287.4 mm3 in the lab), and
  GitHub #645's design shelled closed 2 mm (22730.3 mm3: the body, 91667.5,
  less its closed offset, 68937.2), skipped without the designs.
- OCCT lab case `2415-seam-on-tangent-edge-and-slot-loft`: the chain without
  the patch gives 147950 mm3 for the plain body at 2 mm and no result for
  the boss body; with it the volumes of the seam at +x. #645's split body
  (beside the case) offset closed 1 and 2 mm: no shape without the patch,
  79844.0 and 68937.2 mm3 with it.

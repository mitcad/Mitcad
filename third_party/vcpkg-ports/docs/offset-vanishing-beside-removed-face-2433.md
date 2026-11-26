<!-- SPDX-License-Identifier: MIT -->
# A face vanishing beside a removed face, closed by it: 2433

mitcad#2433, GitHub mitcad/Mitcad#645. Back to the
[port's README](../README.md).

## Overview

- **2433** lets a face whose offset vanishes (a rounding narrower than the
  wall, patch 0100) lie beside a face the thick solid removes: the removed
  face closes it there. A box with its top edges rounded 2 mm, shelled
  inside with its top open, Sharp, by 3 or 4 mm: before, no result at any
  thickness ("the offset faces do not close up" in MitCad); now one valid
  solid whose sides' offsets meet the top's plane.
- It does not cover a wall no thicker than the rounding, nor the arc join
  (Rounded): GitHub #645's reported shell (2 mm with 2 mm roundings) still
  fails, see [Not covered](#not-covered-a-wall-no-thicker-than-the-rounding).
  Both are patches 2463 (the arc join) and 2462 (the walls where the wall is
  no thicker than the rounding) since, in
  [offset-removed-face-met-tangentially-2462.md](offset-removed-face-met-tangentially-2462.md).

## The cases

### A box with rounded top edges, its top open

A box 40 x 30 x 20, its four top edges rounded r, the top removed, shelled
inside by w with Sharp corners (the intersection join). With r = 0 the
thick solid builds at any w. With r = 2:

- w > r: the roundings' offsets turn inside out (their radius r - w is
  negative). `FindVanishingFaces` (patch 0100) finds such four-edged strips
  and has the offsets of the faces on both sides meet each other instead;
  but across the rounding's edge to the removed top there is no face of
  the shape without its caps (a free edge), so the rounding was kept, its
  inside-out offset in the way, and the thick solid failed. With the patch
  the top is the face beyond: the offset of the box's side meets the top's
  plane at w from the side, w - r past the top's edge, and the body less
  the cavity (40 - 2w) x (30 - 2w) x (20 - w) comes out (10010.9 mm3 at
  3 mm, 12618.9 at 4).
- w = r: the side's offset meets the top's plane exactly on the top's edge
  (a rim of no width). No result, before and after.
- w < r: the rounding's offset (radius r - w) does not vanish; it lies
  below the top's plane all along and, tangent to it at the edge, never
  meets it when enlarged. `ContextIntByInt` finds no section there and the
  edge gets no image: no result (Sharp), an invalid solid (Rounded: a
  subshape badly oriented), before and after.

### GitHub #645's loft cut to a slot, its top open

The reporter's body (2415's case: a loft cut to a slot with round ends, its
top edges rounded 2 mm, split at the rounding's crease as MitCad's shell
splits it), the loft's top removed: no result at 1, 2 and 3 mm, Sharp and
Rounded, before and after. At 2 mm (the report) and 1 mm the wall is no
thicker than the rounding (above). Why 3 mm still fails is not found yet:
the top is a sloped B-spline surface, and where it turns down towards a
slot's side the side's offset may meet it past the top's edge, which the
patch does not take (it would build the inside of the wall outside the
body); the rounding's faces may also not all be found vanishing (one of
them is the 0.4 mm2 sliver of the crease split).

## Not covered: a wall no thicker than the rounding

What the reporter's shell needs is a wall at the removed face's edges where
the faces round it meet it tangentially: the kept face's offset ends at its
edge's offset (the edge moved along the common normal by the wall), and a
face between that edge and the removed face's edge (a ruled strip along the
normal, as the thickening of an open shell builds at its free edges in
`MakeMissingWalls`) closes the wall. At w = r that strip lies in the plane of
the side's offset and the rounding's offset is a line. The thick solid has
no such walls for removed faces: its caps are bounded by sections with the
offsets alone. The thickening of the same open shell (`thickshell` in DRAW)
gives the box's right volume, valid, with the arc join while w < r (3822.8
mm3 at 1 mm with r = 2), fails or is invalid at w >= r, and is wrong at any
w with the intersection join. That is mitcad#2433's open part.

## 2433: a face vanishing beside a removed face, closed by it

### 2433-brepoffset-face-vanishing-beside-a-removed-face-closed-by-it

- `BRepOffset_MakeOffset.cxx`, `FindVanishingFaces`: takes the edges of the
  removed faces; a vanishing strip whose pair edge bounds a removed face is
  closed by it (no longer "a free or open edge: cannot be closed"), and the
  removed face is the face beyond that edge in the pairs it gives.
- `BRepOffset_Inter3d::ConnexIntAcrossVanished`: leaves pairs with a removed
  face to `ContextIntByInt`.
- `BRepOffset_Inter3d::ContextIntByInt` (a new optional argument, the
  pairs): a removed face's edge whose kept neighbour vanishes is intersected
  with the offset of the kept face across the vanishing faces, as at an
  edge between the two, and the section goes to both edges (the removed
  face's and the kept face's edge to the vanishing faces). Where the
  section's point nearest the edge's middle is not inside the removed face
  (it runs past the edge), the section is not taken.

## Reports and tests

- `geometry.port.2433` (`p2433-face-vanishing-beside-a-removed-face.cpp`),
  OCCT alone: the box with 2 mm roundings, its top open, Sharp 3 and 4 mm:
  one valid solid of the body's volume less the cavity, the corner the
  rounding fills in the wall.
- OCCT lab case `2433-rounded-top-open`: the box with sharp and with 2 mm
  top edges and #645's split body, the top open, Sharp and Rounded, 1 to 4
  mm (the body 1 to 3). Without the patch the rounded box gives no result
  Sharp at any thickness; with it 10010.9 and 12618.9 mm3 at 3 and 4 mm,
  the rest unchanged.
- `cli.shell_sharp_reported` (`shell_sharp_reported645.json`): #645's design
  shelled closed 2 mm, which the reported shell with the top open does not
  yet join.

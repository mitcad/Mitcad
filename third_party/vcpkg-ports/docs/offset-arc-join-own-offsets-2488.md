<!-- SPDX-License-Identifier: MIT -->
# The arc join between faces of different offsets: 2488

mitcad#2488 (a follow-up of mitcad#2443), GitHub mitcad/Mitcad#642. Back to
the [port's README](../README.md).

## Overview

- **2488** lets the offset's arc join (MitCad's Rounded shell) join faces
  that have offsets of their own (`SetOffsetOnFace`) with different values:
  the tube along their edge runs from the one offset to the other, tangent
  to both, and a corner of three such faces is an eighth of an ellipsoid.
  A box offset 2 mm outward with its bottom at 5 mm: before, every face a
  tube or a corner joined to the bottom took the bottom's 5 mm (MitCad
  refused the shell beforehand); now the walls are 2 and 5 mm, rounded from
  one to the other.
- Where such a tube or corner cannot be built (a curved edge other than a
  circle about whose axis both faces turn, a face turning along a straight
  edge, a corner of other than three faces square to each other), the faces
  it joins still take one offset, as before.

## The cases

### A box with its bottom thicker, rounded

A box 60 x 40 x 20 offset 2 mm with the arc join and its bottom at 5 mm, its
top open (a thick solid) or closed (the offset of the whole box).

`UpdateFaceOffset` gave each face a tube or a corner joins to a face with an
offset of its own that offset (`AddFaces` across the tubes' edges, convex
ones for an outward offset), since the tube (`GeomFill_Pipe` of one radius)
and the corner (a sphere) could only join one offset. The box came out with
5 mm walls all round.

With the patch the faces of a tube's edge keep their offsets where the tube
can join them (`BRepOffset_Offset::JoinsOffsets`), and `BuildOffsetByArc`
builds that tube with the new `BRepOffset_Offset::Init(Path, Edge1, Edge2,
Face1, Face2, Offset1, Offset2, FirstEdge, LastEdge)`:

- its section at a point P of the edge, where the faces' outward normals are
  n1 and n2 and their offsets d1 and d2, is the rational quadratic arc from
  P + d1 n1 to P + d2 n2 whose middle pole is where the two offsets'
  tangents in the section's plane meet, weighted cos(a / 2) (a the angle
  between the normals). It is the affine image of the circle of a tube of
  one offset (that circle where d1 = d2), tangent to both offsets at its
  ends; for faces square to each other a quarter ellipse with semi-axes d1
  and d2;
- along a straight edge where neither face turns the section is extruded
  along the edge (`Geom_SurfaceOfLinearExtrusion`), along a circle about
  whose axis both faces turn it is revolved about the axis
  (`Geom_SurfaceOfRevolution`): exact, one parameter from Edge1 to Edge2
  and the other the edge's (u along the section, v along the edge for the
  extrusion, as the pipe's; the revolution's u is its angle, so there the
  face is made as a pipe with `ExchangeUV`); the face is made as the
  pipe's (the old `Init`'s body, now `initTube`).

A vertex all of whose edges have tubes (a corner), round which the offsets
differ, gets the new `BRepOffset_Offset::Init(Vertex, LEdge, Faces,
Offsets)`: where its three faces are square to each other there, the unit
sphere about the vertex stretched along each face's normal to that face's
offset, an ellipsoid through the ends of the tubes and tangent there to the
offsets, as an exact rational surface (`GeomConvert` of a part of the sphere
away from its poles, its poles stretched); the tubes' end edges are
projected onto it and made same-parameter, and the face is made as the
sphere's (the old `Init`'s body, now `initCorner`).

Expected volumes: the thick solid 20000 + 1780 pi / 3 = 21864.0 mm3 (the
2 mm side walls, the 5 mm bottom, the quarter cylinders of radius 2 up the
vertical edges, the quarter elliptic cylinders 2 by 5 along the bottom's
edges, the eighths of an ellipsoid 2, 2 and 5 at its corners); the whole
offset 72800 + 2396 pi / 3 = 75309.1 mm3.

### A wedge: a corner that is not square

The box with its side x = 60 leaning in to x = 50 at the top, its top open,
offset 2 mm with its bottom at 5 mm: at the leaning side's foot the bottom,
the front and the leaning side meet, but the leaning side is not square to
the bottom. `UpdateFaceOffset` (`AddFacesJoinedByArc`) gives the faces round
such a corner one offset, as before: the bottom's 5 mm spreads to the
front, the back and the leaning side; the left side, joined to them by
straight tubes and square corners, keeps its 2 mm, and the tubes and
corners at its edges run from 2 to 5 mm. MitCad refuses such a shell
beforehand (`rounded_together` in `geometry/src/faceops.cpp`), naming the
corner.

### A cylinder's side thicker, rounded

A cylinder of radius 20, 30 high, its top open, offset 2 mm with its side
at 4 mm: the tube along the bottom's circle is the quarter ellipse 4 mm out
and 2 mm down revolved about the axis (18304 pi / 3 + 80 pi^2 = 19957.47
mm3 for the wall, by Pappus).

## Not covered

- A tube along a curved edge other than a circle about the faces' common
  axis (an ellipse where a slanted plane cuts a cylinder, a spline edge), or
  along a straight edge where a face turns (a cone along its slant meeting
  a plane at an angle that changes): its section would change along the
  edge (a sweep of a varying conic); the faces keep one offset.
- A corner of three faces not square to each other, or of more than three
  faces: no affine image of the sphere is tangent to all the offsets there
  (a filling surface would be needed); the faces keep one offset.

### 2488-brepoffset-arc-join-tubes-and-corners-between-faces-of-different-offsets

- `BRepOffset_Offset.hxx`, `.cxx`: the new `Init` for a tube between two
  offsets and for a corner of different offsets; `JoinsOffsets` (static,
  for an edge and its two faces, for a vertex and its faces); the old
  tube's and sphere's bodies as the private `initTube` (a given surface in
  place of the pipe; the tube's new end edges get their iso-curves as 3d
  curves, `IsoCurve3d`) and `initCorner` (a given surface in place of the
  sphere).
- `BRepOffset_MakeOffset.cxx`, `UpdateFaceOffset`: with the arc join, the
  faces given one offset are found by `AddFacesJoinedByArc` (tangential
  edges; tubes' edges and corners only where `JoinsOffsets` is false) in
  place of `AddFaces(..., ChFiDS_Tangential, RT)`. `BuildOffsetByArc`: a
  tube's edge whose faces' offsets differ gets the new tube, a corner whose
  faces' offsets differ the new corner.

## Reports and tests

- `geometry.port.2488` (`p2488-arc-join-between-faces-of-different-offsets.cpp`),
  OCCT alone: the box with its bottom at 5 mm, offset 2 mm with the arc
  join, its top open and whole: one valid solid each of the volumes above;
  the wedge: one valid solid, its left face at x = -2, its front at y = -5.
- `geometry.shell_own_thickness` (`test_shell_own_thickness.cpp`): the box
  shelled Rounded Outside 2 mm with its bottom at 5 mm, closed, and Both
  (1 and 1, the bottom 3 and 2): exact volumes, the walls' planes, and the
  roundings next to the bottom and the front measured tangent to them (the
  normals of both faces at three points of each common edge within a
  microradian); the cylinder Rounded outside; the wedge refused.
- `cli.shell_own_thickness` (`shell_own_thickness.json`, Shell7): the
  block shelled Rounded outside, its volume and its walls measured.
- OCCT lab case `2488-own-offsets-rounded`: the box (top open, closed,
  inside, outside 1 with the bottom 2, no own offset), the cylinder (top
  open, closed) and the wedge, each on a fresh shape (the offset alters its
  input: a closed offset of a box just offset as a thick solid gives no
  shape, with or without the patch). Without the patch every face takes
  the bottom's 5 mm (the box open 37759.6 mm3, whole 101948.0; the cylinder
  23327.3 and 67766.2; outside 1 with the bottom 2: 13696.4, the wedge's
  left side at 5 mm); with it 21864.0, 75309.1, 19957.5, 61026.4 and 9181.2
  mm3, the expected volumes, all valid, the wedge's left side at 2 mm; the
  box inside and without own offsets unchanged. The lab's other 52 cases
  unchanged; OCCT's offset group: none broken or fixed (681 same, 131
  failing in both, one mass at round-off).

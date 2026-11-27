<!-- SPDX-License-Identifier: MIT -->
# A groove that ends inside a flat face, and a crease split at a sphere's pole (2542)

mitcad#2542, GitHub mitcad/Mitcad#720. Follow-up of
[2140, 2383 and 2418](offset-crease-split-2383.md). Back to the
[port's README](../README.md).

## Overview

GitHub #720's design is a disk of radius 72 cut flat at y = +-48, 60 high,
with a channel of radius 5 along y at the bottom and, on each flat side, a
groove of radius 5 revolved from half a slot: a cylinder from the bottom up
to z = 32 ending in a quarter sphere whose pole lies on the flat side's
edge; the top's long edges are rounded 12. Shelled 2 mm inside with both
round ends removed, Sharp (the intersection join), it failed with an
invalid shape (`BRepCheck_BadOrientationOfSubshape` on a face), and so did
the design's own 3 mm shell.

- **The two sides of a groove share one intersection.** Both edges where
  the flat side meets the groove's wall (x = +-5) are between the same two
  faces. `BRepOffset_Inter3d::ConnexIntByInt` intersects a pair of offset
  faces once, with the first of its edges, and `BRepOffset_Tool::Inter3D`
  keeps the branches along that edge: of the two lines where the groove's
  offset (radius 7) meets the side's (x = +-6.708), only the first edge's
  came back, and the second edge got it too. The groove's offset closed no
  loop and was left out, and the side's offset ran straight through the
  groove. With a flat end the thick solid was valid but 3158.7 mm3 short on
  a box: the walls round the grooves missing, the groove's own surface
  passing through the inside of the wall (a solid the checker passes).
  Where the groove runs the full height the side is two faces and each line
  is a pair's own: that shelled right.
- **The crease split at a sphere's pole.** The edge where the side meets
  the groove's quarter sphere passes through the sphere's pole, which
  splits it in two (a degenerated edge of the sphere between them). The
  pair's intersection is one whole circle; patch 2418's rule gave the second
  half a copy of its own, and nothing cut the copies at the pole: the two
  images are one curve there, and their 2D intersection is the whole curve,
  no point. Each copy kept the other half's arc too, and the side's offset
  came out a disc bounded by the full circle: the invalid face.

## 2542: the pair intersected again for each edge, the copies cut at the pole

### 2542-brepoffset-crease-split-at-a-sphere-pole-cut-there

- `BRepOffset_Inter3d::ConnexIntByInt`: where an edge's face pair was
  intersected already with another of its edges, the pair is intersected
  again with this edge, and the branches whose points across from the
  edge's ends and middle do not all lie on the pieces the edge already
  has (`AlongAPiece`) are added as this edge's own. In the redistribution
  of 2140 and 2418, those branches compete as the nearest piece to the
  edge's points, so the edge takes no copy of another edge's piece where
  its own branch is nearer. Nothing changes where the second intersection
  gives nothing new (a circle that is already there, pieces of one curve).
- `BRepOffset_Inter2d::ConnexIntByInt`: where two consecutive edges of a
  face are one crease split at a vertex (both between the face and one
  other face, no other edge there but degenerated ones: `CreaseRunsOn`) and
  their images are two edges on one curve (`MeetOnOneCurve`: the same
  curve, or every sampled point of each on the other), both get the vertex
  where the curve passes nearest to the vertex, with the orientations the
  intersection would give; `TrimEdges` then cuts each copy to its own arc.

## The cases

The OCCT lab's case `2542-channel-meets-groove` (shapes without locations,
the grooves' seams on their far side, as MitCad's revolve of half a slot
makes them), 2 mm inside, Sharp, both ends open:

| Body | Expected wall | Before | After |
|---|---|---|---|
| box 120 x 96 x 60, grooves the full height | 73775.4 | 73775.3, valid | the same |
| the box, flat-ended grooves | 73605.4 | 70446.7, valid (walls missing) | 73605.4, valid |
| the box, ball-ended grooves | 73565.0 | invalid (a disc) | 73565.1, valid |
| GitHub #720's body | 74600.0 | invalid (a disc) | 74599.8, valid |

The expected walls are the body less the cavity of the points 2 mm or more
inside every kept face: by hand for the boxes, and by the closed offset 2 mm
inside with the open ends' offsets 0 for the report's body (74600.0 mm3;
3 mm: 110814.0, the thick solid 110813.0).

Not covered: a shape whose faces carry a location (a translated sphere): the
offset's input check (`CheckInputData`) evaluates the surface without the
face's location but compares its points with the degenerated edge's vertex
with it, so a translated sphere whose pole is a vertex of the face is
refused as having a degenerated normal (`BRepOffset_BadNormalsOnGeometry`).
The report's error was the invalid shape, not this refusal.

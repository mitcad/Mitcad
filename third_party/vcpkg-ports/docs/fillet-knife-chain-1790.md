# A knife edge running on into an ordinary edge on its line: 1790

mitcad#1988, the case [fillet-knife-edge-1770.md](fillet-knife-edge-1770.md)
left: a knife edge followed on the same line by an edge whose faces meet at
an angle. Back to the [port's README](../README.md).

## Overview

- **1790** refuses, in words and at the vertex, the corner of OCCT's
  fillet where the ball of a knife edge's rounding jumps to the corner of
  the edge running on along its line; before it, the corner of more than
  three edges built a valid body of the wrong shape there.

## The case

A block 130 by 210 by 40 with a bite cut 20 deep from below into its front:
the cylinder of radius 65 about the front edge's midpoint, tangent to the
side walls. Along the line where a side wall meets the front, the wall
meets the bite's cylinder tangentially below the bite's end plane (a knife
edge, the wedge between them of 0 degrees) and the front face square above
it: one tangent chain of two edges, which MitCad's Tangent Chain takes
whole. The ball of a rounding of radius r lies two radii into the wedge
along the knife edge (sqrt(260 r) from the front) and in the corner along
the square edge: it jumps at their common vertex.

OCCT ends both contours at that vertex (the faces of the two edges do not
go on tangentially there), each a stripe of its own. The vertex has four
edges and two stripes, and `PerformMoreThreeCorner` filled it with a plate:
a valid body with a surface far from the edges and, at r = 1, 6611 mm3 more
volume than the body asked for. That body has the end plane grown under
the knife's tip up to the square edge's rounding and a new edge along the
wall between the two roundings' contacts, which no corner of the fillet
builds. Each edge alone: the knife edge rounds (1770: its stripe ends on
the end plane, which grows over the tip's section); the square edge alone
leaves a face of the end plane facing both ways and is invalid.

## 1790: the corner refused

### 1790-chfi3d-corner-where-a-knife-edge-runs-on-along-its-line-refused

- `ChFi3d_Builder::RefuseKnifeEndsOnLine`, from `PerformFilletOnVertex`
  for two stripes at a vertex of more than three edges, before
  `PerformMoreThreeCorner`: where each stripe has one end there, their
  edges go on along one line (within the angular tolerance), one is a knife
  edge and the other not, and they have one face in common, it throws
  `Standard_Failure` with "a knife edge running on along its line into an
  edge whose faces meet at an angle: the ball jumps at the vertex between
  them". The corner fails at that vertex as any corner does: the vertex is
  faulty, `FailedCornerMessage` gives the text, and no body is built.
- `ChFi3d_IsKnifeEdge` (`ChFi3d_Builder_0`): the faces' normals sampled
  along the edge, with the faces' orientations, all within a thousandth of
  a radian of opposite.
- No catch is added; elsewhere nothing changes.

## MitCad's rounding of the chain

`knife_line_apart` (`geometry/src/dressup.cpp`), beside
`concave_after_convex` and `overlap_apart`: where the fillet failed with
that corner, the selected edges of the contours at the vertex that are not
knife edges are rounded after the others, on their result. The knife
edge's rounding ends on the end plane, which grows over the tip's section;
the square edge's then ends on that plane as at any corner of three edges.
The result is the body asked for: valid, its volume the block's less the
tip's section and the corner's, each times its length. Where that fails
too, the fillet is refused with the corner's cause in words ("an edge where
two faces meet tangentially (a knife edge) runs on along its line into an
edge whose faces meet at an angle, and the rounding's ball jumps there ...")
and the vertex as its place (a corner's faulty vertices are now the places
of every corner failure).

## Reports and tests

- `geometry.fillet_knife_chain`: the block with the bite, the two edges in
  one fillet of OCCT's with 1 and 3: not done, the corner failed at the
  vertex on the bite's end plane with the message above (before 1790: done,
  the wrong body); the knife edge alone valid with the exact volume;
  MitCad's fillet of the square edge with its tangent chain valid, one
  solid, its volume the exact shape's within 1e-6 relative, at 1 and 3.
- `cli.fillet_knife_chain`: the block sketched and cut, the square edge
  rounded with its tangent chain from the model, the exact volume.
- OCCT's own blend and chamfer groups (DRAW, 754 cases, against the port
  with 1727): no case changed with 1728 to 1730, 1770 and 1790.

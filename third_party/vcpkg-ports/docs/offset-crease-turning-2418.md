<!-- SPDX-License-Identifier: MIT -->
# A crease turning inside its edge, a crease tangent at both ends, a vanishing tip: 2418

mitcad#2418, GitHub mitcad/Mitcad#641 (follow-up of mitcad#2384). Back to
the [port's README](../README.md).

## Overview

- **2418** lets the offset's intersection join (Sharp) build walls where:
  1. a crease turns from concave to convex inside its edge, its faces
     tangent at a point between: the branch along the crease is kept on
     both sides of the point and joined across it, the branch crossing it
     there dropped;
  2. a crease (one edge) flattens to tangency at both its ends: only the
     branch into the edge is kept at the offsets of its ends;
  3. a face vanishes to a point at a vertex (a rounding whose crease fades
     out there) where a further face touches it at that point only: the
     vanishing face's offset is cut by the further face's offset.

  Before, the thick solid was not done ("the offset faces do not close up"
  in MitCad) and the closed offset neither.

## The case: a three-way pipe (GitHub #641)

Loft1 from a circle of radius 170 at z = 315 (on the z axis) to one of 85 at
z = 0 centred at x = 225 (an oblique cone, its side a B-spline parametrized
by arc length), a cylinder of 170 pulled 60 up from its top and one of 85
pulled 15 down from its bottom, mirrored about x = 0 and joined (patch 2384
makes the join keep both arms), the crotch rounded 20. The Shell removes the
three end caps, 5 mm inside.

The loft's ruling at angle t round the top circle runs from
(225 + 85 cos t, 85 sin t, 0) to (170 cos t, 170 sin t, 315): its radial
component at the top is 85 - 225 cos t. The rim where the loft meets the top
cylinder is concave where |t| < 67.8 degrees (cos t = 85/225) and convex
beyond, the faces tangent at 67.8 degrees; the bottom rim the same. The
analysis gives each edge the kind at its middle.

### 1. The X at a turn

`BRepOffset_Inter3d::ConnexIntByInt` intersects the enlarged offsets of the
loft and the cylinder once for the pair (the top rim is two edges, split at
the loft's seam). The offsets are tangent at the offset of each turning
point, (62.333, +-152.773, 315) at 5 mm, and their intersection there is an
X: the branch along the rim (concave part dipping to z = 313.94 at t = 0,
convex part rising to 315.66 at t = 90 degrees and running on round the far
side of the periodic loft) and a branch crossing it, running up to the
enlarged cylinder's top (z = 387.6) and down to its bottom (z = 255), joined
by pieces of 1 to 8 micrometres. The convex part was a piece of its own that
ended 5 micrometres from the knot; on the mirrored side the section line ran
from the crossing branch into the rim's branch without a vertex, 19
micrometres off the tangency. `CheckIntersFF` took the connected block
nearest the edge's middle (the concave part with the crossing branch) and
the removal of excess edges (closed surfaces) kept the piece whose middle
lay in the direction of the nearest point: the crossing branch's piece
running down to z = 255, for both edges.

`JoinCreaseAcrossInnerTangency` (`BRepOffset_Tool.cxx`) finds the turns on
the pair's crease edges (the sign of the faces' normals' cross product along
the edge, as the face's boundary runs; a root by bisection where the faces
are tangent, not folded back), splits pieces that run through a tangency,
drops at each tangency the knot's pieces and those leaving it across the
crease (the chord over the first half millimetre; their tangent at the very
end turns off in the knot), joins the two pieces along the crease in one
vertex at the tangency (its tolerance their distance, some micrometres),
and keeps the pieces connected to them. The pieces past a turn are oriented
in the offset faces as of the other kind. Rebuilt pieces get a trimmed copy
of their curve: two pieces of one closed curve of intersection shared a
vertex at the loft's seam, which keeps one parameter per curve, and TrimEdge
then ran one of them back over the crossing branch.

The convex part runs from one edge's end round the far side to the other
edge's end. Patch 2140's distribution gave each piece to the edge nearest
its middle (here equally far from both); now an edge keeps too the pieces
nearest its own points, as a copy of its own, so that each copy is trimmed
at its own edge's neighbour (one piece trimmed at both kept the run round
the far side). `TrimEdges` keeps a piece inside a chain of an image's pieces
(the convex part of the bottom rim, between its two joints), which nothing
meets and which was removed as a piece without intersections.

### 2. The crease between the two lofts (without the rounding)

The crease where the loft meets its mirror image runs in the plane x = 0
from (0, 170, 315) down to the crotch and up to (0, -170, 315), the sides
tangent at both ends (patch 2384). `DropCrossingsPastTangentEnd` takes a
crease with one tangent end only; with both, the offsets are tangent at the
offsets of both ends, (0, +-165.173, 316.303) at 5 mm, where the crease's
branch meets its continuation past the end (up to z = 386) and a branch
round the top (the two lofts' extensions cross along the whole top circle).
The block kept was the knot's piece of 2 micrometres. Now only the branch
into the edge is kept at both ends.

### 3. The rounding's tip

Fillet5's rounding vanishes to a point at (0, +-170, 315), where both lofts
and the cylinder meet; the cylinder touches it only there. The offsets of
the two lofts meet the cylinder's offset at (+-0.059, 165, 315.665), the
rounding's offset ends at the offset of the vertex (0, 165.173, 316.303),
0.64 mm past the cylinder's offset. No edge of the shape joins the rounding
and the cylinder, so their offsets were never intersected, and the loops of
the loft's offset could not close. Now the rounding's offset (enlarged) is
intersected with the cylinder's, and the piece nearest the tip, from
(0.059, 165, 315.665) to (-0.059, 165, 315.665), is the image of the
rounding's degenerated edge there, an edge of both faces' offsets; in the
cylinder's offset the images of the two rim edges at the vertex meet it,
not each other (`BRepOffset_Inter2d::ConnexIntByInt`).

## Results

The OCCT lab's case `2418-pipe-shell` (the thick solid as MitCad's Shell
calls it: tolerance 1e-6, no complete intersection, the three end caps
open):

| body | join | without 2418 | with 2418 |
|---|---|---|---|
| reported (crotch rounded 20) | Sharp 5 mm | not done | 2.51223e6 mm3, 15 faces, valid |
| reported | Sharp 2 mm | not done | 1.01575e6, 15 faces, valid |
| reported | Rounded 5 mm | 2.5118e6, 21 faces, valid | the same |
| without the rounding | Sharp 5 mm | not done | 2.52132e6, 13 faces, valid |
| without the rounding | Sharp 2 mm | not done | 1.0185e6, 13 faces, valid |
| rounded by the kernel's fillet | Sharp 5 mm | not done | 2.5131e6, 15 faces, valid |

The closed offsets 5 and 2 mm inside (MitCad's cavity) of both bodies are
valid solids too. Without the rounding, Rounded 5 mm gives an invalid solid
before and after (not this patch's).

## 2418: a crease turning inside its edge, a crease tangent at both ends, a vanishing tip

### 2418-brepoffset-crease-turning-inside-its-edge-joined-across-the-tangency-and-a-vanishing-tip-cut

- `BRepOffset_Tool.cxx`, `JoinCreaseAcrossInnerTangency` (new; `Inter3D`
  calls it before `DropCrossingsPastTangentEnd`, its pieces taking the
  cut pieces' way out of `Inter3D`, those of the other kind with their
  orientations reversed): the turns, the end tangencies of a crease tangent
  at both ends, the split, the dropped and joined pieces, the trimmed
  copies of the pieces' curves (above).
- `BRepOffset_Inter3d::ConnexIntByInt`: the pieces nearest an edge's own
  points (16 inside it) are kept by it too, as a copy of its own
  (`MakeSplitEdge`, stored for both faces as the piece is there) where
  another edge is nearest the piece's middle; after the post-treatment, the
  tip of a face vanishing to a point (above), its piece stored with
  `Store` and bound as the image of the degenerated edge.
- `BRepOffset_Inter2d::ConnexIntByInt`: two built edges meeting at a vertex
  where such an image is among the face's edges meet it each.
- `BRepOffset_MakeOffset.cxx`, `TrimEdges`, `InsideChainOfPieces`: a piece
  inside a chain of an image's pieces gets its joints as its ends.

## Reports and tests

- The reporter's design: `cli.shell_reported`
  (`tools/cli/tests/shell_reported641.json`: the Shell Sharp 5 mm, and with
  Fillet5 suppressed; skipped without the design).
- `geometry.port.2418`: the pipe built from MitCad's loft and the kernel's
  cylinders, union and fillet, shelled by the kernel alone
  ([patch tests](opencascade-patch-tests.md#2418)).
- The OCCT lab: `2418-pipe-shell` (before and after),
  `2418-offset-group-samples` (OCCT's offset cases whose logged result
  changed in one group run: the same with and without the patch).

## Known limits

- A turn is joined only where exactly one piece along the crease leaves the
  tangency on each side; a crease turning back and forth within a tenth of
  a millimetre, or one whose X has a crossing branch running along it
  (within 25 degrees), is left as before.
- The vanishing face is cut only where its other edges at the vertex are
  tangential and the further face's edges at the vertex have images.

# An offset crease that fades where its faces go on smoothly: 2140

mitcad#2140, GitHub mitcad/Mitcad#517: a Y tube shelled inwards. Back to
the [port's README](../README.md).

## Overview

- **2140** keeps only the branch along a crease in the intersection of its
  faces' offsets where the crease flattens to tangency at an end at which
  its faces go on smoothly (G1) into a further face. Before, patch 1190
  took such an end for the end of a wall at a removed face and joined the
  branch to its continuation past the tangency, and the thick solid raised
  an exception without a message.

## The case

A circle of radius 10 swept along a line of 25 (up the z axis) and a
tangent quarter arc of radius 30 (turning towards -x), mirrored about the
plane x = 0 and joined: a cylinder below z = 25 and two quarter tori, the
arms, above it. The arms meet along a concave crease in the plane x = 0
from (0, -10, 25) over (0, 0, 51.46) to (0, 10, 25), two edges split where
both tori's seams (their outer equators) pass. At both ends the arms are
tangent to each other and to the cylinder: the crease's angle fades to
nothing there, and the tori run on smoothly into the cylinder along the
circle at z = 25.

Shelled 2 mm inwards with the three ends removed (Intersection join), the
crease's edge is intersected as a convex one: the offsets of the two tori
(minor radius 8), enlarged past their faces, are intersected by the pave
filler. They cross along an oval in the plane x = 0, of which the half
above z = 25 is the crease's offset and the half below its continuation,
and they touch along the whole circle of radius 8 at z = 25, where both
are tangent to the cylinder's offset. All these pieces meet at the
tangencies (0, ±8, 25), so `CheckIntersFF` keeps them all.

`DropCrossingsPastTangentEnd` (patches 1130 and 1190) handles creases that
flatten to tangency at one end. It found the tangency on the plane of the
torus's boundary there (the circle at z = 25), took it for a wall's end at
a removed face, and joined the branch and its continuation into one edge
that runs down into the cylinder's offset: the tori's offsets could not be
trimmed by it, and `MakeThickSolidByJoin` raised an exception without a
message (MitCad said "the geometry kernel's offset raised an error").

## 2140: the branch alone where the faces go on smoothly

### 2140-brepoffset-crease-fading-into-a-smooth-face-keeps-its-branch-alone

- `BRepOffset_Tool::Inter3D` takes the analysis of the shape
  (`BRepOffset_Analyse`, an optional argument, null by default), which
  `BRepOffset_Inter3d::ConnexIntByInt` passes, and gives it to
  `DropCrossingsPastTangentEnd`.
- There, where the tangent end's vertex has an edge of the first face,
  other than the crease and not its seam, that the analysis finds
  tangential (`ChFiDS_Tangential`) between that face and a face other than
  the second one, the faces go on smoothly past the end: the walls go on
  there as that face's offset, which meets the crease's offset at the
  tangency. The branch is then not joined to its continuation. It is
  kept alone, with the edges it goes on into towards the edge's other end
  (as 1130 keeps it past a solid's end), wherever the tangency lies, and
  cut at the tangencies of the crease's ends: of every edge common to both
  faces (the pair is intersected once for all of them), each end where the
  faces' normals are parallel within 1e-4 and the faces go on smoothly,
  its point on the offset surface. An edge of the branch passing within
  20 % of the offset of a tangency is cut at its nearest point there, and
  of its parts the one whose middle is nearest to the crease's edges is
  kept; `Inter3D` takes those pieces, with their curves on both offset
  faces. Where no boundary at that end is curved across the edge, the
  direction out of the edge orders the branch's pieces. Ends at removed
  faces and at sharp edges are handled as before.
- With `MITCAD_TRACE_OFFSET_CREASE` set, the function prints the crease's
  ends and its faces' angles there, the pieces of the intersection,
  whether the end is smooth, the branch taken and what it keeps or cuts.
- Runs so far: before the cut, the branch kept whole ran past both
  tangencies down to the enlarged faces' end (z = 9.71, two edges split at
  the crotch's top, the trace), and the arm's inner wall was a face whose
  wire does not close (an invalid shell; with OCCT alone, not done). With
  the cut, the pieces run from the crotch's top to (0, -8, 25) and
  (0, 8, 25) as they should, but the thick solid is not done
  (`BRepOffset_UnknownError`: fewer faces than the body's, some offset
  faces not rebuilt), on MitCad's body and with OCCT alone. The offset's
  images then: the arms' inner walls are the whole enlarged tori, not
  trimmed (their seams' offsets do reach the crotch's top); the edges
  where the arms meet the cylinder (tangential) have wrong offsets, one
  without vertices and the two on the other arm's side both from
  (0, -8, 25), as if the cylinder's offset took the vertex at (0, 10, 25)
  for the one at (0, -8, 25). The cut pieces had vertices of their own at
  both ends, also at the crotch's top that the uncut edges share; they now
  keep the edge's own vertex where they are not cut. With that the thick
  solid is done, the cylinder's offset and the edges where the arms meet
  it right, one arm's inner wall whole and valid; the other arm's inner
  wall was the loop of the crease's pieces and the cylinder's edge alone,
  without its seam and end (invalid: not closed across the seam). The
  pieces' curves on the offset faces had been built afresh, each on a
  side of the seam of its own; they are now the pave filler's edges' own
  curves, trimmed. That left the run's result as it was (the arm's wall
  the same loop); the trace showed the curves taken and on the right sides
  of the seam. The cause is ConnexIntByInt's: both crease edges got both
  pieces as their images, and Inter2d, at the crease's vertex where the
  seam goes on, met the seam with the other half's piece; the seam's
  offset was then dropped from that arm's wall. Each crease edge now keeps
  the pieces nearest to it.
  With that, OCCT's own Y tube (`geometry.port.2140`) shells right; on
  MitCad's mirrored tube the left arm's wall is whole and the right arm's
  lost its seam instead. `Inter2d`'s meetings of a seam with an
  intersected edge are now traced too.
- `BRepOffset_Inter2d::ConnexIntByInt` walks a face's wire on the face
  oriented forwards but took the images' curves on the offset face as
  oriented; `BRep_Tool::CurveOnSurface` swaps a seam's two curves on a
  reversed face, so on the mirrored arm (a reversed face) the crease piece
  was intersected with the seam's curve on the other side of the period,
  no vertex came of it, and the arm's inner wall lost its seam (the trace:
  the piece at v = 0 against the seam at v = 2 pi). Where a seam takes
  part, the images are now intersected on the offset face oriented
  forwards.

## Reports and tests

- `geometry/tests/port/p2140-crease-fading-into-a-smooth-face.cpp` (ctest
  `geometry.port.2140`): the Y tube made of OCCT primitives (a cylinder and
  two quarter tori, fused), shelled 2 mm inwards with its three ends open:
  one valid solid, the wall 2 mm thick by points in the trunk, the arms and
  under the crotch, and the volume of the tube less the same tube of
  radius 8, the largest tolerance within 1e-3: passes.
- `geometry/tests/test_y_tube_crotch.cpp` (ctest `geometry.y_tube_crotch`):
  the same through MitCad's sweep, mirror and shell (the mirrored arm a
  reversed face), without and with the crotch rounded with 6 and 50; and
  `cli.y_tube_crotch_reported`, the reporter's design: both pass with this
  patch and 2142, 2143.

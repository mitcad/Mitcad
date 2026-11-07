# A rounding run on past its end over both faces' surfaces: 1580, 1581

mitcad#1425, a follow-up of mitcad#1398 (found among the regenerated
rounding specs, core/import/docs/rounding-specs.md): a wall's top and
bottom edges rounded with 5, each chain ending past a concave corner of
the wall where the stored body runs the blend on over both faces'
surfaces and cuts it with the faces it runs into. OCCT's standard end is
the same construction (the blend past the spine's end vertex, cut by the
faces round the vertex, `PerformIntersectionAtEnd`); here the walk turned
away from it and the walk round the vertex could not reach it. Back to the
[port's README](../README.md).

## Overview

- **1580** ends a contact that has run off the spine's faces and leaves
  its face past the spine's end, into a face that goes on from it and
  does not hold the end vertex, when the other contact has already left
  its face into a face at the end: the
  blend is walked on over both faces' surfaces, and the faces at the end
  are found round the vertex from the first face along the way that holds
  it.
- **1581** computes an end's cut on a face at the end again on that face's
  own domain where the surfaces meet in a loop and the way round outside
  the face was taken.

## The case

A bar's front wall (a plane) runs from a boss, which it meets through a
concave rounding of 3 (a torus), to a concave corner rounded with 3 (a
cylinder about the vertical) that turns it into the side of a tip. The
tip's top and bottom edges are rounded with 4, a rounding that goes on
round the corner up to the wall as a corner patch tangent to the wall, the
corner's cylinder and the tip's rounding. The bar's top and bottom slope
by 2.2 degrees to the tip's and meet them along creases. Each chain
rounded with 5 runs along the torus's edge with the top (where the torus
comes tangent to the top, the rounding vanishes), the wall's edge with it
and the corner patch's edge with it up to the crease, a vertex of the
corner patch, the bar's top and the tip's top.

The ball's contact on the wall goes on round the corner's cylinder below
the corner patch (a radius of 5 against the patch's 4), not onto the
patch. Its contact on the top crosses the crease 0.7 before the vertex,
its contact on the cylinder the cylinder's edge with the tip's side 3
past it. In the stored body after the rounding, the blend is one face
from the end of the wall: the ball rolling between the cylinder and the
top's plane, both continued past their faces (each section's centre 8
from the cylinder's axis and 5 under the top's plane within 0.001 mm),
cut by the tip's top beyond the crease, the corner patch (a sliver of
0.36 mm2 is left of it), the tip's rounding of 4 and the tip's side.

## 1580: the run-on and the faces round the end

### 1580-chfi3d-blend-run-on-past-the-end-over-both-faces-cut-by-the-faces-round-the-end

Before, the walk past the crease ran the top's contact on over the top's
plane unclassified (OCCT's own end handling), then, where the cylinder's
contact reached the tip's side, went on along the side with the other
contact standing on the crease; the corner at the vertex took that last
piece's faces, walked along the side's boundary for the vertex and threw
("the max number of faces reached").

- `ComputeData`: on the walk on past the spine's end, a contact that
  reaches an edge of its face into a face that goes on from it
  (`SearchFace`), neither face holding the spine's end vertex (the contact
  has run off the spine's faces), more than 10 `tolesp` past the spine's
  end, after the other contact left its face into a face at the end, ends
  on its edge as well, and the walk goes on with both sides unclassified
  up to the guide's end. Where the contact's face holds the vertex, it
  meets the face at the end there, and 1430's roll on the other contact's
  edge builds the end as before (`ChFi3d_WalkedOnPastEndEdge`; the notch's
  lip of mitcad#1389, `geometry.fillet_end_edge_turn`).
- `ChFi3d_CutRunOnExtension` (new, in place of
  `ChFi3d_CutSingularExtension` at such an end): the line is kept past the
  later contact's edge while its contacts run at most eight times as fast
  along the guide as at the spine's end, measured along the guide's
  length. Past the end the ball's centre turns round the cylinder's axis
  while the guide's normal planes do not, up to the section where they
  come tangent to the centre's curve (singular). The guide's extension at
  the first end was not parametrised by length (its derivative grew from
  1 to 4), so the plain criterion (twice as fast per unit of parameter)
  cut the bottom chain's line short of its own contact's edge; the cut
  needs the blend up to where it leaves the body, a tenth of a radian
  short of the singular section.
- `PerformIntersectionAtEnd`, a vertex of three faces with side 1's edge
  away from the vertex: where F1 (the cylinder) does not hold the vertex,
  the walk along F1's boundary from that edge stops at the first face that
  does (the corner patch, reached after the tip's rounding, which
  `InsertFacesBetween` finds between the side and the patch at the
  cylinder's top vertex), and the faces at the end go on round the vertex
  from there to the face beyond side 2's edge (`RoundVertexToEdge`): the
  tip's side, its rounding of 4, the corner patch, the tip's top.

## 1581: the cut on a face the way round inside it

### 1581-chfi3d-end-cut-on-a-face-the-way-round-inside-it

The blend's surface and the tip's rounding of 4, each bounded by the box
`PerformIntersectionAtEnd` takes about the two points, meet in a loop
through both; `ChFi3d_ComputeCurves` took the way round outside the
rounding's face, past its edge with the corner patch, and the rounding's
face grew over the blend's (73.7 mm2 against the file's 55.2, 0.27 mm off
the stored body). Where the curve computed on a face at the end that is
not prolonged has its middle outside the face, it is computed again on the
surface bounded by the face's own domain (a hundredth of it wider) and
taken where its middle lies inside the face.

## Results

On the design's stored body before the item, both chains at once: valid
and closed, the stored body's 127 faces (each end's blend one face, the
corner patch's sliver kept); about each end the result on the stored body
and the stored body on it within 0.0007 mm, along each chain within
0.0036 mm (the torus's vanishing ends, where the file's own faces are
fitted); the change of volume -200.2687 against the file's -200.2866 mm3
(9e-5). Through MitCad's dressup (`mitcad-rounding-specs`): NEAR, the
change 5.6e-6 from the file's. Before, the corners at both ends failed.

## Reports and tests

- `geometry.fillet_torus_side_ends`
  (`geometry/tests/test_fillet_torus_side_ends.cpp`): with
  `MITCAD_GEOMETRY_CASES`, the case `fillet-torus-side-ends`, the stored
  body before and after the rounding: the six edges rounded with 5, valid
  and closed, the stored body's faces, the result on the stored body and
  back within 0.01 mm about each end and along each chain, the change of
  volume within 1e-3 of the file's. Fails without 1580 (the corners fail)
  and without 1581 (the tip's rounding 0.27 mm off). No synthetic body:
  the corner patch is the end of an earlier rounding that fades to the
  wall tangentially, which OCCT's own fillet does not make.

<!-- SPDX-License-Identifier: MIT -->
# Whole circular rims rounded to a pole or round every face (2582)

mitcad#2582, GitHub mitcad/Mitcad#741. Back to the
[port's README](../README.md).

## Overview

GitHub #741's three designs round whole circular rims with a radius at
which the rounding takes a face away completely:

- a cylinder 10 across and 10 high, its end rim rounded with 5, the
  cylinder's own radius: the rounding is a hemisphere, its contact on the
  end face the face's centre;
- then the face at the other end rounded with 5 as well: a ball of 5, the
  cylinder left between the two rims as high as the radius taken away too;
- an annulus 40 across with a bore of 20, 10 high, its four rims rounded
  with 5 in one set: a torus of radii 15 and 5, every face of the annulus
  taken away (the top and bottom between the outer and the inner rim's
  roundings, the outer and the bore's walls between the top and the bottom
  rim's).

OCCT gave an invalid solid for the first (the end face kept whole beside
the hemisphere), failed the second and the third, and MitCad built each
0.1 % smaller with a warning that it leaves a sliver of a face.

## 2582: the cap's sphere, its pole as the other contact, rides on both sides, the blends alone

### 2582-chfi3d-whole-rims-rounded-closing-a-cap-to-a-pole-or-taking-every-face-away

- `ChFi3d_KParticular`: a plane/cylinder rounding at a convex circle whose
  cylinder has the ball's radius went to the walk since mitcad#1412 (a short
  arc between a lid's top and an earlier vertical rounding of the ball's
  radius, which the walk takes with the edges beside it, the ball standing
  still over them). Where the spine is closed and every edge of it is an arc
  of that one circle (`ChFi3d_SpineWholeCircle`), the plane's face is a cap
  inside the circle and ChFiKPart's plane/cylinder sphere (`c1sphere`), its
  contact on the plane the cap's centre, is the rounding. The walk had run
  to that centre and given a contact of no length on the plane, which the
  topological build made an inner wire of the cap, kept whole.
- `ChFi3d_ContactsAlongFaceEdges` (0014, 0140): the second rim's sphere
  touches the cylinder left between the rims along the first hemisphere's
  edge, and its other contact is the pole, a point with no curve. The
  contact along the edge was skipped for want of the other contact's curve,
  and the cylinder came out as a face of no area; the other contact's point
  is now the blend surface's at its p-curve, and the contact is stored on
  the edge, the cylinder dropped (`ChFi3d_CutConsumedFaces`).
- `ChFi3d_FilDS`: on the annulus, the roundings meet each other on the
  faces they consume, and ApplyRides (1720) makes the contacts one line and
  sets each such side's face index to 0. The outer bottom rim's rounding has
  both sides so, and the transitions of its curves were composed with no
  face: it came out reversed, split along its seam into two faces of no
  area. They are composed with the orientation of the face of the spine's
  edges that holds the side's contact (`ChFi3d_SpineFaceHoldingContact`:
  side 1's, else side 2's), the face the transition was computed on.
- `TopOpeBRepBuild_Builder::SplitSolid`: with no part of the annulus's
  faces kept, the four tori were added to the solid's face set as elements
  only, and the solid builder grows its shells from start elements: no
  shell, and ChFi3d's Failure_NoResult (1701). Where the solid leaves no
  start element, the faces of the data structure's surfaces are the start
  elements; where any part of a face is kept, nothing changes. Only fillets
  and chamfers put surfaces into the data structure.

## Reports and tests

The OCCT lab's case `2582-whole-rims-at-their-radius`:

| Case | Expected | Before | After |
|---|---|---|---|
| cylinder 5 x 10, top rim 5 | 654.498469 | invalid, 773.498012, 4 faces (the cap kept) | 654.498469, valid, 3 faces |
| then the bottom rim 5 | 523.598776 (a ball) | failed (on the invalid input) | 523.598776, valid, 2 faces |
| the rim in two half arcs | 654.498469 | valid, 654.498569 (walked) | 654.498469, valid, 5 faces |
| a boss 5 x 10 on a plate, its rim 5 | 2254.498469 | invalid, 2507.321699, 9 faces | 2254.498469, valid, 8 faces |
| annulus 40 / 20 x 10, four rims 5 | 7402.203301 (750 pi^2) | failed | 7402.203301, valid, 4 tori |
| annulus 30 / 10 x 10, four rims 5 | 4934.802201 (500 pi^2) | failed | 4934.802201, valid, 4 tori |
| annulus 40 / 20, both bottom rims and the top outer one, 5 | 7776.947272 | invalid, 3206.591696, 6 faces | 7776.947272, valid, 5 faces |
| a cube of 10, its twelve edges 4.9998 (mitcad#1922) | 523.6305 | no result (Failure_NoResult) | 523.630190, valid, 8 faces |
| annulus 40 / 20, the bottom outer rim and both top ones, 5 | 7776.947272 | 7776.947272, valid | the same |
| the cylinder's rim and the annulus's four rims at 4.99 | | valid | the same |
| a cylinder under a spherical cap, its rim 5 (the pole) | | failed | the same (not covered) |

Tests: `geometry.port.2582` (OCCT alone, the cases above),
`cli.fillet_whole_rims` (the three designs made from commands, each at its
size without a note) and `cli.fillet_whole_rims_reported` (the reporter's
designs, skipped without them). The lab's shared set (57 cases): only this
case differs. OCCT's groups blend, chamfer and offset against the chain
without the patch: 1353 same, 167 failing in both, nothing fixed, broken,
changed or random.

Not covered: a rim between a cylinder and a spherical cap (a cap of radius
10 through the rim of a cylinder of radius 5) rounded with 5, so that the
ball touches the cap at its pole only: the rounding between a cylinder and
a sphere about one axis is walked (no analytic one), and OCCT fails at the
exact size; 0.1 % smaller it leaves a spherical disc at the pole, and
MitCad warns that it leaves a sliver (`cli.p9_warnings`). Built at their
size with the patch in the lab (valid, no face under 0.01 mm2; not
compared without it): a frustum's
top rim rounded so that the ball's centre lies on the axis, a hexagonal
prism's top edges rounded with its inscribed radius, a blind hole's bottom
rim rounded with the hole's radius, a cube's edges rounded with half its
side (the ball of that radius).

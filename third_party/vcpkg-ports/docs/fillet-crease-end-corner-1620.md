# A rounding whose edge ends where a convex crease of its face begins: 1620

mitcad#1434, the follow-up of mitcad#1426: a rim rounded up to a vertex
where its cylinder meets the face at the end along a line, at a convex
crease. Extends 1220's ball turning about a crease at the end
([fillet-corners.md](fillet-corners.md#1220-the-ball-turning-about-a-crease-at-the-end))
to a crease that begins at the end's vertex. Back to the
[port's README](../README.md).

## Overview

- **1620** closes such an end, where the cut of the rim's torus by the
  face at the end would leave that face, by rolling the ball on over the
  crease while it touches the end plane (a horn torus about the crease)
  until its centre lies in the plane of the face at the end. Where the
  cut stays on the face, the face cuts the torus as before.

## The case

Two tubes joined: one along x (outside 30 about the line y = 52, z = 15,
the end planes x = -15 and 15) whose outside is cut by the plane
y = 26.0511 (Fk) where it meets the other, the cut leaving a strip of Fk
along each line L where Fk meets the outside (z = -0.0551 and 30.0551,
along x). The crease at L is convex, 30.1 degrees. Item 8 of the
`.f3d` design `6eccefc2` of mitcad#1398's rounding specs (Fillet3) rounds
the outside's rims on the end planes with R = 1.5: four arcs, each ending
on an end plane (Fop) at a vertex V of L. The rounding's contact on the
outside reaches L exactly at the section through V, at P, R from V (the
section contains L, which is square to Fop); its contact on Fop is still
inside Fop there (it reaches Fop's edge with Fk 5.7 degrees further
round).

The stored body after the item ends each rim's torus at V's section and
closes the corner with a horn torus about L through P (major and minor
radius 1.5, 1.342181 mm²): the ball rolled on over L, touching Fop, its
centre turning about L by 59.9 degrees (90 less the crease) until it lies
in Fk's plane, where its contact on Fop reaches Fop's edge with Fk, R from
V; Fk's corner at V is cut along the ball's great circle there (a quarter
circle tangent to L and to Fop's edge, Fk 0.482854 mm², R² (1 - pi / 4),
smaller at each corner).

OCCT's rounding is an analytic torus on the spine prolonged past V
(`ExtentOneCorner`), split by the faces' domains: its end piece runs on
until Fk's plane cuts it, from P to Fop's edge with Fk, and the corner
(`PerformOneCorner`) left that end standing out of the body past Fk (the
torus reached y = 22.43): invalid, 25 faces against the file's 29. That
cut leaves Fk: Fk is a strip there (between L and the other tube, which
lies beyond Fk's plane), and the cut runs into the other tube's material.

The same design's other tube (outside 30 about the z axis, cut by the
plane y = -10) has its rims rounded with 1 by item 3 (Fillet2) up to the
same kind of vertex, at a crease of 70.5 degrees; there the cut of the
rim's torus by the plane y = -10 stays on that face, and the stored body
has the torus cut by it, as OCCT makes it (an edge between the torus and
the plane from L to the plane's edge with the end plane). So the file
turns the ball about the crease only where the face at the end cannot
cap the end.

## 1620: the ball turned about a crease at its end vertex

### 1620-chfi3d-ball-turned-about-a-crease-at-its-end-vertex-into-the-end-face

`PerformPivotAboutCreaseAtEnd` (1220) took only a vertex past L; where
the vertex was L's own end, the faces beyond L cut the end as before. It
now also takes that vertex where:

- Fk, the face beyond L, is a plane through L (the face at the end);
- P lies R from the vertex on L (the contact on Fop a circle about the
  vertex);
- the crease between Fs and Fk is convex and short of a right angle (at a
  right angle the section is Fk's plane and the end is cut there as
  before); 1220's limit of 30 degrees stays for a vertex past L;
- the end piece runs on past V's section and is cut by Fk there (an
  analytic blend: its contact on Fop ends on Fop's edge with Fk, P on the
  section), and that cut leaves Fk (`ChFi3d_CutLeavesFace`: points of the
  cut between its ends, found from the chord by projecting on the blend's
  surface and on Fk's plane in turn, outside Fk). Elsewhere (the cut on
  Fk, or a walked end at the section) Fk cuts the end as before.

The ball turns from the section towards Fk's side of L (tk, the direction
in Fk's plane square to L at less than a right angle to the ball's centre
from P) by the angle between them, 90 degrees less the crease; the
corner stripe is 1220's: the horn torus's piece about P, its side on Fk
the torus's tube circle in Fk's plane from P to Fop, and its contact on
Fop the circle about the vertex up to Fop's edge with Fk. That circle on
Fk is built directly (a section of the torus by a plane through its axis,
at its apex, is not found by `BRepAlgoAPI_Section`), as a circle over the
parameters 3 pi / 4 to 5 pi / 4 with its curve on the torus a line in one
B-spline span over them: a circle reverses its parameter as 2 pi - t, a
line as -t, and the corner at a first end turns its curves.

The end piece is cut back to the section when the end is built: the contact's point on the section is
taken from its curve, and its interference's parameter and the piece's
spine parameter are set there.

On the stored body before the item: valid, the file's 29 faces, every
face of the result on the stored body after the item and every face of it
on the result within 1e-6 mm (the horn tori the file's: about the same
lines, 1.342181 mm² each), the material taken 80.030374 against the
file's 80.030203 mm³ (2e-6 of it); before, invalid with 25 faces and the
file's tori 0.167 mm off. On the replay's body of the design the item's
new faces are the file's within 1e-6 mm, and item 3's rims are as before
(the cut stays on its face).

## Reports and tests

- `geometry.fillet_crease_end_corner`
  (`geometry/tests/test_fillet_crease_end_corner.cpp`): a cylinder of 30
  cut by a plane parallel to its axis at a crease of 30 and of 45 degrees,
  a block joined below the cut face leaving it a strip at the corners
  (1 wide along the lines, 0.1 along the end planes), its rims rounded
  with 1.5: valid, the rims' two tori and four horn tori about lines
  through points R from the vertices, each of area
  R² (pi / 2 - psi) (pi / 2 - 1), the cut face R² (1 - pi / 4) smaller
  at each corner (without the patch: invalid). Without the block the
  tori cut by the cut face, as before the patch. With `MITCAD_GEOMETRY_CASES`: the case
  `tube-end-crease-corners`, the stored bodies before and after the item:
  the result valid with the stored body's faces, on it and it on the
  result within 0.01 mm, the material taken the stored body's within
  0.01 mm³. Each part fails without the patch.

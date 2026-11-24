# A face's outline rounded past a cylinder's rims tighter than the ball: 1590

mitcad#1426, the follow-up of mitcad#1398: a cut face's outline rounded
past the rims of a tube rounded with less than the radius. Extends 0970's
corners tighter than the ball (a cylinder between two planes, in
[fillet-replay-0950-0980.md](fillet-replay-0950-0980.md)) to a cylinder's
rim (a torus between a plane and the cylinder) and to an obtuse wall,
closed by the rim's own ball as 1380 closes a concave corner
([fillet-three-face-corner-1380.md](fillet-three-face-corner-1380.md)).
Back to the [port's README](../README.md).

## Overview

- **1590** splits a convex chain at a rim's torus tighter than the ball
  as at 0970's corner cylinder, and closes the corner by the side the
  other wall's edge with the face U leans to: where it is acute (or
  square), with 0970's vertex blend and torus, the other wall now a
  cylinder about the rim's axis; where it is obtuse, with the rim's own
  ball rolled on between the other wall and the first rounding (a strip)
  and then between the two roundings (a triangle), 1380's construction
  with the balls inside the material. The ends at an obtuse corner walk on
  past their vertex, and a prolonged end of a split chain is not cut by
  the face at the end.
- **2291** (mitcad#2291, GitHub mitcad/Mitcad#592) tells a rim's corner
  obtuse by the other wall's normal where the corner's ball touches it
  (e0), not where the wall meets U: about a cylinder the two differ, and
  a cut-out whose axis lies in U is square to U along its line but leans
  over U at the ball ([2291](#2291-chfi3d-tight-rim-corner-obtuse-by-the-walls-normal-at-the-ball)).

## The case

A tube (bore 25, outside 30, 30 long) is cut by the plane y = -10 (U);
its four rims are rounded with rk = 1 (tori of major radius 29 and 26).
The outline of each cut face is then rounded with R = 1.5: one tangent
chain of eight edges, U's lines on the end planes, the outside and the
bore, and its arcs on the rims' tori (`6eccefc2` item 11 and `c23a90e7`
item 12 of mitcad#1398's rounding specs, the second design's tube 20 and
25). No ball of R rolls round a rim of rk: OCCT walked the chain over the
torus and threw while storing the walk (the rational sections' weights),
on the replayed and on the stored body.

The stored body after the item, at each rim:

- the bore's rims (the bore's edge with U acute, its outward normal
  leaning 23.6 degrees towards U's): the rounding along the end plane
  (a cylinder of 1.5 along x) and the one along the bore (a cylinder of
  1.5 along the tube's axis) end where their axes cross, at the ball C
  touching U, the end plane and the bore; a vertex blend (a cubic
  B-spline, 2.435348 mm²) spans from C's contact on U to the arc of the
  rim's radius about C + (R - rk)(end plane's normal + direction along
  the end plane's rounding), and a torus about the bore's rounding's
  axis (major radius R - rk = 0.5, minor rk, 0.582979 mm²) turns the
  rim's meridian to the end plane's rounding: 0970's construction with
  the bore in the place of the leaning plane;
- the outside's rims (the edge with U obtuse, 109.5 degrees): the ball C
  touching U, the end plane and the outside lies clear of the rim (its
  centre 0.71 from the rim's tube circle, inside it); the end plane's
  rounding runs on past the corner to the rim's meridian through its
  contact on the end plane, the outside's down to z = 1.0228; a strip
  (0.523751 mm²) and a triangle (1.619144 mm², its apex C's contact on
  U) close the corner. Every point of both is rk from a centre R - rk
  from the end plane's rounding's axis and, on the strip, rk inside the
  outside, on the triangle, R - rk from the outside's rounding's axis:
  the rim's own ball rolled on from the rim's meridian between the
  outside and the end plane's rounding, then between the two roundings,
  down to where their contacts on U meet.

## 1590: the rim's corners

### 1590-chfi3d-tight-corners-at-a-cylinders-rim-and-at-an-obtuse-wall-closed-by-the-rims-ball

`ChFi3d_TightArc` (0970's) also takes K a torus between the plane Wp
square to the chain's other line and a cylinder Wo about K's axis whose
line on U runs along that axis (K the rounding of Wo's edge with Wp, its
tube circle rk inside both), and marks a corner obtuse where the other
wall's outward normal at its line on U turns towards U's outward normal
(a plane or the cylinder Wo). `SplitAtTightCorners` splits such chains
too; before, an obtuse wall was left as it was.

`PerformTightCorners`, where Wo is such a cylinder, finds C on the line R
inside U and rw -+ R from Wo's axis (rw Wo's radius, minus where Wo is
convex), on its line's side, R inside Wp; e0 is Wo's outward normal
there, the same all along the rounding Lo; the check that K's axis passes
through the start of T's spine circle becomes that K's tube circle does;
a torus K's seams with the walls are circles with both ends on U, so the
part of them taken away is told by their direction at the point; K is
cut along its meridian at e0. The vertex blend and the torus are 0970's.
An obtuse corner goes to `PerformTightObtuseCorner`.

`ExtendAtTightObtuseCorners` (new, after `ExtendAtTightConcaveArcs`)
lets the ends at an obtuse corner walk on past their vertex by two radii:
the end plane's rounding's contact on it has to reach the rim (0.505
past the vertex in the case). `PerformSetOfKPart` no longer cuts a
prolonged end of status `ChFiDS_Tangent` by the face at the end
(`ChFi3d_KPartCutToEnd` and its variants marked it a free boundary, and
the corner found no end to close): that end's corner cuts it.

`PerformTightObtuseCorner` (new) is 1380's `tcBuildCorner` with the
balls inside the material: the heights are taken along U's inward
normal, the ball of R's centre and the apex rk into the material, the
supports' sides reversed; K's edges with the walls may be any curves
(a torus's seams): where a contact reaches them is found on the curve,
the strip's first section is the rim's ball at the end plane's
rounding's contact on the rim (its contact on the outside taken from the
solved section, where K's sections are not at one height), and the part
of each seam taken away is told by its direction. The strip lies on the
outside and the end plane's rounding (the wall whose contact reaches the
rim lower, as in 1380), the triangle on both roundings; the end plane's
rounding ends on the strip's and the triangle's contacts, the outside's
on the triangle's.

On the first design's stored body before the item: valid, the file's 73
faces, the result within 0.0057 mm of the stored body about the outlines
both ways (the vertex blends: the file's are cubic B-splines; the strips,
triangles and tori within 0.0004 mm), the material taken 83.349390
against the file's 83.349888 mm³; before, the fillet failed. (Those two
volumes were integrated with shells that did not close: the three
bodies' volumes moved by 0.087 to 0.144 mm³ with the point they were
integrated about, 100 mm away. With port patch 2284 they move by 3e-5,
and the material taken is 83.361347 against the file's 83.346460 mm³:
OCCT's rounding holds 0.0149 mm³ less than the file's, whose vertex
blends lie up to 0.0057 mm from OCCT's.) On the
replay's bodies of both designs the item is parametric and verified
(the second design's final body the file's within 7e-7 of its volume,
82 faces as the file's). A block's bottom outline rounded with 0.5 past
vertical corners rounded with 0.3, its front wall leaning 15 and 30
degrees away from the bottom (obtuse): valid, the corners the corner's
own ball (before, invalid at 15 degrees and failed at 30).

## 2291: obtuse at the ball's contact

### 2291-chfi3d-tight-rim-corner-obtuse-by-the-walls-normal-at-the-ball

GitHub mitcad/Mitcad#592 (mitcad#2291): a block 30 long, 10 high and 20
deep whose top meets a concave quarter cylinder of radius rw = 10 (a
cut-out down to the bottom), the cylinder's axis lying in the top's
plane. The end face's edge with the cut-out is rounded with rk = 2 (K, a
torus about the cut-out's axis), then the end face's edge with the top
with R = 3; the chain runs on round K's arc on the top and along the
top's edge with the cut-out. With R = 2 for both the corner was clean
(no tight arc: the ball stays put round K's arc).

1590's `ChFi3d_TightArc` takes the arc (K between the end face Wp and the
cut-out Wo, coaxial) and told the corner by Wo's outward normal at its
line on the top: square to the top's, so the acute corner was built. The
ball C touching the top, the end face and the cut-out lies at
(sqrt(13² - 3²), 7, 17); the cut-out's normal there (e0) turns
R / (rw + R) = 3 / 13 towards the top's normal. Wo's rounding then runs
from the top's normal to e0, 13.3 degrees short of the direction along
the end face's rounding (ep) where the vertex blend's arc lies (in the
plane through C parallel to the top). The probe of the reporter's design
(0.70.0, its faces and edges by the model's queries): the cut-out's
rounding was carried past its contact with the cut-out (a face of
0.0063 mm², edges of 0.027 and 0.70 mm), and the torus (major radius
R - rk = 1, minor rk, 1.663 mm², its sweep 13.3 degrees) ran from ep back
to e0 over the part of K that is kept, its tube circle 0 to 0.03 mm off
K's: the surface folded at the corner, with a notch at each end of the
corner patch in the reporter's picture.

Where Wo is a cylinder (KTorus), `ChFi3d_TightArc` now sets Obtuse from
e0 as `PerformTightCorners` finds it (C on the line R inside U and
rw - sO R from Wo's axis, sO +1 where Wo is convex):
e0 . n = sO xU / (rw - sO R), xU the height of C's line over Wo's axis
along U's normal; where C has no such meridian the rule at Wo's line on
U stays. The corner is then 1590's obtuse one: K's own ball rolls on
between the cut-out and the end face's rounding (from K's meridian where
that rounding's contact on the end face reaches K) and then between the
two roundings, down to where their contacts on the top meet. At the tube
of 1590's case the two rules agree (the bore acute, e0 . n = -0.61; the
outside obtuse, +0.63); planar walls (0970) are not changed, e0 being
Wo's normal there.

## Reports and tests

- `geometry.fillet_tight_rim_corner`
  (`geometry/tests/test_fillet_tight_rim_corner.cpp`): the tube of the
  case, its cut faces' outlines rounded with 1.5: valid, the four tori of
  area rk phi ((R - rk) pi / 2 + rk), the strips' and triangles' areas the
  stored body's within 2e-4 mm² each and every point of them on the rim's
  ball within 1e-3 mm; the block with the obtuse front wall at 15 and 30
  degrees: valid, its corner faces on the corner's ball. With
  `MITCAD_GEOMETRY_CASES`: the case `tube-rim-tight-corners`, the stored
  body before and after the rounding, the result on the stored body and
  back within 0.01 mm, each body's volume its closed shell's (moving by
  at most 1e-3 mm³ with the point it is integrated about, patch 2284),
  the material taken the stored body's within 0.02 mm³ (0.0149 measured;
  the limit was 0.01 mm³ until patch 2284 closed the shells, mitcad#2284).
  Each part fails without the patch.
- `geometry.port.2291`
  (`geometry/tests/port/p2291-tight-rim-corner-obtuse-at-the-ball.cpp`):
  the block of 2291's case built with OCCT alone, the cut-out's rim
  rounded with 2, then the top's edge with 3: valid, free of
  self-interference, no face under 0.01 mm², the corner's faces on the
  surface K's own ball sweeps within 5e-4 mm and on the ball by the
  centre along their normals within 2e-3 mm (B-splines fitted within
  1e-4 through the ball's sections, their normals' error times rk = 2);
  with 2 for both, valid and clean. Each corner face is measured against
  the balls of its own part of the corner (the strip's or the
  triangle's): the other part's balls past their junction reach into it
  (0.21 mm nearer than rk). In DRAW the strip lay on its balls within
  2.7e-5 mm and the triangle on its within 9.6e-5 mm; without the patch
  the rounding was invalid and self-interfering.
- `cli.fillet_tight_rim_corner_reported`
  (`tools/cli/fillet-tight-rim-corner-reported-test.cmake`): the
  reporter's design (never committed, read from `MITCAD_MITCAD_REPORTED`),
  Fillet2 ok without a warning, no face under 0.01 mm², and with 2 for
  Fillet2 as clean as before.

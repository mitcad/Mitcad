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
against the file's 83.349888 mm³; before, the fillet failed. On the
replay's bodies of both designs the item is parametric and verified
(the second design's final body the file's within 7e-7 of its volume,
82 faces as the file's). A block's bottom outline rounded with 0.5 past
vertical corners rounded with 0.3, its front wall leaning 15 and 30
degrees away from the bottom (obtuse): valid, the corners the corner's
own ball (before, invalid at 15 degrees and failed at 30).

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
  back within 0.01 mm, the material taken the stored body's within
  0.01 mm³. Each part fails without the patch.

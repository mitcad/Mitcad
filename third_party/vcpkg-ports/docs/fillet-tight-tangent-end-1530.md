# A rounding's end past a tangent face tighter than the ball: 1530

mitcad#1413, the follow-up of mitcad#1397: a concave rounding whose walk's
contact crosses a tangent edge onto a face of the spine's end that bends
towards the ball more sharply than the ball. Not 1380's class
([fillet-three-face-corner-1380.md](fillet-three-face-corner-1380.md)):
no corner is closed by a smaller ball here; the blend runs on and is cut.
The nearest existing ends are 0160's walk over a rounded rim
([fillet-walks-and-contours.md](fillet-walks-and-contours.md)), 0074's
walked blend cut by the face at the end (`WalkedCutToEnd`,
[fillet-edge-overflow.md](fillet-edge-overflow.md)) and 0531's parts
ending in points ([chamfers.md](chamfers.md)). Back to the
[port's README](../README.md).

## Overview

- **1530** stops a walk where its contact crosses a tangent edge from a
  face F along no edge of the spine onto a face B along the spine that
  holds the spine's end vertex and that no ball of the radius can touch;
  `WalkedCutToEnd` then rolls the ball on over F's surface unbounded,
  cuts that piece by B, section by section from the tangential crossing,
  and ends the stripe in a point on the spine's last edge. No corner is
  made at a vertex where every stripe ends in a point of its own short of
  it.

## The case

A hub (a cylinder U of radius 22.75 about z) stands on a flange (the plane
z = 22). Holes of radius 1.5 at 24.25 from the axis touch the hub, each at
a vertex T on the hub's foot. Each hole's rim is rounded with 0.5: a torus
F30 (R 2, r 0.5) round the hole and, on each side of T, a B-spline face
F47 between the torus, the hole's wall and the hub, whose edge E152 with
the hub is the foot's arc from the flange's end A up to T; F47 is tangent
to the torus across the edge E110 (normals within 0.035 degrees). The
hub's foot is rounded with 1 (`d498ea9d` and `eecf90bd` item 63, nine
edges in three tangent chains between the holes, each chain ending with
an arc like E152). Along E152 the angle between F47's normal and U's falls
from 90 degrees at A to 0 at T. F47 is a saddle whose normal curvature
along the walk is 1.0065 where the contact reaches it: no ball of 1 rolls
on it.

OCCT's walk ran along the foot with its contact on the flange, then on the
torus, up to E110 at (3.83877, 22.75796, 21.72192); `StartSol` reframed
onto F47 (a face along the spine, which 0160's ballCannotTouch test leaves
to the walk) and the next section failed: the contour failed (status 2) on
the stored body and on the replay's alike.

The stored body after the item:

- one B-spline face from the section where the contact on the flange
  reaches the torus to a point P = (3.10129, 22.53762, 22) on E152, 0.29
  short of T: every ball centre along its contact on U lies 1 from U and
  1.5 from the torus's centre circle, also past E110, where F47 is (the
  ball of 1 rolls between U and the torus's own surface);
- F47 cuts it from the crossing on E110 down to P, where the cut meets
  the contact on U; E152 stays a sharp edge from P to T (normals 17.2
  degrees apart at P);
- the same on each side of each of the two smaller holes: four such ends.
  The chains' other ends, at the larger holes, roll on the larger rims'
  tori up to the hole's tangent line as OCCT's walk does.

## 1530: rolled on, cut, a point at the end

### 1530-chfi3d-walk-onto-a-tight-tangent-face-at-the-end-rolls-on-cut-to-a-point

`ChFi3d_TightTangentFaceAtEnd` (new) recognises the end: a constant-radius
rounding's piece whose contact on F ends on an edge E (not at a vertex)
into B, E tangent between F and B, B holding the spine's end vertex the
walk heads for and bordering an edge of the spine, F bordering none, and
`ballCannotTouch` (0160's test: B bends towards the ball more sharply than
1/R in some direction) at the crossing.

- `StartSol` does not reframe onto such a B (no neighbouring face, as
  0160's faces beyond), and `PerformSetOfSurfOnElSpine` ends the walk in
  that direction there, before the tangent plane of the spine's ends
  (`ChFi3d_BuildPlane`) or 0160's bridge (a walk forward from a start
  inside turns back to its start, as at its end).
- `WalkedCutToEnd` takes such a B as it takes a face at an angle: from the
  stripe's last piece it walks on with F's surface unbounded and the other
  face U as it is, up to where the contact on U reaches U's edge with B at
  the end vertex (E152, at P), and cuts that piece by B
  (`ChFi3d_KPartCutPiece`) from the crossing on E to P; the stripe ends in
  the point P (a free boundary), E152 kept from P to T.
- The cut starts where the blend and B are tangent (the contact crosses
  onto B tangentially), where GeomInt_IntSS's line stops 0.006 mm short of
  the crossing and the cut's p-curve on the piece ran past its end
  section. `ChFi3d_CutBySections` (new, `ChFi3d_KPartCutPiece`'s
  `BySections`, both ends given) finds it on 47 sections of the walked
  surface, closer together towards both ends: on each the first sign change
  of the distance to B's surface from the other contact's side, refined by
  bisection, with its foot on B (located from the last one); the p-curves
  on the piece and on B are interpolated through the points and their
  feet, by chord length, and the 3d curve built from the one on the piece.
- `PerformFilletOnVertex` makes no corner at a vertex where every stripe
  ending there ends in a point of its own short of it (T, where the
  chains on both sides end): before, two such ends went to
  `PerformSingularCorner`, which failed.

On the stored body before the item, the nine edges at once: valid, the
four ends' points P within 1e-5 mm of the stored body's vertices; about
the holes the result on the stored body within 0.0015 mm and the stored
body on it within 0.0015 mm (F47's remainders 0.247197 mm² against the
file's 0.247156); the change of volume 2.554215 against the file's
2.548359 mm³ (integrated to 1e-7). Each end has two faces where the stored
body has one: the walked piece up to the section through the crossing on
E110 and the cut piece after it (1.547501 and 0.052383 mm² against the
file's 1.599656). Before, the contour failed.

## Reports and tests

- `geometry.fillet_tight_tangent_end`
  (`geometry/tests/test_fillet_tight_tangent_end.cpp`): with
  `MITCAD_GEOMETRY_CASES`, the case `hub-foot-tight-rim-ends`, the stored
  body before and after the rounding: the nine edges rounded with 1,
  valid and closed, the material added the stored body's within 0.5 %, the
  result on the stored body and back within 0.01 mm about both holes, and
  a vertex of the result within 0.01 mm of each of the stored body's four
  end points. It fails without the patch (the contour fails). OCCT's own
  fillet cannot make the earlier roundings of a hole's rim that touches the
  hub (on a hub and flange of OCCT's solids it throws in the topological
  build), so there is no synthetic body.

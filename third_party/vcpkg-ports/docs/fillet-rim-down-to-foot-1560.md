# A boss's rim rounded down to its foot's rounding: 1560

mitcad#1419, the follow-up of mitcad#1413: a boss's top rim rounded with a
radius that, with the foot's earlier rounding, takes the boss's whole
height, so that the two roundings meet along the foot rounding's edge
where the foot has one. Builds on 0014's contacts along a face's edges in
[fillet-edge-overflow.md](fillet-edge-overflow.md). Back to the
[port's README](../README.md).

## Overview

- **1560** splits an analytic blend's contact where it runs along an edge
  of its face and goes on inside the face past that edge's ends, so that
  the face keeps no strip of no width along the edge; and an end of a line
  hatched on its face's boundary (0014) keeps its own parameter on the arc
  it stops on.

## The case

A cylindrical boss of radius 5 stands 2 high on a plate that covers only
part of its foot; past the plate's side the boss's wall runs on down along
that side. The foot, an arc where the plate is, was rounded with 0.5 (a
torus); the top rim, a whole circle, is then rounded with 1.5 (`d498ea9d`
and `eecf90bd` item 62, Fillet21). The rim's contact on the wall is the
circle 1.5 below the top, 0.5 above the plate: along the foot rounding's
edge on the wall where the plate is, and inside the wall past that edge's
ends.

The stored body after the item: the rim's torus (R 3.5, r 1.5), its
contact circle split at the ends of the foot rounding's edge, which it
shares with the foot's torus; the wall keeps only its part past the
plate's side below the circle (25.929328 mm²); the material taken is the
whole rim's rounding, 14.152911 mm³ (the formula
2 pi (5 - c) 1.5² (1 - pi/4), c the centroid's distance from the corner,
gives 14.152809).

OCCT's analytic blend (`SplitKPart`) is one piece along the whole circle:
0014 hatches a line lying on its face's boundary a little inside the face,
and the circle is one domain there. The wall was cut along the whole
circle and kept the strip between the circle and the foot rounding's edge,
of no width: its wire held both, intersecting itself
(`BRepCheck_SelfIntersectingWire`), and the shape was not valid. The
closed contact's start on the wall's seam was cut at the parameter of the
line hatched inside the face, 1e-4 above the contact, so that the seam's
piece went on past the contact's point and the wall's wire had a gap. The
material taken was 14.147664 mm³ by OCCT alone on the stored body before
the item (5.2e-3 short), and 14.125564 mm³ in the import's result (the
dressup's repair of the invalid shape), 0.027 short (mitcad#1419).

## 1560: the contact split at the edge's ends

### 1560-chfi3d-rim-contact-split-where-it-runs-along-a-foot-roundings-edge

`edgeEndOnContact` (new) finds a vertex V of the contact's face strictly
inside the contact, on it, where exactly one of the face's edges at V (not
of the spine) lies wholly on the contact, the edge's other vertex on the
contact too, and the contact past V away from that edge (halfway to the
contact's end or the next vertex on it) inside the face.

`ChFi3d_ContactsAlongFaceEdges` splits the SurfData at such a vertex, as
it does at 0014's inner vertices of a chain of edges along the contact
(`ChFi3d_SplitAtVertexOnLine`): one surface, lines of their own. The part
along the edge is then a contact along a whole edge, stored as a blend on
that edge with the face beyond it (the foot's rounding); the others run
inside the face up to the vertices. Both roundings meet along the edge,
and the wall keeps only its part past the plate's side.

`FillSD`: an end of a line hatched on its face's boundary (0014's
`HatchOnBoundary`) that stays on an arc away from the arc's vertices
(`SnapToVertex` found none within its tolerance) takes the parameter of
its own point on the arc, not that of the line moved inside the face.

On the design's stored body before the item: valid, the material taken
14.152982 mm³ against the file's 14.152911 (5e-6 relative), every new face
on the file's within 1e-6 mm. The wall's remaining part comes as two faces
split at its old seam, of 24.745720 and 1.183608 mm², the file's one face
of 25.929328 mm² (the import's result has the file's faces). In the
import, item 62's result lies on the stored body after the item within
8e-9 mm (3e-6 before) and its volume 5e-5 mm³ off the file's.

## Reports and tests

- `geometry.fillet_rim_down_to_foot`
  (`geometry/tests/test_fillet_rim_down_to_foot.cpp`): the boss on a plate
  whose side cuts its foot (at two places), the foot rounded with 0.5 or
  0.8 and the rim with the rest of the height: valid, and the material
  taken the rim's rounding's all round within 1e-6 mm³. Fails without the
  patch (not valid, 5.2e-3 mm³ short).

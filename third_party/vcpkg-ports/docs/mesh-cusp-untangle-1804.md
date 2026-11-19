# A face's wire with a cusp meshed: 1804

mitcad#2022, a follow-up of mitcad#2018 (port patches 1801 to 1803): the
bent guide imported from a FreeCAD document, its pads' ends and its ribs
rounded with 0.5, exported as STL at the finest settings. Back to the
[port's README](../README.md).

## Overview

- **1804** untangles, before a face is meshed, the polygons of two
  consecutive edges of its wire that leave their common vertex in nearly
  the same direction (a cusp) and cross each other near it: the face's
  frontier no longer crosses itself, and the face gets its triangles.
  Before, such a face of a fillet's corner plate was left without a
  triangulation at the finest settings.

## The case

The guide's corner plates (`PerformMoreThreeCorner`, where a rib's sloped
edges are rounded down to the flat's rims) pinch to a point at each of
their points on a sharp edge and at the stripe's end on the contact that
goes on tangentially into the tube: at such a point the two borders of the
plate leave in nearly the same direction, 0.2 to 0.5 degrees apart in the
plate's parameter plane. An STL export at a deviation of 0.002 mm with an
angle of 5 degrees (bend radius 24.5 and 25) or 2 degrees (32), or 0.0005
with 2 degrees, failed: "a face could not be triangulated", a corner plate
(no walk over a vertex) left without triangles.

BRepMesh discretises each edge on its own: the stripe's end section, an
arc, finely; the plate's border along the rim, nearly straight, in a few
long segments. Near the cusp the long first segment of the border cut
across the section's polygon (179.6 degrees between the two segments). The
face checker leaves alone a crossing whose loop is smaller than a circle
twice the face's deflection across (a threshold kept "to fit
regressions"), and the face's deflection is at least twice its largest
tolerance, which the plate's approximation makes 0.03 to 0.08: the loop,
far smaller, was not reported, the edges were not refined, and the
Delaunay triangulation of a frontier that crosses itself had no inside
(`BRepMesh_BaseMeshAlgo::commitSurfaceTriangulation` found no triangles).

A closer approximation of the plate does not remove it: with the plate
approximated to its own fit (its tolerances 0.01) the same faces fail at
other settings. The cusp is the corner's geometry: the stripe's section
ends tangent to the sloped side, along the rim, where the border runs on
along the rim; the polygons, each within its own deflection, cross where
the two curves are closer than that.

## 1804: the polygons at a cusp untangled

### 1804-brepmesh-polygons-crossing-at-a-cusp-of-a-wire-untangled

- `BRepMesh_ModelHealer`: `untangleCusps`, one face after another, before
  the faces are checked (and again for the faces of edges amplified after
  a check). For two consecutive edges of a wire with a common vertex whose
  first segments from it lie within 5 degrees of each other, while a
  segment of one polygon crosses a segment of the other (not the two first
  ones), the point next to the vertex is taken out of one polygon: the one
  whose crossing segment is fewer segments from the vertex, unless that is
  its first segment, then the other. The point goes from the edge's curve
  and all its pcurves, so that the faces sharing the edge keep one polygon.
  At last the polygon's first segment runs from the vertex past the
  crossing.
- Elsewhere nothing changes. No catch is added.

## Reports and tests

- `cli.fcstd_bend_reported`: the reported design (from
  `MITCAD_MITCAD_REPORTED`, never committed) with its bend radius set to
  24.5, 25 and 32 exported as STL at deviations of 0.0005, 0.002, 0.01 and
  0.05 mm with an angle of 10 degrees, at 0.002 mm with 5 degrees and at
  0.0005 and 0.002 mm with 2 degrees: every face meshes. Without the patch
  the corner plates fail at the finest three.
- OCCT's own mesh group (the source archive's `tests/mesh`: 255 cases
  run, 444 skipped for want of their data), its bugs/mesh grid (142 run)
  and the blend group (446 run), a DRAW build of the port with 1804
  against the same build without it: no case broke. One result changed:
  bugs/mesh/bug25044_9 meshes its face with 6 triangles instead of 4, its
  largest deflection 0.014 instead of 0.019 (asked for 0.1).
- A DRAW reproduction of the plate needs the reported body: no body built
  from OCCT's primitives alone was tried.

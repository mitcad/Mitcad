# A blend's end past a face tangent to the faces beside it: 1830

mitcad#2039, GitHub mitcad/Mitcad#449: a concave edge rounded or chamfered
with 1 mm where both of its ends sit at a wall's flat side that runs on
tangentially into two curved faces. Back to the [port's README](../README.md).

## Overview

- **1830** cuts the end of a fillet or chamfer at a vertex of more than
  three edges on the two faces beside a face that is tangent to both of
  them there and that the blend does not reach: they meet along the
  intersection of their surfaces from the vertex. Before, the end was cut
  on one of them alone (a shortcut meant for faces on one surface) and the
  result was invalid; without the shortcut the general cut crossed the
  tangent face as well, its three curves crossing each other.

## The case

A low block whose plan is a half disc (radius 5, 5 high) stands against a
wall 10 wide whose top is a half cylinder (radius 5 about an axis along
the edge's normal, from z = 5). The wall's flat sides run on tangentially
into the block's curved side (below z = 5) and into the wall's curved top
(above it). The concave edge where the block's top meets the wall's front
(10 long) is rounded with 1. At each end of the edge five faces meet: the
block's top and the wall's front, the fillet's faces, and round the vertex
the block's curved side, the wall's flat side and the wall's curved top.
The edges between the flat side and the two curved faces are tangent (G1,
not G2), so the vertex counts five sharp edges and the end is made by
`PerformIntersectionAtEnd`.

The result was an invalid shape 142 mm3 smaller than the body (642.7 mm3),
the wall's top left without its curves on the face, an edge 11 mm in
tolerance; the chamfer of the same edge likewise. MitCad refused both
("fillet produced an invalid shape"); the reporter saw the fillet preview a
body without the wall's curved top.

The blend's section lies in front of the wall and above the block, beyond
the vertex from the flat side. Its end is bounded there by the block's
curved side continued upwards and by the wall's curved top continued
forwards, which meet along an ellipse through the vertex (two cylinders of
one radius whose axes cross).

## 1830: the end cut across the faces beside a tangent face

### 1830-chfi3d-end-cut-across-the-faces-beside-a-tangent-face

- The shortcut for five edges whose middle edge (and one beside it) is
  tangent (OCCT's "pro15368") cuts the end on the last face's surface
  alone and takes the first face's edge at the vertex, prolonged to the
  end's first point, as the cut face's boundary. That holds only where that
  edge lies on the last face's surface (the first and last faces on one
  surface, split at the middle one); here the end's first point lay 0.1 mm
  off the wall's top. The shortcut is now taken only where that point lies
  on the last face's surface, within its own tolerance and its edge's.
- A face round the vertex whose two edges there are both tangent to the
  faces beside it, neither crossed by the blend's surface within its
  range, is not cut: the faces beside it are taken as neighbours whose
  common boundary is the intersection of their surfaces (GeomInt_IntSS, as
  for a B-spline edge the blend does not cross), the line through the
  vertex that crosses the blend. The vertex's parameter on it is the
  nearest extremum (the ellipse also passes the edge's other vertex, the
  farthest point), its curves on the two faces are put into the faces'
  periods at the vertex, and they are not shifted by a projection of the
  vertex onto them (the curve and its curves on the faces share their
  parameters). Prolonging a tangent edge along its own curve, as the
  general walk did, follows a line of contact of two surfaces that do not
  cross there: the flat side's surface ran outside both curved faces.
- Elsewhere nothing changes. No catch is added.

## Reports and tests

- `geometry.port.1830` (`geometry/tests/port/p1830-end-cut-across-a-tangent-face.cpp`):
  the half-disc block against the half-cylinder wall, built from OCCT's
  primitives, the edge rounded and chamfered with 0.5, 1 and 3: one valid
  solid each, its tolerances those of the walk, the added volume that of
  the section cut at each end by both cylinders continued (computed in the
  test) within 1e-5 of it (measured: within 2e-6). Without the patch every
  result is invalid, 138 to 193 mm3 smaller than the body. The nearest case
  outside the patch, the wall's top rounded with 3 at its edges only (its
  flat side runs on above the block and is cut): the same valid solid with
  and without the patch.
- `cli.fillet_tangent_seam_reported`: the reported design (from
  `MITCAD_MITCAD_REPORTED`, never committed), its Fillet2 of 1 mm and a
  chamfer of 1 mm in its place: each one valid solid without a note, adding
  2.139248 and 4.970703 mm3 ((1 - pi/4) r^2 L = 2.146 and r^2 L / 2 = 5,
  less the ends the cylinders cut off).
- The OCCT test groups were not run for this patch.

# Fillet crashes and failures: 0020–0026

Crashes, exceptions and wrong shapes of OCCT's fillet on fillets of the
`.f3d` corpora (mitcad#122, mitcad#133). Back to the
[port's README](../README.md).

## Overview

- **0020 to 0026** fix other failures and crashes of the rolling ball
  fillet (mitcad#122, mitcad#133).

## 0020 to 0026

The patches 0020 to 0026 fix OCCT's fillet (`TKFillet`, `ChFi3d`) where
it crashed, threw or built a wrong shape on fillets of the `.f3d` corpora
(mitcad#122, and from 0024 mitcad#133); each one changes the results only
in the cases it describes, and `geometry/tests/test_fillet_kernel.cpp`
(`test_fillet_ends.cpp` from 0024) rebuilds those it could on synthetic
shapes with OCCT alone.

### 0020-chfi3d-crash-fixes

Three causes of segmentation faults. A stripe continued past an obstacle
face that does not hold the edge the contact left by kept an empty
restriction curve (`StartSol`); the seam of the face at a corner was
intersected with a parallel curve, whose points `Extrema_ExtCC` does not
give (`PerformOneCorner`, `IntersectMoreCorner`); `PerformMoreSurfdata` read
the first stripe of the builder instead of the stripe at the vertex.

### 0021-chfi3d-corner-of-contour-closed-at-sharp-vertex

A contour of tangent edges that closes on itself at a sharp corner has both
ends at that vertex; with more than three sharp edges there
(`PerformMoreThreeCorner`) only its first end was found, and the corner
threw.

### 0022-chfi3d-round-edges-asked-for-near-tangent

An edge between faces up to 0.1 rad apart counted as tangent and could not
be rounded; it is refused now only when its faces come within the builder's
angular tolerance.

### 0023-chfi3d-meeting-point-of-periodic-contact-lines

Where a fillet along an arc meets one along a line at a shallow corner, the
meeting point of their contact lines on the common face was taken on the
arc's extension, far from the vertex, and the corner face was
self-intersecting.

### 0024-chfi3d-order-of-periodic-kpart-surfaces

The analytic pieces of a rounding along a whole circle, cut where its
contact line leaves a face (`SplitKPart`), are sorted along the spine
(`PerformSetOfKPart`); the sort kept a stale piece, so one piece came out
repeated and others lost, and the walks between the pieces threw
(`ChFiDS_CommonPoint::Vector`) (mitcad#133).

### 0025-chfi3d-end-cut-by-faces-around-a-cylinder-corner

A rounding whose end is cut by several faces around a cylinder's corner
patch (`PerformIntersectionAtEnd`): the crossing with each common edge is
taken on that edge (two arcs of one circle gave the same one), a crossing
beyond the edge's far end prolongs the faces' common boundary along the edge's
curve and cuts the corner patch's edge there (not behind the vertex itself on
an edge that starts there, which OCCT handles: OCCT's bug32929 gave two
solids, mitcad#1902; not where the rounding passes
beside a face between the end's first and last faces, such as a narrow
chamfer at a step's corner), a seam on a face at the end only no longer
counts as an edge of the vertex, and the end's intersection with that face
is taken across the seam, split there, and the seam cut (mitcad#133).

### 0026-chfi3d-onsame-end-cut-by-several-faces

The end of a rounding whose face runs on tangentially past the vertex (the
OnSame state) with its contact off any edge, on either side of the rounding,
cut by several faces (`PerformIntersectionAtEnd` gave up): the face's edge
at the vertex is prolonged to the contact by a piece of its curve on both
its faces, the spine's edge is cut at the vertex, and the ends on a periodic
face are taken into its domain (mitcad#133). Roundings only: a chamfer keeps
OCCT's corner there (OCCT's chamfer sequences B9 gave no result where stock
gives an invalid one, mitcad#1902).

## Reports and tests

- 0020 to 0023: mitcad#122 and `geometry/tests/test_fillet_kernel.cpp`.
- 0024 to 0026: mitcad#133 and `geometry/tests/test_fillet_ends.cpp`.

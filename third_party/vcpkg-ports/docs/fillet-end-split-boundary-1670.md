# A fillet's end crossing a boundary split past its first edge: 1670

mitcad#1615: a box's outer edges rounded with 7 at the ends of a channel
whose end caps stop 2 short of the flanges' tips. The end of each rounding
is cut by the faces round the notch the cap leaves
(`PerformIntersectionAtEnd`, OCCT's own handling of a fillet's end at a
vertex of three faces or more). Back to the [port's README](../README.md).

## Overview

- **1670** takes the crossing of the fillet with the common boundary of
  two faces round the end on the boundary's next edge where the boundary
  goes on past the edge the walk round the vertex reached it at (a
  straight boundary split at a vertex), instead of prolonging the first
  edge's curve past its end; the edges passed lie under the fillet and go
  with the corner it removes.

## The case

A channel 250 long: a web 3 thick and two flanges 2 thick, 32 wide (x
from -29 to 3), end caps 2 thick between the flanges, at x from -27 to 0:
2 short of the flanges' tips (at x = -29). The caps' inner corners with
the flanges are rounded with 5 (centre 7 from the outside faces), and the
outer edges of the flanges' feet with the end faces (y = 0 and 200, z = 0
and 250, along the whole 32) with 7: concentric with the inner roundings,
a wall 2 thick round the corner beside the caps.

Over the last 2 (x from -29 to -27) the end face is the flange's end
only, 2 wide; the rounding's contact on it (7 from the edge) lies past
it, so OCCT's analytic blend ends at the notch's side (the cap's side,
x = -27) and the end at the flange's tip is made by
`PerformIntersectionAtEnd`, which walks round the vertex at the tip:
Face[0] the cap's side, Face[1] the flange's inside (y = 2), Face[2] the
flange's tip, with the edges between them, Edge[1] the edge of the cap's
side with the flange's inside at the notch's floor. The blend crosses the
boundary of Face[0] and Face[1] (x = -27, y = 2) at z = 7 - sqrt(24) =
2.101; that boundary is two edges, split at z = 2 where the cap's inner
face ended before the inner rounding: Edge[1] from z = 0 to 2 (from the
cap's side) and another from 2 to 7 (from the inner rounding's side).
Generally the crossing lies at z = r - sqrt(4 r - 4), on the upper edge
from r = 4 + 2 sqrt(2) = 6.83 on.

The crossing past Edge[1]'s end was taken for the case of two faces that
part at that vertex: their common boundary prolonged along Edge[1]'s
curve from the vertex to the crossing (a new edge, 2 to 2.101), while
the upper edge stayed whole in both faces, and the flange's inside kept
its corner under the rounding with the notch floor's edge, whose other
face (the end face's narrow part) the rounding removed: a free edge, an
open shell, which MitCad refuses ("would leave the body open",
mitcad#1182).

## 1670: the crossing on the next edge of the common boundary

### 1670-chfi3d-end-crossing-on-the-next-edge-of-the-faces-common-boundary

- `ChFi3d_CrossingOnCommonBoundary`: from the vertex past which the
  crossing lies, the edges of both faces that go on from it in turn, up to
  the first the blend crosses within its range (at most eight): that
  edge, the crossing's parameter on it and point on the blend, and the
  edges passed with their vertices towards the crossing.
- `PerformIntersectionAtEnd`: where such an edge is found (not at the
  corner's vertex, nor at the previous edge's vertex: the faces passed
  beside at a corner's chamfer keep their handling), it is the edge of that
  step, cut at the crossing as any crossing on its edge, its end towards
  the corner the one the chain reached it at; each edge passed is cut at
  its end towards the crossing, the whole of it removed with the corner,
  as the edges between a prolonged edge's end and the vertex are.

On the design's body before the rounding: valid, one closed shell, the
material taken 1319.900 mm³, the exact shape's 1319.91 (4 edges: 30 of
each beside the caps and the web, 49 (1 - pi / 4) each, and the 2 over
the notch, the flange's section outside the rounding's circle, 7.2567);
before, invalid with four free edges.

## Reports and tests

- `geometry.fillet_end_past_split_edge`
  (`geometry/tests/test_fillet_end_past_split_edge.cpp`): a flange with a
  web and an end cap 2 short of the flange's tip, its inner corner rounded
  with 5, the flange's foot at the end rounded with 6.5, 7 and 8: one valid
  closed solid, every edge on two faces, the exact shape's volume (6.5
  crossed the lower edge before the patch too; 7 and 8 left a free edge).

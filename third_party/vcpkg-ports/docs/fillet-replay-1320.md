# Fillets of stored designs: 1320

mitcad#1346: a plate's side outlines rounded along an earlier rounding of
its top edge that vanishes into the side, on the import's own body instead
of the file's stored state. The rounds before are in
[1310 and 1311](fillet-replay-1310-1311.md). Back to the
[port's README](../README.md).

## Overview

- **1320** gives the walk's parameter to the extremity of the second
  contact where `BRepBlend_Walking` stops on a restriction of one surface
  with its two contacts at one point (the blend vanishing there). The
  extremity was set by the overload for a point on a restriction, which
  took the contact's V on its surface for the parameter on the guide.

## The case

A bracket's plate (`5ef2dbd5` of mitcad#343, item 91), its top edge
rounded earlier (a B-spline face) so that at each of the plate's sides,
x = -7 and x = 7, that rounding comes tangent to the side at a vertex and
vanishes into it: the side (a plane), the earlier rounding and a small
cylinder at the vertex are tangent to each other there. The side outlines
are rounded with 0.3 (item 91); each side's chain runs along the earlier
rounding up to that vertex.

## 1320: the vanishing end keeps the walk's parameter

### 1320-blend-walking-vanishing-end-keeps-the-walk-parameter

On the body MitCad builds the earlier rounding is exactly tangent to the
side at the vertex, and the walk between the side and that rounding reaches
the vertex with no width left: both contacts at the vertex, the walk
stopping on the side's edge there (`Blend_OnRst1`). For that singular stop
the walk takes the second contact at the first one's point and sets its
extremity with `BRepBlend_Extremity::SetValue(P, U, V, Tol)`, which is the
overload for a point on a restriction, `(P, W, Param, Tol)`: the contact's
V on the earlier rounding's surface became the extremity's parameter on the
guide (-0.034 on one side, 15.42 on the other, the vertex's parameters on
the earlier rounding's other edge, instead of the walk's 36.96 and 0).
`ChFi3d_FilCommonPoint` took that for the common point's parameter;
`StoreData` (`CompParam`) then looked for the earlier rounding's edge on the
contact's curve on that face at the wrong parameter, found neither an
intersection nor a projection there (the curve ends at the vertex), and
kept the wrong parameter for the contact's end, far outside its curve's
range. The end's degenerated edge (`PerformSingularCorner`) was made from
the vertex across the blend to the contact curve's point there, 0.21 mm
from the blend's other end and 4 mm from the vertex, and the contact's own
edge was left out: the blend's wire crossed itself and the earlier
rounding's wire was left open, on both sides, an invalid result.

The file's stored state 61, whose earlier rounding misses tangency by
0.4 degrees and 2.6 µm at the vertex, built: its walk stopped 0.04 short
of the vertex where a contact reached the spine's own edge (the faces'
gap within that edge's tolerance), with a section 0.0095 wide, and
`ChFi3d_ShrinkVanishingEnd` bent the blend's end to the vertex
(`ChFi3d_ContactOnOwnEdgeAtEnd`).

Both singular stops (on a restriction of the first surface or of the
second) now set the other extremity with the walk's parameter,
`previousP.Parameter()`, as every other extremity of the walk is set. The
same overload is used without a parameter at the first section
(`PerformFirstSection`); it is left as it is there, no case reaches it.
On the case the rounding is valid, and its faces about the two vanishing
ends lie on the stored body after it (state 62) within 0.0063 mm both
ways.

## Tests

- 1320: mitcad#1346, in `geometry/tests/test_fillet_vanishing_side.cpp` on
  the case `plate-side-vanishing-rounding` of `MITCAD_GEOMETRY_CASES`: the
  plate's side outlines on the body MitCad builds rounded with 0.3, the
  result closed and valid, and within the box about the two vanishing ends
  the result and the stored body after it lie on each other within
  0.01 mm (the result is invalid without 1320). A synthetic body needs a
  rounding that comes exactly tangent to a plane at a vertex of three
  mutually tangent faces; the design's body is the case.

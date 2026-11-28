<!-- SPDX-License-Identifier: MIT -->
# A sliver along a line in the polygonal hidden-line removal: 2539

mitcad#2539, GitHub mitcad/Mitcad#729 and #765: a drawing's front view of
a subassembly lost lines at some scales.
Back to the [port's README](../README.md).

## Overview

- **2539** cuts the part of a segment one triangle hides to the stretch of
  the segment inside the triangle's box in the view. A sliver seen almost
  edge on and lying along a segment hid the segment from one of its ends
  to the sliver.

## The case

The view is the front view of a frame of two upright tubes with two
shafts through them; between the uprights two rollers sit on the shafts,
each a tube on two bearings held by two-piece clamping collars. The
bearings and collars are free-form, so the view takes the quick path
(the polygonal hidden-line removal of a triangulation that follows the
view's scale). At 1:2 and 1:5 the shafts' ends outside the left upright
lost one or both outline lines; with hidden edges on, the lines were
drawn hidden. At 1:1 and 1:10 they were drawn.

`MITCAD_DRAWING_TRACE=strokes` showed that the lines were already missing
from the kernel's projection, not lost by the sheet's simplification, and
the projection of the shaft alone (polygonal or exact) drew them whole.
With every body, a collar half hid them. The collar's bore has the
shaft's radius, and its halves meet in a level plane, so each half's bore
is round about the shaft's top or bottom line. Its triangulation has a
generator on that line, and one of its triangles is a sliver, 9.5 mm long
and 0.03 mm high in the view, seen almost edge on, lying along the
shaft's outline segment (one segment from the shaft's end to its other
end) and almost in its plane.

## 2539: the hidden part kept within the hiding triangle

### 2539-hlr-polygonal-hiding-by-faces-seen-edge-on

`HLRAlgo_PolyData::hideByOneTriangle` finds where the segment crosses the
lines of the triangle's three sides and hides the part between the
crossings (or from a crossing to the end of the segment that lies inside
the triangle), cut at the triangle's plane. Along the sliver the segment
meets the lines at a shallow angle: one crossing fell on its side and was
kept, the other fell by rounding just outside its side and was rejected.
With one crossing the segment's start was taken for inside the triangle,
and the part hidden ran from the shaft's end to where the segment crosses
the sliver's plane inside the collar. Which scales meet this depends on
the triangulation.

- The part hidden is cut to the parameters of the segment whose points
  lie in the box of the triangle's corners in the view, widened by the
  algorithm's tolerance; nothing is hidden when none do. A triangle covers
  no point outside its box, so every other triangle hides what it hid.

## Reports and tests

- Test: geometry.port.2539 and the reported design's test, see
  [opencascade-patch-tests.md](opencascade-patch-tests.md#2539).
- The OCCT lab's cases `2539-sliver-along-edge`, `2539-shaft-collar-pair`
  and `2539-shaft-ends`: 130.8, 130.8 and 109.6 mm hidden without the
  patch (the last two at 1:2 and 1:5, none at 1:1 and 1:10), none with it;
  the rest of the lab's shared set unchanged.
- `MITCAD_DRAWING_TRACE=strokes` ([performance](../../../docs/performance.md#drawing-views-mitcad962))
  and `test_geometry_surfaces drawing --round-outlines` trace such a line
  from the kernel to the sheet.

# A rounding wider than a wall's top, cut by a slope: 1850

mitcad#1987 (from mitcad#1926). Back to the [port's README](../README.md).

## The case

A wall 2 thick standing on a floor, its top a strip 2 wide between the
outer wall and the inner one; the top's end edge is chamfered with 3, so
the strip is closed off at its end by a face at 45 degrees. The outer top
edge is rounded with a radius wider than the strip (2 to about 3.5). The
rounding takes the strip away: its ball touches the outer wall and rolls
on the inner wall's top edge, and the slope cuts it at the end, past the
edge's end (the cut runs from the edge's end down the slope to the outer
wall). `geometry/tests/test_dressup_takes_face.cpp` checks it at 3 and
2.5 against the exact volume.

Before the patch OCCT failed at 3 ("Failed to find edge" at the end
vertex), and between 2 and 3 it built a valid body that was wrong: the
start of the walk went from the top over the inner wall (which no such
ball touches) on to the floor at its foot, where a ball touching the
outer wall and the floor from below rounded the floor's edge and took
the whole wall away.

## 1850-chfi3d-rounding-wider-than-a-walls-top-rolls-on-its-inner-edge-cut-by-the-slope-past-its-end

- `StartSolOverflow`: a side of a convex edge's rounding (its ball inside
  the solid) that went over a face its ball touches at no point, at least
  the radius wide, across an edge where the solid turns away by a right
  angle or more (`turnsAwayAt`: the edge convex, the faces' outward normals
  at least a right angle apart), rolls on that edge instead of touching a
  face beyond. Past a sliver (`geometry.fillet_corners`' window) and for a
  concave edge's rounding (`geometry.fillet_overflow`'s boss on a ring)
  nothing changes.
- `RolledOnProlongedEdgeAtEnd` (0342's walk on past the rolled edge's
  end): a piece that ends at the spine's end is walked on too where its
  other contact ends there inside its face, on no edge and off the surfaces
  of the other faces at the end vertex (the slope cuts it further on; where
  the section lies in the face at the end, as 0342's ear sides, nothing
  changes). Where the ball reaches into B, the face whose edge it rolls on
  (the inner wall), and not into the edge's other face (the top), the walk
  rolls on a strip of the other face's surface beside the prolonged edge;
  its curve on that side is then moved onto B's surface, with the guide's
  parameters (the approximation's knots run from 0 to 1 and are set back
  to them), and its transition on B is the piece's own.
- `PerformOneCorner`: such an end at a vertex whose three edges are convex
  (AllSame) is the cap of `ChFi3d_RolledPastEdgeVertex` too. The face at the
  end keeps the side of the cap away from the vertex (an OnSame end's side
  is the other one). A cap that leaves that face (the outer contact running
  on past the face's far edge, from a radius of about 3.5 here) would need
  the faces beyond to cut it too: refused in words, not made invalid.

Result: at 2.5 and 3 valid, the volume within 2e-5 mm3 of the exact one,
the widest tolerance 1e-4. OCCT's 754 DRAW blend and chamfer cases are
unchanged, and so are the geometry and fillet/chamfer ctests.

Test: `geometry.dressup_takes_face` (the strip beside a slope).

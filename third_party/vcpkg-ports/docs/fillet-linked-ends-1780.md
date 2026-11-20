# A convex and a concave rounding crossing before ends joined by a sharp edge: 1780

mitcad#1987, the follow-up of mitcad#1926: GitHub #371's Fillet1 failed
after the support's height edit (d49 = 6 mm). Back to the
[port's README](../README.md).

## The case

The support beside the tower is two legs standing on the plate, their
tops sloping up at 45 degrees towards the tower. Fillet1 rounds the legs'
sides where they stand on the plate with 2 (concave) and their top edges
with 0.5 (convex). At the file's height (d49 = 4) the top runs down into
the plate and both roundings end at one vertex; the corner there joins
them. At 6 mm the top starts 2 above the plate: a front face 2 high
appears, the foot's rounding ends at its bottom vertex and the top's at
its top vertex.

On each side the foot's contact (2 above the plate) and the top's (0.71
below the top edge, in height) cross 0.71 behind the front face. Past the
crossing the side between them is gone: the foot's ball reaches the top's
blend. The two blends cannot be cut by each other there (both are tangent
to the side, one on each side of it), and no corner joins them, their
ends being apart. OCCT's fillet computed both and failed at the crossing
(`ChFi3d_StripeEdgeInter`, "intersect each other"); on a block standing
alone it built an invalid result.

## 1780-chfi3d-convex-and-concave-roundings-crossing-before-ends-joined-by-a-sharp-edge

- `convexConcaveCrossBeforeLinkedEnds`, in `Compute` beside 1400's
  `convexConcaveCrossBeforeEnd`: open stripes, one convex and one concave
  on a face F of both their edges, an end of each at a vertex of a sharp
  edge of F that neither spine holds (the front face's edge with the
  side), each the only stripe ending at its vertex, their contacts on F
  crossing inside both. Both fail as overlapping (`Failure_Overlap`).
  MitCad's fillet then builds the convex rounding first and the concave
  one on its result (`concave_after_convex` in `geometry/src/dressup.cpp`,
  as for 1400's pairs), the foot's ball riding over the top's blend from
  the crossing to the front face; a concave contour meeting a deferred one
  at a vertex (the tower's foot behind the legs) is deferred with it, so
  that its blend's end does not take a part of the deferred edge away
  first.
- `dropEdgeOfFaceGoneBeforeEnd`, in `PerformFilletOnVertex` after
  `PerformOneCorner`: in that second rounding the foot's contact crosses
  from the side onto the top's blend before the front face, so its end
  lies on the plate and that blend, not on the side. The side's edge with
  the front face, from the end vertex up to where the side, the top's
  blend and the front face meet, lies under the new blend all along; the
  corner put no point on it and it stayed in the front face, whose wire
  was then not closed. It is now cut at both its vertices and goes.

Result: Fillet1 builds after the edit (valid, 6126.48 mm3), and so does
Chamfer3 after it. At the file's height 1780 changes nothing (the ends are
one vertex).

## At the file's height: foot corners (MitCad's fillet, no patch)

At d49 = 4 the legs' tops run down into the plate: on each side the foot's
and the top's edges end at one vertex, the third edge there (the top's
with the plate) not rounded, the angle between the two on the side 45
degrees. The file's body is the foot's rounding first, run on to the
front and cut by the top, and the top's 0.5 on that result, running on
over the foot's blend down to the plate (a free-form patch of 2.306 mm²).
OCCT's single build joins the two where their contacts cross instead:
0.8 to 0.9 mm3 at each of the four corners, faces up to 0.38 mm off, so
that the import took Fillet1 as a deviating result.

`FootCorners` in `geometry/src/dressup.cpp` finds such corners (a convex
and a concave selected edge ending at a vertex of three edges, at less
than 60 degrees on their common face, the third edge concave and not
selected) and builds the convex edges after the others, with every
selected convex edge meeting one of them at a vertex or on a face (the
tower slot's 0.5 and the tower top's roundings, whose corners with the
tops then stay as the single build makes them). A concave edge that meets
a later one alone at a vertex (the legs' 5 at the tower's front face, with
the outer tops) is built with it in a pass of its own before the others:
built first, the outer top's rounding ran on over its blend up the
tower's front edge. A later pass takes every piece of a later edge (an
earlier pass cuts the tower slot's 0.5 where the legs' 5 meets it, and the
piece below lies between other faces), and the corners' own convex edges
after the others of their pass (in the other order OCCT's corner at the
tops' far ends found an edge twice). Groups of corners whose passes do
not build (the outer arms' tips, 30 degrees, where the pit's 1 then meets
another blend) keep the single build. On the reported design Fillet1 is
6092.74 mm3 against the file's 6092.12 (6089.43 before), the four slot
corners within 0.01 mm3 of the file's body, and the import verifies it;
what is left is the outer arm tips (0.5 mm3 each), the 0.8 arm's inner end
(0.17 mm3) and the legs' outer tops at the tower (0.1 mm3 each). On a
block alone with no front face OCCT reported an overlap, and against a
wall it failed ("intersect each other"); both now build, as the two
passes do with OCCT alone (`geometry.fillet_linked_ends`).

Test: `geometry.fillet_linked_ends` (a block on a plate, front faces 1.5,
2 and 3 high) and `cli.f3d_edit_reported` (the reported design after the
edit).

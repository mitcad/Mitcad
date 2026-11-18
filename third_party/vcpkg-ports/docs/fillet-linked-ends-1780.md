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
Chamfer3 after it. At the file's height nothing changes (the ends are one
vertex).

Test: `geometry.fillet_linked_ends` (a block on a plate, front faces 1.5,
2 and 3 high) and `cli.f3d_edit_reported` (the reported design after the
edit).

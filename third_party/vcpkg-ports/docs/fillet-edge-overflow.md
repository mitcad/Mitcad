# Fillet edge overflow: 0010–0019, 0050–0077

OCCT's rolling ball fillet (`TKFillet`, `ChFi3d`) where a rounding's
contact leaves its face: the ball touches the face beyond or rolls on the
far edge (mitcad#121). Later work on the same blends is in
[chamfers](chamfers.md) (0140 and later), [the rolling ball](fillet-rolling-ball.md)
(0170 and later) and [the corners](fillet-corners.md). Back to the
[port's README](../README.md).

## Overview

- **0010 to 0019** make OCCT's rolling ball fillet build roundings it
  failed on (mitcad#121).
- **0050 to 0077** continue them: roundings whose contacts leave their faces
  near the ends of the spine or part of the way along it.

## 0010 to 0019: edge overflow

### 0010-brepblend-surfrst-line-on-both-restrictions

A blend rolling on an edge (`BRepBlend_SurfRstLineBuilder`) whose contacts
leave the face and reach the end of the edge in the same step took the
parameter of the wrong arc as its point on the edge and stopped short of the
end; it now ends on both.

### 0011-chfi3d-fillet-edge-overflow

Edge overflow of the rolling ball fillet. Where the ball cannot touch a face
of the edge within that face (a face narrower than the rounding), it touches
the face beyond the face's far edge, or, where that face does not let it, it
rolls on the far edge (a blend on an obstacle edge, which OCCT had only
where a walk met an edge in the middle of a rounding). OCCT found no start
where the contact leaves a face along the whole edge; now the start can be
on the face beyond or on the edge (`StartSolOverflow`). A blend on an edge
then ends where the spine or the face at its end does (`ObstacleAtSpineEnd`,
`ChFi3d_ObstacleEndOnFace`, `ChFi3d_ExtendAtSpineEnds`) or goes round a
closed spine (a periodic guide in the edge/face `ComputeData`), and the
narrow face it consumes vanishes (`ChFi3d_CutConsumedFaces`). Where the
contact leaves a face near the end of the spine, OCCT cuts the rounding with
the face beyond, as before. Fillets only; chamfers are unchanged.

### 0012-chfi3d-closed-stripe-end-on-vertex

A closed rounding whose start lies on a vertex but whose end was not
identified with it gave the builder a vertex index as a point index (an
exception).

### 0013-chfi3d-overflow-across-faces-and-overlapping-roundings

0011's overflow across several narrow faces and on both sides of the edge.
Each side's support moves on across the edge of its face that the section
crosses while the contact lies beyond it, also where the ball touches a
face's surface at no point (a sliver: the face beyond the nearest edge,
within the ball's diameter); a side rolls on the last edge before a face it
cannot touch (not both sides: that ball touches no face); a start touching
the faces it reached must clear the faces it passed and those around them.
The consumed faces between the spine and an edge the blend rolls on can be a
chain of strips (`ChFi3d_CutConsumedFaces`). Two stripes whose contacts on a
face between their edges each lie beyond the other's are computed again
overflowing that face across each other's edge (`OverlappingStripes`,
`OverlapEdge`); a stripe whose blend then lies on another's stays a contour
of the builder but gives no faces (`RemoveCoincidentStripes`). The builder
class gets members for these, so code built against the port's headers must
be rebuilt with them.

### 0014-chfi3d-contact-along-face-edge

A fillet or chamfer whose contact runs along the far edge of a face (a face
exactly as wide as the contact distance). The split of an analytic blend
into the domains of its faces found no domain for a contact line lying on
the face's boundary, so the blend went to the walking path and failed; such
a line now takes the domains of the same line moved a little into the face,
its ends put at the vertices they meet (`SplitKPart`). A blend whose contact
covers a whole edge of its face is then stored as a blend on that edge, as
0011's blends rolling on an edge are, so that the face it consumes is
dropped instead of being trimmed to nothing
(`ChFi3d_ContactsAlongFaceEdges`; a blend made 0.1 % smaller than the face
stays inside it). Walked blends along a face's boundary still fail.

### 0015-brepblend-surfrst-smaller-step-on-failed-reframing

A blend rolling on an edge (`BRepBlend_SurfRstLineBuilder`) stopped at the
first section it could not reframe on an arc; a step too large lands on the
other branch of the solution, outside the face. The step is now halved
first, as the walking between two faces does.

### 0016-chfi3d-rounding-rolls-on-anothers-contact-line

Two roundings of a face narrower than both, where no ball touches the faces
beyond it on both sides (the rims of a plate thinner than the roundings'
diameter), so that 0013 cannot make them one: the earlier rounding is
computed on its own faces, the later one rolls on the earlier one's contact
line on that face (`RideOnOverlap`), the two blends meet on that line
(`ApplyRides`, after the corners; the caps of open roundings meet where the
line ends) and the face vanishes. Both roundings convex or both concave
only. The builder class gets members for this (code built against the port's
headers must be rebuilt).

Where the overlapping roundings also meet roundings at the face's ends
(a prism rounded on all its edges: a roof slab's eave 1.506 wide between
an acute edge and an obtuse one, rounded with 1), the corners there can
still fail or leave the body open (mitcad#1482). Mitcad's fillet then
names the face and the two edges in its error (`narrow_face_cause` in
`geometry/src/dressup.cpp`, `geometry.fillet_roof_slab`); the corners
themselves are not changed by a patch.

On that slab (a section of 7 points, 1.3 thick across its slopes,
extruded 25.2, every edge rounded) OCCT's fillet alone fails at every
radius above 0.65, where the end faces' strips along the slopes (1.3 wide)
and the eaves (1.506 wide, the roundings of their 60° and 120° edges
reaching 2.31 r across) become narrower than the roundings of their two
edges (mitcad#1494). A closed solid exists there (each rounding takes away
a well-defined part), but the kernel lacked:

- the ride on each end face: the earlier stripe, the underside's edge, is
  shorter than the later one, the top slope's edge, which runs on past the
  ends of the line it rides on and throws in its walk (the four slopes'
  pairs are faulty contours; from 0.66 to 0.9 both stripes of each pair).
  The shorter one riding on the longer one's line computes;
- corners at a rider's ends where another rounding ends too: the rider's
  side on the line lies on the support made from the other blend, not on a
  face of the solid (`ChFi3d_cherche_edge`: "Failed to find edge"; already
  on a plate 1.3 thick with its end face's two rims and one vertical edge
  rounded with 1), and the corners that take a rolling ball's section
  between the stripe's two faces (`PerformThreeCorner`) would put its ends
  off the line;
- a face consumed between two rides that end in corners at both ends (the
  plate's end face with the vertical edges at both its ends) was kept
  whole;
- the eave: its two long edges rounded together fail at 0.9 and 1.0 at the
  obtuse edge's ends (`OneCorner`: "fillets have too big radiuses"), and
  with the eave's end edges too with `TopoDS_FrozenShape`.

The patches 1640 to 1642 (mitcad#1499,
[fillet-ride-corners-1640.md](fillet-ride-corners-1640.md)) build the first
three: the ride the other way round, the rider's side on the face, the
corners at its ends (also where its line ends on the arc from its vertex,
as on the slab's eaves) and the consumed face; the plate rounded on all
its 12 edges and the slab's end face pair along a slope build valid closed
solids. The slab rounded on all its edges then stops at the eave's end
edge (mitcad#1500); until that is built, the fillet refuses the slab with
the narrow face it names, at 0.9 as at 1.0.

The patches 1650 to 1652 (mitcad#1500,
[fillet-eave-1650.md](fillet-eave-1650.md)) build the eave: its two long
edges (the tip's rounding rolling on the obtuse edge, the obtuse edge's
rounding dropped) and its end edges, and an end face's strip along a slope
with the eave's end edge. The whole slab then computed but was invalid at
the eaves' obtuse ends, where the corner at the tip and the corner at the
obtuse end both cut the eave's end edge's rounding and their cuts crossed;
with 1653 (mitcad#1530, same file) the corner at the tip cuts the three
blends by each other, and the slab rounded on all its edges builds a valid
closed solid at 0.9 and 1; with 1654, 1656 and 1657 (mitcad#1532) from 0.66 to 0.86 and at 1.1 too (what is left: that file).

### 0017-chfi3d-contact-along-face-edge-ends-at-vertices

0014's ends at the vertices also where the other face's contact ends first
(a stripe ending on slanted or curved faces): those ends are filled in the
processing of the spine's extensions, which did not put them at the vertex;
the arc was cut a hatching shift from it, which left a tiny edge in the face
at the end and an invalid shell (`SplitKPart`).

### 0018-chfi3d-corner-where-the-end-face-is-split-in-one-plane

A fillet or chamfer ending at a vertex where the face at the end is split in
two faces of one plane (a tab fused onto a support): the edge between them
is no longer counted where `PerformIntersectionAtEnd` chooses, by the number
of edges, between walking round the vertex (which failed with "Failed to
find edge" there) and `IntersectMoreCorner`, and `IntersectMoreCorner`
continues the prolonged edge on the opposite face by that face's own edge
(before, any other edge of the vertex, which oriented the face
inconsistently).

### 0019-chfi3d-undone-moresurfdata-corner-fails

Where `PerformMoreSurfdata` returns without computing the corner (its
surfdatas at the end are not as it expects), the vertex now fails with an
exception. Before, the stripe's end kept no points, the data structure
referred to point 0, and the topological build threw
`TopOpeBRepDS_DataStructure::Point` or, without range checks, wrote out of
bounds (heap corruption).

## 0050 to 0077: contacts leaving their faces near the ends

The patches 0050 and later continue 0010 to 0019 (mitcad#121): roundings
whose contacts leave their faces near the ends of the spine or part of the
way along it.

### 0050-chfi3d-kpart-end-runs-into-end-face

An analytic blend (`SplitKPart`) whose contact on one face runs into the
face at the end of the spine, a plane, while the other contact runs on to
the end where its face continues tangentially into that face at the end (a
round slot end rounded under a pocket as wide as the slot), keeps the second
contact up to the end, so that the face at the end cuts the one analytic
blend (`PerformOneCorner`). Before, the rest was walked on the first face's
tangent plane and the vertex failed or gave an invalid shape.

### 0051-chfi3d-rolling-on-across-surfdatas

A blend rolling on an edge whose other contact changes faces while the ball
rolls on along the same edge (a bevel narrower than the rounding along the
bottom of a plate with round corners: the ball's centre stays put at the
corners) no longer cuts the edge where its pieces meet (`ChFi3d_FilDS`); the
cut left the edge's piece twice in the face across it, an invalid face.

### 0052-chfi3d-rolling-start-beside-tangent-face

The start of a blend that rolls on the far edge of a narrow face on one side
and touches, on the other, a face that continues the edge's face
tangentially (a notch whose straight side runs on into a round corner):
`StartSolOverflow` lets the side that crossed a sharp edge roll and keeps
the tangent side's face, and `BRepBlend_SurfRstLineBuilder` solves a start
that touches the surface outside its face again from points across the face
(the ball on the far side of a cylinder). Before, no start was found or the
walk failed at once.

### 0053-chfi3d-corner-after-overflow-onto-face-beyond

The corner at the end of such a blend when the faces beside it run on past
the end (the notch with a tab under its straight side and round corner).
`PerformOneCorner` takes the face at the corner's vertex for the face at the
end, not the face the blend consumes, and, where the far contact's face does
not reach the corner's vertex, prolongs the face at the end to the cork
across the face beyond the face of the spine's edge, cutting the edge
between them where it crosses the face at the end. Before, the cap was not
built (`OneCorner : bouchon non ecrit`).

### 0054-chfi3d-contact-along-a-part-of-a-face-edge

A contact that runs along a part of an edge of its face (a channel wall
exactly as high as the rounding at its foot, the floor rounded down at the
ends, where the contact lies inside the wall) is taken as running along the
edge, as 0014 does for a whole edge, and the edge is cut at its inner ends
(`ChFi3d_ContactsAlongFaceEdges`). Before, the contact stayed on the face
along its boundary and the face's wire intersected itself.

### 0055-chfi3d-kpart-blend-cut-by-face-beyond

An analytic blend whose contact passes over a hole or a cut-out of its face
(a hole through a plate in front of a wall whose foot is rounded, a hole's
rim near the part's side) runs on over it, and the face beyond cuts it: a
piece of the blend whose side on that face is its intersection with the face
beyond fills the gap between the pieces of `SplitKPart`
(`PerformSetOfKPart`). Gaps where the face beyond reaches over the
neighbouring pieces too, or where one of its edges (a seam) ends under the
blend, keep OCCT's handling. Before, the blend failed, or rolled on the
hole's edge.

### 0056-chfi3d-corner-cutting-the-last-two-walked-pieces

The corner where the face at the end cuts the last two walked pieces of a
blend (`PerformMoreSurfdata`): one contact has left its face across an edge
of the face at the end, the other runs on to another edge of it after
crossing a face boundary (a rounding round a rod where it meets a pipe whose
top is flush with the rod's, the rod's side ending in a narrow bevel).
`MoreSurfdata` recognises the corner whichever order the vertex's two edges
come in (before, in the other order the corner went to `PerformOneCorner`,
which drops the last piece and fails). Where the face's cut leaves the last
piece through its end section instead of reaching the point on the edge
(next to the contact the piece dips under the face, its faces' normals being
more than a right angle apart), the piece's surface is extended beyond its
end and cut again; a cut that only touches the piece at that point is not
taken. The pieces' ends, which meet within their tolerance only, are matched
to the nearer points and the curves' orientations follow from the nearer
ends (before, an unorientable shell); the section gets a p-curve on the
previous piece where its projection fails; a last piece prolonged beyond the
point keeps its contact edge only up to that point.

### 0057-chfi3d-kpart-gaps-in-order-across-closed-lines

More of the gaps of 0055 are filled (the rim of a cylinder whose top is
crossed by blind holes or by notches open to a bore; a circle whose contact
on a ledge crosses round notches). The pieces of a closed spine are taken in
the order of their starts (SplitKPart lists the two halves of the piece
across the spine's start first, so the gaps next to it were missed); only
the face beyond's vertices on its boundary with the contact's face count as
under the blend (the bottom of a blind hole refused the cut); on an
intersection line that closes on itself the arc across the line's ends is
tried too; and where the cut runs along two lines that meet, the gap's two
pieces are split at the section through that point rather than at the gap's
middle.

### 0058-chfi3d-rounding-rolls-on-a-convex-far-edge

A rounding whose contact runs off the face of its edge across that face's
far edge, where that edge is sharp and convex, rolls on the edge (it stays
tangent to the other side's face and runs through the edge: a bevel narrower
than the rounding of its edge with a slot's wall, a wall's foot strip lower
than the rounding with the wall leaning back above it). Before, where a ball
could touch the face beyond, the rounding ran onto it: a larger rounding
that cut into the face beyond and whose corners at the spine's ends found no
face at the end. Tangent and concave far edges, edges rounded by the same
fillet, and contacts that pass several faces keep the earlier handling.

### 0059-chfi3d-kpart-blend-cut-by-the-face-at-the-end

An analytic blend whose contact on one face runs into the face at an end of
the spine before that end (the foot of a groove's end wall where the floor
rounds up into the part's side, the wall lower than the rounding there) runs
on to the end and that face cuts it all along, as over a hole (0055), up to
where its other contact reaches that face; there the blend ends in a point,
the stripe's end is a singular free end and no corner is made at the spine's
end vertex. Before, the rest was walked rolling on the wall's top edge and
the corner failed.

### 0070-chfi3d-kpart-cut-to-end-where-the-narrow-face-vanishes

0059 where the wall vanishes (a groove's bottom exactly as deep as the
rounding at its end wall's foot): along the middle of the chain the contact
runs along the wall's top edge, towards the ends past it. A line on its
face's boundary that leaves the hatching undone is hatched on the boundary
(as 0014 does where no domain is found); the cut pieces start from such a
piece's end on the wall's edge; and the inner ends of that piece's contact
along a part of the edge (0054) cut the edge with no part of it kept for the
wall, whose other pieces are gone too. Before, the walk got stuck (Path
failed), or the edge's parts stayed with the face at the end.

### 0071-chfi3d-kpart-cut-by-several-faces-beyond

An analytic blend whose contact passes over a hole or cut-out whose outline
is made of several faces (a slot through a wall rounded at its top edge: its
straight sides, then its rounded end) is cut by those faces in turn, one
piece per face; a face that cuts the blend along a section (a plane square
to a straight spine) gives no piece, and the pieces beside it share only
part of that section, the rest being that face's edge with the blend
(`ChFi3d_FilDS`). A gap across the start of a closed spine is filled too,
and a cut whose end is the start of a closed intersection line is found the
other way round. Before, such gaps were left to the walking, which rolled on
the hole's edge and failed.

### 0072-chfi3d-kpart-cuts-on-closed-spines

The cuts of 0055 and 0071 on closed spines and conic cuts (a ring standing
near a block's side chamfered at its foot as wide as it is high, the side
cutting the chamfer's cone; a hole's rim near the side bevelled):
intersection lines without p-curves (a cone and a plane meet in a hyperbola)
are used, a piece whose lines run across their period's end gets its end a
period later once the gaps are filled, and a contact along a part of a
closed edge counts as such (0054), so the face whose far edge it is goes.
Before, the cut was not found and the chamfer was left to a walking that is
not implemented for chamfers.

### 0073-chfi3d-kpart-pieces-over-a-hole-are-one-face

The pieces of an edge's analytic blend over a hole or cut-out of its face
(cut there by the faces beyond, 0055, 0057, 0071) are one face with the hole
cut out: where a run of pieces returns to the faces it left, all on one
surface, the pieces share one surface in the data structure and get no
sections between them. Pieces split where a contact passes from one face to
another keep their own faces. Before, the blend was a face per piece.

### 0074-chfi3d-walked-blend-cut-by-the-face-at-the-end

0059 for a walked blend: where the walk stops short of the spine's end
vertex because the contact on one face leaves it across an edge into the
face at that vertex, the blend is walked on with that face's surface
unbounded until the other contact reaches its face's edge at the vertex, and
that piece is cut by the face at the end (`ChFi3d_KPartCutPiece`); the
stripe ends in a point there, with no corner at the vertex. The case: the
rounding of an inside corner between a wall and another wall whose top edge
is rounded, the first wall's contact leaving it across its top edge before
the corner's edge reaches the top. Before, the corner at the vertex gave an
invalid result or failed.

### 0075-blendfunc-constrad-rolling-ball-section

The walked constant-radius blend between two surfaces (`BlendFunc_ConstRad`,
and `BlendFunc_ConstRadInv` for its ends on restrictions) is a true rolling
ball: the ball's centre lies at the radius along both surfaces' unit
normals, and each section is its arc in the plane through the centre that
holds both contact normals. The guide's parameter only picks the ball (the
contact points' mid-point in the guide's normal plane). Before, both contact
points lay in the guide's normal plane with the normals projected into it,
which is the rolling ball only where the normals are square to the edge
(0.014 off between a plate and a rod tilted by 40 degrees, radius 2).
Analytic blends were rolling balls already.

### 0076-chfi3d-extension-rolls-on-the-faces-own-surface

Past the end of a spine, where a contact leaves its face with no face
beyond, the blend rolls on the face's own surface, unbounded, when it is a
cylinder, cone, sphere or torus, instead of on its tangent plane at the edge
(`ChFi3d_BuildPlane`), so that it meets the piece before it.

### 0077-chfi3d-walk-past-the-spine-ends-before-a-singular-section

The walk into the straight extensions of the guide past the spine's ends
stops where a rolling ball's contacts run twice as fast along the guide as
at the spine's end, before the guide's planes come tangent to the centre's
curve and the sections get singular (where the approximation failed).

## Reports and tests

- 0010 to 0019 and 0050 to 0077: mitcad#121 and the tests in
  `geometry/tests/test_fillet_overflow.cpp` (for 0075 to 0077 also
  `geometry/tests/test_fillet_rolling_ball.cpp`), which use OCCT's fillet
  alone where they can.

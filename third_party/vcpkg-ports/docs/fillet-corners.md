# Fillet corners where a rounding ends: 0130–0540, 1140, 1170, 1210, 1220, 1240, 1290

The corners OCCT's rolling ball fillet builds where a rounding ends at a
vertex (`PerformIntersectionAtEnd`, `PerformOneCorner`), in the rounds of
mitcad#236 and the follow-ups mitcad#330, mitcad#339, mitcad#341 and
mitcad#342. Most cases are rebuilt with OCCT alone in
`geometry/tests/test_fillet_corners.cpp`. Back to the
[port's README](../README.md).

## Overview

- **0130 to 0139** make the fillet build corners where a rounding ends
  (mitcad#236).
- **0210 and 0211** build more corners where a rounding ends (mitcad#236).
- **0280 to 0283** build more corners where a rounding ends among several
  faces (mitcad#236).
- **0290 and 0291** walk round a corner's vertex through a face met at a
  point and end a rounding past the spine's end where one contact ran off
  its face (mitcad#236).
- **0340 to 0342** end roundings wider than a top or an end face, whose
  balls roll on that face's edge, where the faces at the ends cut them
  (mitcad#236).
- **0400** ends a rounding that vanishes where its faces come tangent at
  the vertex where its contacts reach it, and **0401** splits a rounding's
  end cut where it crosses the seam's line of a periodic face at the end
  (mitcad#330).
- **0410** ends a rounding whose end section lies in the face it runs into
  tangentially (mitcad#236).
- **0430 to 0432** let a rounding beside a face narrower than its radius
  roll on that face's far edge where its overflow would run across another
  rounding, and make the crease where two such roundings meet (mitcad#339).
- **0450** ends a rounding whose face turns tangentially into the face at
  its end there (mitcad#341).
- **0460** ends a rounding on the end edge of a rounding of its radius
  beside it, consuming the corner face between them (mitcad#342).
- **0490** ends a rounding at a face whose trimmed surface its opposite
  contact crosses only past the trimmed bounds (mitcad#236).
- **0540** ends a rounding that runs on into an earlier rounding of its
  radius with the section they share (mitcad#236).
- **1140** ends a blend cut by the face at the end past the spine's end
  vertex, along the line of that face's edge tangent to the other
  contact's face (mitcad#236).
- **1170** takes 1140's end wherever the edge's faces go on meeting along
  its curve continued, and over pieces given up at the end (mitcad#236).
- **1220** ends a rounding whose contact stops at a slight crease of its
  face square to the plane at the end: the ball turns on about the crease
  (mitcad#1203).
- **1240** walks the end of a short edge's rounding on past the spine's
  extension until both contacts reach the edges of the face at the end,
  where the extension (half the edge's length) stopped it short
  (mitcad#1256).

- **1210** rounds over the mouths of slots whose rounded ends touch the
  top face along a crest line, as a construction of its own where the
  corners fail there: the body rounded with slabs through the mouths cut
  out, the slabs put back with the blends prolonged across them and the
  slots' ends grown up to the blends (mitcad#1204).

## 0130: a rounding vanishing at a tangent end

The patches 0130 to 0139 make OCCT's rolling ball fillet build the
corners where a rounding ends that it failed on (mitcad#236);
`geometry/tests/test_fillet_corners.cpp` rebuilds the cases with OCCT alone.

### 0130-chfi3d-rounding-vanishing-at-a-tangent-end

A rounding whose two faces come tangent at the vertex where it ends vanishes
there. Where the faces are tangent there only to within micrometres (a
stored face, an arch whose radius misses the walls it springs from by a
micrometre), or the walk stops a little short of the vertex where its
sections get singular, the stripe ended in a section of micrometres off the
vertex, and the corner failed or built a cap of micrometres whose wire
intersects itself. Where both contact points of the end lie within a
hundredth of the radius of the vertex, both faces hold it and their normals
there are less than 0.002 rad from parallel, the walked contact curves' ends
(in 3d and on the faces) are moved to the vertex within their tolerance, and
the stripe ends in a point there with a degenerated edge
(`ChFi3d_ShrinkVanishingEnd` in `PerformFilletOnVertex`), as where the walk
reaches the vertex exactly.

## 0210 and 0211: more corners

The patches 0210 to 0219 continue 0130's corners where a rounding ends
(mitcad#236); the tests in `geometry/tests/test_fillet_corners.cpp` fail
without them.

### 0210-chfi3d-mitred-ends-at-a-short-sharp-edge

Two fillets that end alone at the two vertices of a sharp edge E (three
sharp edges at each), each with a contact on E's face S and its other
contact on an edge of E's other face (the face at both ends), and whose
contacts on E overlap (E shorter than their two setbacks on it), were each
cut by the face at the end as if the other were not there; the second cut
crossed the first and the corner failed ("fillets have too big radiuses").
The two blends are now mitred (`ChFi3d_Builder::PerformMitredEnd`, before
the one-stripe corner in `PerformFilletOnVertex`; both ends are made at the
later of the two vertices): each ends in two curves, stored as the big
stripe's end of `PerformTwoCornerbyInter` is, the blends' intersection from
where their contacts on S cross up to the face at the end (shared by both),
and its cut by that face from there to its contact on the face's edge. The
contacts on S end where they cross, and E vanishes. Two cylinders of one
radius whose axes cross meet in an ellipse (`IntAna_QuadQuadGeo`), whose
shorter arc is the mitre, exact; other blends are intersected by
`ChFi3d_ComputeCurves`' walk.

### 0211-chfi3d-end-cut-by-a-periodic-face-across-its-seam

Where the face at a rounding's end (an OnDiff or AllSame corner) is periodic
and its seam runs between the two arcs that hold the rounding's contacts,
the arcs' curves on the face lie a period apart, and the cut of the blend by
the face was sought between them round nearly the whole face (it then
crossed the other roundings' ends, or the body came out invalid). The second
point is now taken within half a period of the first (`PerformOneCorner`),
and the cut crosses the seam, where it is split as before.

## 0280 to 0283: corners among several faces

The patches 0280 to 0283 build corners where a rounding ends among
several faces (`PerformIntersectionAtEnd`, `PerformOneCorner`,
mitcad#236).

### 0280-chfi3d-onsame-end-where-the-face-at-the-end-turns-tangentially

A convex corner's top edge rounded up to the vertex where the top plane
turns tangentially into a rim's torus: the wall's contact reaches the wall's
edge with the torus before the vertex, the top's contact crosses the top's
tangent edge with the torus past it. The files' bodies cut the blend with
the torus up to the point where the top's contact meets that edge, which
runs on to it; OCCT failed at the corner. The top's contact is a circle that
meets the torus only on its prolongation across its parameter origin, which
the reframing in `Update` put out of range: such a point is now taken where
nothing else is found. The missing end of the prolonged edge was sought as
the intersection of two faces that touch along it: it is now the edge's own
curve and p-curves continued to the contact's end
(`ChFi3d_ProlongTangentEdge`).

### 0281-chfi3d-walk-round-the-vertex-past-a-short-edge

The walk round the vertex in `PerformIntersectionAtEnd` takes, at each
vertex it passes, the common edge of the faces on both sides there (two
faces may share several edges; the first found was taken, the far end of a
groove), and an edge it reached past a short edge of the vertex (13
micrometres, the corner's wall above a rim's torus) is prolonged from the
vertex where the walk reached it, the short edge in the removed corner
(before, from the corner's vertex, which is off the faces' intersection:
self-intersecting wires).

### 0282-chfi3d-end-cut-past-the-bounds-of-the-faces-at-the-end

A wall's foot rounded along the floor up to where a hole in the floor beside
the wall begins: the floor's contact meets the hole's side, the wall's runs
on. The files' bodies cut the blend with the hole's side and corner chamfer
continued up through it; the second contact's end is now sought on the last
face's unbounded surface where its bounded one gives none (fillets; chamfers
already since 0143).

### 0283-chfi3d-standing-end-piece-one-face-with-the-one-before

There, the blend's piece along the next edge of the line, whose floor
contact stands still, is one face with the piece before it
(`ChFi3d_JoinStandingEnd`: it takes that piece's surface, its p-curves
mapped by 0241's `ChFi3d_AnalyticParamMap`), and the corner puts no section
between them; before, the blend was two faces where the files' bodies have
one.

## 0290 and 0291: the walk round the vertex

The patches 0290 and 0291 go on with the corners where a rounding ends
(`PerformIntersectionAtEnd`, `PerformOneCorner`, mitcad#236).

### 0290-chfi3d-walk-round-the-vertex-through-a-face-met-at-a-point

The walk round the vertex steps, at each vertex it passes, from the face
across one edge of the stripe's face to the face across the next, which
share an edge where three faces meet. Where more meet (a cavity's ceiling
and a rib's top under a groove along the wall, coplanar, meeting at the
point where the groove's side crosses the rib's side) they share none and
the corner failed ("Failed to find edge."). The faces round that vertex
between the two are now faces at the end (`ChFi3d_FacesBetweenAtVertex`),
where the two share no edge at all; a face the fillet crosses at a single
point gets no curve, the edge beyond it is prolonged to that point (the
nearest extremum on the faces' intersection curve, the short way on a
periodic one) with the orientation of the edge it prolongs; the edge crossed
from the first face, its ends on both stripe faces, is cut on the side of
the end where the walk reached it; and the OnSame edge beside the face
holding the contact is prolonged up to the contact (as 0026 does).

### 0291-chfi3d-end-past-the-spine-where-a-contact-ran-off-its-face

A rounding along a top face's edge with a wall, where the top narrows to
nothing at the spine's end and the wall runs on past that vertex into a face
tangent to it: the top's contact runs off the top before the end, and the
piece walked from there past the spine's end on the top's tangent plane has
that side given up (`ChFi3d_Purge`). The corner dropped that piece and
failed. The files' bodies run the blend on past the vertex, cut along that
side by the sloped face beyond the top and at its end by the face beyond the
wall. The given-up piece now takes the surface of the elementary blend
before it where it lies on it, its p-curves projected
(`ChFi3d_ExtensionOnNeighbourSurface`, one face up to the corner);
`PerformOneCorner` keeps it where both its ends at the corner lie on edges
and the piece before it does not end on edges; and
`PerformIntersectionAtEnd` takes, as the given-up side's face, the face it
had before, and bounds such a blend wider along its spine where it does not
cross an edge between the faces at the end within its bounds.

## 0340 to 0342: OnSame ends of roundings wider than their face

The patches 0340 to 0342 go on with the corners where a rounding ends
(mitcad#236): OnSame ends of roundings wider than the face they end
against or than their top, whose balls roll on that face's edge (0170).

### 0340-chfi3d-onsame-end-cut-by-the-face-of-the-edge-the-ball-rolls-on

A ridge on a rod, its lens-shaped end face (2.5 high) rounded with 5: the
ball touches the rod and rolls on the lens's edge with the ridge's side, an
edge from one end of the spine to the other, and past each end rolls on
until it comes tangent to the ridge's side. OCCT took the lens for the face
at the end and failed ("bouchon non ecrit", "no point on arc"). Now the
ridge's side itself is the face at the end (`ChFi3d_RolledEndOnItsFace`,
`PerformOneCorner`): it cuts the blend from the point of tangency to the
rod's contact, its edge with the rod prolonged; the rolled edge goes from it
on both sides of that point.

### 0341-chfi3d-rolling-blend-walked-again-across-a-seam-past-its-ends

The same where the rod's seam runs past an end between the lens's tip and
the point of tangency: the walk stopped on the seam (0011 then ends a
rolling blend at the spine's end), short of that point. The blend is walked
again from its middle with the rod's surface grown, its seam half a period
away (`RolledEndToEndAcrossSeam`).

### 0342-chfi3d-rounding-rolled-on-past-its-edge-cut-by-the-faces-at-the-ends

An ear on a boss, its top narrower than the radius between the boss and a
chamfer, the top's edge with the boss rounded: the ball rolls on the top's
edge with the chamfer, whose ends lie on the ear's sides; OCCT turned it
about the ends (0171) and rolled it on the sides' edges, and the corners
failed or the result was invalid. The blend is walked again rolling on the
edge's line, the edge of a strip of the chamfer's surface, up to the spine's
ends (`RolledOnProlongedEdge`), and each end is an OnSame corner cut by the
ear's side from the edge's end (`ChFi3d_RolledPastEdgeVertex`), also at a
vertex of more than three edges where the side meets the boss at the vertex
only, their new edge the surfaces' intersection from there
(`PerformRolledPastEnd`, `ChFi3d_CurveFromVertexToContact`).

## 0400 and 0401: two exceptions inside a rounding's computation

The patches 0400 and 0401 (mitcad#330) fix two exceptions inside a
rounding's computation; `geometry/tests/test_fillet_corners.cpp` checks
each with OCCT alone, and each check fails without its patch.

### 0400-chfi3d-rounding-vanishing-at-its-end-stops-at-the-vertex

Where a rounding's two faces come tangent at the end vertex of its spine (a
tangency extremity), its walk's last piece may end a little short of the
guide's end, where both contacts reach the edges that end at the vertex
within the vertex's tolerance (on stored bodies whose edges lie within their
tolerance of it). The face beyond them continued the contour, and the stripe
went on with a piece from the vertex whose sections, both contacts on that
face, had no width; its approximation threw (`BlendFunc::GetMinimalWeights`
on a zero angle, "Geom_TrimmedCurve::U1 == U2"). Now such a piece ends the
stripe (`ContactsAtTangentEnd`), and the end is the singular corner, a point
at the vertex, as where the walk reaches it exactly.

### 0401-chfi3d-end-cut-across-the-seam-line-of-a-periodic-face

`PerformOneCorner` split the cut of a rounding's end by a periodic face at
the end only where the extrema of the cut's and the seam's 3d curves met
within `Precision::Confusion()`. Where the seam starts at the corner's
vertex the two curves meet only within the cut's approximation (2.4 · 10⁻⁶
on a gable's ridge ending on a cylinder's seam), or the cut passes just
beyond the seam's start within the vertex's tolerance (a stored body): the
cut was stored unsplit, running across the seam, the seam was lost and the
face came out unorientable (or the body without its floor). Now the cut is
also split where its curve on the face crosses the seam's line
(`ChFi3d_CutCrossesSeamLine`): at the seam's end vertex where it passes
within that vertex's tolerance (the seam keeps its vertex), else at the
seam's point there as before; the pieces' curves on the face are the cut's
own, each in the period of its arc.

## 0410: an end section in the face the rounding runs into

The patch 0410 goes on with the corners where a rounding ends (mitcad#236,
sixth round); `geometry/tests/test_fillet_corners.cpp` checks it with OCCT
alone.

### 0410-chfi3d-rounding-ending-in-the-face-it-runs-into

A vault's end edge rounded with the radius of the block's rounded vertical
corners under it. The ball overflows the corners onto the end face, and
where the vault springs tangentially from the side walls the blend's end
section is the corner's own quarter circle: the blend runs on into the
corner tangentially. OCCT found no face at the end holding both end points
(one is the vertex, the other lies on the end face's edge with the corner,
on no arc), went on to its corner of several faces and failed there ("Failed
to find edge"). Now `ChFi3d_EndSectionOnFace` (in `PerformOneCorner`, not
OnSame, where no face at the end is found otherwise) takes the face at the
vertex that holds the whole end section for the face at the end, gives a
point on no arc the edge of that face it lies on (with the transition of a
contact crossing it), and keeps a point at the vertex on no arc, so that the
faces beyond keep their edges up to it; the cap is the section itself, not
the intersection of two surfaces tangent along it, and the spine's edge is
cut at the vertex as at a singular end. A walk that reached the spine's end
vertex with one contact at a section before the guide's end failed on the
next step from there and invalidated the stripe; the blend now ends there
(`ChFi3d_ContactAtSpineEnd` in `PerformSetOfSurfOnElSpine`).

## 0430 to 0432: overflow across another rounding

The patches 0430 to 0432 come from the fillets left failing at a vertex
after mitcad#323 (mitcad#339, `core/import/docs/f3d-rounds-2.md`, *Fillet corners
left after mitcad#323*): roundings beside faces narrower than their radius
whose overflow (0011) ran across another rounding.

### 0430-chfi3d-rounding-rolls-before-another-roundings-face

The two vertical edges at the end of a window through a wall 3.54 thick
rounded with 2, the end split into a strip 0.5 wide and the rest, which
meets the strip at 0.35 degrees. The rounding beside the strip ran over it
onto the other face (0011; at 0.35 degrees OCCT also takes the two faces for
tangent), up to 2 from its edge, across the other rounding's contact there,
and the corners at the window's top and bottom failed ("fillets have too big
radiuses"). The stored designs' rounding rolls on the strip's far edge.
`ChFi3d_Builder::RollsInsteadOfOverflow`, once the stripes are computed: a
side on a face reached across a narrow face of its edge, whose contact there
lies nearer another stripe's edge than that stripe's own contact, may not
overflow onto that face; the stripe is computed again (as before where that
fails), its start found as an overflow's, and that side rolls on the edge
before the face (also where the two faces only nearly continue each other:
their normals more than 1e-4 apart, and before a sliver it passed).

### 0431-chfi3d-rolled-onsame-end-at-the-end-of-its-contact-curve

The OnSame end of such a blend rolling on an edge up to where the edge ends
on the face at the end: the intersection of its contact curve with that face
came out 6e-15 past the curve's bounds, was not taken, and the corner failed
("bouchon non ecrit"). It is now taken within the curve's parametric
resolution, at the bound.

### 0432-chfi3d-roundings-crease-over-a-narrow-common-face

A box's rim with an inner lip whose bottom edge, lines and corner arcs
meeting at 53 degrees, is rounded with 4 over a ring 1 wide under the lip
and a chamfer beyond it. Each rounding ran over the ring onto the chamfer,
the lines' onto its planes and the arcs' onto its cones, and at each corner
the two had no face in common ("Trying to check orientation of NULL face",
"Failed to find edge"). The stored designs' roundings roll on the ring's far
edges and meet in a crease. `RollsInsteadOfOverflow` also lets two stripes
meeting at a vertex roll before the different faces beyond their consumed
common face; `ChFi3d_IsInFront` no longer takes two sides on no face for a
common face; `ChFi3d_FilBuilder::PerformTwoRolledCorner` makes the crease
(the blends' intersection from their point on the pivot edge to the rolled
edges' common vertex, the stripes ending on it as in the bevel of
`PerformTwoCornerbyInter`), and a walked blend rolling on a circle about the
axis of the cylinder or plane it touches becomes the torus it is
(`ChFi3d_ExactRollOnCircle`, a B-spline patch of it in the walked surface's
layout), which reaches the crease past the walk's end.

## 0450: a face turning tangentially into the face at the end

The patch 0450 comes from the fillets left failing at a vertex after
mitcad#339 (mitcad#341): a rounding ending where one of its faces turns
tangentially into the face at the end.

### 0450-chfi3d-rounding-face-turning-tangentially-into-the-end-face

A rounding ends at the spine's end vertex, where one of its faces (an
earlier rounding) runs on tangentially into the face at the end and the
other meets that face at a sharp edge. At the end of the walk both contacts
reach an edge of their faces; the sharp one ends the walk, the tangent one
did not, and `ComputeData` walked that side on along the face at the end
while the other lay on a tangent plane past its edge: the pieces at the end
were not as `PerformMoreSurfdata` expects, or the corner was invalid. Where
one side's edge goes on tangentially into a face that holds the spine's end
vertex and that the other side's sharp edge bounds too, that face is now the
face at the end on both sides (`TangentIntoEndFace`): the walk ends there
and the corner cuts the blend with it, the cap running from one contact's
crossing of the tangent edge to the other's crossing of the sharp edge. The
tangent side often reaches its edge past the spine's end, where 0077 cut the
walk at twice the contacts' speed at the spine's end, short of that edge: a
line that ends on an edge is kept up to it while its contacts run less than
four times as fast (`ChFi3d_RegularUpToArc`), and the ends above are taken
only where that holds; past it the sections get singular, and such ends fail
at the corner as before.

## 0460: an end on a rounding of the same radius

The patch 0460 goes on with 0410 for the corners where a rounding ends
(mitcad#342): a rounding that ends on the end edge of a rounding of its own
radius beside it.

### 0460-chfi3d-onsame-end-on-an-edge-of-a-consumed-corner-face

A boss 1.5 high on a wall, its long top edges rounded with 2 (rolling on the
foot edges), the feet of those roundings with 2 and the ends' edges with the
roundings with 1.5; then the ends' feet are rounded with 2. At each corner
the ball fits the foot rounding beside it, so the end section is that
rounding's end arc, from the corner's vertex to the arc's other end, where
the opposite contact ends; the corner torus between them (0.5 wide) lies
wholly between the contacts. OCCT took the torus for the face at the end of
its OnSame corner and intersected the blend with it, tangent to it all along
the arc ("echec calcul intersection"). `ChFi3d_EndSectionAlongEdge`
recognises such an end (the points at the vertices of the edge to prolong,
the section on its curve, the face at the end bounded otherwise by the spine
and the opposite face): the face beyond that edge becomes the face at the
end, and the end is built as 0410's end section lying in its face, the cap
the edge's own curve, taking the edge's place there; the consumed face's
edges off the spine are cut at their vertices, so that the builder drops it
(as `ChFi3d_CutConsumedFaces` drops consumed strips). No prolongation of the
opposite contact is made: it ends at the edge's vertex.

## 0490: an OnSame end on a trimmed surface

The patch 0490 goes on with the corners where a rounding ends (mitcad#236,
seventh round): the face at an OnSame end on a trimmed surface.

### 0490-chfi3d-onsame-end-on-a-trimmed-face-past-its-bounds

A plate's bottom edge rounded with 3 up to a conical foot below the plate
(the foot's rim on the bottom, the cone cut flush with the plate's side).
The cone is the face at the end of the OnSame corner, which intersects the
opposite contact (on the side, 3 above the bottom) with that face extended
and caps the blend there. The cone continued above its rim crosses that
contact before the edge's end, but stored bodies carry the cone on a surface
trimmed at the face's bounds, which `ChFi3d_ExtendSurface` (B-splines and
Beziers only) left bounded: no intersection, no cap ("OneCorner : bouchon
non ecrit"). Where that intersection fails, the face at the end is now taken
as its basis surface continues (`ChFi3d_UntrimmedEndSurface`: an elementary
basis, unbounded where not periodic, one period from the trimmed start where
periodic, the face's own parameters); the cap and the prolonged edge are
built there, and the face keeps its surface (a new surface given to the face
lost its seam's second p-curve in the topological build). Ends whose bounded
face is crossed are unchanged.

## 0540: a rounding running on into an earlier rounding's tube

The patch 0540 goes on with the corners where a rounding ends (mitcad#236,
eighth round): a rounding that runs on into an earlier rounding's tube.

### 0540-chfi3d-rounding-running-on-into-an-earlier-roundings-tube

A wall's foot rounded with the radius of an earlier rounding round the
wall's end, whose tube leaves the wall's side tangentially (its spine
curving away from the side's foot) and was cut back to the wall's plane,
where its floor contact crosses it: the new rounding's spine ends at that
crossing, and its section where the two spines are tangent lies in the
earlier tube. At that OnSame end the earlier rounding is the face at the
end; the corner intersected the opposite contact with it, which it only
touches, and the blend with it, tangent to it all along the section
("OneCorner : bouchon non ecrit", "OneCorner : echec calcul intersection").
Now `PerformOneCorner` takes the point where the contact touches the face's
surface within the tolerance, the blend tangent to the face there
(`ChFi3d_ContactTouchesEndFace`), caps the blend with its end section where
that lies in the face's surface, tangent to it (as an end section lying in a
face at the end is since 0410), and prolongs the earlier rounding's edge
with the opposite face along the iso of the face's surface the edge runs on
where the edge's curve ends at the vertex (a stored B-spline;
`ChFi3d_ProlongTangentEdgeAlongIso`, as `ChFi3d_ProlongTangentEdge` does
with a curve that goes on past it). The earlier rounding grows up to the
section, as in the files' bodies.

## 1140: a cut to the end past the vertex along a tangent edge

The patch 1140 goes on with the corners where a rounding ends after an edge
overflow (mitcad#236, ninth round): the face at the end cuts an analytic
blend whose contact ran off its face, and the cut's end lies on the line of
an edge of that face past the spine's end vertex.

### 1140-chfi3d-cut-to-end-past-the-vertex-along-a-tangent-edge

A bar whose edge along the floor is rounded with 3, and beside it a block
that reaches further out, its own edge along the floor rounded with 8, its
floor going 0.073 mm further than the bar's: the block's face beside the
bar is a step between the two profiles that narrows to a point at the
vertex V where the bar's rounding comes tangent to the floor. The step's
foot rounded with 0.5 (its edge with the floor, on tangentially up the
block's rounding): near V the step is lower than the ball, its contact
leaves the step across its edge with the bar's rounding B, and B cuts the
blend all along from there (`ChFi3d_KPartCutToEnd`). That cut ends where
the blend's other contact meets the edge Eq of its face Fo (the floor) with
B at V; Eq ends at V, and the contact meets Eq's line only 0.5 past V. No
end was found; the corner cut the walked blend with B and looked for the
floor's edge with the block's rounding to cross B, which passes 0.9
micrometres above it, and failed ("the edge beside the end does not cross
the face at the end").

Where Fo and B are tangent along Eq and N narrows to a point at V (the
spine's end edge and N's edge with B leave V the same way), their surfaces
go on meeting along Eq's line beyond V. (Not where N narrows to a point at
that edge's other end too, a sliver of N between B and two faces whose
other blend leaves N across the same edge, as a window's floor rounded on
both edges up to a pillar's rounded end: the corners cut those as before.)
The end is now taken there, a new vertex on Eq's curve
continued and on both surfaces, where the blend ends in a point. B grows up
to the cut, Eq is prolonged beyond V to the new vertex
(`ChFi3d_ProlongEdgeBeyondVertex`), and the spine's end edge, under the
blend, is cut at V with no side kept. The piece whose contact leaves its
face may lie on the spine's end edge itself (a step's foot on a rounding
that a smaller rounding of the bar meets tangentially); it then still has
the parameters of that edge's whole blend, extended past the edge, at its
end (an OnSame end's corner sets them): its end section is taken through
its other contact's point there, by bisection on the edge as the end's,
and its parameters at that end are those of its points (on a closed
contact line, in the period that keeps the piece's direction; restored
when no pieces are added). In the stored design the rounding is the file's: the
boolean differences with the file's body are empty both ways, the change
of volume 0.04 % off the file's, and the bar's rounding grown past the
step by the file's 0.41 mm².

## 1170: the end past the vertex along faces that go on meeting

The patch 1170 goes on with 1140 (mitcad#236, tenth round): a rounding round
an outline's corner up to a wall standing on the top across the corner's
rounding.

### 1170-chfi3d-cut-to-end-past-the-vertex-along-faces-meeting-on

A rounding of 1 along a top's outline, round the outline's corner (rounded
with 2) up to the vertex where a wall standing on the top crosses the
corner's rounding at a sharp vertical edge. The contact on the top runs
onto the wall's foot already on the straight edge before the corner: the
analytic piece round the corner lies wholly past the wall, and so does the
straight piece's start, kept as an extension with its top side given up.
`ChFi3d_KPartCutToEnd` took that extension for the first piece and gave
up; the corner capped the straight piece's prolongation with the wall,
which it does not meet ("bouchon non ecrit"). Now pieces at the end with a
side given up come before the first piece on both faces, and the cut
pieces take their place where the end is past the vertex (elsewhere such
an end is left as before). 1140's end past the vertex no longer needs the
edge's faces tangent along it and the face the contact leaves narrowing to
a point there: it needs the edge's curve continued up to the end on both
faces' surfaces (checked at four points), which holds for faces tangent
along it and for a wall and a cylinder running on along their vertical
edge. The wall grows down to the cut, its edge with the corner's rounding
prolonged down to the end, as in the files' bodies.

## 1220: the ball turning about a crease at the end

The patch 1220 follows mitcad#236's rounds (mitcad#1203): a rounding along
a bar's end face whose contact on the bar stops at a crease with a concave
arc that just overlaps the bar.

### 1220-chfi3d-ball-pivots-about-a-crease-at-the-end

A rounding along the outline of a bar's end face, a plane, where a concave
arc meets the bar with a small overlap: the bar's cylinder and the arc's
meet along a line L square to the end face at a slight crease, and the bar
keeps only a sliver between L and the end vertex, rounded along its rim.
The walk stops at L's section (the contact on the bar reaches L, the one
on the end face lies inside it), and the corner failed ("Corner OnSame :
no point on arc"). The file's body rounds on with the ball turning about
L while it touches the end face, until it reaches the end face's edge: a
horn torus about L, cut by the faces it meets (the sliver, its rim
rounding, the arc's cylinder).

`PerformPivotAboutCreaseAtEnd`, a construction of its own beside 0951's
`PerformPivotAtStepEnd` and tried after it, builds that: where the end
piece's contact on one face stops at a point P inside a line edge L of it,
square to the other face, a plane, at a crease of less than 30 degrees
with the face beyond L, and the end's vertex lies past L (where it is L's
own end, as for a rounding stopped at a crease, the faces beyond L cut the
end as before), the stripe ends at L's section (the pieces past
it dropped) and a corner stripe of the horn torus about L closes the end.
The torus's quarter towards the plane is sectioned with the solid
(`BRepAlgoAPI_Section`), and the chain of section edges from P to the
plane gives the pieces of the corner stripe, one face: the turn about P,
its side on the crease face degenerated at the apex, then one piece per
face cut, all against the plane along the circle about L's foot up to the
plane's edge with the last face cut. At a first end the corner stripe
comes before the stripe, its pieces reversed. The degenerated side's
transition is the one that orients the face as the contact's does
(`ChFi3d_FilDS` orients a blend face's edges from side 1's alone).

## 1240: a short edge's end walked on until both contacts leave

The patch 1240 follows mitcad#1253 (mitcad#1256, GitHub mitcad/Mitcad#130):
a short slanted edge of an imported STEP part, rounded with a radius larger
than half its length, whose end vertex lies on an earlier rounding.

### 1240-chfi3d-short-end-walked-on-until-both-contacts-leave

At a vertex of three edges where the face at the end cuts a rounding
(AllSame, OnDiff), the blend is walked past the vertex over the spine's
extension until both contacts have left their faces across that face's
edges, and `PerformOneCorner` cuts it with the face at the end between the
two points. `ExtentOneCorner` prolongs the spine by half the edge's length
whatever the radius. On the reported part a thin wall meets an overhanging
end band along an edge of 0.84 whose top vertex lies on a rounding of 2:
rounded with 0.75 or 1, the contact on the wall meets the rounding's edge
0.16 past the vertex, the contact on the band only 0.49 (0.63 at 1), past
the extension's 0.42. The walk stopped with that contact inside the band,
`FindFace` found no face holding both points, and
`PerformIntersectionAtEnd` built a corner face folded back on itself (0.035
out of the body at 0.75, accepted by the checker) or an invalid shape (at
1, where the bottom end stopped short of the floor too). After the part's
floor was rounded, the edge's remaining sliver of 0.25 gave spikes 0.16
and 0.77 out of the body.

`PerformSetOfSurf` now walks such an end again: where the last piece at an
AllSame or OnDiff end reaches the end of the extension with a contact still
inside its face (the other on an edge of the vertex, or both inside,
`ChFi3d_EndStoppedShort`), the stripe is computed again with that end's
extension longer by twice the largest radius (the walk still stops where
both contacts have left). The new walk is kept where both contacts of each
such end then lie on edges; otherwise the first walk is made again, as
where a narrow face at the end pokes through the blend and two ends are
joined over it (0933). Every other rounding is as before.

On the reported part both corners build one blend face, valid, at 0.5,
0.75, 0.9 and 1, and the sliver at 0.5 to 1, without the warning of
mitcad#1253. A synthetic body with the same corner (planes and a rounding
cylinder, its faces converted to B-splines so that the blend is walked)
builds one blend face at 0.5, 0.75 and 1, removing within 1e-5 mm3 what
the same rounding removes from the body of planes, where OCCT builds a
cylinder exactly; before, 0.75 and 1 built a folded corner face 0.32 out of
the body and an invalid shape.

## 1210: a rounding over slot mouths that touch the top along a crest

### 1210-brepfilletapi-rounding-over-slot-mouths-touching-the-top-along-a-crest

A rim of eight arcs (a circle of radius 93.5 between a top plane and eight
pieces of an outer cylinder, 2.17 high above a rounding of 5) rounded with
2.5. At each of the eight vertices between the arcs a fin 1.5 thick stands
out of the wall; its top is a cylinder of radius 0.75 about an axis 0.75
under the top, so that the top plane and that cylinder touch along the
crest line with no edge there. Four edges meet at each vertex: the two
arcs and the two curves where the fin's cylinder cuts the two cylinder
pieces. The chain stops there, and the corner's walk round the vertex
finds no edge between the top plane and the fin's cylinder
(`ChFi3d_Builder_0.cxx`, "Failed to find edge"): the fillet fails at all
eight vertices. The file's body after the rounding runs each arc's blend
on past the vertex over the fin's top, cut by the fin's side wall and then
by its top cylinder, which grows inwards to the blends up to the crest's
point on the top plane, (90.978, 0, 307.4635) at the mouth on the x axis,
where the two blends meet.

No corner builder of ChFi3d cuts a blend with faces that do not hold the
vertex's edges, so this is a construction of its own, outside ChFi3d's
corners: `BRepFilletAPI_MakeFillet::Build` tries `BuildOverSlotMouths`
(`BRepFilletAPI_MakeFillet_SlotMouths.cxx`) where the computation failed
only at faulty vertices, each of them such a mouth (`FindSlotMouth`: four
edges there, two of them rounded with the same constant radius and
tangent there, between a plane T and two faces A and B; the other two
between A and a cylinder C and between B and C; C's radius smaller than
the rounding's, its axis parallel to T and under it by C's radius, the
vertex on the crest line). It

1. cuts a slab through each mouth out of the body: C's diameter wide,
   from four radii inside to four outside along C's axis, from three radii
   below the crest to one above;
2. rounds that body with the same edges' images, radii and parameters
   (`SetParams`, `StopChainsAtTurns`): each arc's blend now ends at the
   slab's walls, a plain end OCCT builds;
3. puts each slab's part of the body back less what lies past the blends
   beside it: each blend's surface prolonged across the slab (by twice the
   slab's half width along the rounding, and past its contact away from
   the top by six), a half slab round its end section split by it and by
   the top's plane, and the piece holding the rim's corner taken; each such
   piece less the fin's top under C (the part under C's axis and inside C,
   prolonged along it, wider and longer than the slab), one piece at a
   time, so that C and the fin's walls grow up to the blends; then the
   body in the slabs less all those pieces at once;
4. fuses that with the rounded body (each boolean once for all the slabs,
   run in parallel), and takes the result only where it is a valid single
   solid.

Cut all at once, the pieces of a slab's two halves (which touch) and the
fins' tops sharing faces with them came out wrong at some mouths, a fin's
top left 0.75 lower than the file's at one mouth of eight; one piece at a
time they all come out alike.

The result's history gives each face from the body's face on its surface
(or the one it lies on), or as generated from the rounded edge whose blend
or prolonged blend it lies on, and the body's faces with no image as
removed (`Generated`, `Modified` and `IsDeleted` read it).
`ChFi3d_Builder` gives its shape, parameters and `StopChainsAtTurns`
setting for the inner rounding (`InitialShape`, `Params`,
`StopsChainsAtTurns`, `ChainTurnThrough`). Every other fillet is as
before.

On the file's body before the item: valid, 304 faces, every face of the
result on the file's body within 2 µm and every face of the file's above
the rim's rounding on the result within 2 µm (OCCT's walked blend against
the file's along the arcs); the boolean differences of the two bodies are
empty both ways with a fuzzy value of 2 µm (without one, OCCT's booleans
fail on the faces 2 µm apart). Against the file's body, the material within
a wedge of 6 degrees round a mouth is 0.04 mm³ more, within the arc beside
it (39 degrees) 0.19 mm³ more. The rounding of the rim takes about 10 s.

## 1290: no end past the vertex on a face closed round its surface

The patch 1290 restricts 1140 and 1170 (mitcad#1336): a pin of radius 4.5
standing on a plate's top flush with the plate's end, a half round of the
pin's radius whose sides are tangent to the pin, the pin's cylinder and
the plate's end one face, closed round the axis. The pin's foot is rounded
with 1 along its arc of 302 degrees; its ends lie where the sides start,
and patch 0911 has the corner cut them by the sides, which grow up to the
rounding, as the stored design's body has it (`5ef2dbd5` item 51,
round 12 of mitcad#343).

### 1290-chfi3d-cut-to-end-past-the-vertex-not-on-a-face-closed-round-its-axis

From patch 1140 on, `ChFi3d_KPartCutToEnd` took those ends past the
vertex: the side's edge with the pin, Eq, meets the other contact's line
only beyond the vertex, along its line up the pin. But that line is a
closed circle there: the analytic piece's parameters at its ends were the
whole line's, a period apart (the last end's section on the line at the
start of the period), so the pieces cut from it ran the wrong way round;
and at one end Eq's line beyond the vertex is the pin's seam, which Eq was
prolonged over. The fillet came out not valid (two tori, one inside out,
or an unorientable cylinder), and the import's check refused it ("the
fillet would leave the body open"). The end past the vertex is no longer
taken where the other contact's face Fo has a seam: the corner closes it
as before 1140. The cases of 1140 and 1170 (a floor, a wall, a corner's
cylinder, none closed round its surface) build as before.

On the stored body before the item: valid, one torus, the material added
4.76572 against the file's 4.76602 mm³, as with 0911 alone.

## Reports and tests

- 0130 to 0139, 0210 to 0219, 0280 to 0289, 0290 to 0299, 0340 to 0349 and
  0410 to 0419: mitcad#236; 0430 to 0432: mitcad#339; 0450: mitcad#341;
  and `geometry/tests/test_fillet_corners.cpp`, whose tests of 0280 to
  0283, 0290, 0291, 0340 to 0342, 0410, 0430 to 0432 and 0450 each fail
  without their patch.
- 0400 and 0401: mitcad#330 and the valley whose floor's rounding vanishes
  short of its end and the gable's ridge ending on a cylinder's seam in
  `geometry/tests/test_fillet_corners.cpp`.
- 0460: mitcad#342 and the boss's foot rounding ending on a rounding of its
  radius in `geometry/tests/test_fillet_corners.cpp`.
- 0490: mitcad#236 and the plate on a conical foot with a trimmed cone in
  `geometry/tests/test_fillet_corners.cpp`.
- 0540: mitcad#236 and the wall's foot rounded on into its end's earlier
  rounding (its floor edges as circles and as B-splines) in
  `geometry/tests/test_fillet_corners.cpp`.
- 1140: mitcad#236 and the step's foot beside a bar's rounding tangent to
  the floor (failing without the patch) and beside a notch rounded tangent
  to the block's rounding, in both orientations, each against the block
  alone rounded less what lies in the bar's profile, and a tab's top
  rounded concave up to its side roundings tangent to a curved wall
  (closed contact lines), against the material under the rounding's torus
  within the tab's outline, in `geometry/tests/test_fillet_corners.cpp`.
- 1170: mitcad#236 and a block lowered on one side of a wall across its
  rounded corner, its lower top's edge rounded round the corner up to the
  wall, against the lower block alone rounded on the far side of the
  wall's plane (failing without the patch), in
  `geometry/tests/test_fillet_corners.cpp`.
- 1220: mitcad#1203 and a bar with a concave arc meeting it with a small
  overlap, its rim rounded, the end face's outline rounded (failing
  without the patch), against the horn torus about the crease, in
  `geometry/tests/test_fillet_corners.cpp`.
- 1210: mitcad#1204 and the case `fillet-slot-mouths` of
  `MITCAD_GEOMETRY_CASES` (`body.brep`, the reported design's body before
  the rounding; its tangent crests cannot be built with OCCT's booleans) in
  `geometry/tests/test_fillet_slot_mouths.cpp`: the eight arcs rounded with
  OCCT alone, valid, of the expected volume within 1e-6, the history's
  faces (failing without the patch).
- 1240: mitcad#1256 and, in `geometry/tests/test_fillet_step_corner.cpp`,
  the synthetic corner rounded with OCCT alone at 0.5, 0.75 and 1 against
  the body of planes (failing without the patch) and through MitCad's
  fillet, also after its floor is rounded; the reported part
  (`MITCAD_STEP_REPORTED`, skipped without it) at both corners and on the
  sliver, clean at every radius.
- 1290: mitcad#1336 and, in `geometry/tests/test_fillet_tangent_end_face.cpp`,
  a pin on a plate's end, the pin and the end one face with its seam along
  a side, rounded at its foot with OCCT alone: valid, one torus, the
  material added within 1e-6 of the ring at the foot within the sides'
  planes (failing without the patch: not valid, two tori).

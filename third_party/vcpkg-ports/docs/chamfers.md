# Chamfers wider than a face beside them: 0140–0149, 0380, 0381, 0480, 0481, 0530, 0531, 1090

The edge overflow of [0010 to 0019 and 0050 to 0077](fillet-edge-overflow.md)
for chamfers (mitcad#240, mitcad#263). `geometry/tests/test_chamfer_overflow.cpp`
rebuilds the cases with exact volumes on synthetic shapes with OCCT's
chamfer alone. Later chamfer cases from stored designs: 0440 in
[the census file](fillet-census.md), 0650 and 0671 in
[0550 to 0692](fillet-replay-0550-0692.md), 0941 in
[0930 to 0942](fillet-replay-0930-0942.md), 1000 and 1010 in
[0990 to 1020](fillet-replay-0990-1020.md). Back to the
[port's README](../README.md).

## Overview

- **0140 to 0149** make OCCT's fillet build chamfers wider than a face
  beside them (mitcad#240).
- **0380 and 0381** end a walked chamfer at a vertex where the face at the
  end is tangent to both its faces and keep a chamfer's guide on its edges
  (mitcad#263).
- **0480 and 0481** keep the parts of an open intersection line beyond its
  vertices and split a blend's piece over a gap at the start of a closed
  spine (mitcad#263).
- **0530 and 0531** keep a walked chamfer's guide on its edges beside
  analytic blends, build a chamfer wider than a face of its edges along part
  of its chain, the faces beyond that face growing down to it, and end a
  chamfer stopped where its edges turn by its blend run on past the vertex
  (mitcad#263).
- **1090** limits a corner's sharp edge only by a common point that lies on
  it: a convex and a concave chamfer meeting where a sharp edge goes on got
  a corner 32 mm high at one end of a bracket (mitcad#1005).

## 0140 to 0149: chamfers wider than a face beside them

The patches 0140 to 0149 continue 0010 to 0019 and 0050 to 0077 for
chamfers (mitcad#240, mitcad#263): chamfers wider than a face beside them;
`geometry/tests/test_chamfer_overflow.cpp` rebuilds the cases with exact
volumes on synthetic shapes with OCCT's chamfer alone.

### 0140-chfi3d-contact-along-a-closed-face-edge

A fillet or chamfer exactly as wide as its face where the contact runs along
a circular or closed edge (the inner rim of a step ring chamfered by the
ring's width; a lug inside a bore as wide as the chamfer of its edge with
the bore's wall). 0014's hatching of a contact on the boundary moved circles
by translating them, which crosses the boundary: a circle is now moved into
the face by changing its radius, a whole circle is hatched from a point
outside the face, and domains across the start of a closed line are taken
across it (`HatchOnBoundary`). A line on the boundary that the hatcher split
into partial domains is hatched on the boundary too (`LineOnBoundary`). On a
closed spine a piece reaching once round from before the start of the first
edge is no longer dropped (`SplitKPart`). A closed contact along a closed
edge counts as along it: its ends at the edge's vertex are that vertex, its
curve is cut at the vertex inside it, and the edge is cut where a closed
spine starts elsewhere on it (`ChFi3d_ContactsAlongFaceEdges`). Before, such
chamfers failed ("Path failed", or the walking that chamfers do not
implement) or kept the consumed face with zero width.

### 0141-chfi3d-onsame-end-second-contact-beyond-the-edge

The end of a chamfer in the OnSame state (`PerformIntersectionAtEnd`) whose
second contact runs on past the vertex to the face at the end, beyond that
face's edge (the foot of a rib on a wall chamfered wider than the rib's
bottom, which the rib's sides cut): the contact's point on the face's edge
prolonged is looked for on the face's unbounded surface, as for the first
contact. Before, with the chamfer's faces in that order, the point outside
the face was not found and the corner failed. Fillets keep the bounded face
(the unbounded one changed a rounding of a corpus design that the import
matched to one it does not).

### 0142-chfi3d-kpart-cut-by-faces-beyond-that-grow

0055 and 0071 for an analytic blend that adds material (a boss's foot under
a plate chamfered so wide that the contact on the plate runs a little past
the plate's rounded corner and sides): the faces beyond grow down to the cut
instead of being cut back. The crossings of the chain of faces beyond are
found on the prolongations of the edges between them beyond their ends
(`ChFi3d_KPartEdgeCrossing`); there the pieces meet at a vertex of their
own, the edge is prolonged up to it on both its faces
(`ChFi3d_ProlongEdgeBeyondVertex`), and the edges between the contact's face
and a face of the chain wholly under the blend are dropped. On a closed
spine the gap from the last piece round to the first is filled too, with the
parameters taken across the period's end; a cut that the intersection gives
in arcs of one closed curve is taken on that curve; the other contact at a
given end on its face's seam is put on the seam; and a contact along a
face's edge (0014) finds the side of its blend from the other contact's
nearest point (a cut piece's lines have other parameters). Before, such
chamfers failed (their walking is not implemented).

### 0143-chfi3d-chamfer-corner-beyond-the-common-face

The corner of two chamfers whose contacts on their common face leave it
across arcs into one face beyond before they cross (a hexagonal prism under
a disc, chamfered wider than the disc's underside near the hexagon's
corners; the disc's cylinder cuts the chamfers there). Where the corner by
intersection fails, `ChFi3d_ChBuilder::PerformTwoCorner` gives each stripe a
piece cut by the face beyond, from its contact's point on the arc to where
the chamfers' intersection meets that face, and computes the corner between
those pieces, on the face beyond; a seam of the face beyond whose
prolongation that point lies on is prolonged to it on both its sides. While
that corner is computed (a flag of the calling thread), `ChFi3d_IsInFront`
takes the corner vertex's parameters on the face beyond from the nearest
point of its surface; elsewhere it fails as before. Before, the corner
failed.

### 0144-chfi3d-closed-contact-split-at-the-edges-vertex

A closed contact along a closed edge (0140) that passes the edge's vertex
inside the stripe (a step ring's rim chamfered or rounded by the ring's
width, the counterbore's seam turned away from the rim's vertex) splits the
SurfData there into two on one blend surface, one face as 0073's pieces,
each with lines of its own and its p-curve on the face beyond in that face's
period (`ChFi3d_SplitAtVertexOnLine`). 0140 cut the line at the vertex: the
part past the seam had its p-curve on the counterbore's wall a period beyond
the face, and the topological build translated the shared p-curve on the
blend in place for one part, moving the other's too (an unorientable face;
for the rim in two arcs a valid shape whose p-curves on the cone missed
their edges). `ChFi3d_PCurveInFacePeriod` moves a face interference's
p-curve on its face by whole periods into the face's bounds.

### 0145-chfi3d-kpart-pieces-in-their-faces-periods

On a closed spine, the pieces cut by faces beyond over a gap (0055,
0071, 0142) past the spine's start held the other contact's p-curve a
period beyond its face's seam (the boss of 0142 longer than the chamfer,
its seam in the gap: an unorientable face); once the gaps are filled
each piece's p-curve on its face is taken into the face's period.

### 0146-chfi3d-kpart-pieces-one-face-round-a-closed-spine

0073's join of a blend's pieces into one face on a spine closed by one edge:
the piece across the spine's start holds its spine parameters a period on
(the gap between pieces is now taken modulo the period), a run of pieces cut
by faces beyond may go on across the start (runs are followed round the
spine, each piece taking the surface of the one before it in its run, with
no run making a loop), and the joined pieces' p-curves on the blend go on
from the piece before's in its period (`ChFi3d_PCurvesOnSurfAfter`). The
cone of 0142's boss came in four or five faces; now it is one, as in the
file's body.

### 0147-chfi3d-onsame-end-prolonged-along-a-seam

The OnSame end of a stripe whose face at the end meets the opposite face
along that face's seam (a lug in a bore, its top's edge with the wall
blended as wide as the top, the lug ending at the bore's seam).
`PerformOneCorner` takes the prolonged edge's p-curve on the opposite face
as that face uses the edge (an edge left from a seam, used once, keeps both
p-curves; the other one put the prolongation where the blend covers the
wall), and cuts the seam where the prolongation along it ends, its part
along the prolongation kept on neither side (`ChFi3d_CutSeamAlongCurve`).
Before, the wall's wire intersected itself.

### 0148-chfi3d-chamfer-as-wide-as-a-ledge-along-a-line-and-an-arc

A wall's foot of a line and an arc on a ledge as wide as the chamfer.
`HatchOnBoundary` takes a moved line as on the boundary when none of its
samples is outside the face and at least two are on its boundary (the arc's
contact runs on into the face where the line's begins);
`ChFi3d_CutConsumedFaces` accepts a consumed face's strip whose vertices are
those of any obstacle edge of the stripe's side (the ledge's top along the
line's and the arc's edges); `ChFi3d_ContactsAlongFaceEdges` makes a
contact's end along a whole open edge, near the edge's vertex, that vertex.
Before, the chamfer failed or its wires intersected.

### 0149-chfi3d-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face

`SnapToVertex` on a periodic face (a boss as long as the chamfer on its free
end, the contact along the closed edge at the boss's foot): when the vertex
projects onto the line's other end, leaving a piece of no length, the
vertex's parameters a period on are tried. Before, the chamfer failed.

## 0380 and 0381: a chamfer up a crease

The patches 0380 and 0381 go on with 0140 to 0149 for chamfers
(mitcad#263): a chamfer up a crease between a straight side and an arc
and on over the rim's roundings to the top.

### 0380-chfi3d-chamfer-end-where-the-face-at-the-end-is-tangent-to-both-faces

The walk reaches the vertex on the top where the chain's edge meets the two
rims' edges, along which the roundings join the top tangentially; its
contact on the straight side's rounding ends on that rim, the other inside
the arc's rounding. The corner by intersection with the top found nothing,
the top being tangent to both faces, and the general corner threw in its
projection: the chamfer failed. A ruled piece now closes such an end
(`ChFi3d_ChamferEndOnTangentFace`, after `WalkedCutToEnd`): from the end
section to the point of the other rim as far from the vertex as the
section's point on the first, its sides along lines of the top's and of the
rounding's parameters; the stripe ends in that point, and `ChFi3d_FilDS`
cuts the rim there although the end has a degenerated edge.

### 0381-chfi3d-chamfer-guide-deformed-by-a-hundredth-of-its-distance-at-most

`ChFi3d_PerformElSpine` made the chain's guide C2 by removing knots within
up to a tenth of its length; along the wall the guide ran 0.03 to 0.08 off
the crease, and the chamfer's contacts, at its distance from the guide,
moved with it. A chamfer's guide now keeps knots whose removal would move it
by more than a hundredth of the chamfer's smallest distance
(`ChFi3d_ChamferGuideDeformation`); fillets are unchanged.

## 0480 and 0481: a post's foot chamfered past the plate's outline

The patches 0480 and 0481 go on with 0140 to 0149 and 0380 for chamfers
(mitcad#263): a post's foot on a plate chamfered so wide that the cone's
contact on the plate runs past the plate's outline, the outline's faces
growing up to the cone (0142), where the gaps hold the post's seam.

### 0480-geomint-open-line-parts-beyond-its-vertices

Toolkit: `TKGeomAlgo`.

`GeomInt_LineConstructor` kept of an analytic open line (a line, parabola or
hyperbola of two surfaces) with two or more vertices only the parts between
them, dropping those before the first and after the last although they lie
in both domains. A cone meets a plane parallel to its axis in a hyperbola
whose asymptotes are the generatrices parallel to the plane; with the cone's
seam one of them within rounding (8.6e-9 rad off a wall square to it), the
hyperbola crosses the seam twice some 1e8 mm away and only a piece of no
length between those crossings was left, so the chamfer found no cut of its
cone by the wall and failed. Those outer parts are now tested and kept as
the inner ones are, and parts that meet are joined (a line wholly in both
domains is one part, as with no vertices).

### 0481-chfi3d-kpart-gap-across-a-closed-spines-start-split-there

The piece cut by the face beyond over the gap that holds the start of a
closed spine ran across that start, its other contact crossing inside it the
seam of the face closed round the spine (the post's cylinder, whose edge's
vertex is on the seam). The topological build took the contact's edges on
the cylinder for a wire of their own and kept the cylinder whole: an invalid
shape. `ChFi3d_FillKPartGaps` now splits such a piece at the spine's start
where a line crosses its face's seam there (`ChFi3d_SplitPieceAtSection`:
both parts on one blend surface, each line cut at the plane square to the
spine there, the points on the seam on each part's side of it), the part
before the start at the period's end and the part after it at its start, as
`SplitKPart` cuts its own pieces. A piece whose lines cross no seam there (a
rounding's torus over a gap) stays whole, its face's seam where it was:
split, a later rounding along that torus's edges failed in a corpus design.

## 0530 and 0531: chamfers wider than a face along part of the chain

The patches 0530 and 0531 build chamfers wider than a face of their
edges along part of the chain (mitcad#263):

### 0530-chfi3d-chamfer-guide-keeps-to-its-edges-at-the-blends-beside

A walked chamfer's guide takes the tangents of the analytic blends before
and after it (the cones of arcs, meeting the stretch between at a slight
angle) within the chamfer's smallest distance of each end only
(`ChFi3d_AdjustGuideEndsLocally`); before, the adjustment deformed the whole
guide, up to 0.11 mm off a stretch of 16.6 mm, and the chamfer's contacts
moved with it. Other blends are unchanged.

### 0531-chfi3d-chamfer-wider-than-a-narrow-face-cut-by-the-faces-beyond

A chamfer whose contact on a face B runs off B across one of B's edges into
a face beyond at an angle (a fin's tip chamfered wider than the strip of its
side left by the fin's foot chamfer) is computed again with B's surface
unbounded (`ChamferOverNarrowFace`) and cut by the faces beyond B
(`ChamferCutByFacesBeyond`): the material it takes away is kept outside
those faces' material, their surfaces extended, so the chamfer is cut by
such a face from where its contact leaves B back across it to its other
contact, nothing of it is left where its section lies wholly inside, and
there the faces beyond grow down to the chamfer's other face (new curves of
them with it and with each other, prolonging their common edge; B's and the
spine's edges under it bound no face). The parts left are stripes of their
own ending in points. A chamfer whose contour is stopped where its edges
turn (`StopChainsAtTurns`, now offered by `BRepFilletAPI_MakeChamfer` too)
ends there with the end edge's analytic blend run on past the vertex, cut by
the face beyond the crease, up to where its other contact reaches that face
(`ChamferEndPastTurnStop`). Before, the walk would roll on B's edge, which
chamfers do not ("PerformSurf Not Implemented"). Only chamfers that failed
so, or whose walk left a face mid-spine, take the new paths.

## 1090: a corner's sharp edge limited by a common point on it

### 1090-chfi3d-sharp-edge-limited-only-by-a-common-point-on-it

An L bracket, a floor with a wall at its back: the floor's top edge along
one end (convex) and its edge along the wall (concave) chamfered 1, the
wall's vertical edge going on up sharp from their common vertex. The two
bevels' common points lie on different arcs, so the corner is
`PerformMoreThreeCorner`'s, as OnSame + OnDiff: `PerformOneCorner`'s
preparation moves the convex bevel's OnSame common point from the floor's
far vertical edge, where its contact ran to, onto the prolongation of the
sharp edge below the vertex, but keeps that far edge as its arc, with its
parameter there. The plate's limit on the sharp edge comes from the nearer
of the bevels' two common points beside it, the second of equal
distances; both lie 1 from the vertex, and at one end of the bracket
rounding made the moved point the nearer. Its parameter on the far edge
was read on the sharp edge's curve: the plate reached 31 above the floor
up the wall, a B-spline of degree 8 over 122 mm² waving across the
bracket's end, and the body was 16.6 mm³ short. Meshed for display at
0.01 mm that face took 86 000 triangles and 4 s, 8 to 10 s under load
(the `ui-pick` UI test's Chamfer preview, whose click on the face was lost
while the model was busy).

A common point now limits the sharp edge only where it lies on it
(`ParameterOnSharpEdge`): with its parameter on its arc where that arc is
the edge, otherwise with that of its point projected on the edge within
the point's tolerance, between the edge's ends. Of equal distances the
second point is taken as before, unless it lies on another arc and the
first on the edge. Both ends of the bracket, either edge given first, get
the same corner, a plate of 1.85 mm² meshed in about a thousand triangles,
the body within 0.03 mm³ of the two bevels' prisms at each corner. Corners
whose common points both lie on the sharp edge are unchanged.

## Reports and tests

- 0140 to 0149: mitcad#240, mitcad#263 and
  `geometry/tests/test_chamfer_overflow.cpp`.
- 0380 and 0381: mitcad#263 and the rounded crease in
  `geometry/tests/test_chamfer_overflow.cpp`, which fails without either.
- 0480 and 0481: mitcad#263 and the post beyond a square plate in
  `geometry/tests/test_chamfer_overflow.cpp` (its turned plate fails
  without 0480, every seam in a gap without 0481).
- 0530 and 0531: mitcad#263 and the fin whose side strip is narrower than
  its tip's chamfer in `geometry/tests/test_chamfer_overflow.cpp` (the
  chamfer's volume from its sections, cut by the foot's chamfer, swept
  along the edge).
- 1090: mitcad#1005 and `geometry/tests/test_chamfer_sharp_corner.cpp`
  (both ends of the bracket, either edge first, and the floor's whole top
  face: the corner within the bevels' reach and a few thousand triangles at
  most, the ends' volumes equal; one end fails without it).

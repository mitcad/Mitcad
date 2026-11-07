# Fillets ending where a boss meets a plate's side tangentially: 1330 to 1332

mitcad#1359 (GitHub mitcad/Mitcad#157): roundings of a plate's top edge
and of a boss's foot that end where the boss's cylinder meets the plate's
flat side tangentially. Back to the [port's README](../README.md).

## Overview

- **1330** does not split an OnSame end's prolongation where it meets a
  seam of the opposite face at its own start: the prolongation runs from
  the corner's vertex along the seam, and the split gave a curve without
  length.
- **1331** cuts the prolonged edge where an OnSame end's opposite contact
  ends on it short of the corner's vertex, instead of adding the edge's
  piece from the vertex again as a prolongation; a seam of the face at the
  end that starts at the vertex runs on to the cut; and the prolonged edge
  is the one the face at the end shares with the opposite face.
- **1332** keeps the opposite contact's parameter at the end being updated
  where the contact's periodic curve runs a whole period, instead of
  taking it to the other end a period off.

## The case

A sketch of two circles of radius 25 whose centres are 85 apart, joined by
the two lines tangent to both: the first circle and the region between
them extruded 15 (the plate), the second circle 30 (the boss), joined. The
boss's cylinder is the plate's end too, one face from the bottom up; the
plate's flat sides go on tangentially into it along the vertical edges at
x = 85, y = +-25 (from z = 0 to 15). The sketch splits each circle at the
tangent points, and the cylinder's seam runs up from the vertex
(85, 25, 15), where the circle's first arc starts. At each of the two
vertices (85, +-25, 15) four edges meet: the plate's top edge along its
side, the boss's foot on the top, the vertical edge and, at one of them,
the seam.

Two roundings failed at 3 mm (and at 1 and 6): the plate's top edge along
its side at y = 25 (`fillet produced an invalid shape (face:
BRepCheck_UnorientableShape)` in the report, self-intersecting wires on the
bodies here) and the boss's foot (`the geometry kernel cannot end the
rounding at a vertex: a curve of the corner has no length`).

## 1330: the prolongation along a seam from its end

### 1330-chfi3d-onsame-prolongation-along-a-seam-not-split-at-its-start

The boss's foot, rounded with r, has its contact on the top at radius
25 + r from the boss's axis; that contact meets the plate's side before
the vertex, and the side cuts the rounding, growing up to it (an OnSame
end: the side the face at the end, the cylinder the opposite face). The
side grows along the vertical edge prolonged past the vertex, from z = 15
up to the opposite contact's end at 15 + r: `PerformOneCorner` adds that
prolongation (the VARIANT1 curve, here the edge's own line continued,
`ChFi3d_ProlongTangentEdge`) and splits it where it crosses a seam of the
opposite face. At the vertex where the cylinder's seam starts, the
prolongation runs up the seam's line, and `BRepExtrema_ExtCC` gave one
extremum at distance 0, at parameter 15 on both (the vertex; the two lines
not found parallel): the curve was split into a piece from 15 to 15, and
`Geom_TrimmedCurve` threw `U1 == U2`, at every radius.

As for the cap's crossing of the seam earlier in `PerformOneCorner` (its
parameter within 1e-4 of an end, no split), a crossing at one of the
prolongation's ends (within `tolapp3d`) is no crossing now: the curve
stays whole, and `ChFi3d_CutSeamAlongCurve` (0147) cuts the seam where
the curve along it ends. With the seam at the other vertex the rounding
built before.

## 1331: the opposite contact ending on the prolonged edge

### 1331-chfi3d-onsame-opposite-contact-ending-on-the-prolonged-edge-cuts-it

The plate's top edge along its side, rounded with r, runs (as a tangent
chain) round the plate's other end and along the other side, and ends at
both vertices: its contact on the top, at y = 25 - r, meets the boss's foot
at x = 85 - sqrt(625 - (25 - r)^2) (73.13 at 3), before the vertex, and the
cylinder cuts the rounding (an OnSame end: the cylinder the face at the end,
the side the opposite face). The contact on the side, at z = 15 - r, runs
on to x = 85, where it touches the cylinder, tangent to the side there: on
the vertical edge itself, at (85, 25, 15 - r), below the vertex.
`IntersUpdateOnSame` finds that point; `PerformOneCorner` took it for the
end of the vertical edge's prolongation past the vertex and added the
curve from the vertex down to it, the edge's own piece there, without
cutting the edge: the side's and the cylinder's wires ran along it twice
(self-intersecting wires; the report's unorientable face).

Now, where the opposite contact's end lies on the prolonged edge itself
(within 10 `tolapp3d`), inside it and away from the vertex
(`ChFi3d_ContactEndsOnProlongedEdge`), no prolongation is added: the edge
is cut there, its part from the vertex lying under the blend (as an arc the
opposite contact ends on is cut, `cutArcop`). The side then ends along the
contact and the lower part of the vertical edge, the cylinder along the
cap from the cut to the foot's cut and that lower part.

At the vertex where the cylinder's seam starts, the cylinder goes on
across the seam's line from the vertex down to the cut (the corner the
rounding removes lay between the side and the cylinder's other side).
`ChFi3d_SeamRunOnAlongEdge` runs the seam on along its line from the
vertex to the cut, on both of its sides, as 0931 runs a seam on to a cap's
crossing past its end: a curve on the face's iso-line through the seam,
here run the way the seam's curve runs and used on each side with the
seam's orientation there (the faces' builder chains the pieces of a seam
by their orientation: with the curve run the other way the face came out
in three open wires), and the seam itself is kept from the vertex on with
an interference at the vertex (every other edge there is cut away, and
the seam, the boss's top edge and the top face were dropped with them).
The vertical edge there may have the cylinder's curves on both sides of
the seam (an edge left from the seam by the join, used once) or one.

The prolonged edge was the first edge of the face at the end at the
vertex other than the arc the contact ends on: at the vertex where the
seam starts that was the seam (with the seam at y = -25 for the side's
chain's end there). The edge the face at the end shares with the opposite
face is taken first now; at a vertex of three edges it is the only other
one, as before.

## 1332: a periodic contact's end at its range's end

### 1332-chfi3d-periodic-contact-end-kept-at-its-end-of-a-whole-period

On the body as MitCad's sketch and extrusions build it (the case of
`MITCAD_GEOMETRY_CASES` below), the boss's foot's rounding has its contact
on the cylinder on a circle whose range is a whole period, from -3.6e-15
to 2 pi, both ends at the vertex where the seam starts. Of the two points
`IntersUpdateOnSame` finds there (the contact's intersection with the face
at the end, and its crossing with the prolonged edge), equally near the
vertex, the intersection's came first: `Update` put its parameter,
-3.7e-15, a little below the range's start, into the range with
`ElCLib::InPeriod`, which took it to 2 pi - 3.7e-15, the range's other end.
The contact's range collapsed to [2 pi, 2 pi], the cap's end on the blend
lay a period off its other end, and the cap was sought round the whole
torus (`echec calcul intersection` at 3; invalid results at 1 and 6).
Where the range is a whole period, a parameter within the curve's
resolution of the far end that lies a period off the end being updated is
now taken at that end. The synthetic bodies' contacts start at exactly 0
and built without it.

## The two edges at once: 1400 to 1402

Done in mitcad#1374 by the patches 1400 to 1402
([fillet-boss-tangent-together-1400-1402.md](fillet-boss-tangent-together-1400-1402.md)).
With 1330 to 1332 alone, the plate's top edge and the boss's foot rounded
in one fillet failed (an invalid result through OCCT's fillet; MitCad's
fillet reported the fault at the corner). The two blends meet at the
vertices from opposite sides of the top: the side's blend takes material
away below z = 15, the foot's adds it above, and their contacts on the top
cross at (85 - sqrt((25 + r)^2 - (25 - r)^2), +-(25 - r), 15) (67.68 at
3), so that the foot's ball rolls off the top onto the side's blend there.
OCCT's simultaneous corners did not detect this (no `Failure_Overlap`,
which lets MitCad's fillet round the convex edge first and the concave one
on its result); rounded one after the other, the foot's chain runs on
along the side's blend's cut on the cylinder to the vertex where the
side's blend ends, which was valid at 3 and 6 and not at 1 (its contact's
curve on the cylinder ended a period off at the seam).

## The edge picked at its other side: 1570 to 1572

Done in mitcad#1422 by the patches 1570 to 1572
([fillet-boss-tangent-other-side-1570-1572.md](fillet-boss-tangent-other-side-1570-1572.md)).
The tests here picked the plate's top edge at y = 25. Picked at y = -25
(the same tangent chain, its stripe's faces in the other order) the face
at the end cut the blend to a point on the vertical edge instead of the
corner cutting it, and that path had none of 1331's seam handling: the
rounding failed at every radius, on both synthetic bodies and on the body
MitCad's UI builds from a slot and a circle.

## Tests

- 1330, 1331 and 1332: mitcad#1359, in
  `geometry/tests/test_fillet_boss_tangent.cpp`: the plate's top edge and
  the boss's foot, each alone and each with the boss's top edge, rounded
  with 1, 3 and 6 through OCCT's fillet alone and through MitCad's fillet,
  on the body MitCad's sketch and extrusions build with the cylinder's
  seam at either vertex, and on the case `boss-tangent-side-rounding` of
  `MITCAD_GEOMETRY_CASES` (the body as MitCad's sketch and extrusions build
  it with the circles as the sketch tool draws them): every result valid,
  its faces named (each rounding after its edge), the material taken away
  or added as the exact shapes give it (square corners less the ball's
  cylinder or torus, cut by the boss or the sides) within 1e-6 of it,
  42 checks failing without the patches.

# A roof slab's eave rounded wider than it is: 1650 to 1659

mitcad#1500, mitcad#1530, mitcad#1532 and mitcad#1533, the follow-ups of mitcad#1494 after
[1640 to 1642](fillet-ride-corners-1640.md): the eave of the roof slab of
mitcad#1482 rounded with 0.9 and 1, the corners at its ends, and the slab
rounded on all its edges. Back to the [port's README](../README.md).

## Overview

The slab is a section of 7 points extruded 25.2 along x. Its eave is a
level face 1.506 wide between the tip's edge (60 degrees, between the eave
and the top slope) and the obtuse edge where the underside begins (120
degrees). Rounded with r, the tip's ball touches the eave 1.73 r from the
tip and the obtuse edge's 0.58 r from that edge: from r = 0.65 the two
overlap and the obtuse edge's rounding rides on the tip's contact line
(0016); from r = 0.87 the tip's contact lies past the obtuse edge, and the
tip's rounding rolls on the obtuse edge (0011, `RollsInsteadOfOverflow`),
its ball touching the top slope and passing through that edge, and
consumes the eave.

- **1650** drops the obtuse edge's rounding there: riding on the tip's
  contact line, which is now its own edge, its ball touches the underside
  on the edge, a section of no width (as the ride's own does as r grows to
  0.87). It takes nothing away.
- **1651** makes the corner of the tip's rolling blend and the eave's end
  edge's blend (90 degrees, between the eave and the end face), whose
  contact lies on the consumed eave: they meet first where that contact
  reaches the obtuse edge, at its other end; from there they are cut along
  their intersection up to the end face.
- **1652** makes one point of a contact that the corners at both its ends
  cut down to one point inside its face (the eave's end edge's contact on
  the end face, where the strip along the slope vanishes between two rides).
- **1653** makes the corner at the eave's tip where the slope's top edge on
  the end face is rounded too: the tip's rolling blend, the end edge's and
  the slope edge's are cut by each other pairwise and meet at one point.
- **1655** makes the two contacts of roundings that consume a face whose
  edges are all rounded one edge where they end inside that face (a rib's
  full round: with 1652 the end edges' contacts are points at the ridge),
  and drops the face, which was left as a face of no area.
- **1654**, **1656** and **1657** make the whole slab valid from 0.66 to
  0.86 and at 1.1: the plate of a corner with a rider oriented from a
  stripe that rides on no line; the curve a stripe cut to nothing leaves to
  a periodic corner surface (a torus) in that surface's own period; the
  curves of 1653's corner marched the way that keeps their p-curves inside
  the contacts.
- **1658** and **1659** make the slab's shape continuous from 0.5 to 1.2
  (mitcad#1533): of two overlapping roundings the one whose contact lies
  nearer its edge rides, and a ride of no width drops its rider; the
  corner of three at the tip cut by intersections where the end edge's
  blend is cut by a rider's intersection at its other end, and a p-curve
  that does not follow its curve is not taken.

## The cases

With OCCT and 1640 to 1642 alone, at r = 0.9 and 1:

- the eave's two long edges: the corners at the obtuse edge's ends failed
  ("OneCorner : fillets have too big radiuses"): the obtuse edge's rounding
  was computed on the eave and the underside, its contact on the eave
  inside the face the tip's blend consumes, and its end section on the end
  face crossed the tip's blend's;
- with the eave's end edges too: the tip's vertex has the tip's rolling
  stripe (its sides on the top slope and, rolling, on the obtuse edge) and
  the end edge's stripe (on the eave and the end face), no common face;
  `PerformMoreThreeCorner` put the tip's blend's ends off the solid (x =
  -1.39) and the topology failed (`TopoDS_FrozenShape`; with one end edge,
  a crash in `TopOpeBRepBuild_Builder::MakeFaces`);
- an end face's two edges along a slope (riding, 1640) and the eave's end
  edge, from r = 0.66 on: the end edge's contact on the end face, at height
  r, crosses the line both slope roundings ride on; the corners at both its
  ends cut it there, each making a point of its own, and the blend's wire
  was left open with an edge of no length between them (an invalid result;
  near 0.86 the two points happened to be one).

With 1650 to 1652, the slab rounded on all its 21 edges computed at 0.9
and 1, but the result was invalid at the four ends of the eaves' obtuse
edges, where the eave's end edge's rounding is cut at both its ends:

- at the tip, by the corner of three stripes (the tip's rolling blend, the
  end edge and the slope's top edge on the end face, `PerformThreeCorner`),
  from the end edge's contact end on the obtuse edge (1651) to the point Q
  where its contact on the end face meets the line the slope's two
  roundings ride on: both points at that blend's other end;
- at the obtuse edge's end, by the corner with the underside's end face
  rounding (the rider, `PerformTwoCornerbyInter`): their intersection runs
  from Q to the rider's contact on the underside and dips up to 0.06 (at
  r = 0.9, along the eave) past the first cut.

The end edge's blend between the two cuts was a sliver whose wire crossed
itself (`BRepCheck_SelfIntersectingWire`, area -0.048 at r = 0.9).

## 1650 to 1659

### 1650-chfi3d-rounding-of-an-edge-another-rolls-on-all-along-dropped

- `Compute`, after `DropOverlapsRunOutByOther`: an open stripe of one edge
  E that another stripe of one SurfData rolls on from vertex to vertex (a
  side on curve E, `rollsOnWholeEdge`), the two edges of a kind (both
  convex or both concave) and bounding one face, is dropped with its
  corners (`myDroppedStripes`, the corners' map built again): it stays a
  contour of the builder and gives no faces, as with
  `RemoveCoincidentStripes`.

### 1651-chfi3d-corner-of-a-rolling-blend-and-a-blend-on-the-face-it-consumes

- `ChFi3d_IsInFront`: where the two SurfData have no common face, a side
  of one on curve E and the other's contact on a face F of E whose end
  away from the vertex lies on E: F is their common face, the parameters
  those of that end's point on both (on the rolling side's line by
  projection). `PerformTwoCornerbyInter` then cuts them along their
  intersection from that point up to the end face (the end edge's blend
  the small one, the tip's the big one).
- `PerformTwoCornerbyInter`: the big stripe's point on the pivot is taken
  on its contact by projection where its parameter there is off the point
  (the rolling blend's SurfData runs on past the spine's end, 0011; its
  cut by the end face reached x = -1.39 and gave the vertex there a
  tolerance of 2.1).
- `Compute`, after the corners: a contact of a stripe of one SurfData that
  the corner at one end cut down to its point on an arc at the other end is
  one point: the corner's point index is replaced by the one on the arc
  (in the stripes and in the corners' curves) and the contact's parameter
  at that end is the other end's; a rolling side that starts there takes
  that point on the arc, and its obstacle edge goes whole, as at a rolling
  blend's end at a vertex (no point on it there, cut at its vertex beyond):
  its part up to the point is the end edge's blend's, the rest the rolling
  blend's side.
- `ChFi3d_FilDS`: such a side of a stripe of one SurfData (its two ends one
  point, its parameters one) gives a degenerated edge of the blend's face
  and no curve on its face (the eave's end edge's blend is a triangle:
  the intersection with the tip's blend, its contact on the end face and
  its cut by the underside).
- `ChFi3d_CutConsumedFaces`: an end of the rolling blend short of the
  obstacle's vertex, where another stripe's contact on the consumed face is
  reduced to that point, counts as an end at the nearer vertex, so that the
  consumed face goes with its edges cut there.

### 1652-chfi3d-contact-cut-to-one-point-by-the-corners-at-both-ends

- `Compute`, after the corners (with 1651): a contact of a stripe of one
  SurfData whose two ends are the corners' points within ten tolerances,
  inside its face (on no arc), gets one point index for both ends (in the
  stripe and in the corners' curves) and one parameter; `ChFi3d_FilDS`
  (1651) gives it a degenerated edge.

### 1653-chfi3d-three-blends-cut-by-each-other-where-one-rolls-on-the-consumed-faces-edge

No ball rolls between the three faces at the tip (it would touch the
consumed eave), and the corner of three stripes put the end edge's cut at
that blend's other end. The three blends are cut by each other, as the
union of the three roundings' removal sets is: each pair along its
intersection, from where their contacts meet (the end edge's and the tip's
at the point on the obtuse edge, 1651; the tip's and the slope edge's on
the top slope; the end edge's and the slope edge's at Q on the end face) to
the point P where all three surfaces meet. The end edge's blend then keeps
the piece between that corner's two curves (to P and from P to Q), the
rider's intersection from Q and the underside's cut, and its contacts on
the eave and the end face are single points (1651, 1652).

- `threeCornerByInter`, in `ChFi3d_FilBuilder::PerformThreeCorner` after
  its three `SearchFD`: where one stripe's side towards another is on curve
  E (rolling on E) and the other's contact on their common face (1651)
  reaches E at its far end, the three pairs facing each other, one SurfData
  each at the corner. The intersection of those two runs from that point to
  where the other's second contact meets the rolling blend
  (`ChFi3d_IntCS`, `ChFi3d_ComputeCurves`); the third surface cuts it at P
  (`ChFi3d_IntCS`); the three intersections are computed from where each
  pair's contacts meet (`ChFi3d_ComputesIntPC`) to P. Each stripe's end is
  its two curves, on its surface (the stripe in the data structure at that
  end, as the big one's end in `PerformTwoCornerbyInter`), its points and
  its contacts' parameters those of its pairs. A corner of this kind that
  fails is a failed corner ("ThreeCorner : the blends do not meet").

### 1655-chfi3d-roundings-consuming-a-face-whose-edges-are-all-rounded-meet-on-one-contact-edge

A rib 2 wide, its four top edges rounded (or bevelled) by 1, the full
round of `cli.p9_warnings`: with 1652 the end edges' contacts on the top
face are one point each, at the ridge, and the long edges' contacts lie
along each other on the ridge, ending at those points inside the face.
0950's `joinEndsAlongContacts` joined such contacts only where they end on
an edge of the face, so each long blend kept its own contact edge and the
top face was left as a face of no area (two edges along the ridge, valid
for BRepCheck). Without 1650 to 1652 the result was invalid, and Mitcad
built the dressup 0.1 % smaller with an info.

- `joinEndsAlongContacts`: ends of the two contacts inside the face (on no
  arc) are joined too where every edge of the face is an edge of a
  stripe's spine (`allEdgesRounded`): their points are made one, and with
  both ends one the contacts are one edge as before. As no edge of the
  face is left for the contacts to end on, the face's edges are cut at
  their vertices (a vertex interference leaving the face on both sides,
  as `ChFi3d_CutConsumedFaces` does), and the builder drops the face.

The rib (2 by 40 by 20, also 1.5 and 4 wide) rounded and bevelled by half
its width: one valid closed solid, the top face gone, the two long
roundings meeting on one ridge edge, the volume the exact shape's (the
round 1582.356927 at 2 wide: 1520 + 19 pi + 8/3).

### 1654-chfi3d-plate-of-a-corner-with-a-rider-oriented-from-a-stripe-riding-on-no-line

Between r = 0.66 and 0.86 the tip's rounding does not roll yet, and the
left eave's obtuse edge's rounding rides on the tip's contact line (on the
right eave the tip rides on the obtuse edge's). The eave's end edge's
contacts cross the two ride lines (on the eave and on the end face) at the
same two points at both its corners: the corner at the tip
(`PerformThreeCorner`, a torus there) and the one at the obtuse end (the
two riders and the end edge, `PerformMoreThreeCorner`'s plate) cut its
blend between those points, and 1640's `dropStripesCutToNothing` drops it.
With OCCT and 1640 to 1642 alone, as with 1650 to 1653, both corners' faces
at the left eave came out reversed (area -0.34 and -2.68 at r = 0.8).

The plate: `PerformMoreThreeCorner` took the orientation of the plate's
p-curves from the corner's first stripe, at the left eave the obtuse
edge's rider, whose side on the line has its transition from the line's
support (`RideOnOverlap`, 1640).

- `PerformMoreThreeCorner`: that orientation (`orien`) is taken from the
  first stripe at the corner that rides on no other's line (the first one
  where all ride).

### 1656-chfi3d-curve-left-by-a-stripe-cut-to-nothing-in-the-period-of-the-corner-surface

The torus: `dropStripesCutToNothing` gave it the plate's curve with a
p-curve of its own projected on the torus (`GeomProjLib::Curve2d`), which
landed a period away in u. The torus's face was built over the other part
of the torus, with an edge of no length that was no degenerated edge.

- `dropStripesCutToNothing`: on a periodic surface the new p-curve is moved
  by whole periods to the start of the p-curve it replaces.

### 1657-chfi3d-corner-of-three-curves-marched-the-way-keeping-inside-the-contacts

At r = 1.1, at the far end's left eave, 1653's intersection of the eave's
end edge's blend and the slope's top edge's blend, marched from where
their contacts meet on the end face (both blends touch that face there, so
that the intersection leaves it tangent to both contacts), had p-curves
crossing the contacts' p-curves near it (by 0.005 in the blends'
parameters): both blends' wires crossed themselves.

- `threeCornerByInter` (1653): each pair's intersection is computed from
  where their contacts meet to the point of all three and, where its
  p-curves cross either contact's p-curve to the side away from that point
  (sampled at 40 points), from that point back; the way crossing least is
  kept.

### 1658-chfi3d-the-rounding-whose-contact-lies-nearer-its-edge-rides-a-ride-of-no-width-drops-the-rider

With 1640 to 1657 the slab's volume jumped twice near r = 0.87:

- the stripes' order chose the rider of each eave's pair: on the left eave
  the obtuse edge rode on the tip's line, on the right eave the tip on the
  obtuse edge's line, a shape of its own (the right eave's group took 2.4
  less than the left's at 0.868), which jumped by 2.3 at 0.869, where the
  tip's rounding begins to roll (1650);
- from 0.862 to 0.868 the left obtuse edge's ride, its section narrower
  than 0.012, failed in its walk, and 1640 took it the other way round
  (the volume up by 2: 3331.68 at 0.862 against 3329.58 at 0.86).

The ride that converges to 1650's roll is the obtuse edge's: as the radius
grows the tip's contact line reaches the obtuse edge, the ride's section
narrows to nothing, and the tip's rounding then rolls on that edge.

- `Compute`, the rides of overlapping stripes: the stripe whose contact on
  the face lies nearer its own edge rides (`contactReach`: r / tan(a / 2),
  a the angle of its two faces on the ball's side at the edge's middle);
  equal ones (the end faces' strips, a plate's rims) keep their order.
- Where that ride fails and the other's contact line lies within a
  fiftieth of the rider's radius of the rider's edge all along
  (`rideOfNoWidth`, sampled), the rider is dropped with its corners, as
  1650 drops it once the other rolls on its edge (it takes less than 0.003
  a side there), not ridden on the other way round.

### 1659-chfi3d-corner-of-three-cut-by-each-other-where-a-stripe-is-cut-at-a-ride-line-at-both-ends

With the obtuse edge's rounding dropped (1658), or with the eave's
obtuse edge not rounded at all from r = 0.7 on, the slab was invalid at
the eave's ends: the tip's corner of three (`PerformThreeCorner`, a ball's
torus) cut the end edge's blend along a section near the obtuse end, and
the corner of the end edge and the underside's rider (which rides on the
slope edge's line, 0016), at the obtuse end, cut it along their
intersection: the two cuts crossed. And with 1658 the right eave's corners
come in the other order: at r = 0.85 `dropStripesCutToNothing` kept the
plate's curve and projected it on the tip's torus, a p-curve wandering
across the torus's period along its pole (the slab valid, but 6.7 short).

- `PerformThreeCorner`: where a stripe Y of the three shares a face with
  another, Z, ridden on there by W (`RideOnOverlap`), and W and Y alone
  meet at Y's other end, the corner is made by 1653's
  `threeCornerByInter` with Y and the third stripe as its pair: the blends
  cut by each other, as at Y's other end.
- `dropStripesCutToNothing`: a projected p-curve that does not follow its
  curve on the surface within ten tolerances (sampled) is not taken; the
  other end's curve is kept instead.

## What is left

- The corners change form at the radii where the rides do: at 0.862 the
  tip's corner of three becomes the intersections of 1653 (1659), and the
  slab's volume goes up by about 0.09 there (3327.30 at 0.86, 3327.37 at
  0.862, where the trend gives 3327.27); elsewhere from 0.5 to 1.2 it falls
  steadily.

## Reports and tests

- `geometry.fillet_eave` (`geometry/tests/test_fillet_eave.cpp`), OCCT's
  fillet alone, all results one valid closed solid with no face of no
  area, at r = 0.9 and 1:
  - the eave's two long edges: no face left on the eave; the material
    taken 25.2 times the tip's rounding's section (the triangle of the tip,
    its contact on the top slope and the obtuse edge, less the ball's
    segment over that chord): 13.959146 and 16.375489, within 1e-5;
  - the eave's four edges: as above, and twice what each end edge's
    rounding takes outside the tip's (its removal set {x < r, z < 17.8 + r,
    outside its tube}, running on to the underside, less the tip's,
    integrated over the height): 14.155052 and 16.594249 against OCCT's
    14.155064 and 16.594264, within 1e-4;
  - an end face's two edges along the left slope and the eave's end edge,
    at 0.7 too: the material taken at least what the slope's pair or the
    end edge takes alone, at most their sum;
  - the tip's edge, the eave's end edge and the left slope's top edge on
    the end face (the corner of three, 1653): the material taken 25.2 times
    the tip's section, and over the section outside it the larger of the
    end edge's and the slope edge's depths in x (both 90 degree edges on the
    end face), integrated on a grid of 0.004 along the slope by 0.0005
    across: 22.793091 and 27.245162 against OCCT's 22.794044 and 27.246177,
    within 2e-3 (the slope edge alone at its ridge end differs by 4e-4 the
    same way).

  Without the patches the long edges and the four edges fail, the end
  face's three edges give invalid results, and the corner of three gives
  an invalid sliver of the end edge's blend.
- `geometry.fillet_roof_slab`: the slab rounded on all its 21 edges with
  0.5, 0.7, 0.8, 0.862, 0.865, 0.9, 1 and 1.1 through Mitcad's fillet,
  each a valid closed solid (0.9 and 1 with 1653, 0.7, 0.8 and 1.1 with
  1654 to 1657, 0.862 and 0.865 with 1658 and 1659), its volume against
  the union of the roundings' removal sets and the concave ceiling
  corners' fills (the edges along x as sections along 25.2, the obtuse
  edges' balls through the tips' lines they ride on, the end faces' edges
  as depths in x over the section, integrated on strips along its sides):
  OCCT takes between 0.12 (r = 0.5) and 0.54 (r = 0.8) more, the
  corners at the 14 vertices of the ends filling between the roundings (a
  ball's corner at a convex vertex of three 90 degree edges takes about
  0.06 r^3 more than their union); the check allows from 1.5 r^3 more to
  0.03 less.
- `geometry.fillet_rib_full_round` (`geometry/tests/test_fillet_rib_full_round.cpp`,
  1655): a rib 2, 1.5 and 4 wide, its four top edges rounded by half its
  width (2 and 4 wide also bevelled): one valid closed solid, no face of
  no area, no face left on top, the exact volume within 1e-6 mm³.
- `cli.p9_warnings` and the Linux UI test `ui-timeline`: the rib's full
  round and bevel build at their size with no information; the info of
  a dressup built 0.1 % smaller (mitcad#1106) is now shown on a boss's top
  edge rounded with the boss's radius (a dome), which OCCT still cannot
  close to a point.
- `geometry.fillet_*` and `geometry.suite.*` pass with the patches.

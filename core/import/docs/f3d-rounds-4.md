# The .f3d import: notes on single rounds, part 4

Notes on single rounds of work on the corpus. Part 4: rounds 12 to 20 of
mitcad#343, and the later items (chamfers, deviating results, an assembly's
remaining fallbacks, a sketch solved within the tolerance). Part of the
[.f3d import](f3d.md) notes; the [import documents](../README.md#documents)
list the other topics.

## Roundings left in three designs (round 12 of mitcad#343)

The roundings that still fell back in `5ef2dbd5`, `a9e82a9a`/`de7b8b9d`
and `6418ebb1`/`920a1cef`, each design imported alone with the failure
dump, every failing definition run through MitCad's dressup and OCCT's
fillet alone on the replay's body and on the history's stored body before
the item (its edges found there on the stored edges' curves). `a9e82a9a`'s
item 159 (Fillet37) is parametric and verified since round 4 (0580,
0581).

- **`a9e82a9a`, `de7b8b9d` item 17 (Fillet7, radius 1.5 on both rims of a
  ring 2.5 thick, cut by five notches): parametric, verified.** OCCT alone
  with the file's ten edges in the order the file lists them gives the
  stored body after the item (every new face on the file's, the change of
  volume within 5.5e-5 mm³); the dressup failed at four arc ends. No ball
  touches both planes, so the later of each arc's two roundings rides on
  the earlier one's contact line on the band (0016), and the file lists
  each arc's top edge first: the top roundings are whole and the blends
  meet 1.5 below the top. The dressup added its contours in the body's
  order of its edges, the bottom edge first; that bottom rounding was the
  whole one, and the riding top one failed at its arc's end. The contours
  now go to OCCT in the order the sets list their edges
  (`geometry/src/dressup.cpp`, `seed_order`; test
  `test_mitcad_fillet_of_both_rims_follows_the_order_of_its_edges`).
  Where that order does not build, they are tried once more in the body's
  order: `6418ebb1` item 45 (round 8's join over the boss strips) builds
  only when its join is walked from one of its two ends, which the body's
  order gives and the file's does not.
- **`5ef2dbd5` items 51 and 53 (Fillet5, Fillet7, radius 1 round the foot
  of a pin standing out of a wall): parametric, verified (port patch
  0911).** The arc at the pin's foot ends where the wall ends in a slot
  whose sides are tangent to the pin; at each end the wall's edge with the
  slot's side runs into the vertex tangent to the arc, and the side goes on
  tangentially from the pin across their common edge. The stored body cuts
  the rounding by the slot's side, which grows up to it (the side's edge
  with the pin prolonged by 1, the torus running on to both sides). OCCT's
  end state, decided from the faces' sides inside the tangent edge, came
  out OnDiff: the walk rolled on the wall's edge into the vertex (0770)
  and ended in a point there, 0.136 mm off the file's (on the replay's
  body an unorientable face). 0911 makes such an end OnSame where the
  tangent edge leaves the vertex behind the spine's edge (the face it
  shares with the spine's edge has a reflex corner there): on the stored
  body the change of volume within 2.4e-4 mm³ of the file's, every new
  face on the file's within 1e-5 mm; item 53 (the mirrored pin, with its
  end's rim) within 0.0074 mm³. Where the tangent edge leaves into that
  face (an earlier rounding coming in at the vertex, as at `5ef2dbd5` items
  62 and 63 and `a9e82a9a` items 44 and 186), the state is decided as
  before: an OnSame end failed in those corners. OCCT-only test:
  `geometry/tests/test_fillet_tangent_end_face.cpp` (fails without 0911).
- **`a9e82a9a` item 18 (Fillet8, radius 1.5 on both rims of the ring's
  outer band, two whole circles): parametric → fallback, following item
  17.** It was parametric on the file's body after item 17 (item 17's
  fallback brought it); on the stored body OCCT's fillet gives -759.80 mm³
  against the file's -759.79. On item 17's own result, which has the file's
  faces, OCCT's rounding whose blend rides on the other's contact line
  comes out with that blend's face reversed (its area -1617 mm², its wire
  not closed), and the dressup's other definitions give -844.38 mm³ (11 %
  off). The two bodies differ in how the band's circles run: the replay's
  start at 180 degrees (the vertex (-140, 0)), the bottom one clockwise
  about the axis and the top one counterclockwise; the file's start at 108
  degrees, both counterclockwise. The same failure with the whole body
  turned about the axis, and a synthetic disc 2.5 thick rounded on both
  rims with 1.5 builds at every seam angle and with its circles either way
  round, so the cause is not shown. Not traced further.

Not fixed, each failing on the history's stored body before the item as
well (kernel gaps, not upstream noise), unless said:

- `6418ebb1`, `920a1cef` item 37 (Fillet1, radius 2): three concave
  roundings along the intersection curves of three struts (cylinders),
  ending at a triangle 1 mm wide where the struts meet. The stored body
  consumes the triangle, and the three blends meet each other in edges 2.32
  long that run to one point, with no corner face. OCCT's ends find the
  triangle narrower than the blend ("the rounding's end does not meet the
  face at the end"). Needed: three roundings run on past their ends over a
  small face and cut by each other.
- `6418ebb1`, `920a1cef` item 44 (Fillet6, radius 0.5, the three struts'
  curves where a cylinder rounds the node): the node cylinder's seam ends
  at one rounding's end vertex, and the rounding's cap on that cylinder
  crosses the seam's line 0.31 past the seam's end; the cylinder face
  comes out with a wire not closed. The stored body runs the seam on to the
  cap (25.48 long against 25.05). Needed: the corner prolonging the seam of
  the face at the end past its end vertex (`PerformOneCorner`'s seam
  crossing takes only crossings on the seam's edge). The input's other
  invalid faces (slivers of 0.02 mm²) are the stored bodies' own.
- `6418ebb1`, `920a1cef` item 41 (Fillet3): 5 of its 11 edge references
  are lost in the file; with the 6 left OCCT's result lies within 0.002 mm
  of the file's faces but removes 6.56 against 7.52 mm³, and its wire
  self-intersects. Item 42 (Fillet4, eight edges between a plane and a
  B-spline face whose edges have tolerances up to 8.9e-3): the rounding's
  surface twists; the stored bodies there are themselves invalid.
- `a9e82a9a`, `de7b8b9d`: item 29 (Fillet13) and item 31 (Fillet16, 48
  edges) stop part of the way in the walk between two faces (one contour
  on the stored body, two and eight on the replay's); item 137 (Fillet34)
  stops where it rolls on an edge (mitcad#237's class); item 191
  (Fillet44, radius 10 on two short edges of a narrow plane) rolls on the
  plane's edges and both end corners fail ("no point on arc", "bouchon
  non ecrit"); item 99 (Chamfer7, two distances) not traced; item 33
  (Fillet17) has 16 of its 48 edge references lost in the file; item 156
  (Fillet36) builds 2.9 % off in volume with faces up to 0.063 mm off, a
  deviation the import dialog can keep (the owner's rule of 18.10.2026).
- `5ef2dbd5`: item 30 (Fillet4) stops in the walk between two faces; item
  38 (Fillet12) fails at a vertex of more than three sharp edges ("two
  faces met round the vertex share no edge"); items 85 and 86 (Chamfer1,
  Chamfer2) as in *Chamfers left after mitcad#240* and round 9; items 87,
  91, 92 and 93, which run on the stored bodies after items 85 and 86, fail
  in the walk, at a corner, with unorientable faces and in a rational
  section's weights (mitcad#239).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `a9e82a9a`, `de7b8b9d` 17 | 2 / 2 parametric, verified, inputs read (contours in the file's order) | |
| `5ef2dbd5` 51, 53 | 2 / 2 parametric, verified, inputs read (0911) | |
| `a9e82a9a`, `de7b8b9d` 18 | 0 / 2 (was parametric on the file's body after item 17) | the ride on item 17's result reverses the riding blend's face; not traced further |
| `6418ebb1`, `920a1cef` 37 | 0 / 2 | three blends over a consumed triangle, cut by each other |
| `6418ebb1`, `920a1cef` 44 | 0 / 2 | the cap crosses the seam's line past its end; the seam not prolonged |
| others listed above | 0 / 26 | kernel gaps on the stored bodies, losses in the file, a 0.06 mm deviation |

Designs, each imported alone (features parametric of all, before →
after; history states matching): `5ef2dbd5` 81 → 83 of 98 (55 → 57 of
67); `a9e82a9a` and `de7b8b9d` 183 → 183 of 199 (item 17 in, item 18
out); `6418ebb1` and `920a1cef` 41 → 41 of 50. Unchanged with the
contours' order and 0911: `db73863f` 135 of 145, `52ba7a15` 97 of 110,
`61526c66` 14 of 14.

## Roundings left after round 12 (round 13 of mitcad#343)

The roundings round 12 left, each design imported alone with the failure
dump, traced on the replay's body and on the history's stored body before
the item through OCCT's fillet alone and the dressup.

- **`a9e82a9a`, `de7b8b9d` item 18 (Fillet8, radius 1.5 on both rims of the
  ring's outer band): parametric, verified, inputs read (port patch
  0930).** The first deviation was not the bodies' but the direction of
  the band's circles: the replay's bottom circle turns about −z, its top
  one about +z, both from (−140, 0); the file's both turn about +z. No
  ball touches both flat faces, so the later rounding rolls on the earlier
  one's contact line (0016), and the two blends meet on that line, run the
  earlier spine's way; the later blend used it from its start to its end
  as if its own spine ran that way, and where the two spines run opposite
  ways its face came out reversed (−1617 mm²) with a wire not closed. 0930
  runs that side from the line's end to its start and turns the line's
  transition on the later blend's surface. On item 17's own result: valid,
  −759.8016 against the file's −759.79 mm³ in either order of the rims; the
  import matches state 16 within 1.4e-7. OCCT-only test `geometry.fillet_ride_against` (a
  disc whose rims' circles turn opposite ways; invalid without the patch).
- **`6418ebb1`, `920a1cef` item 41 (Fillet3): parametric, verified, inputs
  inferred** (5 of its 11 edge references are lost in the file; the 6 left
  match state 17 within 2.1e-5). OCCT's result failed the checker on pieces
  of input edges whose curves on their faces lie off their tolerance by as
  much as the input edges' own (the checker, sampling the whole input
  edges at other points, passed them). The dressup now widens the
  tolerance of such a piece of an input edge to its deviation where that
  is no larger than the input edge's own
  (`detail::widen_inherited_curve_tolerances`, test
  `test_mitcad_fillet_keeps_an_input_edge_s_own_deviation`); the faults
  left are the input's own slivers, away from the rounding.

Open, for round 14 (`6418ebb1`, `920a1cef` items 44 and 45, where the
roundings end at the node rod; done in round 14, below):

- Item 44 (Fillet6, radius 0.5 where the three bores' curves meet the
  rod) falls back. The rod's seam (a B-spline curve) ends at one rounding's
  end vertex; the rounding takes material off there, and the rod's face
  grows up to the corner's cut, which crosses the seam's line 0.31 past the
  seam's end. `PerformOneCorner` splits a cut at the seam only on the
  seam's edge or at its vertices, so the rod's face comes out with a wire
  not closed. Running the seam on along the face's own iso-line from the
  vertex to the cut builds it: the rod's face has the file's area (412.4848
  against 412.4839 mm²), every new face within 6e-4 mm of the file's, the
  remaining faults the input's own (with the widening above). With item 44
  parametric, though, item 45 runs on the replay instead of the stored
  state 20, and fails there.
- Item 45 (Fillet7, radius 0.5 on three pieces of the edge between a large
  torus and a plane that the rod interrupts; 2 of its 5 references are lost
  in the file). Item 43's faces there are the file's within 1.6e-4 mm (its
  contact curve on the rod; the file's own curve lies 9.3e-5 off its
  surfaces); the two differ in where item 43's torus has its seam (u = 0
  against the file's 0.54, through the rounding's end on the rod). Past the
  pieces' ends the contact on the plane runs onto item 43's torus, which
  wraps the rod's foot, and the roundings never reach the rod, the face
  that cuts them in the file's bodies:
  - On the stored body the torus's seam splits the walk of piece #2 past its
    end into lines at 18.6559, 19.1182 and 20.0791 (its spine 17.92 long);
    on the replay it is one line up to the plane's edge at 19.1182. Port
    patch 0077's `ChFi3d_CutSingularExtension` cuts that line at 18.0021,
    as its contact speeds up from 1.25 to 20.5 on the torus of radius 0.6,
    and leaves the line's end on the plane's edge, at the parameter found
    at 19.1182, 6.07 mm away: the corner then fails ("an edge at the
    vertex has no curve on a face"). On the stored body the lines past the
    seam start past the spine's end and are not cut. Without the cut the
    replay's walk is the stored body's.
  - Piece #0's start has the same stale end (6.55 mm off); without the cut
    its corner's cut on the face at the end is the one another corner
    already made there ("fillets have too big radiuses").
  - On the stored body piece #0's walk past its start jumps to a solution
    on piece #2's path (from −1.07 to −1.71, about 7 mm from its guide),
    which avoids those corners and gives the file's change within 0.068
    mm³: a success by accident.
  - Needed: the roundings ended where the rod cuts them, a face their
    contacts never reach (a new corner), and the end of a cut line no
    longer left on the arc found past the cut.

Not fixed, each failing the same way on the history's stored body before
the item (kernel gaps):

- `a9e82a9a`, `de7b8b9d` item 29 (Fillet13, radius 1.5, twelve edges): the
  slot's two side walls, strips 3.0 wide between the ring's cylinders of
  radius 112 and 115, rounded on both edges. The two roundings exactly fill
  each strip; the file makes each strip one torus (113.5, 1.5). Two of
  OCCT's four contours fail in the walk at 1.30 of 32.68 along their spine,
  where the strip ends at the slot's end cylinder.
- `5ef2dbd5` item 30 (Fillet4, radius 3): the rounding along a concave
  cylinder (radius 10) and a slot's profile, whose convex arc (radius 12)
  goes on tangentially into a concave inner corner of radius 1.25, tighter
  than the ball. The walk fails 0.13 before that tangent edge (2.93 of
  15.57); the file ends the rounding's piece on the arc in a point (a patch
  with a degenerate edge) and goes on with two more pieces onto the side
  wall: a rounding of a tangent chain through a corner tighter than its
  ball, which OCCT's walk does not do.
- `5ef2dbd5` item 38 (Fillet12, radius 2 on a line 13 long between a face
  2.2 wide and a strip 1.34 wide whose far side is a large torus): both
  ends are vertices of four edges, the bar's ends already rounded with 0.5
  (a cylinder and a torus are the faces at the end). OCCT's corner for one
  stripe at more than three edges fails ("two faces met round the vertex
  share no edge"); the file rolls the rounding over the strip onto the
  torus, which cuts it along most of its length and at the ends.
- `a9e82a9a`, `de7b8b9d` item 99 (Chamfer7, 1 on the bore's rim face and 2
  down its wall, the rim of radius 85 in two arcs): along the shorter arc
  the wall is 1 high above a ledge, and the file's chamfer runs over the
  ledge's edge and on down the wall below (a cone piece of 0.30 mm²);
  OCCT's chamfer fails on both bodies.
- `a9e82a9a`, `de7b8b9d` item 191 (Fillet44, radius 10 on two edges of a
  plane wall in a groove of radius 5): both ends fail ("the rounding's
  contact does not reach the edge at the vertex"); the file's rounding is
  four small pieces about 2 wide. Items 31 and 137 not traced further.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `a9e82a9a`, `de7b8b9d` 18 | 2 / 2 parametric, verified, inputs read (0930) | |
| `6418ebb1`, `920a1cef` 41 | 2 / 2 parametric, verified, inputs inferred (5 references lost in the file; widened inherited tolerances) | |
| `6418ebb1`, `920a1cef` 44, 45 | 0 / 2 (44 falls back; 45 parametric on the stored state, as before) | the roundings of 45 end at the rod, which their contacts never reach; 0077's cut leaves a stale end on the plane's edge (open, round 14) |
| `a9e82a9a`, `de7b8b9d` 29, 31, 99, 137, 191; `5ef2dbd5` 30, 38 | 0 / 12 | kernel gaps on the stored bodies, above |

Designs, each imported alone (features parametric of all, before →
after; history states matching): `a9e82a9a` and `de7b8b9d` 183 → 184 of
199 (119 → 120 of 145 built); `6418ebb1` and `920a1cef` 41 → 42 of 50
(17 → 18 of 25; the final body's volume within 5.3e-6 of the file's, as
before). Unchanged: `5ef2dbd5` 83 of 98, `db73863f` 135 of 145 (its final
body's deviation of 0.49 mm is the same with OCCT without 0930 and without
the widening), `52ba7a15` 97 of 110, `61526c66` 14 of 14.

## The roundings at the node rod (round 14 of mitcad#343)

Items 44 and 45 of `6418ebb1` and `920a1cef`, which round 13 left open,
with port patch 0931 back and the port's patches 0932 and 0933, traced on
the replay's body before item 45 through OCCT's fillet alone:

- **Item 45 (Fillet7): parametric, verified, inputs inferred** (2 of its 5
  references are lost in the file; the 3 pieces of the torus's edge with
  the plane match state 21 within 3.1e-6). The pieces' ends past their
  vertices run onto item 43's torus round the rod's foot, where 0077 cuts
  the walks before singular sections; the cut ends were taken up again
  from the cut (0932 ends them there). Each pair of ends at one of the
  rod's two strips is one blend in the file. Over one strip the contact
  on item 43's torus leaves it and comes back (round 8's join, which found
  no strip: the cut ends' contacts lie on no arc of it; 0933 finds it
  across an edge of the torus). Over the other the contact stays on the
  torus and the strip pokes through the blend: the file's face there has
  a hole bounded by a closed curve of 3.66 mm on the rod, which keeps a
  lens of 0.888 of its 2.26 mm². 0933 joins such ends as one piece over
  the whole strip, which `PierceBlends` holes. On the replay's body the
  rounding builds as the 3 pieces or with the strips' 2 edges, in every
  order of the contours (8 orders of the 5, 4 of the 3), within 0.0033
  mm³ of the file's change of 7.5193 mm³; before, two corners failed.
  OCCT-only test `geometry` (`test_post_foot_rounded_over_two_caps`, two
  sizes: a post's foot rounded over two caps that poke through it).
- **Item 44 (Fillet6): parametric, verified, inputs read** (0931, round
  13), now that item 45 builds on the replay after it.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `6418ebb1`, `920a1cef` 44, 45 | 4 / 4 parametric, verified (44 inputs read, 45 inferred: 2 references lost in the file) | |

Designs, each imported alone (features parametric of all, before →
after; history states matching): `6418ebb1` and `920a1cef` 42 → 43 of 50
(18 → 19 of 25; the final body's volume within 4.1e-6 of the file's,
5.3e-6 before; with 0931 alone item 45 ran on the replay and the final
body lay 2.2 mm off). Unchanged: `a9e82a9a` and `de7b8b9d` 184 of 199 (120
of 145 built), `5ef2dbd5` 83 of 98, `db73863f` 135 of 145 (its final body's
deviation of 0.49 mm as before), `52ba7a15` 97 of 110, `61526c66` 14 of 14.

## A key's edge rounded past its side (round 15 of mitcad#343)

The roundings round 14 left in `5ef2dbd5`, `a9e82a9a`/`de7b8b9d` and
`6418ebb1`/`920a1cef`, each design imported alone with the failure dump,
traced through OCCT's fillet alone on the history's stored body before
the item and on the replay's.

- **`5ef2dbd5` item 38 (Fillet12, radius 2): parametric, verified, inputs
  read (port patch 0940).** The edge is a key's top edge in a bore: the
  key's side B, a plane strip between the edge and the bore's wall C (a
  torus) across a concave crease, is 2.2 high in the middle and lower than
  the ball towards both ends, where B and C end at the key's end
  roundings of 0.5. The first deviation is in the walk, not at the
  corners where it failed: past the crossings 0.538 either side of the
  middle, where the ball's contact on B reaches the crease, OCCT's ball
  rolled on the crease (0011's obstacle), a ball that sticks out of the
  key's side, and the corners at the ends (four edges each) failed. The
  file's rounding is one cylinder tangent to the top and to B's plane all
  along, cut where the bore's wall runs through it; the wall grows over
  the strip up to that cut, and at each end the end rounding cuts the
  blend from where its junction with the wall, run on along its circle,
  meets the blend, down to the top. 0940 makes 0531's narrow-face
  computation take such a rounding (only across a concave crease) and lets
  the face beyond cut the blend's narrow side from the crossing on. On
  the stored body: valid, every new face on the file's within 1e-3 mm, the
  change -9.5996 against the file's -9.5954 mm³; in the import its faces
  lie on the file's within 0.01 mm but for the file's sliver of B (0.0024
  mm²) at one end, 0.020 mm off: the file's two ends differ (at the
  edge's start the wall grows up to the top and B keeps that sliver, at
  its end the end rounding cuts the blend), ours are alike. OCCT-only test
  `geometry.fillet_narrow_cut` (a key in a spherical hollow; with end walls
  invalid without the patch).
- Knock-ons in `5ef2dbd5`, item 38 no longer bringing the stored state:
  item 39 (Fillet13) rounds the file's edge of the blend and the wall at
  the edge's start, 7.041 long up to the top there; ours ends 0.35 mm
  shorter at the end rounding's cut, so its edges are found against
  history state 24 (parametric and verified as before, inputs inferred
  instead of read). Item 97 (Extrude5, the part of a cut across components
  that cuts nothing in this one) was skipped because the bodies before it
  were the file's state; item 37's corner faces up to 0.063 mm off (its
  verified result, within the deviation the owner accepts) now reach it,
  and a base feature brings the state (fallback). The final bodies are the
  file's as before.

Not fixed, each failing the same way on the history's stored body before
the item:

- `a9e82a9a`, `de7b8b9d` item 29 (Fillet13, radius 1.5): the tangent
  chain is the whole outline of a window's ceiling and floor, strips 3 wide
  between the ring's cylinders: convex along both arcs, concave round the
  window's ends, where the wall's ends are full-rounded (half cylinders of
  1.5 standing into the window). The chain changes from convex to concave
  at its tangent vertices; OCCT's inner contour fails in the walk. The
  file makes each strip one torus (113.5, 1.5) ended by the wall ends'
  half cylinders, which grow up to the tori's tubes. Needed: the chain cut
  where it turns concave, the roundings ended by the faces there grown up
  to them. Item 31 (Fillet16) rounds the same strips of the pattern's
  copies (its stored-state definition 16 edges of those faces), not traced
  separately.
- `a9e82a9a`, `de7b8b9d` item 191 (Fillet44, radius 10): the walk rolls
  on the wall's top edges as the file's four pieces do, but past the
  spine's start it runs on 3.07, rolling on two edges onto the wall beside
  the groove, its end points 5 mm past the vertex on no arc ("Corner
  OnSame: no point on arc"); the file cuts the blend there by the end
  plane and, for 0.14 mm past it, by that wall. The other end fails
  writing the cap. Needed: the run past the start ended where the faces at
  the end cut it.
- `a9e82a9a`, `de7b8b9d` item 137 (Fillet34, radius 9 in slots 19 wide):
  the slots' end edges 3.7 long fail in the walk rolling on an edge; one
  long edge alone gives an invalid shape at the slot's end, where a step
  of 6 by 2 meets it. Not traced further.
- `a9e82a9a`, `de7b8b9d` item 99 (Chamfer7, 1 on the rim, 2 down the wall):
  the spine is the whole rim (closed), which 0531's chamfer over a narrow
  face does not take. Along the short arc the wall is 1 high above the
  underside of a ledge; the chamfer's contact 2 down lies below it, in the
  air, and the file's cone is cut lengthwise by that underside (which
  keeps r ≤ 84.5) and near the arc's ends by a pocket's cylinder (a piece
  of 0.30 mm²). Needed: the narrow-face chamfer on a closed spine and a
  third kind of crossing (contact on the narrow face past it in the gap,
  the other contact in the face's material: the face cuts the chamfer
  lengthwise and keeps its part outside it), cut by several faces along
  the gap.
- `5ef2dbd5` item 30 (Fillet4, radius 3): the chain along a cylinder of
  radius 10 and a profile of arcs of radius 0.4, 1.25, 12, 1.25 and 0.4;
  OCCT's walk fails before the concave arc of 1.25, tighter than the ball,
  as in round 13.
- `6418ebb1`, `920a1cef` item 37 (Fillet1): the stored bodies before and
  after the item are invalid themselves; not traced further.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 38 | 1 / 1 parametric, verified, inputs read (0940) | |
| `5ef2dbd5` 39, 97 (knock-ons) | 39: parametric, verified, inputs inferred (read before); 97: fallback (skipped before) | the file's two ends of item 38 differ, ours are alike; item 37's accepted 0.063 mm reaches item 97 |
| `a9e82a9a`, `de7b8b9d` 29, 31, 99, 137, 191; `5ef2dbd5` 30; `6418ebb1`, `920a1cef` 37 | 0 / 13 | kernel gaps above |

Designs, each imported alone (features parametric of all, before →
after; history states matching): `5ef2dbd5` 84 → 85 of 98 (58 → 59
items matched; inputs 60 read and 3 inferred → 60 and 4; the final bodies
as before). Unchanged, item by item: `a9e82a9a` and `de7b8b9d` 184 of
199, `6418ebb1` and `920a1cef` 43 of 50, `db73863f` 135 of 145, `52ba7a15`
95 of 110, `61526c66` 14 of 14.

## A cut that changes nothing beyond the tolerance, and a chamfer's measured face (round 16 of mitcad#343)

- **`52ba7a15`: 97 of 110, not 95.** Round 15's 95 was an import without
  the design's reference designs: imported alone with them (the reference
  folder next to the file is found by itself), main 102ffd7b and 4010c8eb
  both give 97 parametric, 1 fallback (Mirror1) and 12 skipped; without
  them (`--no-references`, or the file copied without its folder) both give
  95, items 9 and 66 (Extrude1, cuts through all of the linked parts'
  bodies) falling back with "there are no bodies to extrude through". No
  change between rounds 11 and 15 dropped an item.
- **`5ef2dbd5` item 97 (Extrude5): skipped again, not a fallback.** It is
  the part of item 66's cut across components that falls in the bar's
  component. In the file its operation changes the bar's body by 0.063 mm³
  and 4 faces, its faces within 4.3e-3 mm of the state before: a few
  micrometres where the cutting tool grazes the body. None of its
  definitions touches the body, on the replay or on the stored state before
  it ("does not cut into any participant body"). Since round 15 the bar
  carries item 37's corner faces 0.063 mm off the file's (a verified result
  the owner accepts), so the bodies were not the item's state within the
  tolerance and a base feature brought the state in. Now an item whose
  states before and after agree within the geometric tolerance
  (`Importer::change_within_tolerance`: every solid paired in its
  component, the same or with its faces on the other's within the
  tolerance both ways, the sheets the same), on bodies the replay took as
  the state before it, is skipped with the bodies as they are ("the file's
  history shows no change by it beyond the geometric tolerance"), carrying
  what they carried. Test: `an_item_whose_change_stays_within_the_tolerance_is_skipped`.
- **A distance-and-angle chamfer's measured face from the edges'
  directions** (`5ef2dbd5` item 85, Chamfer1, the fin's root chamfered by
  a distance and an angle, the file's flag set). Its `flip` was taken against the first face of each edge's
  name, whose order follows the names (`E{F123:…|F142:end(…)}` on one
  replay, `E{F141:end(…)|F80:…}` on another): the same input gave the file's
  chamfer when the item before it was a base feature and one 8.6 % off when
  it was replayed, item 85 parametric only on the stored state. The file's
  flag (`isFlipped`, decoded per set) now picks the face against the file
  edge's own direction, as a two-distance chamfer's sides are
  (`Importer::decoded_sides`): not flipped, the face on its left; flipped,
  the other. A tangent chain takes its edges' majority, and sets whose
  chains differ are split. Item 85 builds the file's chamfer on both bodies
  (its state within 5.3e-6). The controlled models of a block chamfered by
  5 mm at 30° with the flag on and off (`c33814fe`, `8728caa3`) had their
  chamfers on the wrong face, taken only because the block's measures came
  within 8.3e-5; now each is its state within 2e-16. The other designs with
  distance-and-angle chamfers imported again keep every outcome
  (`98a29231`, `24523ade`, `eecf90bd`, `d498ea9d`).

The rest of `5ef2dbd5`, traced on the history's stored body before each
item (now: items 86, 87, 91, 92 and 93 fall back):

- Item 86 (Chamfer2, 0.263 at 45° on the fin's two top chains): fails on
  the stored body too, with the file's edges as two chains ("the distance
  may be too large"; before port patch 0940 a self-intersecting wire). The
  sides are not the cause (45° on faces at right angles). Not traced
  further.
- Item 87 (Fillet27, radius 0.2): OCCT fails two contours on the stored
  body ("cannot end the rounding at a vertex where more than three sharp
  edges meet").
- Item 91 (Fillet20, radius 0.3 round the bar's outline on both sides): the
  corner's computation gives no result on the stored body.
- Item 92 (Fillet21, radius 0.3 on both rims of an arch-shaped slot through
  the bar): the first deviation is the file's own. The stored body's slot
  wall steps by 4.3 µm at the rims' ends: two plane strips 14 by 0.0043
  (0.0596 mm²) join walls 0.0043 apart. OCCT's result has the file's 190
  faces but the side plane comes out with a broken wire (area −642 mm²
  against 689) and the strip at the chain's end with none. The import's
  collapse of strips narrower than the tolerance (mitcad#299) does not take
  it, as the strip stands for a step between parallel walls, not for a
  crease. In the file the rounding runs on across the step and ends 0.12
  below it in a corner (tori of 0.079 mm²). Needed: the step taken as one
  wall within the tolerance, or OCCT's end across such a step.
- Item 93 (Fillet22, radius 0.5): OCCT fails the rounding's computation
  along its chain on the stored body (one faulty contour; mitcad#322 listed
  it among the rational sections' weights).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `52ba7a15` 97 of 110 | confirmed on 102ffd7b and 4010c8eb | 95 only without its reference designs |
| `5ef2dbd5` 97 | 1 / 1 skipped as before (its change 4.3e-3 mm) | |
| `5ef2dbd5` 85 | 1 / 1 parametric on the replayed body, verified | |
| `5ef2dbd5` 86, 87, 91, 92, 93 | 0 / 5 | kernel gaps on the stored bodies; 92: the file's 4.3 µm step |

## A chamfer that hung in the kernel (mitcad#495)

`98a29231` (also `24523ade`) item 31, Chamfer1: the feet of four round
bosses (radius 7.5, 13 long) under a plate chamfered by 13 on the plate's
underside. Without `--hang-limit` the import never returned; with it the
item fell back as hung. Traced with the import's trace (each definition is
now named before it is evaluated, [*Time limit, stop and hangs*](f3d-performance.md#time-limit-stop-and-hangs)), the
replayed body before the item from the dressup dump
(`MITCAD_IMPORT_DRESSUPS_ITEMS` naming item 27, the last item before it
that changes the body), and a probe outside the repository sampling the
stack on a timer:

- **Where it hung:** not in the chamfer itself. OCCT's chamfer returned an
  invalid result in 0.1 s; the dressup healed it (`healed`,
  `ShapeFix_Shape`), and `ShapeFix_IntersectionTool::FixIntersectingWires`
  never returned on one boss's bottom face: two arcs of the boss's rim in
  two wires of that face met end to end, their intersection was a segment
  of no length at the common vertex, and the edges were "cut" there and
  intersected again for ever (`CutEdge` reported a trimmed circle cut
  though it cuts only trimmed lines). Port patch 0942 makes `CutEdge`
  report a cut only where it made one; the repair then goes on (with it
  alone the definition fails at once with "an invalid shape").
- **Why the result was invalid:** two of the bosses stand under the
  plate's corners, rounded with 20 about the bosses' axes, one side of
  each corner slanted (tangent to the arc). The chamfer's contact on the
  underside (radius 20.5) runs 0.5 beyond the outline round the corner and
  along both sides (port patch 0142's gap, cut by the chain of the faces
  beyond, which grow down to the cone). The bosses' seams, the closed
  spines' starts, lie just before the gap, so the gap is long and ends
  towards the spine's end; its last crossing's parameter on the boss's
  circle was taken a period early (-0.286 for 5.997), and the last piece
  ran more than once round the circle (the bottom face's rim in arcs over
  four wires, the cone's seam a tolerance of 58 mm). Port patch 0941 takes
  the crossings in the period that follows the piece before the gap. Each
  boss alone on the replayed body: two built right before, the other two
  +618 and -733 mm³ off and invalid; now all four are the file's.
- **On the file's stored body before the item** (state 18): OCCT's
  chamfer, with the bosses' feet found by their geometry, did not hang
  there before either (its repair returns). With the patches the two
  bosses above build there too, but the result is invalid at another boss,
  whose gap crosses the plate's edge exactly at the boss's seam: the
  crossing lies 1e-7 mm before the spine's start (on both bodies), and the
  stripe is split at the start, leaving a piece of that length; on the
  stored body its parameter on the boss's circle is 1.3e-8 rad off the
  seam (1e-15 on the replayed body), and the boss's bottom face gets edges
  back and forth over those 1.3e-8 rad. Not fixed: the replay builds and
  verifies, so no retry on the stored state is made.

Item 31 is now parametric and verified on the replayed body (its change
of volume +25112.5803 against the file's +25112.5808 mm³, the change
difference within the tolerance), and the import returns without a hang
limit. The items after it replay on the chamfer's result instead of the
file's state 19; their outcomes are unchanged (item 33, Fillet3, falls
back as before, its note now that the rounding's surface and the face at
its end do not intersect, before that the surface twists).

The designs of mitcad#343, each imported alone once with the patches
(parametric / fallback / skipped of all items; before: round 16's, on
main 904963b8):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `98a29231`, `24523ade` | 79 / 4 / 5 of 88 (item 31 hung) | 80 / 3 / 5, item 31 parametric and verified, no hang limit | as before (2.7e-8) |
| `52ba7a15` | 97 / 1 / 12 of 110 | same | as before (2.2e-5) |
| `5ef2dbd5` | 85 / 6 / 7 of 98 | same | as before (8.0e-7) |
| `a9e82a9a`, `de7b8b9d` | 184 / 8 / 7 of 199 | same | as before (1.1e-6) |
| `6418ebb1`, `920a1cef` | 43 / 5 / 1 (+1 partial) of 50 | same | as before (4.1e-6) |
| `db73863f` | 135 / 2 / 8 of 145 | same | 4.5e-4 |
| `61526c66` | 14 of 14 | same | 1.6e-5 |
| `4d3c4d1b` | 33 / 2 of 35 | same | 3.8e-7 |
| `eecf90bd`, `d498ea9d` | outcomes kept in round 16 (no counts) | 101 / 7 / 12 of 120 | 4.8e-7 |

`eecf90bd` and `d498ea9d` have another item the kernel does not finish:
item 50 (Fillet7, radius 1 on ten edges) runs on in OCCT's corner of more
than three edges (`PerformMoreThreeCorner`, the plate surface's projection
of a boundary curve, `ProjLib_CompProjectedCurve`), with or without these
patches (the patched files are not on that path); the corpus's hang limit
(600 s) gives it its state, as before. Not traced further here.

Left from the synthetic cases: the boss's seam inside the gap, in its
piece along the corner's arc (seams from 200 to 240 degrees in the test's
geometry), gives an invalid result with and without 0941: the piece across
the spine's start is split there, and its part after the start keeps its
parameters on the boss's circle a period on.

## Rounding ends at a pillar's halves, over a consumed sill and at a wall's step (round 17 of mitcad#343)

Each item traced with the dressup dump and OCCT's fillet alone on the
replayed body before it (a probe outside the repository):

- **`a9e82a9a` item 29 (Fillet13, radius 1.5 on a window's floor and
  ceiling in a ring wall 3 thick):** the pillars' full-rounded ends at
  the rounding's ends are each two half cylinders of one surface, split
  where the window's other edges end. OCCT's corner cuts a rounding's end
  by one face at the end and fails across the two halves; the stored
  bodies have each end as one face. The dressup's second tries now go on,
  after the merged edge pieces, with the faces of one surface that meet
  along an edge at the selected edges' vertices joined as well
  (`merge_edge_pieces` with faces, `geometry/src/dressup.cpp`). Every new
  face on the file's within 2e-6 mm, the change of volume -60.7576
  against the file's -60.7451 mm³. Test (`geometry`):
  `test_window_floor_rounded_up_to_pillar_ends_in_halves`.
- **Item 31 (Fillet16, the same rounding on four windows of a circular
  pattern, 48 edges):** a sill's two roundings, each half the wall's
  thickness, take the sill away, their contacts on it along each other at
  the wall's middle. OCCT took them for roundings too big for the face, or
  left the sill as a sliver of no area, depending on the copy. Port patch
  0950: valid, every face on the file's within 2e-6 mm, -243.0297 against
  -243.0189 mm³. Test: `geometry.fillet_consumed_strip`.
- **Item 33 (Fillet17):** before, 16 of its 48 edge references were lost
  on the replayed body; on items 29's and 31's results its edges are
  found against the history's state 187, and the item is parametric and
  verified.
- **`5ef2dbd5` item 92 (Fillet21):** round 16 found the file's 4.3 µm
  step in the slot's wall. The stored bodies end the rounding at the
  section through the step's vertex and close it with a quarter torus
  (both radii 0.3): the ball turns on about its contact's end point until
  it lies in the wall's plane, cut by the stepped-back wall. Port patch
  0951 builds that end. On the body before the item: the file's 190
  faces, every face on the stored body's (distance 0 both ways), -1.7315
  against -1.7477 mm³; the item is parametric and verified. Test:
  `geometry.fillet_step_end`.

Left, each at its first deviation:

- **`5ef2dbd5` item 91 (Fillet20, radius 0.3 round the bar's outline):**
  the rounded edge's faces (the wall and item 30's blend) come tangent at
  its end vertex, their normals 0.037 rad apart 0.04 mm before it and
  0.0037 rad at it. The walk stops 0.04 mm short, where its contact on
  the blend reaches the rounded edge itself (the section 0.011 wide). A
  vanishing end is taken within 0.02 of the radius only (0.006 mm), so the
  corner goes to the intersection at the end, whose plate fails
  (`GeomPlate_BuildPlateSurface::ProjectCurve`). The file's blend ends in
  a point at the vertex. Needed: the walk on to the vertex, or the end
  taken as vanishing where a contact runs onto the rounded edge.
- **Item 93 (Fillet22, radius 0.5):** the chain runs round two vertical
  corners rounded with 0.3, less than 0.5; the walk round a corner's
  cylinder fails in its approximation (`StoreData`), the ball's centre
  having to turn about the corner's axis at a negative radius. The file
  builds a torus and a vertex blend there. Needed: a set-back vertex
  blend.
- **Item 87 (Fillet27, radius 0.2 on the fin's top edges):** at the vertex
  where both faces change at once (a cone and the cylinder of the fin's
  top to a B-spline sliver and the extrusion surface of the top, the
  chain kinked), the next piece's first section is not found
  (`PerformFirstSection` reframed on both faces): the side contact there
  still lies on the face before. In the file the sliver keeps a part, the
  contact passing from the cone over it to the next cone.
- **Item 30 (Fillet4, radius 3, concave, over B-spline faces):** the walk
  stops before the piece's end without reaching an arc, twice
  (`ComputeData`'s check that the walk got far enough); the same on the
  stored body. Not traced further.
- **Item 86 (Chamfer2):** OCCT's chamfer fails on the replayed and the
  stored body; not traced further (round 16).
- **`a9e82a9a` item 191 (Fillet44, radius 10):** the rounded edges lie at
  the foot of a wall face only 0.86 high; the ball's contact on the wall's
  plane lies above its top. The walk starts 5.28 past the chain's start
  (the corner: "Corner OnSame : no point on arc") and ends on arcs 1.2 and
  2.0 from the end vertex ("OneCorner : bouchon non ecrit"). The file's
  blend is cut by the top face and the end faces.
- **Item 137 (Fillet34, radius 9):** at the edge's end vertex, where more
  than three edges meet among small step faces, the walk ran past the
  vertex to the top face's edge 2 beyond it; the intersection at the end
  does not find the face it looks for and stops at its limit of 15 faces
  ("the max number of faces reached").
- **Item 99 (Chamfer7, two distances):** OCCT's chamfer fails on the
  closed chain over a narrow face; not traced further.
- **`6418ebb1` item 37 (Fillet1):** the stored bodies before and after
  the item are themselves invalid (slivers away from the item), so the
  item cannot be checked against them; the rounding of three blends over
  a consumed triangle fails in OCCT. Not fixed.

The designs of mitcad#343, each imported alone once with port patches
0950 and 0951 and the face join (parametric / fallback / skipped of all
items; before: round 16's, on main 904963b8):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `5ef2dbd5` | 85 / 6 / 7 of 98 | 86 / 5 / 7, item 92 parametric and verified | as before (8.0e-7) |
| `a9e82a9a`, `de7b8b9d` | 184 / 8 / 7 of 199 | 187 / 5 / 7, items 29, 31 and 33 parametric and verified | as before (1.1e-6) |
| `6418ebb1`, `920a1cef` | 43 / 5 / 1 (+1 partial) of 50 | same | as before (4.1e-6) |
| `db73863f` | 135 / 2 / 8 of 145 | same | as before (4.5e-4) |
| `52ba7a15` (with its reference designs) | 97 / 1 / 12 of 110 | same | as before (2.2e-5) |
| `61526c66` | 14 of 14 | same | as before (1.6e-5) |
| `4d3c4d1b` | 33 / 2 of 35 | same | as before (3.8e-7) |

## A rounding vanishing on its own edge (round 18 of mitcad#343, `5ef2dbd5` item 91)

`5ef2dbd5` item 91 (Fillet20, radius 0.3 round the bar's outline on both
sides, 22 edges), traced with the dressup dump and OCCT's fillet alone on
the replay's body before it (items 88 to 90 replayed on item 87's
fallback) and on the history's stored body before it, which fail and
build alike:

- **First deviation.** On each side the outline's chain runs along item
  30's concave rounding (radius 3), which goes on tangentially into a
  corner rounding of 0.4 tangent to the side plane: at the chain's end
  vertex V the rounded edge's two faces come tangent and the blend
  vanishes. The stored faces meet only within the edge's tolerance
  (1.5e-3): a gap g between faces at an angle a moves the line where they
  meet sideways by g / a, and the ball's contacts, 0.3 a / 2 from that
  line, reach the edge itself where a² = 2 g / 0.3. At all four ends the
  walk's contact on item 30's rounding met the rounded edge 0.014 to 0.042
  short of V (sections 0.004 to 0.011 wide) and the walk stopped. An end is
  taken as vanishing only within a hundredth of the radius (0.003), so the
  corner went to the intersection at the end, whose plate failed (one
  vertex); each tangent chain alone gave an invalid shape. The file's
  blend ends in a point at V.
- **Fix (port patch 0961).** Where a lone stripe's contact ends on the
  spine's own edge into a vertex where its faces come tangent, its section
  no wider than sqrt(2 R tol) of that edge and both contacts within the
  radius of the vertex, the end is put at the vertex; the blend between
  the stop and the vertex is narrower than that section (0.011 here, the
  closing piece within half of it of the true blend). The blend's B-spline
  surface is bent to the vertex with its contact curves, so that the end
  needs no tolerance as wide as the stretch left (0.04: the dressup's check
  of inflated vertex tolerances took that for a corner left open). On both
  bodies: valid, the file's 174 faces, every face on the stored body's
  after the item within 0.004 mm both ways; the import verifies the
  item (its new faces and the file's within 0.01 mm). OCCT-only test
  `geometry.fillet_vanishing_own_edge` (a tower's corner and foot rounded
  on a slab flush with its side, the foot's torus moved 0.0002 to 0.001 off
  the side within its edges' tolerance; the fillet fails without 0961).
- **Item 92 (Fillet21) on item 91's result (port patch 0962).** With item
  91 replayed, round 17's rim rounding meets the 4.3 µm wall step with the
  wall on the blend's other side (the faces' order changed); 0951's torus
  is then reversed in u, and its cut by the stepped-back wall, computed on
  the reversed torus, failed: the side plane's wire came out broken (area
  −642 mm²). The cut is now computed on the torus as built and its curve
  mirrored onto the reversed one: valid, every face on the stored body's
  within 0.004 mm; parametric and verified, inputs read.
- **Item 95 (Fillet24, radius 0.2 on the walls' edges below the step).**
  With 0951's end built on both sides, it got quarter tori at both its
  ends 0.04 off the file's; the stored bodies run that rounding on across
  the step, cutting the recess's 4.3 µm, until item 92's torus cuts it.
  0962 takes the pivot only where the recess ends in a strip back to the
  wall (0951's case); here item 92's rounding ends it, and OCCT runs the
  blend on as before: the file's 229 faces, every face of the result on the
  file's within 4.3e-4 mm (before, one end's torus 0.04 off as well). Test:
  `geometry.fillet_step_end` (0951's cases with the body's faces in the
  reverse order; a second rounding below the step after the first above
  it).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 91 Fillet20 | 1 / 1 parametric, verified, inputs read (0961) | |
| `5ef2dbd5` 92 Fillet21 | 1 / 1 parametric, verified, inputs read (as before; now on item 91's result, 0962) | |
| `5ef2dbd5` 95 Fillet24 | 1 / 1 parametric, verified, inputs read (as before) | |

The two designs, each imported alone with port patches 0961 and 0962
(parametric / fallback / skipped of all items; before: the same branch
without them, round 17's port):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `5ef2dbd5` | 86 / 5 / 7 of 98 | 87 / 4 / 7, item 91 parametric and verified (history states matched 60 → 61) | as before (8.0e-7) |
| `a9e82a9a` | 187 / 5 / 7 of 199 | 186 / 6 / 7: item 44 (Fillet19) cut by the item's 60 s, see below | as before (1.1e-6) |

`a9e82a9a` item 44 is the same rounding as before (its first definition
agrees, with the same corner faces up to 0.077 mm off), but the imports
ran while the machine's load stood at 35 to 43 (a release build and other
agents): that definition took 9.5 to 19.6 s against 4.1 s in the run
before, and the second, which before failed after 8.4 s, was cut by the
item's time (60 s) at 19 to 23 s; the item had taken 35 s before. The
chains of that definition build the same with and without the patches
(the first alone on the body before it: the same result).
## Round 18 of mitcad#343

### An underside outline rounded past corners tighter than the ball (`5ef2dbd5` item 93)

Item 93 (Fillet22, radius 0.5 on five edges): the underside of the bar's
end, a plane, rounded along its outline of a side wall square to it, a
corner rounded with 0.3, a front wall leaning 15 degrees over it (its edge
with the underside acute), a second corner and the other side wall. Traced
with the dressup dump and OCCT's fillet alone on the replayed body and on
the history's stored body before the item (a probe outside the
repository); both failed the same way.

- **The first deviation** is before `StoreData`, where round 17 saw it
  fail: no ball of radius 0.5 rolls round a convex corner of 0.3, its
  centre would have to turn back about the corner's axis. OCCT walked the
  chain over the corner's arc anyway; the walk ran backwards there and its
  approximation threw when it was stored. The stored body after the item
  stops both walls' roundings (cylinders) where their axes cross, at the
  ball touching the underside and both walls, and closes each corner with
  a vertex blend and a torus: the vertex blend (a cubic B-spline of 0.245
  mm²) runs from that ball's contact with the underside, a point, along
  both roundings' ellipses to an arc of the corner's radius in the plane
  of the ball's centre; the torus (major radius 0.2, minor 0.3, about the
  front rounding's axis) turns the corner's cylinder from the front wall's
  direction to that plane, meets the side wall along an arc of radius 0.2
  and cuts the corner's cylinder along its section through the front
  rounding's contact. The vertex blend is the ellipses' family about the
  ball's centre: each of its sections through the centre square to the
  underside is the quarter ellipse from the contact point to the arc.
- **Port patch 0970** builds that construction: a contour is split at
  such an arc into contours that end there, and each corner gets the
  exact vertex blend (a rational biquadratic patch, tangent to both
  roundings) and the torus as corner patches; an obtuse wall is left as
  before. A second fault surfaced at the split contours' ends: OCCT's
  analytic rounding of a contour's last edge dropped all its pieces where
  no contact runs into a face at that end, and the side's rounding was
  walked as a B-spline that the end face at the outline's far end then
  cut into an invalid face; such pieces are now kept, as at a contour's
  first end. On the stored body before the item: valid, the file's 197
  faces, every face on the stored body's within 0.002 mm both ways, the
  change of volume -1.5382 against the file's -1.5432 mm³ (0.33 %). OCCT-
  only test: `geometry.fillet_tight_corner` (a block's bottom outline past
  front corners of 0.3, the front wall square and leaning 15 degrees, and
  a whole outline: the material taken within 1e-6 mm³ of the integral
  where the walls are square; the leaning wall's fillet fails without the
  patch).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 93 | 1 / 1 parametric, verified, inputs read (0970) | |
| `5ef2dbd5` 94, 95 (after it) | 2 / 2 parametric, verified, on item 93's result instead of the stored state | |

The designs, each imported alone once with port patch 0970 (parametric /
## A groove's rim rolled over a strip and cut at a notch's walls; a slot's rounding round a wall's end (round 18 of mitcad#343)

Items 191 and 137 of `a9e82a9a` (also `de7b8b9d`), each traced with the
dressup dump and OCCT's fillet alone on the replayed body before it and on
the history's stored body (a probe outside the repository).

- **Item 191 (Fillet44, radius 10): parametric and verified (port patch
  0990).** A groove of radius 5 runs under a slot's end wall, whose face is
  left a strip 0.86 high between the groove and two concave roundings of 9
  (item 137's) and the ceiling; the groove passes through a notch in the
  wall's 1 mm thickness. The rounded edges are the groove's rim on the strip
  across the notch. The ball's contact on the strip lies above it, so the
  ball rolls on the strip's top edges (the roundings' end sections and the
  ceiling's edge) in four pieces, as the file's; the walk was the file's.
  The first deviation was at both ends: past them OCCT walked on with the
  groove's contact run off onto the wall beside the notch and failed both
  corners. The stored bodies keep the rolling blend up to where its ball
  comes tangent to the rounding of 9 (0.026 mm before OCCT's walk ended at
  one end), run its surface on along its tangents and cut it by the wall
  beside the notch, by the notch's wall and by the rounding's surface, which
  grow up to the cut (the rounding by 0.0056 and 0.135 mm). 0990 builds that
  end. On the body before the item: valid, every face on the stored body's
  within 1.6e-4 mm both ways, the material in a box round the rounding
  207.3352 against the stored body's 207.3360 mm³ (the same on the stored
  body before the item). OCCT-only test: `geometry.fillet_rolled_end_cut`
  (fails without the patch).
- **Item 137 (Fillet34, radius 9, concave, on the ceiling edges of two
  slots' walls in both copies of a pattern): not fixed.** The two edges along
  one side wall build; the two along the other fail at their end vertex,
  where the 1 mm wall the rounding runs along ends at a foot 6 wide that the
  rounding's section (9 wide) overhangs. OCCT walks the rounding on past the
  vertex: its contact on the ceiling runs on to the edge of a block 2
  beyond, its contact on the wall stops at the wall's end edge. With no edge
  to prolong, the corner goes to `PerformIntersectionAtEnd`, which walks
  round the vertex along the ceiling's edges from the block's edge to find
  the faces at the end; that walk can only come back to the vertex round the
  ceiling's whole outline, and stops at its guard of 15 faces ("the max
  number of faces reached"). Raising or removing the limit would not help:
  the faces it would collect lie away from the end. The stored bodies end
  the cylinder in the foot's plane where the foot covers the section and
  beyond it revolve the end section about the wall's end edge: a torus with
  both radii 9 over 264 degrees (the ball touching the ceiling and that
  edge), most of it inside material, which fills the corners of the gaps
  round the wall's end and is cut by nine faces (the foot's sides, the wall's
  other side, the next wall across a gap of 0.89, a block's three faces and
  the ceiling along tangent arcs). Needed: a corner that sweeps the end
  section about the end edge and trims the swept blend against body faces
  away from the vertex, through thin walls, which OCCT's corners (cut by the
  faces round the vertex) do not do.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `a9e82a9a`, `de7b8b9d` 191 | 2 / 2 parametric, verified, inputs read (0990) | |
| `a9e82a9a`, `de7b8b9d` 137 | 0 / 2 | the end revolves about the wall's end edge, cut by nine faces away from the vertex; no such corner in OCCT |

The designs, each imported alone once with port patch 0990 (parametric /
fallback / skipped of all items; before: round 17's):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `5ef2dbd5` (with its reference designs) | 86 / 5 / 7 of 98 | 87 / 4 / 7, item 93 parametric and verified; the others' outcomes the same | 1.1e-6 (8.0e-7 before: item 93's own 0.005 mm³) |
| `a9e82a9a` | 187 / 5 / 7 of 199 | same (falling back: items 99, 123, 137, 156, 191) | as before (1.1e-6) |

### `5ef2dbd5` item 87: a fin's top edges rounded over slight creases

Item 87 (Fillet27, radius 0.2 on a fin's top edges, 12 edges): the fin's
top is an arc and a straight part; along the arcs its sides are chamfered
(items 85 and 86, on the stored state), and slivers join the chamfers to
the straight part's sides. Every face meets the next at a crease of one to
five degrees (normals within the tenth of a radian OCCT's fillet takes for
tangent), and both faces of the rounding change within a few micrometres
of one section, the chain kinked by about a degree and a half there. The
file's edges are the arcs' and the straight parts'; short sliver edges lie
between them. Traced with the dressup dump and OCCT's fillet alone on the
replayed body before the item (the stored state: items 85 and 86 fall
back), five causes, each hidden behind the one before:

- **The chains stopped at the kinks** (`StopChainsAtTurns`, port patch
  0180): each selected edge became a contour of its own, ending at a
  vertex of four sharp edges where the corner failed ("cannot end the
  rounding at a vertex where more than three sharp edges meet", rounds 16
  and 17). A contour now runs on over an edge not asked for that leads
  tangentially to an edge asked for.
- **The first walk after the arc's torus failed at once**, round 17's
  first deviation (`PerformFirstSection`): the torus ends 0.0007 short of
  the arc's end, the walk's guide started with that 0.0007 of the arc, the
  spine's kink was turned within it, and the guide carried on before its
  start (where the section's reframing looks) ran off the spine by
  millimetres. The guide is now built from a stretch of the arc before it.
- **The walk onto the straight top stopped** at a kink of a degree: the
  top holds the spine's end vertex, so it was taken for the face at the
  end (port patch 0750); a face of the spine's own edges is not.
- **Both faces change at one section**: the contact on the chamfer side
  leaves its face 0.004 mm before the top contact reaches the top's next
  face, but with that face's 1.4 degree crease the ball touching it sits
  0.003 mm further back, so the walks overlapped and a side contact lay
  off its face. Where one contact crosses onto a tangent face and the
  other lies within ten times the approximation tolerance of such an edge
  ahead, both change at once, the section starting on both arcs.
- **The sliver's concave crease of five degrees** into the side cone: the
  ball cannot pass its contact across it; it turns about the crease's
  edge for some 0.1 mm along the spine. OCCT went on as over a tangent
  edge and its walk on the cone started 0.096 mm further on, the gap
  closed by edge tolerances of 0.1 mm (the result valid, its volume 2 to
  19 mm³ off). Now the ball rolls on the crease's edge from the ball
  before until it touches the cone, the blend bordering the face whose
  part beside the crease stays.

All in port patch 0980. On the replayed body before the item: valid, the
dressup's result with every new face on the stored body's within 0.0083
mm and the stored body's within 0.0024 mm of the result's, the change of
volume -0.2909 against the file's -0.2888 mm³. The file's own result ends
the arc's torus past the kink and extends a sliver's surface over the
straight part's start; ours runs one rounding through, within those
distances. OCCT-only test `geometry.fillet_concave_crease` (a box's top
front edges rounded where the front turns outward by 3 and 5 degrees about
a slanted crease; the fillet fails at a vertex without the patch).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 87 | 1 / 1 parametric, verified, inputs read | |
| `5ef2dbd5` 88, 89, 90 (on item 87's result) | 3 / 3 parametric, verified (90: inputs inferred, as before) | |

The designs, each imported alone once with port patch 0980 (parametric /
fallback / skipped of all items; before: round 17's, on the branch
`343-r17`):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `5ef2dbd5` | 86 / 5 / 7 of 98 | 87 / 4 / 7, item 87 parametric and verified | as before (8.0e-7) |
| `a9e82a9a` | 187 / 5 / 7 of 199 | same, item by item | as before (1.1e-6) |

Item 90 (Fillet25) now runs on item 87's result instead of the stored
state: verified as before, its geometric check noting seven of the file's
new faces (0.8 % of their area) up to 0.044 mm off at its corners.
| `a9e82a9a`, `de7b8b9d` | 187 / 5 / 7 of 199 | 188 / 4 / 7, item 191 parametric and verified, every other item as before | as before (1.1e-6) |
| `5ef2dbd5` | 86 / 5 / 7 of 98 | same, item by item | as before (8.0e-7) |

## A fin's top chamfered over slight creases; a rounding past a concave corner tighter than its ball; an outline's rounding again (round 19 of mitcad#343, `5ef2dbd5` items 86, 30 and 91)

Each item traced with the dressup dump and OCCT's chamfer or fillet alone
(and MitCad's dressup) on the replay's body before it and on the history's
stored body (a probe outside the repository).

- **Item 86 (Chamfer2, 0.263 at 45 degrees on both top chains of the fin
  1 thick): parametric and verified (port patch 1000).** Each chain is an
  arc and a profile's spline stretch in turn, whose top faces meet at the
  chain's vertices in concave creases of 1.4 and 2.2 degrees, and the side
  strips between the top edges and the root's chamfer (item 85, a walked
  B-spline on the replay, a cone in the file) are narrower than the
  chamfer along the spline stretches (0531's case, mitcad#263). The first
  deviation was at the first crease: 0980's turn about a concave crease
  (a rounding's ball cannot pass it) took the chamfer's piece after the
  arc's analytic one for a blend rolling on the crease's edge, which
  chamfers do not have, and the chamfer threw before any piece was made,
  on both bodies ("the distance may be too large"). A chamfer's contact
  passes such a crease; the file's result does too (its arc's cone ends at
  the vertex's section and the next piece starts on the next top face).
  With that, the computation again over the narrow strip's surface threw
  in 0671's cut of an analytic chamfer to the spine's end, which asked
  about the spine's edge on the unbounded strip; and on the replay the
  root's B-spline, grown up to the chamfer along its sections, ran past
  its surface's bounds (17 of 82 in its parameter), where the import's
  geometric check (`analysis_boundary_distances`, projections kept to the
  bounds) found the file's root cone 0.138 mm off the result, which lay
  on it. 1000 lets a chamfer pass the crease, keeps 0671's cut to the
  spine's edge's own faces, and continues a face's B-spline surface past
  its bounds where the face grew past them (the edges' curves stored again
  with it). On both bodies before the item: valid; on the stored body the
  file's 124 faces, every face of the file's on the result within 3e-4 mm
  and the result's on the file's within 1e-3 mm; in the import its new
  faces and the file's lie on each other within 0.01 mm, the change of
  volume 2.5 % off the file's (the replay's root chamfer, a B-spline,
  measured by its faces). Items 87, 88, 89 and 90 (roundings of the fin)
  now run on item 86's result instead of the stored state: each
  parametric and verified as before. OCCT-only test
  `geometry.chamfer_crease_strip` (a fin's tip, a line and a cubic turned
  up from it by 2 and 4 degrees, chamfered along both, the side whole and
  narrowed by a foot chamfer whose face is planar or a B-spline; fails
  without the patch).
- **Item 30 (Fillet4, radius 3, concave): not fixed, the file's corner is
  its kernel's own construction.** The rounded chain runs where a wall
  stands on the bar's horizontal rod (a cylinder of radius 10 about x):
  the wall's plan is arcs of 0.4 (convex), 1.25 (concave), 12 (convex),
  1.25 and 0.4, tangent in turn. The first deviation is OCCT's walk into
  the concave arc of 1.25, tighter than the ball: the ball's centre would
  run round it at a radius of 1.25 - 3 < 0, and the walk fails 0.13 before
  the arc's end (rounds 13 to 17), on the stored body as well. The ball
  that touches the arc of 12, the arc of 0.4 and the rod at once has its
  centre at (±4.540, -4.297, 45.270) and touches the rod at P (±4.5444,
  -3.3039, 42.4384). In the file each blend runs on past that ball: the
  one along the arc of 12 until its contact on the wall reaches that
  arc's end (its ball's centre at height 45.414), the one along the arc of
  0.4 likewise (45.667), both ending on the rod at P; the arc of 1.25 is
  cut at height 45.667. Between them two B-spline faces close the corner:
  a triangle of 3.74 mm² with its apex at P (a degenerate edge), bounded
  by a curve on each blend (on the blends within 8e-5 mm, but no section
  of them, not planar, not linear in their parameters) and by a circle of
  radius 1.25 tangent to the wall of 12 at the first blend's end, tilted
  to meet the second blend; and a strip of 0.68 mm² from that circle to
  the arc of 1.25. The triangle's middle lies 0.2 mm inside both blends'
  balls (above both blends), so neither the two blends cut
  by each other nor a rolling ball's corner (the ball at the three faces,
  a sphere, and the ball rolling up the wall's vertical corner) is the
  file's: those patches are 13 % of the item's new area each side, beyond
  the 2 % a corner patch may be off. Needed: that corner's construction,
  which the stored body shows only as fitted surfaces; building a
  look-alike would be a substitute construction.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 86 Chamfer2 | 1 / 1 parametric, verified, edges read by their names in the file (1000) | |
| `5ef2dbd5` 87, 88, 89, 90 (on item 86's result) | 4 / 4 parametric, verified, as before | |
| `5ef2dbd5` 30 Fillet4 | 0 / 1 | the corner past the concave arc tighter than the ball is the file kernel's own patch (above) |

- **Item 91 (Fillet20, radius 0.3 round the bar's outline, round 18's
  0961): parametric and verified again (port patch 1001).** On main
  962006e9 it fell back, where round 18's branch for it had it parametric:
  that branch did not have 0980, and the bundle that merged round 18's
  four branches did not import this design. Bisected on OCCT's fillet
  alone with the item's definition: with the port's patches up to 0970 it
  builds on both bodies (the stored body's 174 faces, -2.2689 mm³), with
  0980 every one of its four chains fails in the walk, on the replayed
  and on the stored body. Of 0980's five changes, its guide built from a
  stretch of the next edge (where a walked piece ends a hair past the
  start of the next edge, at which that edge's analytic piece starts)
  is the cause: each chain's walked first piece, from the vertex where the
  blend vanishes, ends 0.018 past the next edge's start, and the guide
  took a stretch of 0.136 of that edge. The edges go on tangentially
  there (2e-6 rad), so there was no turn to keep out of the hair, and the
  guide, approximated across the vertex, passed its end 1e-5 mm and
  5e-4 rad off the spine's point and tangent, where the analytic piece's
  section is; the walk's first step from that section failed. 1001 takes
  the stretch only where the spine turns at that vertex by more than a
  thousandth of a radian (item 87's kinks are a degree and a half). Item
  91 builds as with 0961 alone; items 86, 87 and 93 stay parametric and
  verified; items 92 to 95 run on item 91's result as before round 18's
  bundle. With every item from 84 on now replayed, the bar's final body
  carries the corner deviations those verified items keep (items 87 and
  90: faces up to 0.053 mm off at the fin's top corners), 0.078 mm³
  (9.8e-6 of its volume) where item 91's fallback had brought the stored
  state back before. The reference design `99ef321f` gains its item 21
  (Fillet31, the same outline's rounding on the bar's other part):
  parametric and verified, 17 → 18 of 24. No OCCT-only test: the hair
  comes from the next edge's analytic piece starting 0.018 past the
  vertex on the design's faces, which no synthetic body reproduced; the
  design's item is the check (0980's own case, item 87, still builds).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 91 Fillet20 | 1 / 1 parametric, verified, edges read by their names in the file (1001) | |
| `5ef2dbd5` 86, 87, 93 together | 3 / 3 parametric, verified | |
| `99ef321f` 21 Fillet31 | 1 / 1 parametric, verified (was a fallback) | |

The designs, each imported alone once with port patches 1000 and 1001,
built through vcpkg (parametric / fallback / skipped of all items;
before: main 962006e9 with its port):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `5ef2dbd5` (with its reference designs) | 88 / 3 / 7 of 98 (history items matched 62; reference `99ef321f` 17 / 5) | 90 / 1 / 7, items 86 and 91 parametric and verified, every other item as before (matched 64; reference 18 / 4) | the bar 9.8e-6 (3.6e-7 before, see above), the others as before |
| `a9e82a9a` | 188 / 4 / 7 of 199 | same, item by item (falling back 99, 123, 137, 156) | as before (1.1e-6) |

## A bore's rim chamfered over a pocket; moved faces; a rounding's end at a torus' seam (round 19 of mitcad#343, `a9e82a9a` items 99, 123 and 186)

Each item traced with the dressup dump and OCCT's chamfer or fillet alone
(and MitCad's dressup) on the replay's body before it and on the history's
stored body (a probe outside the repository). The same items in
`de7b8b9d`, the same design, follow.

- **Item 99 (Chamfer7, 1 on the top and 2 down the wall of a bore's rim of
  radius 85 in two arcs): parametric and verified (port patch 1010).** A
  pocket from below breaks through the bore's wall and ends 1 under the
  top, so along the shorter arc the chamfer's contact 2 down the wall lies
  in the pocket all the way. OCCT's analytic pieces give the longer arc
  one piece and the shorter arc none; the faces round the gap (the
  pocket's side, its ceiling, the side) were to cut the blend over it, but
  the search for the edge after the gap never went back to the gap's own
  first edge on the closed rim, and the chamfer failed. 1010 lets that
  search go round the closed spine. On the body before the item: valid,
  every face on the stored body's within 2e-6 mm both ways, the change of
  volume -522.6458 against the file's -522.6452 mm³. OCCT-only test
  `geometry.chamfer_rim_cutout` (a disc's rim in two arcs over a pocket,
  either arc first, the seam at either corner; fails without the patch).
- **Item 123 (MoveFace2, faces of a body moved by a translation): parametric
  and verified (the new `move_face`).** Moved faces fell back to the stored
  body: MitCad had no such operation. The decoder now reads a move face's
  faces (named, found in the history at the item's place) and the rigid
  matrix of its transform input, and the import gives one `move_face` per
  body ([commands.md](../../model-api/src/commands.md), *Move face*): the moved
  faces keep their surfaces, moved, and their neighbours extend or shorten
  along their own surfaces up to them (`geometry/src/move_faces.cpp`).
  Tests: `moved_faces_come_in_as_a_move_face` (`src/tests.rs`),
  `move_face_moves_faces_by_a_rigid_motion` (mock kernel),
  `geometry/tests/test_move_faces.cpp` (OCCT).
- **Item 186 (Fillet43, three roundings in turn): parametric and verified
  (a healing fix and port patch 1011).** With item 99 parametric, item 186
  ran on a replayed body for the first time, and failed twice:
  - The input body had been changed by an earlier item's dressup, a breach
    of the rule that operations leave their inputs alone. A dressup's
    failed try is healed on a copy of the body (`ShapeFix_Shape`), but the
    copy shared the input's curves on surfaces, and healing moves those in
    place (a wire on a periodic surface by whole periods): a torus of the
    input was left with p-curves a period apart, an invalid face. The input
    check compares handles, not the curves' values, so it did not see it.
    A copy that is healed now gets p-curves of its own first
    (`detail::own_pcurves`, `geometry/src/history.cpp`; used in
    `dressup.cpp`'s healing and strip collapse). Test `test_own_pcurves`
    (`geometry/tests/test_input_check.cpp`).
  - A pocket's top rim rounded with 1 up to an earlier rounding's torus
    (a bore's rim rounded with 0.5) whose parametric seam lies at the
    rounding's end vertex: at that end the contact runs past the torus'
    edge and its point on the torus came out a period beyond the face's
    domain, so the intersection at the end was walked the long way round
    (a curve of 3.3e6 mm and a body of 1e12 mm³ that the checker took for
    valid). 1011 takes that end in the period next to the other end. On
    the replayed and the stored body: valid, -7.525 and -7.615 mm³ at the
    seam end (the other end -7.618 and -7.512), every face of the three
    roundings on the stored body's within 9e-4 mm both ways. No synthetic
    body reproduced the end past the face's edge at the seam.

`a9e82a9a`: 190 parametric, 2 fallbacks (items 137 and 156, as before), 7
skipped of 199, each run alone with its reference folder; the final bodies
as before or closer to the file's.

## A drafted peg's corners rounded down to a block's walls (round 20 of mitcad#343, `52ba7a15` item 37)

Item 37 (Fillet6) of `52ba7a15` fell back on main f1cae0d2 ("the geometry
kernel's computation of the rounding along edge … fails"), where round 17
had it parametric and verified. Traced with the dressup dump and OCCT's
fillet alone (and MitCad's dressup) on the replay's body before the item,
with TKFillet built from the port at each patch from 0951 on and with the
dressup's healing (`detail::own_pcurves`) switched off.

- **The cause: port patch 0980 (round 18), not round 19.** The design was
  last imported in round 17; with the port's patches up to 0970 the item
  builds, with 0980 it fails, and 1000, 1001, 1010, 1011 and
  `own_pcurves` change nothing (the item's input is the same body, its
  stored state 2.2e-9 mm off). Of 0980's five changes, the turn about a
  concave crease (`StartSol`) is the one.
- **The case.** The item rounds the four corner edges of a square peg with
  0.5. The peg's sides are drafted by 3 degrees; 4 below the plate it
  hangs from, it runs into a block whose walls go on straight down from the
  peg's sides there (concave creases of 3 degrees), and whose vertical
  corners item 36 rounded with 0.4. So the peg's sharp corners poke out of
  those cylinders down to 6.2355 below the plate, where the corner edges
  end on them. In the file's result the rounding ends at the creases: the
  block's walls and its corner cylinders grow up to where they meet the
  rounding (to 3.841 and 3.447 below the plate), and the corner below goes.
- **First deviation.** The contacts of the corner's analytic piece (a
  cylinder between the two planar sides) reach both creases at one
  section. Before 0980 OCCT took the walls for the sides continued (their
  normals within a tenth of a radian) and rolled the ball on down the
  block's corners, rounding them with 0.5 where the file keeps 0.4: the
  material taken 1.127 against the file's 0.932 mm³, the corner faces 0.04
  mm off, which the import's check accepted as corner faces (round 17).
  With 0980 the ball turned about both creases' edges at once and the
  fillet failed, on all four corners.
- **Fix (port patch 1020).** The walls are faces at the end as port patch
  0750 has them (a contact's kink into a face at the spine's end, which
  cuts the blend), but they hold no vertex of the spine: they go on
  smoothly into the corner's cylinder, which holds the corner edge's end
  vertex. Such a face now counts as a face at the end too, where a walk
  ends on the crease and, for a corner edge running from the block up,
  where the walk after the analytic piece starts there. On the replay's
  body before the item: valid, the material taken 0.931662 against the
  file's 0.932227 mm³, every face of the result within 1.4e-4 mm of the
  file's body after the item and the file's within 8.2e-5 mm of the
  result. In the import: parametric and verified, its faces on the file's
  within the tolerance (no corner faces off). A face beyond the crease that
  holds the end vertex itself stays the walk's where a piece starts there:
  0980's fin top (`5ef2dbd5` item 87, its stored body) failed at its corner
  when it was taken for a face at the end. OCCT-only test
  `geometry.fillet_crease_into_end_walls` (the peg on its plate and block,
  drafts of 3, 4 and 5 degrees, the corner edges either way along the
  corners: valid, the block's corner cylinders from its foot up past the
  creases, the material taken within 1e-3 mm³ of the peg rounded alone
  with the block carried on up into it; with 0980 and without the patch
  the fillet fails, before 0980 it rounds the block's corners).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `52ba7a15` 37 Fillet6 | 1 / 1 parametric, verified, edges read by their names in the file (1020) | |
| `5ef2dbd5` 87 Fillet27 (0980's case) | 1 / 1 parametric, verified, as before | |
| `52ba7a15` 34 Mirror1 | 0 / 1, as before | its result is not in the file's history |

The designs of mitcad#343, each imported alone with its reference folder,
TKFillet built from the port with 1020 (parametric / fallback / skipped of
all items; before: main f1cae0d2):

| Design | Before | After | Final bodies |
|---|---|---|---|
| `52ba7a15` | 96 / 2 / 12 of 110 | 97 / 1 / 12: item 37 parametric and verified, every other item as before | as before (2.2e-5) |
| `5ef2dbd5` | 90 / 1 / 7 of 98 | same, item by item (reference `99ef321f` 18 / 4 of 24 as before) | as before (9.8e-6) |
| `a9e82a9a`, `de7b8b9d` | 190 / 2 / 7 of 199 | same, item by item | as before (6.0e-7) |
| `db73863f` | 135 / 2 / 8 of 145 | same, item by item | as before (4.5e-4) |
| `6418ebb1`, `920a1cef` | 43 / 5 / 1 of 50 | same, item by item | as before (4.1e-6) |
| `61526c66` | 14 / 0 / 0 of 14 | same | as before (1.6e-5) |
| `4d3c4d1b` | 33 / 2 / 0 of 35 | same, item by item | as before (3.8e-7) |

## Chamfers left after mitcad#240 (mitcad#263)

The import chamfers item 4 of mitcad#263 left after port patches 0380 and
0381, imported again by content id with the dressup dump
(`MITCAD_IMPORT_DRESSUPS`, `MITCAD_IMPORT_DRESSUPS_ITEMS` for the items
before them), each failing definition run through OCCT's chamfer alone on
the replay's body and on the history's stored body before the item (its
edges found there by their geometry).

**A post's foot chamfered beyond the plate's outline** (`db73863f` item
122, Chamfer1). A concave chamfer of 6.688 round the foot of a post of
radius 10 on a plate whose outline (arcs of radius 16 and notches) lies
inside the cone's contact circle (radius 16.688) but for the notches'
corners, which it passes exactly. The replay's body is the stored state
(the item before falls back), and OCCT fails the same way on both. The
import's retry on the foot circle merged from its two arcs got as far as
the analytic cone cut by the outline's faces (port patches 0055, 0071,
0142), which grow up to it; two things failed there, both fixed in OCCT:

- the intersection of the cone with two of the notches' side planes kept
  only a piece of no length of the hyperbola (port patch 0480,
  `GeomInt_LineConstructor`): the planes are parallel to the cone's axis
  and square to its seam's direction, which the cone's seam missed by
  8.6e-9 rad, so the hyperbola crossed the seam twice far away and the
  parts outside those crossings were dropped;
- the piece over the notch holding the post's seam, the spine's start, ran
  across that start, and the topological build kept the post's cylinder
  whole (port patch 0481 splits the piece there).

Now parametric: its change of volume 1701.2022 against the file's
1701.1995 (1.6 · 10⁻⁶), every face as in the stored body. OCCT-only test:
the post beyond a square plate in `geometry/tests/test_chamfer_overflow.cpp`.
The design imported again: features parametric 120 → 121 of 145, inputs
read 69 → 70, the final bodies within 4.3 · 10⁻⁸ of the file's as before;
`5ef2dbd5` and `52ba7a15` unchanged (77 of 98, 92 of 110). (A first form
of 0481 split every piece over the start of a closed spine; the torus of
`db73863f` item 106, a rounding over such a gap, then had its seam moved
and the rounding of item 107 along its edges failed: only a piece whose
line crosses its face's seam there is split now.)

**A fin's top edges chamfered wider than its end face** (`5ef2dbd5` item
86, Chamfer2). A chamfer of 0.263 at 45 degrees on both top chains of a
fin 1 mm thick whose end faces are strips 0.24 mm wide on average (between
the top edges and the contact of the root's chamfer, item 85, a concave
cone round the base's torus): the chamfer's contact on the end face lies
beyond it, on the root's cone, along most of each chain. The replay's
body is the stored state within 2.7 · 10⁻⁵ (the root's chamfer is a
B-spline where the file has the cone, 0.006 mm² apart); OCCT fails the
same way on the stored body: the walk reaches the strip's far edge and
would roll on it, which chamfers do not implement ("PerformSurf Not
Implemented"). The bodies stored in the file have no chamfer face along
the profile's extrusion curves: there the root's cone is extended up to
the fin's top face; along the two arcs, where the strip is wider than the
chamfer, the chamfer is a cone and strips of the end face 0.05 mm wide
remain. Walking the chamfer on with the end face's surface unbounded (as
patch 0160 does for roundings over a rim) builds the blend, but the root's
cone does not cut it between its contacts (the contact on the extended
plane lies inside the material under the cone), so no existing overflow
path gave the file's result. Two more things kept it from the file's: the
walked stretches' guide was deformed up to 0.11 mm off its edges by the
tangents of the cones beside it (the chamfer 0.18 wide mid-stretch), and
at the vertex where the chain stops at a turn the walk past it made a
patch the file does not have.

Fixed in OCCT (port patches 0530, 0531): a chamfer's guide takes those
tangents only near its ends (0530); a chamfer whose contact leaves a
narrow face is computed again with that face unbounded and cut by the
faces beyond it, which grow down to the chamfer's other face, and a chain
stopped at a turn (now also for chamfers, `StopChainsAtTurns`) ends in the
blend cut by the face beyond the crease (0531). On the stored state the
result has the file's 124 faces, their areas within 0.003 mm² in all.
OCCT-only test: the fin's tip chamfered wider than its side strip in
`geometry/tests/test_chamfer_overflow.cpp` (four forms, the volume from
the sections within 1e-4). Now parametric and verified (its faces on the
file's within 0.01 mm). The design: 77 → 78 of 98 imported without its
reference copies; with them, item 85 (Chamfer1) falls back as on main
(its measured face resolves differently there, with the same input), so
77 of 98 with item 86 parametric.

**A chamfer of a fillet's corner** (`52ba7a15` item 61, Chamfer5). Not a
chamfer case: the import's only definition is its guess against the
history ("edges found against ASM history state 36"), and the body
deviates from the file's before it. The first deviation is item 38 (Fillet7, on the stored
state: item 37 falls back), verified within the volume tolerance but with
OCCT's corner patches of 0.900 mm² where the file's are 0.666 mm² (the
roundings beside them 15.21 against 15.46 mm²); item 60 (Fillet9) then
builds corner patches the file does not have, and the guessed edges (0.17
mm, between those patches and the rounding) do not exist in the file.
Not fixed here (the fillet corners' shapes). (Round 11 of mitcad#343
found the item failed in the file instead, its definition failing on the
file's exact state too: *A chamfer that names the rounding's operation
before it*, below.)

| Item (content id) | Achieved / target | Cause of the gap |
|---|---|---|
| `db73863f` 122 Chamfer1 | 1 / 1 parametric, verified | |
| `5ef2dbd5` 86 Chamfer2 | 1 / 1 parametric, verified (0530, 0531) | |
| `52ba7a15` 61 Chamfer5 | 0 / 1 | upstream: the corner patches of item 38 and item 60 differ from the file's; the edges are a guess (since round 11 of mitcad#343: failed in the file, skipped) |

## Results that deviate from the file's, kept at import (mitcad#379)

An item whose decoded definition builds a result that is not the file's
takes the file's bodies, as before (*Matching rules*, [*Fallback*](f3d-fallbacks.md#fallback)). That
result is not thrown away unmeasured any more (`core/import-replay/src/deviations.rs`):

- While the item's definitions are tried, the first decoded definition
  (not a guess) that builds a result the history does not take is
  recorded (`Importer::record_deviation`), nothing more: most items then
  find their state with a later definition. Where the kernel failed the
  decoded rounding or chamfer, a definition that builds with some of its
  edges only (the same size, a guess of the history's) is recorded the
  same way, with the edges it leaves out (`missing_edges`).
- Where the item takes the file's bodies in the end, the recorded
  definition is built again on the bodies before it and measured against
  the file's states (`Importer::deviation_now`): the volume it adds or
  removes against the file's change for it, the largest distance of its
  new faces from the state after it and the share of their area farther
  than the geometric tolerance (the file's new faces against its result
  the same), each side against the bodies the other side changed, at up
  to 8 points a face and 600 a side (1–2 s on the large bodies of
  `db73863f`; measuring every face of every body took up to 175 s), and
  the volumes of the bodies it changed against the stored ones. The
  item's report entry gets `deviation` with those numbers, and the end of
  the import gives the faces of the design where it deviates (those of
  the stored geometry standing in for it on which the file's faces off the
  result lie) for the application to select.
- With `Options::accept_deviations` naming the item (the application's
  dialog *Keep Deviating Results?*, `mitcad-cli import-f3d
  --accept-deviations`), the definition is added again and kept,
  parametric and `verified` false, with `accepted_deviation` and a note.
  Each body it changed carries an `Offset`: what it changed of the body
  against what the file's operation changed of it (volume, area, first
  moment of volume and face count; a difference the body carried before,
  an earlier item's approximation, stays as the replay has it). Every
  comparison with the history takes the body's measures with that offset
  taken off (`Importer::current_bodies`, and the measures of definitions
  evaluated ahead), so an item after it is checked by its own change; its
  new faces are measured against the file's state as any item's. A later
  item that does not give its state there takes the file's bodies, and the
  base feature that brings them replaces the deviated body (its offset
  goes with it; undone with the document's undo). The final comparison
  does not replace a body that carries an offset.

Tests: `src/tests/deviation_tests.rs` (mock kernel: listed by default;
kept, the item after it replays on it and the final body keeps the
deviation; a later item whose change is not the file's brings the file's
body back); `cli.import_f3d_deviation*` on a synthetic design
(`test_exchange --write-f3d-deviation`: a cube whose ASM history bevels an
edge, `mitcad_f3d::testdata::bevelled_cube_history_blob`, with its
timeline as a dump whose rounding of that edge removes 8.58 mm³ where the
file's bevel removes 20; an external dump whose items carry the file's
result numbers names the history's operations as the file's own design
does); `tools/ui-import-deviations-test.sh`.

On `db73863f` (*An assembly's remaining fallbacks* below) kept with
item 88: the rounding stays parametric with -246.51 mm³ against the
file's -225.30 (+9.4 %), its new faces up to 0.121 mm off (4.1 % of their
area beyond 0.01 mm), and the items after it replay on it.

## An assembly's remaining fallbacks (mitcad#379)

`db73863f` (145 items in 7 components: a housing, a plug screwed into
it, a nut and a tool; `f3d.corpus_designs`) imported by itself with the
trace, its final bodies the file's exactly before and after. Each item
that did not come in parametric, traced to its first deviation:

- **Thread1 (item 96), now parametric.** The modelled thread's angle is
  measured where the history's states show its groove (`thread_angle`,
  the file's axis pointing down). The states hold every component's
  bodies, and the housing's internal thread lies round the plug's in the
  same place: their union filled the groove (19 % of a pitch empty at the
  pitch radius instead of half), no angle was found, and the thread built
  at angle 0 came out turned against the file's (the same volume, its
  centre 0.08 mm off). Only the bodies of the thread's own component are
  looked at now: the angle is found (half of each pitch empty at all four
  angles) and the thread gives its state.
- **Fillet7 (item 109), now a fallback.** The rounding after that thread,
  of the plug's end edge, continues along the thread's helical edge in
  the file. On the stored thread (the fallback before) its one edge gave
  the state; on the modelled thread the edge's tangent chain takes the
  helical edge, and the rounding along it stops where it rolls on an edge
  of a neighbouring face (the edge overflow, mitcad#121); with its one
  edge alone it changes 35.53 mm³ against the file's 37.11, a corner face
  0.054 mm off. A candidate to keep (*Results that deviate*).
- **Extrude3 (item 93).** Its profile record names one curve the sketch
  no longer has (the sketch was edited after the item: its only curve
  now is the projected circle of radius 30 round it; the file's cut, a
  circle of radius 27.5 judging by the ring it leaves, is not in the
  file). No region of the sketch is its profile; the note says so now
  (`features::profile_curves_gone`). Not to be built from the history's
  faces (a substitute). No record of the file keeps the curve
  (mitcad#389): the profile names it by its ids (`crv_primary_id` 136),
  and of the design stream's objects that carry such ids (every one looked
  at, also those no sketch lists, `mitcad-f3d-inspect attrs <file>
  crv_primary_id`) none is of that sketch: the two with the same id are
  curves of two other sketches, and the sketch's own container lists only
  the projected circle. The item stays skipped with its note; nothing is
  left to recover.
- **Revolve4 (item 132).** The triangle revolved about Axis1 lies where
  the file's sketch puts it (the axis through the sketch's origin, the
  sketch's frame the file's), and the file's cut removes 151.90 mm³ from
  the housing round its threaded bore, changing five faces: a cone of
  158 mm², two B-spline faces of 1190 mm² (the bore's thread flanks), a
  cylinder of 486 mm² and a plane of 1439 mm². On the history's stored
  body before the item, OCCT's section of the revolved triangle (768.66
  mm³) with the body finds those five faces (four edges, 362.6 mm), yet
  its cut changes the body by -0.05 mm³ and its common is the whole
  triangle (768.66 mm³), with or without a fuzzy value up to 1e-3; points
  of the triangle classify mostly outside the body. The boolean keeps the
  wrong sides of the faces it split (the thread's B-spline flanks among
  them): a kernel fault, on the replay's body ("does not cut into any
  participant body") as on the stored one (`--retry-bodies replace`). What
  is needed: OCCT's classification of the split faces fixed for these
  flanks. **Now parametric (mitcad#389):** the flanks were not the cause.
  The body's four cylinders round the bore (its thread's crest among
  them, one face running 3.4 turns) are stored as cones of a half angle
  of 1e-13 rad, their apexes 1e14 mm away; OCCT's intersection of the
  revolved triangle's cone with such a face found no curve (with the
  face as the cylinder it is, the circle), so the cut's section missed
  the crest and the split faces could not be classified. Cones whose half
  angle is zero within the angular precision (1e-12 rad) are now read as
  cylinders (`convert.rs` `cone_half_angle`, `brep_import.cpp`): on the
  stored body the cut then removes 151.913 mm³ (the file 151.904) and the
  item builds its state.
- **Fillet1 (item 120).** A rounding of radius 10 of a circle's concave
  edge (a boss of radius 10 on a face 20 wide each side) adds 852.7 mm³ in
  the file; OCCT's rounding of the same edges (the replay's body is the
  state's within 3.1e-5) removes 1924.6: it rolls over the edge's faces
  onto the walls beyond (7 faces changed, the file's 3; its new faces up
  to 2.06 mm off). The edge overflow (mitcad#121).
- **Plane1 (item 11), partial.** Offset 5 mm from a face of another
  component's body (an O-ring of the housing's component), in the nut's
  component, just after a captured position moved the nut 10 mm. During
  the replay the offset plane comes out at 53.43 (the face taken where the
  nut is at its stored transform), the file's at 43.43, while the
  occurrences at that point are where the file has them (the nut 10 mm
  up, the trace lists them); the same definition added to the imported
  design at that point of the timeline (opened again, marker after the
  captured position) gives 43.43, and so does it on one thread. So the
  link's move is lost when the import adds the plane, not in the inputs;
  where in the model's evaluation is not found. **Now parametric (mitcad#389):**
  the import put the plane into the component of the face it is offset
  from (`def_component` counted the linked body as the plane's own), where
  its link's target (the nut's occurrence) does not lead: the link fell
  back to the face's own path and moved the face by nothing. A
  construction plane's linked bodies no longer count, as an extrusion's
  objects and a sweep's bodies do not.
- **Sketch14 (item 140), partial.** Its four linked curves (two lines,
  two fixed splines) are where the sketch's plane cuts the body, not
  projections of edges (their ends 3.8 and 4.7 mm from any edge of the
  replay): kept as fixed reference geometry; one coincidence the file's
  own positions keep only within 7.2 µm is left out. **Now projections
  (mitcad#389):** they are not section curves but the boundary of the
  thread's root face projected onto the plane through the thread's axis:
  two lines, its end edges seen edge on, and two splines, its helices of
  5 turns, which the replay's thread splits into an edge per turn. A
  linked curve now becomes the projection of an edge of any kind (a
  B-spline seen edge on projects to a line) or of a chain of edges end to
  end (`linked.rs` `chains`; the model's `Projection::chain`, the pieces
  joined into one B-spline), where every point of the pieces' projection
  lies within the geometric tolerance of the curve (here 6e-9 mm for the
  lines, 8.1 µm for the splines) and the sketch's regions stay those of
  the file (with the points the file holds on a curve on it). The
  coincidence of the profile's corner with a spline is still left out:
  solved with it the corner moves 1.3 µm, beyond the sketch's tolerance
  of 0.1 µm; the item stays partial for it. **Now parametric (mitcad#396):**
  the corner lies on the replay's spline within 4e-8 mm; the solver's start
  on the spline moved it (*A sketch solved within the geometric tolerance*
  below).
- **Sweep1 (item 9):** unchanged, the known sweep over itself (*Sweeps*).
- **Fillets** (items 27, 74, 75, 88, 121, 133, 137, 142, 143): see the
  table. Imported with `--retry-bodies replace`, each was tried again on
  the history's stored bodies before it and failed there the same way:
  not noise of the items before, the kernel's. Item 137's three edges the
  file's references resolve are exactly the three edges its state lost
  (lengths 5.32, 5.32 and 5.10; its fourth reference is lost in the
  file): they are the file's edges, and OCCT fails on them.

| Item | Class | Cause |
|---|---|---|
| 27 Fillet4 | rounding end (mitcad#343, mitcad#236) | the roundings along the thread's edges end where the rounding's end does not meet the face at the end |
| 74 Fillet1 | corner (mitcad#343) | three roundings meeting at a corner of the housing: invalid (unorientable), each alone fails |
| 75 Fillet2 | rounding end (mitcad#343) | an edge at the vertex has no curve on a face the corner takes it on; the faces are the stored ones of item 74's fallback |
| 88 Fillet6 | corners (mitcad#343) | the rounding is the file's but at three corners: three of the file's new faces of 234.8 mm² each lie up to 0.063 mm off (5.8 % of the new area, more than the 5 % corner patches may hold), five of OCCT's up to 0.123 mm; the change 8.6 % off. A candidate to keep |
| 121 Fillet2 | edge overflow (mitcad#121) | the chain of ten edges stops where it rolls on an edge of a neighbouring face |
| 133 Fillet7 | rounding end / corner (mitcad#343) | the walk round the vertex at the end does not come back to the rounding; with both edges the corner finds two faces with no common edge; alone, invalid shapes |
| 137 Fillet5 | rounding end (mitcad#343) | the corner's computation gives no result; the rounding along the concave edge fails |
| 142 Fillet5 | rounding on a thread's edge (mitcad#343) | invalid (unorientable) along the edges of a cut face with the thread's face |
| 143 Fillet6 | rounding end (mitcad#343, mitcad#236) | the rounding's surface and the face at the end do not intersect; the faces are the stored ones of item 142's fallback |

Imported again by content id (items parametric / partial / fallback /
skipped; final bodies 6 / 6 the file's exactly, before and after):

| | Before | After |
|---|---|---|
| default | 122 / 2 / 14 / 7 | 122 / 2 / 14 / 7 (Thread1 parametric, Fillet7 109 a fallback) |
| `--accept-deviations all` | | 125 / 2 / 11 / 7 (items 88, 109, 120 kept; the bodies after them the file's again where a later fallback brought them) |
| default, mitcad#389 | 122 / 2 / 14 / 7 | 124 / 1 / 13 / 7 (Plane1 and Revolve4 parametric; Sketch14's 4 linked curves projections, the design's linked curves as projections 84 → 96 of 197) |
| default, mitcad#396 | 124 / 1 / 13 / 7 | 125 / 0 / 13 / 7 (Sketch14 parametric; final bodies 6 / 6 the file's, modelling inputs 69 read / 14 inferred, unchanged) |

## A sketch solved within the geometric tolerance (mitcad#396)

`db73863f`'s Sketch14 (item 140) left out the coincidence of a profile's
corner with a linked spline: solved with it, the corner moved 1.3 µm,
beyond the sketch's tolerance of 1e-4 mm. Traced:

- **The file's spline is an approximation.** Its two linked splines are
  the thread root's helices (radius 20.66495 mm, 5 turns) projected onto
  the plane through the thread's axis; the file stores each as 50 cubic
  pieces (151 control points, interior knots of multiplicity 3) whose
  extremes reach 20.6563 mm instead of the radius: up to 8.1 µm off the
  exact curve. The file's corner lies on the exact curve: 7.2 µm off the
  stored spline (what mitcad#389's first look saw as "kept only within
  7.2 µm").
- **The replay's curve is exact.** The replay's six root edges keep their
  radius within 1.3e-6 mm, the chain joins them exactly
  (`join_curves`), and the file's corner lies 4e-8 mm off the joined
  projection. Neither the knots nor the join moved it.
- **The solver's start moved it.** A point on a spline is solved with the
  spline's parameter as an unknown, started at the best of a uniform
  sampling (32 samples per control point; on the joined spline of 105
  control points over 5 turns, a step of about 0.1 mm along the curve).
  The solve shared the gap between the parameter and the point, and the
  corner slid 1.3 µm along the curve. The start is now refined between the
  samples next to the best one (golden section, `mitcad-solver`
  `System::spline_param`): the corner stays within 1e-7 mm, and the
  sketch comes in parametric with every constraint.

The sketch's acceptance follows the import's geometric tolerance
(mitcad#224) too: a solve that moves the file's points by more than
1e-4 mm but within the tolerance (0.01 mm by default, `--geometric-tolerance`)
is the file's sketch where the regions the solved sketch bounds are the
file's (`linked::regions_kept`, the regions with the solved positions
against those with the file's, the points held on a curve put on it);
its note says how far the points moved ("solved within the geometric
tolerance: points up to … mm from the file's"), and the item's
`geometric_deviation` records it above 1e-3 mm. A corner 7 µm off a
stored approximate spline keeps its coincidence that way; a solve that
would split a region (a line 2 µm short of an edge, lengthened across it
by its dimension) keeps the file's positions with the constraint left
out or the dimension driven, as before. Tests:
`a_point_on_a_long_spline_stays_put` (solver),
`a_corner_micrometres_off_a_linked_spline_keeps_its_coincidence`,
`a_solve_within_the_tolerance_that_changes_the_regions_drives_the_dimension`.


## A drum of a 2026 writer, item by item (mitcad#719)

A drum of six components (68 items, design `3282222f`) written by a
2026 writer (2706) came in with 17 of its items parametric: 37 fell back,
among them every hole ("no diameter") and the first extrusion of every
component ("no bodies to extrude through"). The causes, in the order they
showed:

- **Parameters of class version 10.** None of the design's 263
  parameters decoded: the flags after the expression take 11 bytes in
  class version 10 (10 in versions 8 and 9), the user flag still the
  last, and the `u32` before the comment is 5. Without parameters no
  feature had its sizes (`decode::parse_parameter`).
- **A tail that lists its own item.** A hole whose sub-items are the hole
  itself ends its tail with `u32 1 | ref self | 00 00 00 | ref health`;
  read 20 bytes later the zeros look like a tail with an empty base name,
  the index 256 and no sub-items, so three holes lost their result
  numbers and their states. The `u32` after the index is zero on every
  item; a candidate with another value is no tail. The same false match
  took the tails of derived contexts and of cut-paste items in other
  designs (their names, result numbers and sub-items decode now).
- **A tapped hole's modelled thread is its last operation.** The thread
  sub-item has a result number of its own, the hole's plus one, whose
  state cuts the thread into the bore (`_f3d.last_result_no`, modelled
  threads only). The hole with its thread is checked against that state;
  before, it was checked against the bore alone (14 % off) and the next
  hole took the thread's state.
- **Patterns of tapped holes repeat their threads.** A thread leaves no
  tool, so a pattern of the holes copied bores without threads (6 % less
  volume removed). Threads are repeated as fillets and chamfers are, on
  the copies of their faces (`dressup_copies.rs`; a mirror's image turns
  the other way), and the import lists a tapped hole's thread with the
  hole in a pattern's features.
- **Circular patterns about an arc edge.** MitCad's axis of a circular
  edge (and an origin axis) points the way of its largest coordinate; the
  file turns about its stored axis line, here (0, -1, 0). The angle is
  negated where they disagree (`Importer::circular_angle`), so the copies
  go the file's way round instead of off the segment. A rectangular
  pattern along an edge or an axis of its own negates its distance alike.
- **A pattern of faces whose selection lost a neighbour.** The faces of
  two cut-outs were selected before a corner of the sketch was rounded:
  two walls the recipes name as neighbours have the rounding between them
  now. The selection with the face that meets both (and no other
  selected face) is tried first, as the file's result has it on every
  copy, then the selection as it is (`ops::bridging_faces`; its inputs
  count as inferred). The tool of faces of separate cut-outs is a
  solid of each (`geometry::face_tool`; it was one shell, not connected).
- **Holes at distances.** A hole whose position definition gives two
  edges of its face and the distances comes in at those distances
  (`face_offsets`) first: the point the file keeps with it lay 3.2 mm off
  (the volume the same, so the history took it, and the bodies after it
  were 3.2 mm off). Holes on a shaft's side are measured from the
  shaft's end face (the one slot of role 33 holds that face): the new
  placement `cylinder_offset` (the distance along the plane's normal,
  along the axis for an end square to it), where the kept point lay
  296 mm off. Points on a cylindrical face go across its axis.
- **Cost.** A hole's history guesses come after its decoded definitions
  are tried (with the history and a decoded position; else there are
  none after the first part), and bodies the file's operation left alone
  are not compared for removed material: on the shaft with modelled
  threads that boolean took 13 minutes and more than 6 GB at a hole of
  another component.
- **An item's own operations before its last.** A rectangular pattern of
  holes makes its copies in operations before its last one, which changes
  nothing; it is no item that failed in the file. The operations no item
  names count as the next item's only after an earlier item named one of
  the blob's (the blob's start is no item's).

Now 58 of the 68 items are parametric and one falls back; the others are
the placing of occurrences and one pattern skipped with it. What is left:

- **Tapered pipe threads.** Two holes on the shaft are tapped `1/2 - 14
  NPT` (cosmetic): the file's thread sizes each bore to its taper in two
  operations of its own (75 mm³ a hole), which MitCad has no thread for.
  The holes come in, but the state after them is brought by a base
  feature; the rectangular pattern of the holes is 1.1 % off and skipped,
  and the circular pattern of both falls back.
- **The start of a tapped hole's modelled thread.** MitCad's thread on a
  hole's wall starts at angle 0 (the walls do not exist before the hole);
  the file's starts elsewhere, 0.42 mm apart, and the pattern's copies
  take the angle about their own axes. The bodies match the file's in
  volume (1e-7) but not within the geometric tolerance.
- **A row of dimples 0.4 mm off.** The segment's rectangular pattern of
  dimples matches its state within 2.4e-4, its body 0.4 mm off the file's
  after it (cause not traced yet).

### The follow-ups (mitcad#733)

Now 60 of the 68 items are parametric, none falls back, and no base
feature stands before a pattern; five of the six final bodies lie on the
file's within the geometric tolerance (the shaft within 1e-4 mm, the
others exactly), from three:

- **Taper pipe threads.** The thread objects of the two `1/2 - 14 NPT`
  holes and of the lid's `Rc 1/2` hole (type `ISO Taper Internal`) store
  the angle 0, the tap drill as both diameters and the pitch diameter 0,
  so the decoder took them for no thread; after the handedness byte come
  five numbers, all 0 on straight threads: the taper's full angle (2
  atan(1/32)), the height h, the tap drill, the length and the pitch
  diameter at the small end. The lid's hole lists no sub-items: its
  thread is the thread object that names it as its hole. The file's
  thread sizes the bore to the tap drill (0.18 mm³) and cuts its taper
  (150 mm³), a cone of 1:16 on the diameter whose minor diameter at the
  hole's start is the small end's pitch diameter grown over the length,
  less h (NPT: E1 − 0.8 P, within 1e-4 mm of ASME B1.20.1; the `Rc 1/2`
  hole's 0.22 mm below ISO 7-1's gauge diameter less 2 h, so the import
  takes the file's numbers as the thread's `diameters`). MitCad has taper
  pipe threads now (`npt`, `iso_taper` with `taper`, which the import sets;
  threads saved before stay straight), cosmetic ones tapering the bore,
  and the thread's operations end the hole's state, as a modelled one's
  do.
- **A pattern whose copies have no taper.** The shaft's rectangular
  pattern of the two tapped holes keeps its copies as plain bores (sized,
  not tapered), while the circular pattern of the holes and that pattern
  tapers its copies of the holes. The pattern's objects are tried with
  the holes' threads, then (inferred) without them.
- **The angle of a tapped hole's modelled thread.** It is measured in the
  thread's state as a thread item's (the hole is made on its own first to
  read its walls): the two M10 threads stand at 60 degrees. A pattern's
  copy of a modelled thread is the original moved there: its angle is
  where the moved helix lies on the copy's face, not the original's angle
  about the copy's own axis (`thread::copied_angle`).
- **The segment's dimples, left.** The first deviation is the segment's
  rectangular pattern (item 51) of the dimple, its drill dent and their
  circular pattern (8 rows over SegmentAngle × 7/8 = 26.25 degrees,
  identical compute): the file's state after it has the original column
  at the circular pattern's steps of 3.75 degrees, but every copied
  column at steps of 3.757143 degrees, 26.3/7 (the dimples' counterbore
  axes in the stored body: 166.875, 170.63214, 174.38929 … degrees). The
  copies lie up to 0.40 mm (the eighth row) off the replay's, which
  repeats the circular pattern's own transforms. 26.3 is the total angle
  rounded to a tenth of a degree, the angle's display precision; the
  file stores no 26.3 (neither the parameters nor the design stream).
  Repeating a nested pattern with a rounded angle is not done: it is a
  quirk of one file's history, so the body after the pattern stays 0.40
  mm off (its volume within 1e-5).

## Corners where a rounding ends after an edge overflow (ninth round of mitcad#236)

The designs whose roundings the census of 11.10.2026 left failing at a
vertex, imported again one by one with the dressup dump (18 designs by
content id), still fail at a rounding's end in five items; each failing
fillet was replayed from the dump with OCCT's fillet alone on the
replayed body before it (for item 23 of `f5e08b18` the history's stored
body lies within 5e-12 of it).

| Item | Kind at the end | Now |
|---|---|---|
| `f5e08b18` 23 (Fillet14, 0.5 along a step's foot) | the contact on the step leaves it into the bar's rounding, which ends the blend 0.5 past the vertex on its edge's line with the floor | parametric and verified (port patch 1140) |
| `f9d6d78a` 24 (Fillet4, 1 along a top outline) | the outline's corner arc runs past a wall rising from the top: the arc's piece lies wholly past the wall and is dropped, and the corner caps the next piece's prolongation with the wall, which it does not meet | falls back (`the rounding's end does not meet the face at the end`) |
| `99ef321f` 22 (Fillet32) | OnSame end on a bore's rounded rim: the contact does not reach the edge at the vertex | falls back, not traced further |
| `49ccc315`, `e41d24ac` 35 (Fillet9) | corners of two roundings and four sharp edges: two faces met round the vertex share no edge | falls back, not traced further |
| `b596234a`, `9c3751cd` 63 (Fillet10) | the corner joining two roundings: two faces met round the vertex share no edge | falls back, not traced further |

**`f5e08b18` item 23.** A block beside a bar reaches 0.073 mm further
out along the floor than the bar; the bar's floor edge is rounded with 3
and the block's with 8, so the block's face beside the bar is a step
between the two profiles that narrows to a point where the bar's rounding
comes tangent to the floor. The item rounds the step's foot with 0.5.
Near that point the step is lower than the ball: its contact leaves the
step across its edge with the bar's rounding (the edge overflow), and the
bar's rounding cuts the blend from there to the end; the cut's end, where
the floor contact meets the bar's rounding, lies on the line of the
floor's edge with the bar's rounding 0.5 past its end. OCCT's analytic
cut to the end looked for that point on the edge only; the corner then
looked for the floor's edge with the block's rounding to cross the bar's
rounding, which passes 0.9 µm above it, and failed. With port patch 1140
the end is taken on the edge's line continued (the two faces are tangent
along it), the bar's rounding grows up to the cut and the edge is
prolonged to the end, as the file has it: the result's faces lie on the
file's (the boolean differences with the stored body are empty both
ways), its change of volume is 0.04 % off the file's, and the bar's
rounding grows by the file's 0.41 mm². The same chain's other three ends,
built before by the corner, now come from the same cut (one of them, whose
cap the corner had cut along the wrong arc of the intersection, is right
only now).

**`f9d6d78a` item 24 (left).** It runs on the stored bodies of item 22's
fallback. The rounding of 1 along the top's outline ends at the vertex
where the outline's corner cylinder (radius 2) meets a wall rising from
the top at a sharp edge. The blend's contact on the top already lies on
the wall's foot at the start of the outline's straight edge, so the
analytic piece along the corner arc (a torus) has its top side wholly past
the wall; it is given up, and the corner caps the straight piece's
prolongation with the wall's plane, which that prolongation does not
meet. The file's result keeps the torus along the arc (0.49 mm²) and a
sliver of the cylinder (0.064 mm²), cut by the wall's plane grown by
0.23 mm². What it needs: the arc's piece cut by the wall (a cut to the end
whose face beyond is the wall, met by the top contact before the piece
starts).

The 18 designs, imported again with the patch, item by item against the
import before it: only `f5e08b18` item 23 changes (fallback to parametric:
26 of 28 items parametric, 2 skipped; the final bodies within 5e-6 of the
file's by volume as before); every other item of the 18 keeps its
outcome.

## A rounding round a corner cut by a wall across it; the corners left (tenth round of mitcad#236)

The items the ninth round left, each replayed from the dressup dump with
OCCT's fillet alone on the replayed body before it.

| Item | Kind at the end | Now |
|---|---|---|
| `f9d6d78a` 24 (Fillet4, 1 along a top outline) | the outline's corner arc runs past a wall standing on the top across the corner's rounding | parametric and verified (port patch 1170) |
| `99ef321f` 22 (Fillet32, 0.5) | the chain turns at a convex kink between two cylinders just before its last edge, 0.84 long, which ends where an earlier rounding of a bore's rim crosses the cylinder: no piece on that edge, and the OnSame corner finds no point of the contact on an arc ("no point on arc"). The file's result pivots the ball about the kink (a torus of radii 0.5 and 0.5) and ends against the rim's rounding | falls back |
| `49ccc315`, `e41d24ac` 35 (Fillet9, 2.5 round a rim of eight arcs) | at each of the eight vertices between the arcs a radial hole of radius 0.75 grazes the top face (its crest at the top's height): the walk round the vertex meets two faces with no common edge ("Failed to find edge") | falls back |
| `b596234a`, `9c3751cd` 63 (Fillet10, 3, concave) | two struts' cylinders of radius 19.5 meet a facet of 5.4 mm² at the joint, narrower than the radius; the corners of the two roundings at the facet's ends: two faces round the vertex with no common edge | falls back |

**`f9d6d78a` item 24.** It runs on the stored bodies of item 22's
fallback. A rounding of 1 along the top's outline, round the outline's
corner (rounded with 2) up to the vertex where a wall standing on the top
crosses the corner's rounding at a sharp vertical edge. The contact on the
top runs onto the wall's foot already on the straight edge before the
corner: the analytic piece round the corner lies wholly past the wall and
the straight piece's start is an extension with its top side given up.
The cut to the end (`ChFi3d_KPartCutToEnd`) took that extension for its
first piece and gave up; the corner capped the straight piece's
prolongation with the wall, which it does not meet. With port patch 1170
the given-up pieces make way for the cut ones, and the end past the vertex
(1140) needs only the wall and the corner's cylinder to go on meeting
along their edge's line: the wall grows down to the cut, 1 below the top,
as in the file. OCCT alone on the replayed body: the boolean differences
with the stored body are empty both ways, the change of volume 0.013 % off
the file's (the torus 0.4888 against the file's 0.4886 mm², the wall
grown by the file's 0.23 mm²).

The 18 designs of the ninth round, imported again with 1170 item by item
against the import before both rounds: `f5e08b18` item 23 (ninth round)
and `f9d6d78a` item 24 change, from fallback to parametric (`f9d6d78a`
69 of 78 items parametric, 5 fallbacks, 4 skipped; the bodies its items
build after it within 7e-8 of the file's by volume); every other item of
the 18 keeps its outcome.

## The corners left after the tenth round (eleventh round of mitcad#236)

The three items the tenth round left, replayed again from the dressup dump
with OCCT's fillet alone on the replayed body before each, and compared
with the stored body after it near the failing vertices. None is fixed:
each needs a corner construction of its own, larger than a guarded change
to an existing one.

- **`99ef321f` item 22 (Fillet32, 0.5 along a wall's outline).** A bore
  of radius 3 through the wall runs 0.02 mm into a round bar of radius 5
  standing on the wall (the axes 7.98 apart against the radii's 8), and
  the bore's rim on the wall is rounded with 0.3: the bar's top carries a
  sliver of the bore's surface and of that rim rounding about 0.84 wide.
  The chain's last edge, the wall's edge with the bar across that sliver,
  is its own stripe; at its end both of its contacts lie inside their
  faces (the bar's contact 0.5 off the wall passes beside the rim
  rounding, the wall's contact 0.43 outside it), so the OnSame corner
  finds no point on an arc ("Corner OnSame : no point on arc"). The
  file's result pivots the ball about the sliver's edge line (a torus of
  radii 0.5 and 0.5 about it) and trims the rim rounding slightly.
- **`49ccc315`, `e41d24ac` item 35 (Fillet9, 2.5 round a rim of eight
  arcs).** At each of the eight vertices between the arcs a vertical slot
  1.5 wide runs down through the wall, its top end rounded with 0.75 so
  that its crest touches the top face along a line at the rim: the top
  face and the slot's end cylinder meet only along that tangent line. The
  walk round each vertex meets two faces with no common edge ("Failed to
  find edge"). The rounding must run over the slot's mouth, opened by the
  rounding itself.
- **`b596234a`, `9c3751cd` item 63 (Fillet10, 3, concave).** At each of
  two nodes of a frame of tubes (outer radius 19.5) three struts meet a
  triangular facet of 5.4 mm², narrower than the radius; the item rounds
  the facet's three edges. The two roundings at each facet corner, with
  the struts' sharp edge between them, fail ("Failed to find edge"). The
  file's result replaces the facet by three blend faces between the
  struts (B-splines of about 239, 239 and 164 mm²) that run on along the
  struts' intersections: a node blend of three tubes, not a corner of
  two roundings.

## The pivot corner over the sliver of `99ef321f` item 22 (mitcad#1203)

The design's profile, extruded along x from the end face x = -7 to 7, has
a convex arc of radius 5 (the bar, axis at y 15.5, z 10) followed by a
concave arc of radius 3 (axis at y 16.7457, z 17.8808) that meets it not
tangentially but along the line L (y 16, z 14.975), with a turn of about
9 degrees: the concave arc runs 0.02 mm into the bar. The concave arc's
edges with the end faces are rounded with 0.3, which leaves a sliver of
the bar's surface next to the end face, 0.3 long along x and 0.84 wide,
between L's end and the vertex V where that rounding meets the bar. Item
22 rounds the end face's outline with 0.5; the chain's last edge is the
end face's edge with the bar across the sliver, up to V.

What the file's result does there (its stored body after the item):

- The rounding along the bar's end edge (a torus of radii 4.5 and 0.5)
  ends at the section through L.
- From there the ball pivots about L: its centre turns on a circle of
  radius 0.5 about L in the plane x = -6.5, from the bar's normal at L
  through about 84 degrees, touching the end face all along (a horn
  torus of radii 0.5 and 0.5 about L, 0.22 mm², its contact with the end
  face a circle of radius 0.5 about L's foot).
- That torus is cut by the concave cylinder, by the 0.3 rounding and by
  what is left of the sliver (0.027 mm² of the bar), and ends in a point
  on L at x = -6.5; the 0.3 rounding is trimmed by 0.0034 mm², and the
  chain's last edge and most of the sliver are gone.

OCCT's stripe for the last edge reached V with both contacts inside their
faces, and no existing end fitted: the walk stops at L's section, what
follows is an extension of no length, and the corner failed. Patch 1220
(`PerformPivotAboutCreaseAtEnd`, a construction of its own beside 0951's
`PerformPivotAtStepEnd`) builds the file's end: where the end piece's
contact on one face stops inside a line edge of it square to the plane at
the end, at a crease of less than 30 degrees, with the end's vertex past
that line (not its own end, where the faces beyond cut the end as
before), the stripe ends at that
line's section and a corner stripe of the horn torus about it closes the
end, cut by each face it meets on its way to the plane (a section of the
torus with the solid), against the plane along the circle about the
line's foot; at a first end the corner stripe runs the other way. On the
item's dump (both ends of the outline) the result is valid and its
boolean differences with the stored body are empty both ways; the new
faces match the file's (the horn torus 0.220835 against 0.220833 mm², the
sliver's remnant 0.027086 against 0.027066 mm²). In the import of the
design alone item 22 (Fillet32) is parametric and verified, change
difference 0, and so is item 23. Knock-on: the fallback at item 22 used
to bring the file's body, which hid the deviations of items 20 and 21
(Fillet30, change difference 0.107, and Fillet31, both verified within
0.01 mm): the final body now differs from the file's by -0.0021 % in
volume, with the same 150 faces. The items fixed in mitcad#236's rounds
(`f5e08b18` 23, `f9d6d78a` 24, `69ca6c23` 8) import as before, their
reports the same with and without the patch. OCCT-only test: a bar with
a concave arc meeting it with a small overlap, its rim rounded, the end
face's outline rounded, in `geometry/tests/test_fillet_corners.cpp` (no
result without the patch).
## The node blend of `b596234a`/`9c3751cd` item 63 (mitcad#1205)

Item 63 (Fillet10, 3) rounds the five valleys where the tubes of a frame
meet (outer radius 19.5, bores of 17.5, along four edges of tetrahedra
from one point, the frame cut by a side plane): at each of two nodes three
tubes meet, and a rod of radius 2.5 along the node's axis, capped square
to it just past the point where the three tubes' surfaces meet, leaves a
facet of 5.4 mm² between them. The valleys run from the facets' vertices
to the side plane, one from node to node. With the edges found by their
names in the file OCCT fails at the nodes ("the rounding along edge … and
the rounding of another edge intersect each other"; with the facet's
edges, as the ASM history state names them, "Failed to find edge").

What the file's result is: each valley's rounding is the ball of 3
rolling between the two tubes (its contacts on them are plane sections,
ellipses, as for two tubes of one radius whose axes cross), run on past
the facet through the node, and the three roundings at a node keep what
lies outside each other: they meet along their crossings and at one point
of the node's axis, and the facet is buried. The point is where the
rounding's pipe about its ball's centre (an ellipse at 22.5 from both
axes) crosses the axis, 35.2017 from the tubes' crossing along it; the
file's vertex lies at 35.202. No sphere closes the node (a rolling-ball
corner would put one about the point 22.5 from all three axes, 3.77 off
the file's vertex): the ball's centre for the meeting point lies past
where the ball touches the third tube.

`geometry/src/node_dressup.cpp` builds that, MitCad-side, as
`mitred_dressup` does for its corners: where OCCT's fillet fails, a set
of concave edges between convex cylinders of one radius whose axes cross,
each valley ending on a plane or at a vertex of a facet all of whose
vertices end valleys of the set, is rounded valley by valley. Each
valley's rounding is OCCT's own, of the valley edge of the two tubes alone
(as solid rods, cut by the end's plane at a far end and, past a node,
square to the valley three radii beyond the facet's vertex), less the
rods; at a
node it keeps out of the node's other tubes (within a ball about the
facet's vertex: inside a tube it adds nothing, and past the node its bore
may lie there). The roundings are joined to the body together; the
union does the cutting at the nodes. A valley the booleans split into
edges between the same two faces is one rounding.

On item 63's dump the result is valid, with the file's 34 faces, and its
faces lie on the file's (its roundings' areas within 0.01 mm² of the
file's 239.19, 239.18 and 163.65 mm²); its volume is 0.007 mm³ off the
file's. In the imports of the two designs alone item 63 is parametric and
verified, and so are items 64 and 65 after it (item 65 fell back before).
Knock-on: the final body now differs from the file's, 42 faces against 39
and -0.0003 % in volume: item 64 (Fillet14, two edges of radius 1 at the
side plate's hole) splits faces otherwise than the file, as it did on the
file's own body when item 63 fell back (40 faces against 36 then), which
the fallback at item 65 used to hide. Geometry test:
`geometry/tests/test_fillet_tube_node.cpp`, the frame of the design with
two nodes (its roundings on their pipes within 1.2e-6 mm, meeting on the
nodes' axes within 1.6e-6 mm of the computed point; the fillet fails
without the construction).

## The rods' feet of `b596234a`/`9c3751cd` item 64 (mitcad#1207)

After item 63 builds (mitcad#1205), item 64 (Fillet14, 1) is parametric
and verified, change difference 0, but its result has 39 faces against
the file's 36 (and the final body 42 against 39). The item rounds the
feet of the two rods along the nodes' axes (radius 2.5, mirror images in
z = 0) where they stand on the side plate's inner plane; the two feet
meet at two vertices in z = 0, where the rods' own edge between them
(left sharp) ends. Near the first vertex the plate's hole (radius 7.5)
passes 0.83 from it, inside the rounding's radius: both roundings run
over onto the hole's wall there.

The file's result: each foot rounding one face (16.6899 mm²), its contact
leaving the plane onto the hole's wall at z = ±1.055, the two meeting in
a crease in z = 0 from the hole's wall to the rods' edge (a mitre of
mirror images); the hole's faces whole (141.3717 and 141.3916 mm²).

What MitCad builds: OCCT's fillet fails at the first vertex ("BRep_Tool::
no parameters on surface", ChFi3d_IsInFront: the two stripes' common face
there is the hole's cylinder, which does not hold the corner's vertex).
Each stripe has three pieces on three surfaces: one with its contact on
the plane, then two rolling on the hole's rim edge (OCCT's edge overflow,
the ball touching the rim's line) down to z = 0, where it ends at the
rim's vertex, its contact on the rod at z = ±0.035, not on the rods'
edge: PerformTwoCorner finds no pivot there. The file's rounding instead
touches the hole's wall past the rim (its contact there a curve on the
wall, 0.016 below the rim at z = 0), one face with its part on the plane.
The cut-out route of mitcad#311 builds it instead (the holes cleared, the
rounding built on the plane, the holes cut out again): each foot rounding
comes out as two faces on two surfaces of OCCT's corner on the cleared
plane (16.504 and 0.197 mm², together 0.0107 mm² more than the file's
face), and one hole's face in two (136.698 and 4.674 mm², together the
file's 141.372) along that corner's edge in z = 0. So 3 faces more: the
pieces of a different corner, not a split of the file's faces; within
the import's check of 0.01 mm.

Tried, none giving the file's faces: OCCT's corner with the vertex's
parameters on the hole taken from the nearest point (port patch 0143's
mode for chamfers) gives an invalid result (the corner falls through to
the plate filling: no pivot) and changes the input's geometry; each foot
alone fails at the other rod ("OneCorner : bouchon non ecrit"); a foot on
the body cut at z = 0 fails in the data structure's rebuild
(TopOpeBRepDS_DataStructure::Point); joining the pieces the cut-out
route leaves (one surface on both sides of an edge the put-back made) does
not apply, the pieces lying on different surfaces. A corner construction of its own in the port, beside
PerformTwoRolledCorner (two stripes on a common face
(here the hole's wall, not holding the vertex) whose other contacts lie
on two faces meeting along an edge through the vertex (the rods' edge),
each prolonged to and cut by the crease where their surfaces cross, from
the common face to that edge, both ending on it), would build the corner,
but not the file's faces: OCCT's three pieces of each rounding stay three
faces (40 against 36, where the cut-out route gives 39). One face per foot
needs the walk to take the hole's wall rather than its rim past the plane
and one surface over the plane and the wall, which OCCT's fillet does not
make (a face per piece of the stripe on its own surface).
## A rounding over slot mouths whose tops touch the top face (mitcad#1204)

`49ccc315`, `e41d24ac` item 35 (Fillet9, 2.5 round a rim of eight arcs),
traced on the body before the item from the dressup dump (it is the
history's stored state: the item before falls back). Fixed by the OCCT
port's patch 1210 (`third_party/vcpkg-ports/docs/fillet-corners.md`): item
35 of both designs is parametric and verified.

**The configuration.** The rim is a circle of radius 93.5 at the top
(z = 307.4635) split into eight arcs, the edges between the top plane and
eight pieces of the outer cylinder. Each piece is only 2.17 high: below it
an earlier rounding of radius 5 (a torus about the axis, stored as
B-splines) turns the wall outwards, so the ball of 2.5 overflows the
cylinder onto that rounding. At each of the eight vertices a fin 1.5
thick (two planes 0.75 off a radial plane) stands out of the wall; its
top is a cylinder of radius 0.75 about a radial axis 0.75 below the top,
so that its crest touches the top plane along a radial line with no edge
there. Four edges meet at each vertex: the two arcs and the two curves
where the fin's top cylinder cuts the two cylinder pieces. The top plane
and the fin's cylinder have no common edge.

**OCCT.** The eight arcs are tangent, but each vertex has four edges, so
the spine stops there: eight stripes, each ending at two vertices, with
two roundings and the fin's edges at every vertex. The corner's walk
round the vertex looks for the edge between the top plane and the face
beyond the stripe's side (`ChFi3d_Builder_0.cxx`, "Failed to find edge"):
there is none, the faces touching only along the crest line. All eight
vertices fail; with one arc's edge alone its two vertices fail the same
way.

**The file's result** (the stored body after the item, volume 820.97 mm³
less, 326471.995 → 325651.020 mm³). Each arc's blend (the ball between
the top plane and the radius-5 rounding; the 2.17-mm cylinder piece is
consumed) runs on past the vertex over the fin's top and is cut there by
the fin: along its lower part by the fin's side plane (from
(93.45, 0.75, 304.63) to (92.76, 0.75, 306.71) at the +x fin), along its
upper part by the fin's top cylinder, up to the crest's point on the top
plane, (90.978, 0, 307.4635), where the two arcs' blends meet. The fin's
top cylinder grows inwards along its own surface to the blends (3.53 →
7.16 mm²), and the top plane ends at that point.

**The fix.** No corner builder of ChFi3d (`PerformOneCorner` OnSame and
OnDiff, `PerformIntersectionAtEnd`, the cut to the end of 1140 and 1170,
`PerformMoreThreeCorner`) intersects a blend with faces away from the
vertex's own, so patch 1210 adds a construction of its own, tried by
`BRepFilletAPI_MakeFillet::Build` only where the fillet failed at faulty
vertices that are all such mouths: the body less a slab through each
mouth is rounded (the blends then end plainly at the slabs' walls), each
blend's surface is prolonged across its slab, and the slabs' part of the
body is put back less what lies past the prolonged blends, keeping the
fins' tops, so that the fins' cylinders and walls grow up to the blends.
On item 35's body: valid, 304 faces, the faces of each body on the other's
within 2 µm (OCCT's walked blend against the file's along the arcs), the
boolean differences empty both ways with a fuzzy value of 2 µm. The test
is `geometry.fillet_slot_mouths` on the case `fillet-slot-mouths` of
`MITCAD_GEOMETRY_CASES`.

**After it.** Item 36 (Fillet10) of both designs is now tried on the
built item 35; it failed (mitcad#1215, below) and is fixed by patch 1230.

**Tried first.** Removing the eight fins (`BRepAlgoAPI_Defeaturing` of the
faces of each), rounding the healed rim and putting the fins back: the
healed body came out invalid and smaller, so the rounding found no edges
to take. A synthetic ring with fins whose crests touch the top (OCCT
booleans of a ring, boxes and cylinders) comes out invalid at the tangent
line, so the test reads the case's body; with the crests 0.1 below the
top the rim's rounding builds (no vertex on the rim).

## The inner rim rounded over the top of `49ccc315`/`e41d24ac` item 36 (mitcad#1215)

Item 36 (Fillet10, 2.5) rounds the inner rim of the ring whose outer rim
item 35 rounded (above), traced on item 35's built body from the dressup
dump with OCCT's fillet alone. Fixed by the OCCT port's patch 1230
(`third_party/vcpkg-ports/docs/fillet-rolling-ball.md`): item 36 of both
designs is parametric and verified, items 35 and the others as before.

**The configuration.** The ring's top is 5 wide between the inner cylinder
(radius 88.5) and the outer rim (93.5). Item 35's blend of 2.5, which
overflowed from the outer cylinder onto the rounding below it, meets the
top at radius 90.978: the top left is 2.478 wide. The inner rim is a closed
chain of 17 circular edges between the top and the inner cylinder (the
cylinder is split at the eight mouths and at its seam). A ball of 2.5
rounding it would touch the top at radius 91.0, 0.022 past the top's far
edge, where item 35's blend begins tangentially; that blend curves away
from the ball exactly as fast as the ball's own sphere (both radius 2.5),
so the ball touches it along a whole section. OCCT's walk overflowed onto
it and failed at once ("the walk between the two faces fails").

**The file's result.** The inner blend is a torus of radius 2.5 about the
axis passing through the top's far edge all round (its distance to the
torus centre circle 2.5000 to 2.5006 in the file's B-spline), meeting item
35's blend there; the top face is gone.

**The fix.** The ball rolls on the top's far edges (OCCT's surface/edge
blend) instead of overflowing onto a face curving away as fast as itself.
Along the far boundary of the top the crests at the mouths leave sliver
edges of 2 to 7 µm between the top and a fin's cylinder in item 35's
result (at 90° and 270° in `49ccc315`, at 0° and 45° in `e41d24ac`); the
ball turns about the end of the edge it rolled on until it touches the edge
after the sliver (or rolls on from that edge's start where it reaches it
already), the piece of the ball between running along the sliver. At 270°
the inner cylinder's seam lies under a crest: the cylinder contact changes
face there. On both bodies the result is valid, its faces and the file's
round the rim on each other within 2 µm. The test is
`geometry.fillet_roll_over_blend` on the cases `fillet-roll-over-blend` and
`fillet-roll-over-blend-2` of `MITCAD_GEOMETRY_CASES`.

## A pin's foot rounded where a plate's end goes on from it, again (mitcad#1336)

`5ef2dbd5` imported at 88 / 3 / 7 (parametric / fallback / skipped of 98
items) on dev 49c6482c3, where main 641b06c1 (round 20 of mitcad#343) had
90 / 1 / 7: items 51 (Fillet5, the pin's foot of round 12) and 87
(Fillet27, the fin's top of rounds 18 and 20) fell back. Traced on this
design alone with the dressup dump, OCCT's fillet alone on the replay's
body and on the history's stored body before the item (OCCT built from
the port at each patch for TKFillet), and both builds' imports.

- **Item 51, two causes, each enough.**
  - **OCCT: port patch 1140 (with 1170).** On the stored body (the pin and
    the plate's rounded end one face, closed round the axis) OCCT's fillet
    alone is valid with the port up to 1120 (one torus, the material added
    4.76572 against the file's 4.76602 mm³) and not valid from 1140 on (an
    unorientable cylinder, a second torus with a negative area).
    `ChFi3d_KPartCutToEnd` took both ends past the vertex (1140's end along
    a tangent edge), where 0911's corner had cut them: the other contact's
    line is a closed circle, the analytic piece's parameters at its ends
    the whole line's a period apart, the last end's section found at the
    line's start, and at one end the side's edge with the pin was prolonged
    over the pin's seam. Port patch 1290 leaves such ends (the other
    contact's face closed round its surface) to the corner, as before 1140.
  - **The join of faces on one surface (mitcad#1261).** The replay's body
    after item 50 (Extrude7, the pin joined onto the plate) kept the pin's
    side (56.55 mm²) and the plate's rounded end (13.75 mm²) apart, where
    641b06c1 made them one face (70.30 mm², as the file). The pin is
    extruded from two regions; the extrusion's join (`joined` in
    `extrude.cpp`) makes its side one face closed round the axis, its seam
    where the regions met: along the plate's side's edge with its end.
    OCCT's merge after the join leaves such a closed face apart from an
    open piece of its surface (it joins open pieces into a closed face),
    and the join of faces on one surface did too. Such a group is now
    joined again with every closed face cut in two first
    (`ShapeUpgrade_ShapeDivideClosed`, `join_same_surfaces`): the pin and
    the plate's end one face again, the body after item 50 the file's 42
    faces. On that body the dressup builds item 51 with 1290: parametric,
    verified, its edge read by its name in the file.
- **Item 87 followed item 51.** At 641b06c1 it was parametric through the
  retry on the file's exact state (state 57, the varsi body replaced; the
  fin's rounding fails on the replay's body, whose fin top item 86's
  chamfer leaves 0.06 mm³ off the file's, within the tolerance). With
  item 51's fallback, its base feature brought the varsi body to the file's
  state 33 as well, so the replay before item 87 matched state 57 within
  the tolerance: the retry kept the replay's body and failed on it. With
  item 51 parametric again, item 87 is parametric through the retry as at
  641b06c1 (matched, its main faces on the file's within 0.01 mm).

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 51 Fillet5 | 1 / 1 parametric, verified, edge read by its name in the file (1290, the join) | |
| `5ef2dbd5` 87 Fillet27 | 1 / 1 parametric, verified, inputs read, on the file's exact state (as at 641b06c1) | the rounding fails on the replay's body (item 86's chamfer 0.06 mm³ off): not built on the replay since round 20 |
| `5ef2dbd5` 30 Fillet4 | 0 / 1, as before | the file kernel's own corner (round 19) |

The design imported alone (release build, its reference folder): 90 / 1 /
7 of 98, history states matched 64 of 67 (as round 19), final bodies as
before (9.7e-6). `f3d.corpus_designs` names it (items 51 and 87
parametric, 87 allowed the retry on the exact state, a new seventh field;
30 falling back). Tests: `geometry.fillet_tangent_end_face` (a pin on a
plate's end, its seam along a side, failing without 1290) and the
boolean part of `geometry.suite` on the case `pin-plate-end-join` of
`MITCAD_GEOMETRY_CASES` (the body after item 50 as the boolean left it:
the pin and the plate's end one face, failing without the cut; no
synthetic layout reproduced the merge's refusal: a pin made with its seam
there, by a primitive or by a face of the plate's own cylinder, was joined
without it).

## A fin's top rounded on the replay's body after its chamfer (mitcad#1341)

`5ef2dbd5` item 87 (Fillet27, radius 0.2 on the fin's top edges) was
parametric only through the retry on the file's exact state (state 57):
on the replay's body the rounding failed. Traced with the dressup dump,
item by item on the replay's bodies and the history's stored ones, and
OCCT's chamfer and fillet alone (OCCT built from the port at each patch).

- **Not a deviation of the replay before the item.** Item 86's chamfer
  (Chamfer2) on the replay's body lies on state 57 within 3 µm (its cones
  end at the arcs' meridian sections, the file's a few micrometres short of
  them on the side strip). Run on the file's stored state 56 itself, the
  same chamfer gives the same body, and the rounding failed on it the same
  way: the operation, not the replay's state before it.
- **Three causes in OCCT, each hidden behind the one before** (port
  patches 1300 to 1302, `third_party/vcpkg-ports/docs/fillet-replay-1300-1302.md`):
  - Where an arc's torus ends, the rounding's contact on the top lies on the
    crease between the arc's cylinder and the spline's extrusion, convex by
    1.4 degrees. 0980's turn about a concave crease took it for concave
    (`ChFi3d::DefineConnectType` judged it 0.17 mm into the extrusion,
    where the spline has turned past the crease); the ball rolled on the
    crease's edge, which it leaves at once, and the walk failed on both
    sides (1300).
  - Coming to an arc's torus, the walk's contact on the chamfer reached the
    torus's start 2.2 µm along the spine before the guide's end; a step of
    2.2 µm made a piece the result's build could not join, the arc's edge
    left on one face (1301).
  - Three of the chamfer's faces (the parts that 0531 cuts it into over the
    narrow side strips) had no name: OCCT's history did not give them their
    edges, so the file's edge along the arc on one side was not found by its
    name (1302).
- **Item 89 (Fillet29, radius 1 round the fin's end) now builds on the
  file's exact state.** Before, it ran on item 87's result on state 57 (the
  retry had brought the fin's body to it); on the replay's chain both its
  sides' roundings are taken for overlapping where they reach the fin top's
  rounding (OCCT's walk then fails), and on state 59 it builds. Run on state
  58 with the replay's item 88 it builds too, so the first deviation is item
  87's own result at the fin's end: its corner faces there lie up to
  0.013 mm off the file's (a torus of 1.24 mm² in the file), within the
  import's check of item 87's corner faces. Not fixed here.
- **Item 85 (Chamfer1, the root's chamfer of 1.6 at 40 degrees between the
  fin's sides and the base's tori) is a walked B-spline** where the file
  has cones: OCCT has no analytic chamfer between a plane square to a
  torus's axis and the torus. The walked faces lie on the file's within
  1 µm and the items after it build on them; an analytic piece for it (a
  cone, built and checked within 5e-5 mm of the file's) made item 85 the
  file's, but the walked intersection of its end with the fin's spline
  extrusion, whose profile is only C0 at a knot, then got a curve 1.4 mm
  off on the extrusion and item 87's end at the base a broken edge: left
  for a follow-up with item 89.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 87 Fillet27 | 1 / 1 parametric, verified, edges read by their names in the file, on the replay's body (1300 to 1302) | |
| `5ef2dbd5` 88 Fillet28 | 1 / 1 parametric, verified, on the replay's body | |
| `5ef2dbd5` 89 Fillet29 | 1 / 1 parametric, verified, on the file's exact state (state 59) | the fin top's rounding's corners at the fin's end, up to 0.013 mm off the file's |
| `5ef2dbd5` 30 Fillet4 | 0 / 1, as before | the file kernel's own corner (round 19) |

The design imported alone (release build, its reference folder): 90 / 1 /
7 of 98 as before; `f3d.corpus_designs` names #89 (no longer #87) among the
items allowed the retry on the exact state. Test:
`geometry.fillet_fin_top_creases` on the case `fin-top-chamfer-rounding` of
`MITCAD_GEOMETRY_CASES` (the stored body before the chamfer and that body
chamfered by the import): every chamfer face generated by a contour edge,
the rounding built and closed, the stored body after it on the result
within 0.01 mm (each patch's part fails without it).

## A fin's end rounded on the replay's body (mitcad#1345)

After mitcad#1341, `5ef2dbd5` item 89 (Fillet29, radius 1 along the fin's
end face's edges with the root's chamfer and rounding, up each side to the
top) built only through the retry on the file's exact state (state 59).
Traced on the replay's bodies and the history's stored ones, with OCCT's
fillet alone (OCCT built from the port at each patch).

- **Item 87's end at the fin's end face (the first deviation).** The fin's
  last arc meets the end face, an extrusion of a spline stretch of the
  profile that goes on from the arc tangentially and falls away from it.
  The file's torus along the arc (1.24 mm² on each side) runs on past the
  vertex where the arc's edge ends, cut by the end face up to where it
  meets the chamfer's edge with the end face, 1.2 mm past the vertex. OCCT
  ended one side's torus at the vertex with a corner face (0.005 mm²) and
  left the chamfer's cone to meet the end face directly: 0.013 mm off the
  file's; the other side, walked, matched within 3e-4 mm. Run on the file's
  own state 57, OCCT gave the same end, so the operation, not the replay
  before it, was the cause. Port patch 1310
  (`third_party/vcpkg-ports/docs/fillet-replay-1310-1311.md`): the analytic
  blend runs on past the end, cut by the end face. Item 87's faces about
  the fin's end now lie on state 58 within 2e-5 mm.
- **Item 89 on the replay (the cause of its retry).** Each side's chain
  ends at the top at an OnSame vertex, and 0600's walk on past it with the
  end face's surface unbounded runs the contact on the end face over the
  top, past the top's crease. The replay's end face is the extrusion of
  the profile's spline stretch alone (a B-spline curve of four knots), the
  file's state the extrusion of the whole spline (thirteen knots, running
  on past the stretch's ends): the walk fell off the stretch's end at once
  and the two sides' roundings were taken for intersecting. That is why
  item 89 built on state 58 with the replay's item 88, as mitcad#1341 found,
  and also, with 1310 in, still failed on the replay. Port patch 1311: the
  extruded face's curve run on past the stretch's end by four radii
  (curvature continuous, within 1e-4 mm of the file's spline over the first
  1.6 mm). On the replay's body item 89 builds, and the stored state 60
  lies on it within 0.0006 mm round the fin's end.
- **Item 91 (Fillet20, radius 0.3 round the outlines of the plate's two
  sides) now needs the retry on the file's exact state (state 61).** It ran
  on the file's state before, because item 89's retry had brought the body
  to it; on the replay's body it gives an invalid result (crossing wires),
  with or without 1310 and 1311. Its walk along the plate's side where an
  earlier rounding of the plate's top edge meets it ends where that
  rounding vanishes into the side: on the replay that rounding ends in a
  point at the side's vertex and the walk ends in a point there, while the
  file's ends 0.04 short of it; the faces there lie within 2 µm of the
  file's. Left for a follow-up.
- **Item 87 at the creases between the top's arcs and spline stretches**
  (not the fin's end): the import's check passes it with seven of its new
  faces up to 0.012 mm off the file's, the walked faces next to the creases
  (their contact lines on the top lie some 0.07 mm from the file's, the
  faces tangent to the top there). Nothing after it depends on those faces;
  not changed here.

| Item | Achieved / target | Cause of the gap |
|---|---|---|
| `5ef2dbd5` 87 Fillet27 | 1 / 1 parametric, verified, edges read by their names, on the replay's body; its end at the fin's end face the file's (1310) | |
| `5ef2dbd5` 88 Fillet28 | 1 / 1 parametric, verified, on the replay's body | |
| `5ef2dbd5` 89 Fillet29 | 1 / 1 parametric, verified, edges read by their names, on the replay's body (1311) | |
| `5ef2dbd5` 90 Fillet25 | 1 / 1 parametric, verified, inputs inferred as before, now on the replay's body (seven of the file's new corner faces, 0.8 % of their area, up to 0.044 mm off) | |
| `5ef2dbd5` 91 Fillet20 | 1 / 1 parametric, verified, on the file's exact state (state 61) | the walk along the plate's side ending in a point where an earlier rounding vanishes into it; was on the file's state through item 89's retry |
| `5ef2dbd5` 30 Fillet4 | 0 / 1, as before | the file kernel's own corner (round 19) |

The design imported alone (release build, its reference folder): 90 / 1 /
7 of 98 as before; `f3d.corpus_designs` names #91 (no longer #89) among the
items allowed the retry on the exact state. Test: `geometry.fillet_fin_end`
on the cases `fin-top-chamfer-rounding` and `fin-end-rounding` of
`MITCAD_GEOMETRY_CASES` (each patch's part fails without it).

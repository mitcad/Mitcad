# Fillets of stored designs: 0950–0980

Rounds 17 and 18 of mitcad#343: window sills consumed by two roundings, a
wall stepping back by micrometres, roundings vanishing at a tangent vertex,
a chain past a corner tighter than the ball, and a chain over creases of a
few degrees. The projection fix 0960 is in
[offset, projection and pipes](offset-projection-and-pipes.md#0960-projlib-projection-boundary-bisection-stops-at-adjacent-parameters).
The earlier rounds are in [0930 to 0942](fillet-replay-0930-0942.md), the
later ones in [0990 to 1020](fillet-replay-0990-1020.md). Back to the
[port's README](../README.md).

## Overview

- **0950** lets the roundings of both edges of a face exactly as wide as the
  two take the face away: their contacts along each other on it do not
  clash, the points their corners make at the contacts' ends are one, and
  the two contacts are one edge (window sills in a ring wall rounded with
  half the wall's thickness on both edges, mitcad#343).
- **0951** ends a rounding where its wall steps back into the material by
  much less than the radius (a narrow strip, square to the rounding, at the
  end): the blend ends at the section through the vertex and a quarter
  torus, its ball turning on about its contact's end, closes it, cut by the
  stepped-back wall (a rim's wall stepping back by 4.3 micrometres,
  mitcad#343).
- **0961** ends a rounding in a point at its spine's end vertex, where its
  faces come tangent, when its walk stopped short of the vertex on the
  spine's own edge (the faces meeting only within that edge's tolerance),
  its surface bent to the vertex with its contact curves (a bar's outline
  rounded up to where an earlier rounding runs tangentially into the bar's
  side, round 18 of mitcad#343).
- **0962** builds 0951's quarter torus whichever side of the blend the wall
  is on, and only where the recess beyond the step ends in a strip back to
  the wall; a later rounding whose recess an earlier rounding ends runs on
  across the step (mitcad#343).
- **0970** rounds a chain past a wall's convex corner rounded with less than
  the radius, where no ball rolls round: the chain is split at the corner's
  arc, both roundings end where their axes cross, at the ball touching the
  face of the chain and both walls, and a vertex blend from that ball's
  centre to the corner's arc closes the corner, with a torus where the
  second wall leans over the face (an underside outline rounded with 0.5
  past corners rounded with 0.3, mitcad#343).
- **0980** rounds a chain whose faces turn into their neighbours at creases
  of a few degrees, which the fillet takes for tangent, both faces of the
  blend changing at nearly one section: a contour runs on over a short edge
  not asked for to the next edge asked for, the guide of a walk next to an
  analytic piece is built from a stretch of its edge before it, both faces
  change at once where the contacts reach their arcs within a hair, and the
  ball turns about a concave crease, rolling on its edge, instead of jumping
  across it (a fin's chamfered top edges, mitcad#343).

## 0950: window sills consumed by two roundings

The patch 0950 comes from mitcad#343 (round 17): the floors and ceilings
of windows in a ring wall 3 thick (radii 112 and 115), rounded with 1.5
on both edges, ending at the pillars' full-rounded ends, the windows the
copies of a circular pattern. The two roundings of a sill are halves of
one torus (113.5, 1.5); their contacts on the sill lie along each other at
the wall's middle.

### 0950-chfi3d-roundings-consuming-the-face-between-them-meet-on-one-contact-edge

`ChFi3d_StripeEdgeInter` took the two contacts, which overlap all along, for
roundings too big for the face; such a pair (one segment covering both
ranges) now meets along the contacts. After the stripes are in the data
structure (`joinEndsAlongContacts`), the points two such stripes' corners
made where the contacts end on an edge of the face are one, the edge leaves
the face on both sides of them, and where both ends are one the second
contact edge is replaced by the first (its curve on the second blend
projected anew, its transition turned where the two run opposite ways), the
face holding that edge on both sides. Before, the sill came out as a sliver
of no area, or with the arc's pieces left on it and on the pillar's end,
depending on the window's copy. On the design's body before the item (48
edges, four windows): valid, every face on the stored body's within 1e-6 mm
both ways, the change of volume -243.0297 against the file's -243.0189 mm³;
before, two of the four windows failed.

## 0951: a wall stepping back by micrometres

The patch 0951 comes from mitcad#343 (round 17) as well: a rim's edge
rounded with 0.3 up to a vertex where the rim's wall steps back by 4.3
micrometres. The wall's face at the edge ends on a strip that narrow (the
step's face, square to the rounding); the stored bodies end the rounding
at the section through the vertex and close it with a quarter torus.

### 0951-chfi3d-rounding-ending-at-a-step-back-of-its-wall-closes-with-a-torus

OCCT found no face for the rounding's end and left an invalid shape.
`PerformPivotAtStepEnd`, tried first for the end of a lone stripe, takes an
end where the contact on a plane ends on its edge with a plane strip
narrower than a tenth of the radius, the face beyond the strip parallel to
the first and the strip's width into the material, and the other face a
plane square to both: the stripe ends at the section through the vertex, and
a corner stripe of one piece closes it, the torus the ball sweeps turning on
about its contact's end point until it lies in the wall's plane (both radii
the rounding's), cut by the stepped-back face. The section becomes two
edges, the one over the strip bounding it. On the design's body before the
item (14 edges): valid, every face on the stored body's (distance 0 both
ways), the change of volume -1.7315 against the file's -1.7477 mm³; before,
the result was invalid.

## 0961 and 0962: a bar's outline rounded to a tangent vertex

The patches 0961 and 0962 come from mitcad#343 (round 18): the same bar's
outline rounded with 0.3 on both sides (22 edges, the item before that
rim's rounding). Each side's chain runs along an earlier concave rounding
(radius 3) that goes on tangentially into a corner rounding of 0.4, itself
tangent to the bar's side: the outline edge's two faces come tangent at its
end vertex V, and the blend vanishes there.

### 0961-chfi3d-rounding-vanishing-where-its-walk-stopped-on-its-own-edge

The stored faces meet only within the edge's tolerance (1.5e-3). A gap g
between faces at an angle a moves the line where they meet sideways by g /
a, and the ball's contacts, R a / 2 from that line, reach the edge itself
where a² = 2 g / R: the walk's contact on the earlier rounding met the
spine's own edge 0.014 to 0.042 short of V (sections 0.004 to 0.011 wide) at
all four ends, and the walk ended there. The end was not taken as vanishing
(0130 takes contacts within a hundredth of the radius), so the corner went
to the intersection at the end, whose plate failed at one of them (each
tangent chain alone gave an invalid shape). Where a lone stripe's contact
ends on the spine's own edge into V, a tangency extremity, its section no
wider than sqrt(2 R tol) and both contacts within the radius of V
(`ChFi3d_ContactOnOwnEdgeAtEnd`), the end is put at V as 0130 does, without
comparing the faces' normals; the blend left between the stop and V is
narrower than that section. `ChFi3d_ShrinkVanishingEnd` then bends the
blend's surface with its contact curves (its boundary isos): all three cut
by a full knot at one parameter, the surface's end row of poles put at V, so
that the surface ends in V and the tolerances are the curves' sampled
deviation from it and from their faces, not the distance the end moved (as
wide as 0.04, which the dressup's check of inflated vertex tolerances took
for a corner left open). On the design's body before the item: valid, the
file's 174 faces, every face on the stored body's after it within 0.004 mm
both ways.

### 0962-chfi3d-step-end-torus-cut-on-its-direct-frame-and-only-at-a-recess

With the outline rounded, 0951's rim rounding (the next item) met the step
with the wall on the blend's first side. 0951 reverses the torus in u there
(the blend's normal asks for it); its cut by the stepped-back face, computed
on that reversed torus, whose frame is indirect, failed, the end went to the
other corners, and the side plane's wire came out broken (area -642 mm²).
The cut is now computed on the torus as built, its end points' u negated,
and its p-curve mirrored in u onto the reversed torus: valid, every face on
the stored body's within 0.004 mm. With both ends so built, the item after
it (radius 0.2 on the walls' edges below the step) built quarter tori at
both its ends 0.04 off the file's: the stored bodies run that rounding on
across the step, cutting the recess's 4.3 micrometres, until the rim's
rounding (whose torus ends the recess at its far side) cuts it. The pivot is
now taken only where the stepped-back face's edge with the other face ends
at its far end on a strip back to the wall's plane (a recess, 0951's case);
elsewhere OCCT runs the blend on as before, and that item has the file's 229
faces, every face of the result on the file's within 4.3e-4 mm.

## 0970: a chain past a corner tighter than the ball

The patch 0970 comes from mitcad#343 (round 18): a part's underside (a
plane) rounded with 0.5 along an outline of a side wall square to it, a
corner rounded with 0.3, a front wall leaning 15 degrees over it (its
edge with the underside acute), a second corner and the other side wall.
The ball cannot roll round a convex corner tighter than itself: its centre
would turn back about the corner's axis. OCCT walked the chain over the
corner's arc anyway and threw storing the walk (`StoreData`, the
approximation of a walk running backwards). The stored bodies end both
walls' roundings where their axes cross and close the corner with a vertex
blend (a B-spline patch, its third side the corner's arc turned square to
the underside, a point at the ball's contact with the underside) and a
torus about the front rounding's axis (minor radius the corner's, major
the difference of the radii), which cuts the corner's cylinder.

### 0970-chfi3d-chain-past-a-wall-corner-tighter-than-the-ball-vertex-blend-and-torus

`SplitAtTightCorners` (new, as `Compute` starts) splits a convex contour of
constant radius R at an arc between a plane U and a cylinder K of radius
below R, between two lines whose other faces are planes tangent to K, one of
them (Wp) square to the other line, the other (Wo) square to U or leaning
over it (an obtuse one is left as before): the runs between such arcs become
contours whose ends at the arcs have the status `ChFiDS_Tangent`, the arcs
left out. `PerformTightCorners` (new, before the corners at vertices) finds
the ball's centre C, R inside U, Wp and Wo, and builds the vertex blend C +
sin t A(u) + cos t R n (n U's outward normal, A(u) the arc of K's radius
about C + rho np + rho ep in the plane through C parallel to U, rho = R -
rk, np Wp's outward normal, ep along Wp's rounding towards the corner), an
exact rational biquadratic patch tangent to both roundings along their
ellipses through the ball's contact with U; where Wo leans, the torus about
Wo's rounding axis turns K's section from Wo's normal to ep and meets the
vertex blend along its arc, Wo's rounding along an arc of radius R, Wp along
an arc of radius rho, and K along K's section. The roundings end on those
curves, the vertex blend and the torus are corner stripes on the roundings'
surfaces and Wp, and K is cut along its section. `SplitKPart` keeps the
pieces at the last end of such a contour, as at its first end (before, it
dropped them all and the analytic rounding was walked). On the design's body
before the item (the outline's five edges): valid, the file's 197 faces,
every face on the stored body's within 0.002 mm both ways, the change of
volume -1.5382 against the file's -1.5432 mm³ (the file's vertex blends are
cubic B-splines); before, the fillet failed.

1590 ([fillet-tight-rim-corner-1590.md](fillet-tight-rim-corner-1590.md))
takes the same corners at a cylinder's rim (K a torus, Wo the cylinder)
and closes those where Wo's edge with U is obtuse with the corner's own
ball rolled on between the roundings.

## 0980: a chain over slight creases

The patch 0980 comes from mitcad#343 (round 18): a fin's top edges rounded
with 0.2, the top an arc and a straight part, the sides chamfered along the
arcs only, slivers joining the chamfers to the straight part's sides. Every
face meets the next at a crease of one to five degrees; the selected edges
are the arcs' and the straight parts', short sliver edges between them.

### 0980-chfi3d-rounding-across-slight-creases-of-its-faces-at-one-section

Five changes, each needed by the next. A contour stopped at a turn
(`StopChainsAtTurns`) runs on over an edge not asked for that leads,
tangentially, to an edge asked for: each selected edge was a contour of its
own, ending at a vertex of four sharp edges where the corner failed. The
guide of a walk that starts a hair short of the end of its edge after an
analytic piece (less than a hundredth of the guide's length) is built from a
stretch of that edge before it, its parameters kept: its first span was that
hair, the spine's kink turned within it, and the first section's reframing,
looking a little before the start, met the guide extrapolated off the spine.
A kink onto a face holding the spine's end vertex (`KinkIntoEndFace`) ends
the blend only where no edge of the spine lies on that face. Where one
contact crosses onto a tangent face and the other lies within ten times the
approximation tolerance of such an edge ahead, both change faces at once,
and the first section starts on both arcs where they are reached within ten
times the guide's tolerance of each other: a walk on one new face and one
old was taken between, out of order with its neighbours. A face taken for
tangent across a concave crease of more than the angular tolerance does not
continue the face before: the ball rolls on the crease's edge from the ball
of the piece before until it touches that face (its reference face there),
the edge taken on the face whose part beside it stays, the walk not
completed back over the piece before, and the crease's point where the ball
leaves it onto the face it rolled on given that piece's transition only.
Before, the walk on the next face started some 0.1 mm further on and the gap
was closed by edge tolerances of 0.1 mm. On the design's body before the
item (12 edges): valid, every new face on the stored body's within 0.0083 mm
and the stored body's on the result's within 0.0024 mm, the change of volume
-0.2909 against the file's -0.2888 mm³; before, the corners failed.

## Reports and tests

Each of these tests fails without its patch.

- 0950: mitcad#343 and five windows in a ring wall 3 thick, the pillars'
  full-rounded ends in two halves each, both edges of every sill rounded
  with 1.5, in `geometry/tests/test_fillet_consumed_strip.cpp` (valid, no
  face of no area, the material taken within 2e-3 mm³ of the integral over
  the sills' outlines; the fillet fails without the patch).
- 0951: mitcad#343 and a block's edge rounded with 0.3 up to where its wall
  steps back by 0.0043, the step below and above the edge, one rim and
  both, as built and mirrored, in `geometry/tests/test_fillet_step_end.cpp`
  (valid, a quarter torus at each end, the material taken within 2e-6 mm³
  of the integral; invalid without the patch).
- 0961: mitcad#343 and a tower on a slab flush with its side, the tower's
  corner rounded with 0.4 and its foot there with 3, the side's edge with
  the foot's torus rounded with 0.3 up to the vertex where the three are
  tangent, the torus moved 0.0002 to 0.001 off the side within its edges'
  tolerance, in `geometry/tests/test_fillet_vanishing_own_edge.cpp` (valid,
  the blend ending in a point at the vertex, the vertices' tolerances below
  0.01, the material taken within 0.5 % of the exact body's; the fillet
  fails without the patch).
- 0962: mitcad#343 and 0951's cases with the body's faces in the reverse
  order, and the edge below the step rounded with 0.2 after the edge above
  it with 0.3, in `geometry/tests/test_fillet_step_end.cpp` (the reversed
  cases valid as the others; the second rounding's cylinder runs on past the
  step up to the first's torus, with no torus of its own; the reversed cases
  invalid or failing without the patch).
- 0970: mitcad#343 and a block's bottom outline rounded with 0.5 past front
  corners rounded with 0.3, the front wall square to the bottom and leaning
  15 degrees over it, and the whole outline of a block with all four
  corners rounded, in `geometry/tests/test_fillet_tight_corner.cpp` (valid,
  the vertex blends on their exact surface within 1e-6 mm, the tori where
  the wall leans, the material taken within 1e-6 mm³ of the integral where
  the walls are square; without the patch the leaning wall's fillet fails,
  and the square corners come out with other corner patches, 0.0065 mm off
  the vertex blend).
- 0980: mitcad#343 and a box's top front edges rounded with 1 and 0.5 where
  the front turns outward by 3 and 5 degrees about a slanted crease from the
  top edge, its faces B-splines, the edges given either way, in
  `geometry/tests/test_fillet_concave_crease.cpp` (valid, no edge tolerance
  above 1e-4, the material taken within 0.5 % of the two edges' sections
  along them; the fillet fails at a vertex without the patch; the test
  covers the turn over the crease, the other four changes are verified on
  the design's body only, as no synthetic body reproduced the creases'
  micrometre jumps at one section).

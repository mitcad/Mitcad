# Fillets and chamfers of stored designs: 0550–0692

Roundings and chamfers that the `.f3d` import's rounds of mitcad#343 and the
`.ipt` import's replay gaps (mitcad#235) found failing, built as the bodies
stored in the files have them. The later rounds are in
[0700 to 0772](fillet-replay-0700-0772.md). Back to the
[port's README](../README.md).

## Overview

- **0550** ends a rounding over pieces cut by the face at the end where that
  face's edge is in two pieces at a vertex no other edge meets (mitcad#343).
- **0580 and 0581** round a pocket's walls over ribs up to blocks flush with
  the contact: a contact line along a concave edge of its face is off that
  face, and roundings joined over a rib whose sides cut them along sections
  meet inside the piece over the rib's end (mitcad#343).
- **0600** ends a rounding at a vertex where a wall starts by its blend run
  on past the vertex along its other face up to the wall's edge with that
  face, cut by the wall along its side (mitcad#343).
- **0620** rolls a rounding on the convex edge where its contact leaves a
  narrow face before the end of the spine, as in its middle, where that
  edge is not of a face at the end vertex (mitcad#343).
- **0650** cuts an analytic chamfer over a gap by a chain of up to nine
  faces beyond, along the arc nearest the blend's piece where the face
  beyond grows to the cut, from gap ends at vertices of the contact's face,
  and keeps the pieces about a closed spine's start (a flange chamfered past
  the strip of its top up to the roots of the teeth standing on it,
  mitcad#235).
- **0651** starts a walk on the seam of a face closed round its surface on
  the seam's other side where it leaves the face across it, and ends a
  spine's extension where its walk or surface fails (a thread's crests
  rounded along all their turns, mitcad#235).
- **0670** makes the rounding round a wall's corner rounded with the ball's
  radius, between two analytic pieces, the piece of the standing ball's
  sphere (a tab's top outline on a steep cylinder across it, mitcad#235).
- **0671** cuts an analytic chamfer whose contacts both leave their faces
  before its edge's ends across its whole width there, by a chain of faces
  beyond grown on to it (a thread's groove ending in a flat face, the edge
  where it meets the bore chamfered past the flanks, mitcad#235).
- **0690** carries the seam of a face at a rounding's end down along an arc
  that continues it to the rounding's contact (a bar's top edge rounded
  where the bar meets a bore's wall at its seam, mitcad#235).
- **0691** takes the end of an OnSame rounding whose face at the end
  continues the opposite face tangentially where the contact crosses the
  prolonged edge, near the corner, and **0692** puts an analytic blend's
  contact end found within a vertex's tolerance at the vertex (a recess's
  rim rounded up to a rib across it whose bottom edges are rounded,
  mitcad#235).

## 0550: an end over pieces where the end face's edge is in two

The patch 0550 comes from the fillet corners left after mitcad#341
(mitcad#343): a rounding whose end lies over two pieces, where the edge of
the face at the end is in two pieces.

### 0550-chfi3d-end-over-pieces-where-the-end-faces-edge-is-in-two

A bar joined to a cylinder, its long top and bottom edges chamfered and the
chamfers' edges with its side rounded; the side's concave edge with the
cylinder is rounded with 3, on tangentially over the bar's roundings and
chamfers up to the caps. Near each cap the cylinder's contact reaches the
cap first; the walk goes on until the other contact crosses from the bar's
rounding onto the chamfer, and its extension over the chamfer (the
cylinder's side given up) reaches the chamfer's edge with the cap: the end
lies over two pieces, both cut by the cap (`PerformMoreSurfdata`). That end
is taken where the piece before the last ends on an edge of the face at the
end at the vertex; the cap's edge with the cylinder is in two pieces here
(the profile's vertex where the bar's side met the cylinder stays on it
after the chamfer and the rounding took the side's top away, with only those
two pieces at it), and the point lies on the piece away from the vertex. The
end was not taken, `PerformOneCorner` dropped the extension and failed in
`PerformIntersectionAtEnd`, whose walk round the bar's rounding, a face
without the vertex, did not come back ("the max number of faces reached").
An edge now counts as the arc at the vertex where it goes on from it between
the same two faces through vertices no other edge meets
(`ChFi3d_ArcGoesOnAs`, in `MoreSurfdata` and `PerformMoreSurfdata`). The
result is the one built where the cap's edge is one piece, as the bodies
stored in the files are (0.3 µm from them in a stored design).

## 0580 and 0581: a pocket's walls over ribs

The patches 0580 and 0581 come from the same round (mitcad#343): a pocket's
ceiling rounded with its walls (radius 8), the walls' edges broken by ribs
1 thick standing out 6 from them, the roundings ending at blocks whose side
lies where they touch the ceiling. The bodies stored in the files have one
cylinder along each wall, cut by the ribs and ended by the blocks.

### 0580-chfi3d-kpart-contact-along-a-concave-edge-off-its-face

Past a rounding's end the ceiling's contact line runs along the block's
concave edge with the ceiling. `SplitKPart` hatches the line on its face,
and the hatcher took that stretch as inside the face or outside it as the
line lay a rounding error (1e-14) to one side of the edge or the other: on
one wall the line's domain ended at the block and the corner cut the blend
with the face at the end, on the mirrored wall it ran on to the ceiling's
rim, 21.6 past the end, and the corner failed or built faces of reversed
orientation. No ball of the blend touches a face along a concave edge of it
(the face beyond rises into the ball), so a straight contact line along such
an edge is hatched moved off the face across it by the shift
`HatchOnBoundary` uses, its ends near a vertex snapped to the vertex
(`HatchOffConcaveEdges`).

### 0581-chfi3d-roundings-joined-across-a-rib-with-square-sides

0241's `ChFi3d_JoinAcrossNotches` joins the roundings on either side of a
rib, one stripe's blend running on over the gap cut by the rib's faces. The
rib's sides cut the blend along sections and give no piece, so the piece on
the rib's end starts where the blend crosses that face's far edge, not at
the other stripe's end point, and the stripes met at that point: the piece's
edge on the rib's end started there, the rib's side lost its edge with the
blend and the shell was open. The piece is now split at its middle, its half
at the other stripe's end goes to that stripe, and the stripes meet at the
split, a whole section on one face; the rest of the end section is the
side's edge with the blend, which `ChFi3d_FilDS` makes between two pieces of
one stripe (`PartSection`). Only where the blends are one surface with the
same sides and the stripes run the same way at the gap. And where a stripe
joined before is joined again (three edges, two ribs), every surface data on
the old surface takes the new one, not only the extended stripe's: the
stripe joined first was left a face of its own with no edge at the section
between them.

## 0600: an OnSame end cut along its side

The patch 0600 comes from the next round (mitcad#343): a convex edge
between a band N and a cone Fo rounded with 1, ending at a vertex V where a
notch wall B (a plane at 8 degrees to the edge) starts. Past V, N goes on as
a narrower tongue between the notch walls and Fo goes on beside B (B's edge
with N convex, with Fo concave: an OnSame end). The bodies stored in the
files run the blend on past V along Fo until its contact on Fo reaches B's
edge with Fo (5.9 past V), and B cuts it along its side from there back to
where its contact on N crosses B's plane (4 before V); B grows up to the
cut.

### 0600-chfi3d-onsame-end-cut-along-its-side-by-the-face-at-the-end

OCCT walked on past V until the contact on N left the tongue across its edge
with a face tangent to it (4 past V, the contact on Fo still short of B),
and the corner took that edge's face for the face at the end: an invalid
result (faces of reversed orientation) or two roundings that cross.
`ChFi3d_Builder::WalkedOnSameToEndFace` now finds such an end after the
stripe is walked (OnSame, a vertex of three edges, the last SurfData past V
with its contact on N on an edge that is not B's and its contact on Fo
inside Fo) and walks that SurfData again from its other end, N's surface
unbounded and the guide run on past its end, up to where the contact on Fo
reaches B's edge with Fo; that SurfData replaces the last one, and
`PerformOneCorner`'s OnSame corner cuts it by B and prolongs B's edge with N
up to the contact on N, as at an ordinary OnSame end. The stripe is left as
it was where that walk does not reach the edge; analytic blends (not walked)
are not changed.

## 0620: a contact off its face before the end

The patch 0620 comes from the round after (mitcad#343): a concave edge
between a revolve's torus and a narrow B-spline face rounded with 0.1 along
its tangent chain, on into the torus's edge with a planar sliver tangent
to the B-spline face. Near the chain's middle the B-spline face is
narrower than the rounding: its contact leaves the face across its convex
edge with another torus, and the ball rolls on that edge and on the
sliver's edge that continues it.

### 0620-chfi3d-contact-off-its-face-before-the-end-rolls-on-the-edge

The chain's one guide reaches both ends of the spine, so the walk's end
handling (`ComputeData`) took the edge the contact met for the end region
(it is a few edges from the end vertex, `IsObst`) and went on over the
B-spline's surface beyond the edge, unclassified, until the other contact
met the torus's edge with the sliver; the stripe stopped there, short of the
spine's end, and the corner at the end failed. `ChFi3d_ObstacleBeforeEnd`
now takes a contact on a convex edge of its face more than 10 `tolesp`
before the spine's end, whose face beyond does not hold the end vertex, for
an obstacle: the walk ends there, and `PerformSetOfSurfOnElSpine` goes on
with a piece rolling on the edge, as where a walk meets an edge in the
middle of a rounding. Edges of faces at the end vertex, concave or tangent
edges and edges at the spine's end are handled as before, and so is a
contact more than ten radii from the spine's point at its guide parameter
(mitcad#1883): beside a cone's apex at the spine's start (OCCT's
blend/simple H9, a revolve of 270 degrees whose cone's edge with the end
plane is rounded from the apex) the first section put the cone's contact on
its base edge round the far side, 280 from the spine, and the walk, taking
that edge for an obstacle, failed (`test_fillet_occt_cases.cpp`). Nor
does a chamfer's contact (mitcad#1901): a chamfer has no piece that rolls
on an edge (`ChFi3d_ChBuilder`'s `PerformSurf` on an arc is not
implemented), and OCCT's chamfer sequences dist_angle B3 B4 and
equal_dist B3 failed.

## 0650 and 0651: a flange under teeth and a thread's crests

The patches 0650 and 0651 come from the `.ipt` import's replay gaps
(mitcad#235): a flange's rim chamfered past the strip of its top up to the
roots of the teeth standing on it (a pulley's, a pinion's), and a thread's
crests rounded with 0.15 along all their turns.

### 0650-chfi3d-kpart-cut-by-a-long-chain-of-faces-beyond

The chamfer's cone runs under every tooth, whose seven faces (root and tip
roundings, flanks, tip) cut it and grow down to it (0055, 0071, 0142). The
chain of faces beyond from the one the contact leaves its face into to the
one it comes back from may be nine faces long (`ChFi3d_KPartCutChain`, four
before). Where no arc of the intersection between a cut's ends lies on the
face beyond (it grows to the cut), the arc whose coordinate along the spine
spans the least is the cut: the other arc of a root rounding's cylinder with
the cone runs round the cylinder's far side (`ChFi3d_KPartCutCurve`, the
first arc before); and ends on two lines of the intersection that meet end
to end, or through a third (it splits where a rounding tangent to the root
circle touches the cone), give the arc joined along them. For chamfers, a
gap whose ends are vertices of the contact's face (the contact runs along
the root arcs and leaves the face where they end) starts and ends on the
face's edges into the faces beyond there, which bound no face any more
(`ChFi3d_FillKPartGaps`; before, the chamfer failed, "Chaining is
impossible"). On a closed spine the first and the last SurfData are
neighbours across the spine's start, so a contact along an edge that the
start splits runs on along it (`ChFi3d_ContactsAlongFaceEdges`; before, the
edge was cut there, an open wire), and a piece of a spine of several edges
whose line runs across its period's end into the edge is kept (`SplitKPart`;
before, it was taken for a piece past the edge's end).

### 0651-chfi3d-walk-from-a-seam-and-extension-ending-where-it-fails

Each crest's strip of a turn is a face closed round its cylinder, its seam
where the thread starts. The walk past the spine's start starts where the
walk before stopped, the crest's contact on that seam at the end of the
face's parameters, and runs out across it: its first step left the face and
was halved until it failed ("Path failed"). It now starts a period on, on
the seam's other side, where the solution a little way on lies beyond the
face's bounds (`BRepBlend_Walking::Perform`). The extension past the spine's
ends (half the spine's length) walked on over the thread's run-out, where
the rounding vanishes and the guide nearly stops while the sections go on:
the surface over them had weights below zero or strayed far from them. A
step wholly past the spine's end whose surface fails so (it throws, or
strays from its sections by more than 100 times the approximation's
tolerance) fails as a step whose walk fails (`CallPerformSurf`), and ends
the extension where it started (`PerformSetOfSurfOnElSpine`; before, the
stripe failed). Steps within the spine fail the stripe as before.

## 0670 and 0671: a standing ball's sphere and a groove's end wall

The patch 0670 goes on with the `.ipt` import's replay gaps (mitcad#235): a
tab's top outline rounded with the radius of its rounded corners, the top a
steep cylinder across the tab.

### 0670-chfi3d-standing-ball-sphere-in-a-gap-between-pieces

Round each corner the ball's centre stays on the corner's axis, its contact
on the top at one point, and the rounding is a piece of the ball's sphere.
The walls' roundings are analytic and end where the ball reaches the corner;
the walk between them (its sections' mid-points in the guide's normal
planes, 0075) swept round the corner before the guide reached it, as the
guide climbs the steep top and its normal planes lean, and then found no
ball ("Path failed"). A gap between two pieces of the stripe whose piece
before ends, or whose piece after starts, with its contact on an edge into a
face on a cylinder of the ball's radius about an axis through the ball's
centre, the other contact inside its face (`ChFi3d_StandingBall`), now gets
the piece of that ball's sphere from that section to the one where the
contact's sweep round the corner reaches the corner's other edge
(`ChFi3d_StandingBallPiece`, its side on the top a degenerated edge of the
blend's face); the walk takes the rest of the gap, if any, from where its
sections put that ball (`ChFi3d_StandingBallSection`). Also the gap across
the period's end of a closed spine.

### 0671-chfi3d-kpart-chamfer-cut-across-its-width-by-faces-beyond-at-both-ends

A thread's groove in a bore ends in a flat face; the edge where that face
meets the bore is chamfered further than the groove is deep. The contact on
the flat face leaves it across the tip's edge, the contact on the bore
leaves it where the next turn begins, both before the edge's ends;
SplitKPart left one short piece and the corners at both ends failed
("OneCorner : bouchon non ecrit"). At each end of an open spine of a chamfer
that takes material off, `ChFi3d_KPartCutChainToEnd` now cuts the chamfer
from the end piece's section on by a chain of faces beyond (the tip's
rounding, then a flank), their edges prolonged past their vertices on the
flat face along lines, circles or their faces' isolines
(`ChFi3d_KPartEdgeCrossing`, `ChFi3d_ProlongEdgeAlongIso`), up to the face
whose surface meets the other contact at Q, where the chamfer ends in a
point. Q lies inside an edge of the other contact's face (that edge's part
from Q to the end vertex is replaced by a curve bounding its face beyond and
the grown face) or on its prolongation past the end vertex (the edge is
prolonged to Q). The edges under the chamfer at the end vertex bound no face
any more. `ChFi3d_KPartCutPiece`, `ChFi3d_MakeExtremities` and
`ChFi3d_FilDS` take an end in a point at a vertex of its own; such a stripe
is not computed again as a walked chamfer over a narrow face. MitCad's
chamfer joins the contacts' pieces at the vertices they leave between them
(`piece_vertices` in `geometry/src/dressup.cpp`): the files keep one edge
there, and OCCT's roundings of the crests beside them failed at those
vertices. Only where the contact leaves its face inside the end edge's own
range, more than 1 % of the chamfer's width from both of its ends
(mitcad#1898): at a box's corner the contact leaves its face at the end's
section, where the face at the end meets the edge, and OCCT's corner
handles it (OCCT's chamfer sequences A1 A4 A6 A8 C2 failed or came out
invalid); on a spine of several edges a contact leaving its face along the
edge before the end edge is no case for the end edge's pieces (OCCT's
bug354_2 failed).

## 0690: an end along the line of a seam

The patch 0690 goes on with the `.ipt` import's replay gaps (mitcad#235): a
bar's top edge rounded where the bar meets a bore's wall at the wall's
seam.

### 0690-chfi3d-end-on-a-face-along-the-line-of-its-seam

The rounding ends on the wall, whose seam runs up from the corner's vertex
along the line of the bar's side edge on the wall (the edge continues the
seam). The wall the rounding uncovers lies on both sides of that line from
the rounding's contact on the bar's side up to the vertex. PerformOneCorner
took the cut of the rounding's end a period away from the wall's domain and
nothing stood for the seam below the vertex: the wall's face was
unorientable and the solid's volume wrong (where the side edge kept both
curves of the seam it was split off, the OCC32 pseudo-seam code made its
piece a seam with its curves the wrong way round). Where one point of a cut
stored whole lies on such an arc (`ChFi3d_ArcAlongSeamLine`), the cut's
curve on the face is now put in the period of the other point's arc, and the
arc's part from its point to the vertex is stored on the face twice, with
the arc's curve and translated by the period: a piece of the seam.

## 0691 and 0692: a recess's rim up to a rib

The patches 0691 and 0692 go on with the `.ipt` import's replay gaps
(mitcad#235): a recess's rim on a part's bottom rounded up to a rib across
the recess whose bottom is the part's bottom, the rib's bottom edges
rounded up to 45-degree slopes.

### 0691-chfi3d-onsame-end-on-a-tangent-face-the-crossing-nearest-the-corner

Each half of the rim's rounding ends on the rib's rounding, which continues
the bottom (the opposite face) tangentially along the prolonged edge
(OnSame). The contact on the bottom, a circle round the recess's axis, only
touches the rounding's surface, at both ends of the half, and
`IntersUpdateOnSame` took the far end's touch, a diameter away. Where the
face at the end is tangent to the opposite face along the prolonged edge
(either way round), the contact's crossing with that edge is now computed
too and the point nearer the corner's vertex taken.

### 0692-chfi3d-kpart-contact-end-within-a-vertex-tolerance-at-the-vertex

The rim's contact on the recess's cone leaves the cone exactly through the
vertex where the rib's rounding meets its slope. In a part's own body the
edge's curve ends a little past that vertex, within its tolerance, and the
hatching found the contact's end inside the edge: a sliver of the edge was
left, and the cone's wire intersected itself. `FillSD` now puts an end found
inside an arc within the tolerance of one of its vertices at that vertex
(`SnapToVertex`).

## Reports and tests

- 0550: mitcad#343 and the bar on a cylinder rounded where the cap's edge is
  in two pieces in `geometry/tests/test_fillet_corners.cpp`.
- 0580 and 0581: mitcad#343 and the pocket rounded over ribs up to flush
  blocks in `geometry/tests/test_fillet_corners.cpp` (the volume added
  against the rounding's section).
- 0600: mitcad#343 and the plate with an overhanging tongue in
  `geometry/tests/test_fillet_corners.cpp` (the tongue's left edge rounded
  so that the contact runs off it, against a tongue wide enough for the
  contact).
- 0620: mitcad#343 and the wall on a floor that narrows before the end in
  `geometry/tests/test_fillet_corners.cpp` (the volume added against the
  full rounding's section and the rolling ball's).
- 0650: mitcad#235 and the flange under a ring of teeth in
  `geometry/tests/test_chamfer_overflow.cpp` (its rim chamfered a little
  past the teeth's roots and exactly up to them, by distance and angle and
  by two distances, its top circle in one or two edges: the volume against
  the revolved section less the teeth extended down).
- 0651: mitcad#235 and the cylinder under two tilted planes in
  `geometry/tests/test_fillet_kernel.cpp` (the rounding of one arc of its
  top, walked past its end from the cylinder's seam: the stripe is made).
- 0670: mitcad#235 and the boss topped by a steep bowl in
  `geometry/tests/test_fillet_results.cpp` (its top's outline rounded past
  corners of the ball's radius: each corner one face on the ball's sphere,
  its area the spherical triangle's).
- 0671: mitcad#235 and the groove's end wall chamfered past its flanks in
  `geometry/tests/test_chamfer_overflow.cpp` (three reaches: the removed
  volume against the chamfer's wedge within the groove carried on past the
  wall; one edge between the chamfer and the top through MitCad's chamfer).
- 0690: mitcad#235 and the bar across a tube in
  `geometry/tests/test_fillet_results.cpp` (its top edge rounded into the
  bore at the bore's seam, as fused and copied: the volume and faces of the
  one with the seam turned away).
- 0691 and 0692: mitcad#235 and the disc whose recess a rib crosses in
  `geometry/tests/test_fillet_results.cpp` (its rim rounded up to the rib:
  the ends at the contact's crossings with the rib's tangent lines; with the
  edges at the rib's roundings carried past their vertices within
  tolerance, the result of the exact body).

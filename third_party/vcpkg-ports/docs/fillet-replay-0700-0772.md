# Fillets of stored designs: 0700–0772

Roundings from the eighth and ninth rounds of mitcad#343 and the `.ipt`
import's replay gaps (mitcad#415), built as the bodies stored in the files
have them. The earlier rounds are in [0550 to 0692](fillet-replay-0550-0692.md),
the later ones in [0790 to 0822](fillet-replay-0790-0822.md). Back to the
[port's README](../README.md).

## Overview

- **0700** joins two stripes whose balls are one ball rolling over a narrow
  face between them into one blend that face cuts (a notch in the blend's
  side), drops the stripes under such a ball, and cuts a hole in a blend a
  face of the solid pokes through, that face keeping its part outside the
  blend (mitcad#343).
- **0720** drops a face at a rounding's end vertex that the end passes
  wholly beyond, the face beyond it cut once, and **0721** prolongs an
  OnSame end's edge as its own curve where that lies on both faces'
  surfaces (a circle of two coaxial surfaces across a cone's seam;
  mitcad#343).
- **0740** gives a rounding's end on a face at the end split round the
  vertex into faces of one surface, each with its own surface, the curves
  its arcs lack on the piece the cut goes on (a bar's top edge rounded into
  a bore whose wall is three faces round the corner); **0741** ends an
  analytic blend's contact that leaves its face at a vertex on the edge of
  the face from the spine's end vertex also where that edge's curve stops
  short of the vertex (the recess's rim of 0691 and 0692); and **0742**
  extends a blend's guide past an open end of its spine along the curve the
  end edge is a piece of (a thread's root rounded up to the faces that cut
  it; mitcad#415).
- **0750** rounds both edges of a thread's crest narrower than the
  roundings with one blend between the flanks, cut by the faces at the
  thread's ends along their long oblique curves: a guide along a helix and a
  B-spline face swept along one are continued by their screw motion past
  their ends, a face at the end met at a kink of a degree cuts the blend,
  and a stripe computed over the crest that runs out at both ends takes the
  other's place (mitcad#343).
- **0770** rolls a rounding on an edge that runs into its end vertex tangent
  to its other face up to that vertex, where it vanishes, and **0771** stops
  a contour at a vertex where the faces of both its edges come tangent, each
  side's rounding vanishing there (a block on a post as wide as the block,
  mitcad#343).
- **0772** joins two stripes on the same face and on one plane whose edges a
  corner of the solid interrupts into one blend over the corner, cut by the
  corner's sides, which grow up to it (a bulge's foot on a housing's rounded
  corner, mitcad#343).
- **1060** keeps 0700's stripes under another one's ball to the region that
  blend fills: an edge beyond the blend's other face, where that face ends
  at a convex corner and the blend runs on past it for the corner, is not
  under it, and its own rounding stays (a slot's floor flush with a ledge in
  front of the wall, mitcad#840).

## 0700: blends cut by a narrow face they roll over

The patch 0700 comes from the eighth round of mitcad#343: a big torus's
edges with a plane rounded with 0.5, the chain broken by two narrow strips
of a boss (a cylinder standing on the plane) between the torus and earlier
roundings of 0.6 (tori tangent to the plane), onto which the plane's
contact runs. The bodies stored in the file have one rounding face over
each strip: the ball keeps touching the torus and the earlier rounding,
the strip's edge with the torus lies under it, and the boss cuts the
rounding, of the strip only a lens being left.

### 0700-chfi3d-blends-cut-by-a-narrow-face-they-roll-over

A new file, `ChFi3d_Builder_Trim.cxx`, and two calls in `Compute`.

- `JoinOverNarrowFaces`: two stripe ends of one radius on the same two faces
  Fo and N, the contact on N of one of them ending on an edge of a narrow
  face C that holds both ends' vertices, and no other stripe at those
  vertices but stripes whose edges all lie between Fo and C (under the ball,
  dropped). The first end's blend is walked again from a section some radii
  before its end (or from its end piece's start), N's surface unbounded,
  along a guide through the ball's centres of the two ends' pieces (past the
  spine's end A's own guide runs straight on and its sections slide along
  the ball's path, `ChFi3d_CutSingularExtension` then cut the walk back):
  its contact leaves N across an edge of C and comes back across one. The
  cut of the walked blend by C between those points (`GeomInt_IntSS`, its
  lines joined end to end) may reach back along the blend past both points
  (item 45: 0.59 and 0.62 past points 0.48 apart, the cut 3.80 long): the
  walked blend is three pieces on one surface, one face for `ChFi3d_FilDS`
  (as 0581's joined pieces), the middle one's side on C the cut (as
  `ChFi3d_KPartCutPiece`'s, whose pieces hold one span a section and could
  not hold it). The other end's piece is cut back to the section of the same
  ball beyond the cut, and the two ends meet there as free ends with a
  common section edge (as `ChFi3d_JoinAcrossNotches`); A's end piece is cut
  back to the walk's start. Before, each end got a corner at its vertex of
  C, which failed ("Failed to find edge"); analytic ends (a cylinder between
  two planes) are walked the same way. It runs before the stripes' contacts
  on a common face are taken for one (`myRideStripes`): an analytic end's
  contact runs on to the end of its face for the corner.
- Stripes under another's ball: a stripe whose edges all lie between Fo and
  a narrow face C, along which another stripe's contact on Fo runs within
  one of its pieces (on Fo and a face other than C), on the side of that
  contact its blend covers, is dropped; not where the ball touches C (a wall
  beyond a rib the rounding is joined over goes on from the blend's other
  face: its own short stripe stays).
- `PierceBlends`, once a stripe is in the data structure: where an edge of
  Fo with a face C lies under the blend, C's surface cuts a piece's surface
  in a closed curve inside it, and every edge of C lies under the blend (its
  points beyond the blend's surface from its ball's centre), the curve is a
  hole of the blend's face and the whole edge of C's lens (the part of C
  outside the blend); C's edges are cut at their vertices so that the
  builder drops them. Before, the blend ran through C.

## 0720 and 0721: corners and ends on the design's own stored bodies

The patches 0720 and 0721 come from the corners and rounding ends of
mitcad#343 that a design's history fails on its own stored bodies (the
patches 0700 to 0719 are that ticket's notched blends).

### 0720-chfi3d-face-at-the-end-passed-wholly-beyond-is-consumed

A housing's corner edges rounded with 20, each ending at a vertex of a plane
square to it. A cylinder standing out of that plane leaves of it a small
piece at the vertex (two sides on the rounded edge's faces, the third on the
cylinder) and the pieces beyond; the rounding's contacts pass the small
piece and end on the pieces beyond, and its end is cut by them and by the
cylinder between them. The walk round the end vertex
(`PerformIntersectionAtEnd`) met a piece beyond, the cylinder, the small
piece, the cylinder again and the other piece beyond: the cylinder was cut
twice, both times up to its edge with the small piece (outside it), and the
small piece was kept (the shell unorientable). Now a face between two visits
of one face across the same edge, holding the end vertex, whose edge with
that face the rounding does not cross (its surface about the end, also
widened along the spine), lies in the removed corner: it leaves the faces at
the end with the second visit, and its edges are cut at their vertices so
that the builder drops it.

### 0721-chfi3d-onsame-prolonged-edge-own-curve-on-both-surfaces

An external thread's flank rounded with 0.1 along its edge with the chamfer
cone at the thread's start, where the flank comes out of the root cylinder:
an OnSame end, the cone's edge with the cylinder (a circle of both, coaxial)
prolonged past the vertex up to the rounding's contact on the cone, across
the cone's seam. OCCT intersected the cone and the cylinder for it: the cone
is nearly flat there, the intersection's p-curve ran the long way round the
cone and its 3d curve strayed 0.1 off the circle; the seam was not cut and
the cone's face lost its seam and its edge with the cylinder.
`ChFi3d_ProlongTangentEdge` (the edge's own curve continued, so far for
faces tangent along it) now also takes faces not tangent there whose
surfaces both hold the curve continued (within 10 `tolapp3d` at three
points), tried before the surfaces are intersected; and its p-curves are
parametrized as the 3d curve also when it runs from the edge's last vertex
on a periodic curve (the trimmed circle's parameters come into its first
period, the p-curves kept the edge's: the pieces split at the seam had
p-curves a period off on the face beside). Split at the opposite face's
seam, such a prolongation's pieces take the crossing in the curve's own
range and their own p-curves, each moved by whole periods into the face's
domain (projected anew, the piece from the vertex lay a period off: a rib's
foot on a rod, rounded across the rod's seam,
`test_rib_foot_rounded_across_the_rods_seam`). Where the prolongation is
still the surfaces' intersection (the edge's curve an approximation ending
at the vertex, as on the replay's modelled thread), its ends on the periodic
opposite face are taken in one period (`ChFi3d_Recale`): a period apart, the
intersection ran the long way round.

Corrected by mitcad#1883 after OCCT's own cases: the edge's curve is not
continued where a p-curve's end lies past its surface's parameters in a
direction that is not periodic (an ellipse's meridian continued past the
axis, through the pole of its surface of revolution: OCCT's blend/simple
H3, invalid), the ends are taken in one period only on an opposite face
that goes round the whole period (on a revolve of 270 degrees the point at
the pole moved off the face), and a p-curve that already has the 3d
curve's range (a circle's p-curve that is a circle, reversed as the circle
is) is no longer made a B-spline, whose other parametrization put its
points off the circle's at equal parameters and raised the edge's
tolerance to 1.5e-4 (OCCT's bugs/modalg_7 bug28828_16, a throat chamfer,
allows 1e-4; now 8.7e-5) (`test_fillet_occt_cases.cpp`).

## 0740 to 0742: a bore's wall in three faces, a recess's rim, a thread's root

The patches 0740 to 0742 go on with the `.ipt` import's replay gaps
(mitcad#415, after mitcad#235's 0690 to 0692).

### 0740-chfi3d-end-face-split-round-the-vertex-pieces-of-one-surface

A bar's top edge rounded where the bar meets a bore's wall, the wall round
the corner's vertex three faces of one cylinder (over the bar's top, beside
the bar below it and beside it above), each on its own cylinder object, as
booleans that keep faces of different origins apart leave it.
`PerformIntersectionAtEnd` walks round the vertex through the three pieces
and, the pieces being tangent along both edges between them, puts the cut on
one of them (`oneintersection`): the part of the contact's arc on another
piece from the contact to the vertex becomes the edge between the two. Its
curve on the piece with the cut, and the arc's point there (`Hc1`, `Hc2`),
were asked of the arc, which has curves only on its own faces' surfaces:
null where the pieces do not share the surface object, the edge without a
curve on that face and the point taken at (0, 0), the face invalid or the
cut along a wrong curve. `ChFi3d_ArcOnSameSurface` now computes such a curve
(the edge's curve projected on the face's surface, in the face's period at
the vertex, checked to lie on it) where the arc has none.

### 0741-chfi3d-kpart-contact-end-at-a-vertex-on-the-edge-to-the-spines-end

The recess's rim of 0691 and 0692 rounded up to the rib, the edges between
the recess's cone and the rib's roundings with curves that stop short of
their vertices with the slopes within those vertices' tolerance (as an
imported body's may). The rim's contact on the cone leaves it exactly at
such a vertex; the hatching found the end there only on the cone's edge with
the slope, `FillSD` put it on that edge, and the corner (OnSame) took the
slope for the face at the end instead of the rib's rounding: the cut ran
across the bottom and the result was invalid. An end found at a vertex of an
arc that does not hold the spine's end vertex is now on the face's arc that
holds both, at the vertex, as when the hatching finds both arcs there.

### 0742-chfi3d-guide-past-an-open-end-along-the-end-edges-own-curve

A thread's root (the helical edge between its flanks) rounded up to the
faces that cut it at its ends, as the `.ipt` import's rounding of an
internal thread's root running out past a blind bore's bottom (an item of
`d9aec91e`). The walk starts and ends some way past the spine's open ends
along the guide (`ChFi3d_PerformElSpine`), which
`GeomLib::ExtendCurveToPoint` extended there smoothly towards a point far
along the end tangent; on a helix of small lead that leaves the root within
millimetres, the walk's contacts swerved, and the faces at the ends were cut
along curves joined by vertices with tolerances of up to tens of millimetres
(14 mm in that part, where a later cut then merged a circle's vertex 12 mm
away into one and a rounding of that circle failed). The guide now goes on
along the end edge's own curve where that is a B-spline going on past the
edge, else along the iso-line of its face's B-spline surface the edge runs
on, where either goes on far enough (`ChFi3d_OwnCurveExtension`,
`ChFi3d_IsoCurveOfEdge`; the guide's own part keeps its parameters); else
the smooth extension continues from as far as one goes.

## 0750: a thread's crest rounded on both edges

The patch 0750 comes from the ninth round of mitcad#343: an external
thread's crest (a cylinder band 0.8 wide, three turns) rounded with 0.8 on
both its edges. The two roundings' contacts on the crest cross, and
OverlappingStripes (0013) computes each again overflowing the crest, the
ball touching both flanks; the bodies stored in the file have that one
blend, the crest consumed. At the thread's ends a plane square to the axis
and a chamfer cone at 60 degrees meet the helix at about a degree and cut
the blend along curves 83 long, one flank's contact running on 17 past the
end of its edge, the other's meeting the face 65 before.

### 0750-chfi3d-thread-crest-rounded-on-both-edges-cut-by-the-ends

- The guide of the walks past a spine's end follows the end edge's own curve
  or its face's iso-line as far as these go on (0742), then turns towards a
  far point along its end tangent (`ChFi3d_PerformElSpine`); the crest's
  helices and the flanks' iso-lines stop about where their faces do. Past
  the end of a helix the sections turn away from the ball's path (more than
  a radian 60 past a helix of radius 28) and `ChFi3d_CutSingularExtension`
  cut the walk back before the contacts reached the face at the end. A guide
  that a screw motion maps onto itself, within a ten-thousandth of its
  length (the edges' approximation), is now continued by that motion where
  0742's extension does not reach far enough, by a turn at most
  (`ChFi3d_ScrewContinuedCurve`: a copy of its end moved by the motion,
  joined with a knot of multiplicity the degree, the guide itself
  unchanged).
- The walks' face surfaces (`ConexFaces`, `StartSolOverflow`): a B-spline
  surface that a screw motion sweeps along one of its parameters (a thread's
  flank; a surface of revolution) is evaluated as continued by that motion
  past both ends of that parameter, by a turn or its own length at most
  (`ChFi3d_ScrewContinued`, `ChFi3d_LoadScrewContinued`: its own poles and
  knots over its range, moved copies of its ends beyond, so that the
  contacts' p-curves are the face's). The flank whose contact runs on past
  the spine's end is stored ending about where its face ends; past its
  bounds the walk ran on the end spans' polynomials and lost the ball.
- A contact that leaves its face onto a face that holds the vertex at that
  end of the spine, the two faces' normals along the edge nowhere within the
  builder's angular tolerance but within the tenth of a radian `SearchFace`
  takes for a face continued (a flank running into the chamfer cone at 0.8
  degree), now ends on that face, which cuts the blend (`KinkIntoEndFace` in
  `ComputeData`). Before, the next walk rolled the ball on the cone.
- Of two stripes computed again over the face between their edges
  (`myOverlapStripes`), one that is a single piece on the two faces beyond
  that face, both its contacts ending on edges at both its ends, takes the
  other's place, which is dropped with its corners
  (`DropOverlapsRunOutByOther`). The two blends are one, but each stripe may
  run out to the faces at the ends at one end only, or its start section lie
  elsewhere, so that `RemoveCoincidentStripes` kept both and their corners
  failed.
- A start section of a stripe computed again over such a face is taken only
  where the ball touches the face across the other stripe's edge and its own
  edge's other face (`StartSolOverflow`; near the face's end a section ran
  on to the thread's root and the end plane), and the test whether the two
  contacts overlap (`contactBeyond`) samples the contact at its
  interference's parameters and projects on the other within its range (on a
  contact line of several turns a projection landed a turn away, its ends'
  points off the line where the contact left its face).
- Walks of more than three turns of a thread still fail where their
  surface's approximation fails ("Failed approximation"), not split.

## 0770 to 0772: a block on a post and a housing's corner

The patches 0770 and 0771 come from the ninth round of mitcad#343: a block
on a post as wide as the block (the post's top circle touches the block's
long bottom edges at two points, where the block's underside narrows to
nothing and the post's side is tangent to the block's long sides), the
post's foot rounded with the post's radius, then the block's bottom outline
rounded over that rounding.

### 0770-chfi3d-rolling-on-an-edge-into-the-end-vertex-where-it-vanishes

The foot's rounding: its contact on the underside runs off across the long
edges, and from there the ball rolls on those edges, touching the post, up
to the two points, where the edges run into the post's top tangent to its
side and the rounding vanishes. OCCT took those edges, which end at the
rounding's end vertex, for no obstacle (an arc at the end vertex was the
face at the end's: `StartSol`), ended each side where the contact left the
underside and extended it by tangent planes; the corners at the points
filled the rest wrongly. An arc into the end vertex tangent to the blend's
other face (`ChFi3d_ArcTangentToFaceAt`, within 0.01 rad) is now an
obstacle, rolled on up to the vertex. Several roundings that all vanish at
one vertex each end in a point there (`ChFi3d_ShrinkVanishingEnd`, so far
for a rounding ending alone there: the faces' normals taken at the contacts'
ends, as a surface may degenerate at the vertex, an end of coinciding
contacts short of the vertex moved too, the end parameters matched within a
thousand times the parametric confusion, and the contact curves cut by full
knots where they are twice the end's distance from the vertex before their
end poles are moved, so that only that last piece bends), and the corner is
the singular one.

### 0771-chfi3d-contour-stops-where-the-rounding-vanishes

The bottom outline: along each long side its edge runs over the foot's
rounding, whose face comes tangent to the side at the point (the foot's
rounding ends in a point there, its surface degenerate at that corner). OCCT
carried the contour on through the point (the edges continue each other
there); its walk crawled into the point, or its last section, the singular
one given at the point, took parameters on the degenerate face's iso far
from the contact's, and the approximation between failed or wandered
(p-curve tolerances of tens of millimetres). The faces' normals at a vertex
where a surface has none are now their limits along the edge
(`TangentExtremity`), the contour stops at a vertex where the faces of both
edges come tangent (`PerformElement`), a walk into such a vertex also ends
where its section has no width within a hundredth of the radius
(`ContactsAtTangentEnd`), and the line's end there is solved again from its
last section at least 5e-4 of the radius wide, each section from the one
before (its step on the faces no longer than the walk's before), until
narrower within a hundredth of the radius of the vertex
(`ChFi3d_EndBeforeVanishing`); the corner moves that end to the vertex
(0770). The contour stops only where the faces come tangent within 5e-4
rad, the width relative to the radius at which that walk ends (the block on
the post: within 7e-5), not at any vertex within the angular tolerance
(mitcad#1883): two cylinders of one radius whose axes miss each other by
1e-3 cross at 0.008 rad where their intersection curves come nearest
(OCCT's blend/bfuseblend B7), and the rounding of those curves, stopped
there, failed; it walks through such a vertex as before (`test_fillet_occt_cases.cpp`).

### 0772-chfi3d-stripes-joined-across-a-corner-of-the-solid

From the same round, a housing whose top is a plane with a bulge (a tilted
cylinder) on its rounded vertical corner, the bulge's foot on the top
rounded. The foot is two edges, each from inside the top to the corner's
rounding, reached just past its tangent lines; the ball of each rounding
rolls on between the top's plane and the bulge past that end, its contact on
the plane leaving the top across the first side's edge and coming back
across the second's. OCCT ended each stripe at its edge's end, and the
corners there filled the gap wrongly. `JoinAcrossCorners` (after
`JoinOverNarrowFaces`) now pairs such free ends (one constant radius, at
most 8 radii apart, the last pieces on the same face and on faces of one
plane, their vertices on a third face, the corner) and walks one blend from
the middle of the first stripe's last piece along a guide through both last
pieces' ball centres, the plane unbounded. The edges the contact crosses
give the sides; the edges between the sides and the corner's face, run on,
meet the blend at the junctions; the blend is cut by the three faces between
those points (their surfaces' intersection, interpolated with its p-curves)
and split into five pieces up to a section inside the second stripe's last
piece, where the two stripes share an edge. Where the contact crosses the
sides' edges, each edge keeps its part beyond the blend (the point's
transition set by the side its vertex on the corner's face lies on, as the
walk may run either way along the spine). The edges between the sides run on
to new vertices at the junctions, and the corner face's edges now under the
blend are dropped.

## 1060: an edge beyond the blend's other face is not under its ball

The patch 1060 comes from a FreeCAD document's fillet (mitcad#840): a wall
with a slot whose floor is flush with the top of a ledge in front of the
wall, both concave edges at each corner where the slot's side wall meets
the wall's front face rounded with 1 (the slot floor's edge along the side
wall, and the ledge's edge along the wall's front, 2.25 long), the convex
edge between the side wall and the front left sharp. The bodies stored in
the file have both roundings running on past the corner by the radius, the
two quarter cylinders cut by each other.

### 1060-chfi3d-under-anothers-ball-not-past-its-other-face

0700's stripes under another one's ball took every point of a stripe's
edges within 4 radii of the other blend's contact on the common face, on
the side of that contact toward the blend's other contact, for a point
under the blend. The slot floor's rounding runs on over the ledge's top for
its corner (its analytic piece's contact up to the end of the face), and
the ledge's edge lies within 1 to 3.25 of that contact, on its side toward
the side wall: its stripe was dropped, its rounding missing from the
result, and the slot's rounding ended at the wall's front face by its
one-stripe corner. The material added came out 1.349 mm3 short of the
stored body's at the design's four such corners.

- `edgesUnderBlend` (`ChFi3d_Builder_Trim.cxx`): a point counts only where it
  lies on the ball's side of the blend's other face's tangent plane at the
  other contact (the ball of the section through the point's projection on
  the contact, `ballAt`; within a hundredth of the radius). The region a
  blend fills lies between its contacts and the corner of its two faces; a
  point past the other face's plane, as the ledge's edge past the side
  wall's plane, is outside it. The edges of a narrow face under a ball rolling
  over it (0700's case) lie between the contact and the faces' corner, on
  that side.

With 1060 both stripes stay, meet at the corner in the two-stripe corner
(each blend cut by the other), and the fillet on the design's body before
the item comes out valid with the stored body's volume (180282.1201 mm3)
and its rounding faces' areas.

## Reports and tests

Each of these tests fails without its patch.

- 0700: mitcad#343 and the corner rounded over a cap of a ball standing out
  of it in `geometry/tests/test_fillet_pierced.cpp` (the cap notching the
  rounding, with and without the cap's edges with the wall rounded too, and
  poking through it: the volume and the cap's area left against the plain
  corner's rounding with the ball fused to it).
- 0720 and 0721: mitcad#343 and the bar with a tilted rod standing out of
  its end (the bar's edge rounded past the end's corner piece: valid, the
  volume removed against the bar's and the rod's within the rounding's
  corner) and the post with a rib under a chamfer cone (the rib's side
  rounded along the cone just short of the cone's seam and turned away from
  it, the post a cylinder or a spline: the same cone face and, on the
  cylinder, the same volume removed) in
  `geometry/tests/test_fillet_corner_cases.cpp`.
- 0740 and 0741: mitcad#415 and, in `geometry/tests/test_fillet_results.cpp`,
  the bar across a ring of two layers whose bore's wall is in three pieces
  round each corner (the bar's top edges rounded: valid, the pieces kept
  apart, the volume removed that of the wall in one face) and the disc
  whose recess a rib crosses with the edges at the rib's roundings stopped
  short of their vertices (the result of the exact body).
- 0742: mitcad#415 and the ring whose bore is threaded from below it to
  above it, the thread's root rounded up to the ring's faces, in
  `geometry/tests/test_sweeps.cpp` (valid, its vertices within 1e-3, the
  volume added and each face's gain against the section the ball fills).
- 0750: mitcad#343 and the thread whose crest is rounded on both edges
  between end planes in `geometry/tests/test_fillet_thread_crests.cpp`
  (valid, the crest consumed, and a turn of thread more taking away the
  section's volume swept round the axis at its centroid).
- 0770 and 0771: mitcad#343 and the block on a post as wide as the block in
  `geometry/tests/test_fillet_lens_overflow.cpp` (the post's foot rounded:
  valid, its quarters alike, the material added between the torus within
  30 degrees of the block's axis and the whole torus; then the block's
  bottom outline rounded: valid, the two points kept, its quarters alike).
- 0772: mitcad#343 and the bulge on a housing's rounded corner in
  `geometry/tests/test_fillet_lens_overflow.cpp` (valid, the junctions at
  the height of the blend within 1e-4, nothing beyond the first side above
  the top, the material added alike on both sides of the corner's bisector;
  and with the two edges given in the other order, the blend walked the
  other way).
- 1060: mitcad#840 and the wall with a slot whose floor is flush with a
  ledge's top in `geometry/tests/test_fillet_corner_past_wall.cpp` (the
  slot floor's edge and the ledge's edge along the wall rounded at the
  convex corner left sharp: valid, the material added and each rounding's
  face against the two quarter cylinders running on past the corner into
  each other, with OCCT alone and by name).

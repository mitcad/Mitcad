# Fillets of stored designs: 0790–0822

A thread's run-out (mitcad#422) and the tenth round of mitcad#343: the
traits of the bodies stored in files, a housing's corner and a key on a
shaft. The contour fix 0840, found after the ninth round, is in
[walks and contours](fillet-walks-and-contours.md#0840-chfi3d-contour-never-takes-its-own-edge-again).
The earlier rounds are in [0700 to 0772](fillet-replay-0700-0772.md), the
later ones in [0870 to 0911](fillet-replay-0870-0911.md). Back to the
[port's README](../README.md).

## Overview

- **0790** ends a rounding whose edge runs on tangentially into an edge not
  rounded, the face at the end going on tangentially from the rounding's
  face, where that face's surface, followed back along the rounding, cuts
  it across, a face on that surface closing the end (a thread's crest
  rounded down to its run-out; mitcad#422).
- **0800 to 0803** take the traits of stored bodies into OCCT's fillet: an
  OnSame end's cap points brought into one period however many periods
  apart (a cylinder trimmed turns below its basis's parameters), faces taken
  for tangent within seven times the tolerance of their edge, a contact ending
  on the spine's own edge near the vertex where the spine goes on passed
  through that vertex, and the point where a cap leaves the prolongation
  tangentially given a tolerance that takes in their crossings near it (a
  thread's chamfer and a groove's floor, mitcad#343).
- **0820** makes the blend 0772 joins across a corner one face per stripe,
  as the stored designs' roundings are.
- **0821** rounds both edges of a lens (a face of two edges, one concave and
  one convex, narrower than the roundings) with one S-shaped section, the
  convex blend's ball touching the concave blend's from outside.
- **0822** rolls a rounding on both edges of such a lens into its ends,
  where the edges meet and the rounding vanishes in a point (a key on a
  shaft, mitcad#343).

## 0790: a thread's crest rounded down to its run-out

The patch 0790 goes on with the `.ipt` import's replay gaps (mitcad#422,
after mitcad#415).

### 0790-chfi3d-tangent-end-face-cuts-the-blend-back-along-the-stripe

A thread's crest (between the post's cylinder and the groove's lower flank)
rounded down to the thread's run-out, a straight cut whose lower flank is
the plane tangent to the groove's flank along the start profile. The crest
runs on there into the run-out's crest, which is not rounded: an OnSame end
whose face at the end goes on tangentially from the flank. Its plane only
touches the rounding near the vertex; OCCT's corner cut the rounding there
by that touch, running it on down to the run-out. The bodies the `.ipt`
files store end such a rounding where the plane cuts it across a turn back
(331 degrees up, where the crest rises out of the plane again), a face of
0.006 mm² on the plane closing it and the crest staying sharp from there
down. `PerformCapAtTangentEndFace` (before the other ends of a lone stripe
at a vertex of three edges): where the end is OnSame and the face at the end
goes on tangentially (G1, across an edge at the vertex) from a face of the
spine's edge, each contact is followed back from the end to where it first
passes from that face's solid's side of its surface to the outside; where
both do so on one surfdata more than four radii from the vertex, the
surfdatas past the cut are dropped, the last one ends on the curve where the
surface cuts it, and a corner stripe on that surface closes the end up to
the point where the surface crosses the spine's edge, which is cut there;
the spine's end becomes a free one. A cut whose two contacts fall on two
surfdatas (the stripe split there, as at a face's seam) is not made: the
corner at the vertex is tried as before. Elsewhere nothing changes (the end
of the same thread's other crest at the run-out, whose flank's plane never
cuts its rounding across, keeps its corner at the vertex, as the files'
bodies have a corner patch there).

## 0800 to 0803: traits of the stored bodies

The patches 0800 to 0803 come from the tenth round of mitcad#343: the
traits of the bodies stored in files that kept OCCT's fillet from the
roundings round a thread's chamfer cone and up to a groove's floor.

### 0800-chfi3d-onsame-cap-points-in-one-period-of-a-stored-cylinder

A thread's flank rounded round its chamfer cone from where the flank comes
out of the root cylinder: an OnSame end whose face at the end is that
cylinder. Stored bodies hold such a cylinder trimmed to one period starting
turns below the basis surface's parameters (u from -11.82 to -5.54), the
edges' curves on it in that range; the cap's point on the arc came from the
arc's curve (u = -11.824), the opposite contact's point from its projection
on the surface (u = 0.743), two periods apart. `ChFi3d_Recale` moved one of
them by one period, so the box for the blend's intersection with the
cylinder spanned a period beside the cap, which was not found ("echec calcul
intersection"). It now brings the point within half a period of the other
however many periods apart (for points under one and a half periods apart
nothing changes).

### 0801-chfi3d-tangent-faces-within-ten-times-the-edge-tolerance

`ChFi3d::IsTangentFaces` compared the points of the edge's curves on the two
faces within max(0.001, 1.5 times the edge's tolerance); each curve may lie
within the tolerance of the edge's own curve, so the two up to twice that
apart, and the approximated helices of stored threads have curves on their
faces up to 4.8 micrometres off their own (tolerances of 1.4 to 3.7
micrometres). A crest band and the roundings beside it, tangent within 1e-3
radian, were taken for faces meeting at an angle: a rounding round the
chamfer cone stopped its contour at those edges' vertices (`FaceTangency`),
three roundings meeting end to end that walked past each other, and a walk
across such an edge stopped there (`IsG1` in `SearchFace`). The positions
are compared within seven times the edge's tolerance now (at least 0.001; the
normals within a tenth of a radian as before): a curve 4.8 micrometres off on
an edge of 1.4 is 3.43 tolerances off, two such curves 6.86 apart (mitcad#1986;
it was ten times). Test: `geometry.port.0801`.

### 0802-chfi3d-contact-on-its-own-edge-near-a-vertex-passes-it

That rounding, one contour now, runs along the cone's edges with an earlier
rounding of the crest and then with the flank, which goes on tangentially
from it across an edge ending at the vertex V between the contour's two
edges. Cone and rounding meet at 1.7 degrees at V, so the blend is 0.015
wide there, its contacts 7.5 micrometres from the edge; the stored edges lie
up to 4 micrometres off their faces. The contact on the rounding met the
edge with the cone itself 0.034 before V and the walk ended there
(`SearchFace` found the cone beyond). A contact point on the spine's edge
between the contact's face and the blend's other face, within ten times its
tolerance or its vertex's of the vertex ahead where the spine goes on into
another of its edges, a face tangent to the contact's face holding that
edge, is taken at that vertex with the contact's way as its vector
(`ChFi3d_OwnEdgeNearVertex` in `ComputeData`): the walk goes on through the
vertex into that face (`SearchFaceOnV`).

### 0803-chfi3d-onsame-cap-leaving-the-prolongation-tangentially

A thread cut as a groove, its crest's edges rounded up to the groove's
floor. Each rounding ends where its contact on the crest's cylinder meets
the floor, the floor's edge with the cylinder prolonged up to there; the
floor cuts the rounding along a cap that leaves that circle tangentially and
runs inside it a few micrometres off for a while. The cap's approximation
crossed the circle 0.053 from their common point (its tolerance 5e-5, the
circle's 1e-7): the floor's wire was self-intersecting. Where the two meet
tangentially at that point (within a hundredth of a radian), their crossings
within a hundredth of the cap's length are added to the box the point's
tolerance is made from (`ChFi3d_TangentCapCrossings`).

## 0820 to 0822: a housing's corner and a key's lenses

The patches 0820 to 0822 come from the tenth round of mitcad#343: the
housing's corner of 0772 again, and a key on a shaft (a cylinder standing
out of the shaft's side), each end of the key a lens between the key's
convex arc and the shaft's concave arc.

### 0820-chfi3d-stripes-joined-across-a-corner-one-face-each

0772 walked the joined blend from the middle of the first stripe's last
piece to a section inside the second's, so that each corner had three faces
(the first piece's rest, the walked blend, the second piece's rest), and the
walked face was generated by no edge. The stored bodies have two, one of
each edge, the first's holding the cut by the first side and the second's
the cuts by the corner's face and by the second side; a later rounding of
the sides' edges names its edges by those faces. Now the walk runs from the
first stripe's last piece's far end (the other face's surface taken within
its own period about the face, else widened by its size and ten radii, as
the contacts at the pieces' far ends lie past their faces) to the second's
far end; the first stripe takes the walk's pieces up to the section through
the first side's junction, the second the rest, on a copy of the walked
surface, its lines and p-curves in its own spine parameter and its sides in
its own order; the two stripes end on that section. The walk starts on the
stripe whose edge runs out of the corner where the other's runs into it,
whichever order the edges are given in. The faces generated by the edges are
the walked surface and its copy. Where a corner at a stripe's far end
extends the last piece's surface (`PerformIntersectionAtEnd`), the stripe's
other pieces on that surface take the extended surface too, so that they
stay one face.

### 0821-chfi3d-lens-rounded-on-both-edges-s-shaped-section

Both edges of the key's lens rounded with a radius wider than the lens. The
stored designs' concave rounding is a ball touching the shaft and rolling on
the lens's convex edge; their convex rounding is a ball touching the key and
that concave ball from outside; the concave face lies between its contact on
the shaft and the convex blend's contact on it, and towards the lens's ends,
where the convex blend's contact comes down to the concave one's on the
shaft, the shaft cuts the convex blend up to where its contact on the key
reaches the key's sides. OCCT rolled each rounding on the other's edge; the
concave one found no start. A failed stripe of one edge and a constant
radius whose face on one side has two edges, it concave, the other convex
and the edge of another such stripe, is now taken out with that stripe while
the others are done (`DeferLensRoundings`, only where nothing else failed)
and joined with it after `JoinAcrossCorners` (`FinishLensRoundings`,
`JoinLensPair`): the concave blend walked rolling on the convex edge from
the start the stripe's overflow start finds; the balls at the ends Q,
touching the shaft at its contact and the key from inside; the pipe of the
concave radius round the concave balls' centres (the concave blend's
surface, run on past its contacts) up to where those centres turn more
sharply than the radius; the convex blend walked between the key's surface
and the pipe, first along the convex edge (where its balls fold back before
Q) and then along a guide through those balls' centres and on to the balls
at Q; its cuts by the shaft from where its contact leaves the key (E) to Q
(`ChFi3d_CutBetween`, 0772's cut shared); the convex stripe in three pieces
(on the cut, on the key, on the cut), its other side the edge the two blends
share, the concave stripe in three pieces on the pipe, both ending in Q, the
lens's edges dropped. The two stripes' blends are not intersected with each
other (`IsLensPair`). Where the join fails, the concave stripe fails as
before.

### 0822-chfi3d-rounding-rolling-on-both-edges-into-a-lens-end

The key's other lens, only its convex arc rounded, wider than the lens. The
ball touches the key and rolls on the lens's concave edge; where its contact
on the key reaches the key's sides it rolls on those too, up to the lens's
ends, where the two edges meet and the rounding vanishes in a point (the
stored designs' faces there: the piece rolling on both edges, split where
the key's side is split by a vertex, and a three-sided piece ending in that
point). The walk's first step starts in the middle and ends at both sides;
OCCT went on rolling on both edges at its last end, but then walked back
from its first end with the faces and edges of the steps at the other end
and failed; where a side's edge ended at a vertex short of the lens's end,
the walk stopped there (the edge's other face holds the end vertex), and the
corners failed. The walk back now goes on with the first step's faces, edges
and obstacles (saved after it), its contacts at its start taken to be on
their edges; a blend rolling on two edges, one of which ended at a vertex
short of the spine's end vertex where another edge of its reference face,
not tangent across, takes it on, goes on rolling (`RollsOnPastVertex`). The
line's sections narrower than 5e-4 of the radius at an end where both
contacts lie within a hundredth of the radius of the spine's end vertex are
dropped (`ChFi3d_EndRstRstBeforeVanishing`; the approximation fails on
sections of almost no width), and the corner moves that end to the vertex
(`ChFi3d_ShrinkVanishingEnd`). Where the side's edge runs into the lens's
end unsplit, the walk still ends where the contact reaches it and the corner
cuts the blend by the shaft (within 0.016 mm of the rolling ball's faces).

## Reports and tests

Each of these tests fails without its patch.

- 0790: mitcad#422 and the post whose thread runs out along a straight cut,
  its lower crest rounded down to the run-out, in
  `geometry/tests/test_sweeps.cpp` (valid, one small face on the run-out's
  flank plane where it cuts the rounding a turn back, the crest left sharp
  from there down to the run-out against the helix's length).
- 0800, 0801 and 0803: mitcad#343 and, in
  `geometry/tests/test_fillet_crest_corners.cpp`, the post with a rib under
  a chamfer cone whose cylinder is trimmed two turns below its basis's
  parameters (the rib's side rounded up to it: the cone's face and the
  volume removed as on the plain post), the block of two halves whose top's
  split edge has curves on the two halves 1.8e-3 apart (its top front edge
  one contour through the split: valid, the volume of the edge's rounding)
  and the shaft with a thread cut as a groove whose floor's and ceiling's
  rims are circles 2 micrometres inside the shaft (a crest edge rounded up
  to them: valid, the volume of the section along the edge).
- 0802 has no such test: its contact meets the spine's own edge only
  through the stored bodies' micrometre deviations at the walk's steps
  there, which no synthetic body reproduced.
- 0820: mitcad#343 and the same housing as 0772's test in
  `geometry/tests/test_fillet_lens_overflow.cpp` (the blend one face per
  edge: the first's with the cut by the first side, the second's with the
  cuts by the corner's rounding and the second side, sharing an edge from
  the first side's junction; in both orders of the edges).
- 0821: mitcad#343 and the key on a shaft in
  `geometry/tests/test_fillet_lens_overflow.cpp` (both edges of the key's
  lens rounded: valid, the convex blend's contact ending on the key's sides
  within 1e-3, no face left on the lens's plane, the material added alike
  on both sides within 0.01 mm3, the two blends' faces as large as the
  stored design's within 0.01 mm2).
- 0822: mitcad#343 and the key on a shaft whose side is split near the
  key's other end, in `geometry/tests/test_fillet_lens_overflow.cpp` (that
  end's convex arc rounded, the split at two heights: valid, the contacts'
  ends within 1e-3, no face left on the lens's plane, a face past each split
  ending in a point at the lens's end, the material removed alike on both
  sides within 1e-3 mm3 and in all as the rounding's within 0.002).

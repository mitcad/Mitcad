# Roundings through corners where the ball stays put: 1600, 1601

mitcad#1427, a follow-up of mitcad#1398 (found among the regenerated
rounding specs, core/import/docs/rounding-specs.md): an opening's wall
rounded all round with the radius of the plate's earlier roundings, its
corners where the ball stays put in those roundings' tubes. The ball that
stays put between two tubes of its radius is 1520's
([fillet-stationary-ball-1520.md](fillet-stationary-ball-1520.md)); here it
stays put between a tube and a plane, inside one chain (1600) and where
two chains meet (1601). Back to the [port's README](../README.md).

## Overview

- **1600** ends a walk at the analytic piece after its guide only where
  its other contact lies within a tenth of that piece's section of the
  piece's point too, not where a ball that stays put keeps one contact
  where the piece starts while its other contact still turns round the
  tube it touches.
- **1601** joins two stripes of one radius R at a vertex where one runs on
  into the ball of an earlier rounding of R and the other ends on two
  planes that ball touches: the one's cylinder run straight on to that
  ball's section, the other cut back to it, the two meeting there as free
  ends.

## The case

A plate 10 thick with an opening: the opening's wall W (a plane) meets the
plate's top T and bottom, a cylinder of 5 whose rims an earlier rounding
of 0.2 rounds (tori of 4.8 / 0.2), and a side P at a crease of 3.5
degrees. P's rims are rounded with 0.2 too (cylinders K about lines along
P), going on into tori of 0.2 along arcs; a later feature's face X (an
extrusion of such a torus's section, tangent to T) lies between T and
the torus at W. The item rounds W's whole edge loop with 0.2.

W's boundary turns where its edge with X meets its edge with the torus
(X and the torus meet at a crease), at the top and at the bottom: two
contours. A runs along W's edges with X, T, the cylinder's torus, the
cylinder, the bottom's torus, the bottom and the bottom's X; B along W's
edges with the bottom's torus and K, P and the top's K and torus.

At each corner the ball touches W, T (or the bottom) and the third face,
and its centre lies on the earlier rounding's centre line:

- on the cylinder's side (inside A) the ball rolls on T and W along W's
  edge with the torus until its contact on T reaches the torus's edge with
  T; there its centre is on the torus's centre circle, and from there on
  it stays put while its contact turns round the torus's section from T
  down to the cylinder (a quarter of the section), its contact on W at
  the point where A's cylinder piece along the cylinder starts;
- on P's side (where A and B meet) B's ball stays put over K and the torus
  (both tubes of 0.2 whose centre lines run through its centre C, R from
  W, T and P), its contact turning round K's section from P up to T; A's
  ball rolls on T and W (its contact passes over X onto T) up to a
  section 0.165 short of C.

The stored body after the item has at every corner a piece of the ball's
sphere; on P's side A's cylinder runs straight on to C's section and meets
B's piece of the sphere there, X and the torus's part at the corner gone
under the ball. OCCT failed at both vertices where A and B meet ("BRep_Tool::
no parameters on surface", PerformTwoCorner); and the build of the result
failed further on the cylinder's side, where A's walk had stopped short of
the ball's part.

## 1600: the walk goes on into the ball that stays put

### 1600-chfi3d-walk-ends-at-the-next-analytic-piece-only-where-both-contacts-reach-it

Over W's edge with the cylinder's torus A is walked from the analytic piece
along T (a cylinder) to the one along the cylinder. 1301's
`ChFi3d_ReachedAnalyticPiece` ended a walk where either contact lay within
ten times the spine's tolerance of the point where the next analytic piece
starts, for a walk whose other contact lags a few micrometres across a
slight kink of the spine
([fillet-replay-1300-1302.md](fillet-replay-1300-1302.md)). Where the ball
reaches the torus's centre circle its contact on W is that point already
(the ball stays put from there), so the walk ended with the piece on T and
W: the part of the ball over the rest of the edge was missing, and the
data structure's build of the gap failed (`PartSection` took the two
sections for one shared in part, its section of no length: "Knots
interval values too close", or "U1 == U2"). Walked the other way, from the
cylinder's piece (as at the bottom, where the spine runs the other way),
the ball's part was there.

Now the other contact must also lie within a tenth of the analytic piece's
section (the distance between its two points there) of the piece's point
on its side: 1301's lag of a few micrometres is far within it, the
ball's other contact a quarter turn short (0.28 mm from the piece's point,
whose section is 0.098 mm) is not, and the walk goes on over the torus with the ball staying put
(1520's walk) up to the cylinder's piece.

## 1601: two stripes meeting at the ball's section

### 1601-chfi3d-two-stripes-meet-at-the-section-of-an-earlier-roundings-ball-that-stays-put

`JoinAtStillBalls` (new, `ChFi3d_Builder_StillBall.cxx`, after
`JoinAcrossCorners` in `Compute`) takes each vertex of two stripes of one
constant radius R and tries each as A:

- A's end pieces lie on two planes W and T, up to (at most four pieces
  in) a cylinder of R whose contacts are unbounded lines (an analytic
  piece, which can be run on);
- B has a section at the end of one of its pieces towards the vertex whose
  contacts lie on those lines, one on W's and one on T's, at one section
  of the cylinder (their difference square to its axis), R from the axis
  point C between them;
- A's run-on to that section lies beyond A's end within 8 R, its contacts
  on W's and T's faces (classified on the faces);
- B's spine past that section runs over edges between W and a tube of R
  about a curve through C: a cylinder of R whose axis passes through C, or
  a torus of minor radius R whose centre circle does (the ball stays put
  there).

Then A's cylinder piece is run on to that section (its contact lines'
parameters, its points B's, its spine parameter in proportion), A's
pieces past it and B's pieces past the section are dropped, and the two
ends meet as free ends: their points and the section's edge (computed on
B's surface, its p-curve on A's) shared, as `JoinOverNarrowFaceFrom`'s
ends meet (`ChFiDS_FreeBoundary`); the vertices' corners are those of the
ends left. X, the torus's part at the corner and the edges between lie
under the ball and go.

## Results

On the replay's body before the item (OCCT alone), W's twelve edges with
0.2: valid and closed; every new face within 8e-6 mm of the stored body
after the item and its faces within 7e-6 mm of the result (the faces
about the opening within 1.2e-5 mm both ways); 171 faces against the
stored body's 169 (A's pieces walked along T next to the cylinder's
torus, on the same cylinder as the analytic pieces next to them; MitCad's
dressup merges them: 169); the change of volume about the opening
-0.053109 against the file's -0.053108 mm³ (of the whole body -0.053063
against -0.052871, the difference the replay's own outside the item:
faces of other features 1e-4 to 2.7e-4 mm off the stored body's). Through
MitCad's import the item is parametric and verified. Without 1601 the
corners at both vertices fail; without 1600 (with 1601) the build of the
result fails on the cylinder's side.

## Reports and tests

- 1600 and 1601: mitcad#1427, in
  `geometry/tests/test_fillet_still_ball_corners.cpp`
  (`geometry.fillet_still_ball_corners`): a D-shaped block (a cylinder cut
  by a wall), its top rim rounded with R first, the wall's edges with the
  top, the torus and the cylinder rounded with R (1 on a cylinder of 5,
  0.2 on 5, 2 on 8), the edges given each way round: valid and closed,
  each top corner a face on the ball's sphere within 1e-4, every face about
  the corner on the construction's surfaces within 1e-4, the same change
  of volume both ways. Without 1600 the rounding throws or (2 on 8) makes
  an invalid body. With `MITCAD_GEOMETRY_CASES`, the case
  `fillet-opening-still-corners`: the replay's body before the item
  (`body.brep`) and the stored body after it (`after.brep`), the wall's
  twelve edges with 0.2 each way round: valid and closed, a face on each
  of the four corner balls, the result on the stored body and the stored
  body on it within 0.01 mm about the opening, the change of volume about
  the opening within 1e-3 of the file's (-0.053109 against -0.053108
  mm³). Fails without 1601 (the corners) and without 1600 (the build). No synthetic body for 1601: the turn of the wall's
  boundary comes from a later feature's face over the end of an earlier
  rounding.

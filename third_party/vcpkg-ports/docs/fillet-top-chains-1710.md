# A fin's top rounded along its outer and inner edges: 1710 and 1711

mitcad#1926, a follow-up of mitcad#1904 and mitcad#1819 (GitHub #371's
design, its Fillet1), after
[fillet-wall-run-on-1700.md](fillet-wall-run-on-1700.md): the same fin's
top rounded with 0.5 along its outer and its inner edges, alone and with
the foot's rounding of 3. Back to the [port's README](../README.md).

## Overview

- **1710** judges two stripes whose blends lie on one face overlapping by
  the spine edges their blends run along, not by the first edge of each
  chain that bounds the face, so that two chains running round a U-shaped
  face are no longer taken for blends overlapping across it.
- **1711** fails a corner of more than two blends at once where the parse
  of the edges and faces round its vertex meets an edge twice, instead of
  searching the stripes' intersection with an index one past a trimmed
  stripe's surfdata (a fillet that did not return).

## The case

The body of [1700](fillet-wall-run-on-1700.md#the-case): a U-shaped fin
with arms 2 wide on a plate, its top one plane face sloping at 30 degrees
down to the plate at the arms' ends. The top's edges with the fin's sides
form two chains, each running round the U: the outer one (eight edges,
from one arm's end round the U's bottom to the other's) and the inner one
(seven edges). Every edge of both chains bounds the top. Fillet1 rounds
both with 0.5 (with fourteen other sets, the foot's rounding of 3 among
them).

## 1710: overlapping chains judged by the edges their blends run along

### 1710-chfi3d-overlapping-chains-judged-by-the-spine-edges-their-blends-are-on

The two chains rounded with 0.5 failed with `Failure_Overlap`.
`OverlappingStripes` (0013) looks for pairs of stripes whose blends lie on
a common face between their edges, each contact beyond the other as seen
from the other's edge (`contactBeyond`), and computes them again
overflowing that face. It took each stripe's edge on the face with
`spineEdgeOf`: the first edge of the spine that bounds the face. For two
chains running round the top that is the outer chain's edge along one
arm's outer side and the inner chain's along the other arm's inner side,
whatever surfdata is looked at. The blend along one arm's outer edge
(contact at x = -273.931) was measured against the inner chain's first
edge, along the other arm's inner side, and the blend along that arm's
inner edge (contact at x = -288.431) against the outer chain's first edge,
on its own arm's outer side: each contact lay nearer the other chain's
edge than that chain's own contact did, so both counted as beyond each
other although they are 14.5 apart. Computed again overflowing the top,
both stripes failed.

- `spineEdgeOfData` (new): the edge of a spine that bounds the face where
  a given surfdata lies: where several edges of the spine bound the face,
  the one at the surfdata's middle spine parameter
  (`ChFiDS_Spine::Index`), else the one nearest its contact on the face;
  `spineEdgeOf`'s edge where one edge or none bounds it (as before).
- `OverlappingStripes` takes both stripes' edges with it, and so does
  `RollsInsteadOfOverflow` (the same test for a blend that ran over a
  narrow face onto another stripe's face).

Result: the two chains round together, valid; the material taken away is
3.8759 mm3 against 3.8773 for the outer chain alone and the inner chain
alone together (they meet only at the arms' ends, a millimetre apart).

## 1711: a corner whose parse meets an edge twice

### 1711-chfi3d-corner-parse-meeting-an-edge-twice-fails

With 1710, the foot's rounding of 3 and the top's roundings of 0.5 in one
fillet compute their stripes, and the fillet did not return (no result
after 5 minutes). At the vertex where an arm's outer side, its top and
the plate meet, the rounding of 3 consumes the outer side (1700), and
`PerformMoreThreeCorner`'s parse of the edges and faces round the vertex
met the top's outer edge twice: the stripe along it stood at two places
of the corner. The first intersection of neighbouring stripes trimmed that
stripe's surfdata; its index at the second place still named the
surfdata it had before, one past the sequence's end, and the search for
the stripes' intersection (`ChFi3d_SearchFD`, `ChFi3d_IsInFront`) read
past it and did not return.

- `PerformMoreThreeCorner`: after the parse, an edge met twice throws the
  parse's construction error (as a parse that finds no next edge does),
  so that the corner fails at its vertex (`Failure_Corner`, a faulty
  vertex) at once.

The fillet now fails at those two vertices in a moment. Building that
corner needs the 0.5 rounding along the top's outer edge to run on the
rounding of 3 where that rounding consumes the outer side (a rounding
between the top and another blend), which OCCT's fillet does not do: the
whole of Fillet1 is still not built (mitcad#1926).

## The whole rounding: what is left

The design's rounding has fifteen sets. With the port's patches (1700,
1710, 1711 among them) twelve of them build together in one fillet,
valid (the foot's 3, the slot's edge, the tower's top inner edge, the
tower's and the support's other edges, the plate's rim and the inner
sides' feet). The other three fail, each where a face between two
roundings is narrower than both, and the file's stored body there has the
later rounding's ball rolling on the earlier rounding's surface, tangent
to it (checked on the stored body's faces):

- the tower's top outer edge with 1 beside its top inner edge with 2: the
  top is 2.127 wide at the legs and 3 at the apex (2 + 1, an exact fit);
  the ball of 1 touches the tower's outer side and the torus of the
  rounding of 2 below the top, which vanishes. OCCT: the two blends
  intersect (`Failure_Intersecting`), or, without the other sets, the
  mitred end at the legs' fronts fails;
- the plate's edge with the tower and the support rounded with 2 (a
  concave rounding) beside the plate's back edge rounded with 1 (convex):
  the plate is exactly 2 wide behind the tower; the ball of 1 rolls on the
  back face and on the torus of 2. OCCT: `Failure_Intersecting`. Alone the
  set builds, but the torus's contact touches the plate's back edge at one
  point without a vertex there, and the plate's wires intersect (an
  invalid result; with the back edge rounded the touch does not occur);
- the fin's top edges with 0.5 at the arms' ends, where the inner sides'
  feet are rounded with 1 (concave, the side 1.45 high there) and the
  foot's rounding of 3 takes the outer side away: corners at the arms' ends
  fail (`Corner_Two`, `OneCorner`).

0016's ride lets a rounding pass through another's contact line on the
face (both convex or both concave), which is not that shape (it meets the
other blend on the face; at the tower's legs its contact on the outer side lies
about 1.2 higher than the stored one). What the
fillet lacks is a ride on the other blend's surface: the later stripe
walked between its own face and the earlier stripe's blend surface, the
earlier blend trimmed by the later one's contact curve on it, the face
between them dropped, roundings of either kind, and the corners where
such a ride ends at a face with more roundings (the legs' fronts, the arms'
ends).

[1720](fillet-tangent-ride-1720.md) adds that ride, of either kind of
rounding, where the two stripes run the whole length of the face between
and end alone at faces at their ends; rides ending at corners of more
blends and part rides are left (its *What is left*).

## Reports and tests

- `geometry.fillet_top_chains`
  (`geometry/tests/test_fillet_top_chains.cpp`): with
  `MITCAD_GEOMETRY_CASES`, the case `fillet-wall-run-on`: the top's outer
  and inner chains rounded with 0.5 by OCCT's fillet alone build, valid,
  taking away what the two chains take alone within a thousandth (fails
  without 1710: `Failure_Overlap`); with the foot's rounding of 3 as well
  the fillet returns, its result valid or its failure at a corner (without
  1711 it does not return).
- OCCT's own blend and chamfer groups (the private suite's `run.sh`, a
  DRAW build of the port with and without 1710 and 1711): 754 cases each,
  no case changed.

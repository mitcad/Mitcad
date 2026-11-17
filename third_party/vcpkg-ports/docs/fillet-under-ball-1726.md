# A stripe under another one's ball only where its ball is found: 1726

mitcad#1926, GitHub #371's Fillet1: the pit inside the fin's U, whose edge
with the plate is rounded with 5, came out of the result unrounded beside
the fin's foot rounded with 3, and the result was done and valid. Back to
the [port's README](../README.md).

## The case

The design's body before Fillet1 (`fillet-wall-run-on` of
`MITCAD_GEOMETRY_CASES`): a U-shaped fin on a plate, its arms 2 wide,
their tops sloping down into the plate at the arms' ends. Its outer foot
is rounded with 3; the arms' outer sides are lower than 3 at their ends,
so that the foot's walked ends run on over those sides' planes and are cut
by the top grown out to them (port patch 1700). Inside the U, 2.5 behind
the arms' ends, the plate falls into a pit by a 45-degree face; that edge
is rounded with 5 (set 4 of the design's fifteen), the pit's sides with
the arms' inner walls with 1, and the inner walls' feet with 1 and 0.8.

With the foot's rounding (`fillet1 body r3 e12` in the ticket's scratch
driver) the pit's rounding of 5 was missing: the result's volume was the
foot's rounding alone (6110.5095 mm3), the 45-degree face still met the
plate in a sharp edge. With the twelve, thirteen and fourteen sets of
Fillet1 that 1710 to 1725 build the same: the pit's rounding of 5 and the
pit's sides' roundings of 1 were not in the results, which were called
valid. The file's stored body has them (a cylinder of 5 along the pit's
edge, the sides' roundings of 1 running on over it as tori).

## 1726: the ball of the stripe it belongs to

### 1726-chfi3d-under-anothers-ball-only-where-its-own-ball-is-found

0700's stripes under another one's ball (`JoinOverNarrowFaces`): a stripe
whose edges all lie between a face Fo and a narrow face C is dropped where
another stripe's blend on Fo and a third face rolls over them; 1060 counts
a point of those edges only on the ball's side of that blend's other face
(`edgesUnderBlend`, the ball through the point's section, `ballAt`).

- The pit's edge (between the plate and the 45-degree face) lies within
  four radii of the foot's contact on the plate at the arms' ends, on the
  side of that contact toward its other line. 1060's ball was sought with
  the radius passed in, the radius of the stripe whose edges are tested
  (5), not the foot's (3): no ball of radius 5 has that section, and where
  no ball was found the test was skipped and the point counted. The foot's
  pieces at the arms' ends have no ball's section anyway: their other line
  is no contact but the cut by the top grown out to them. So the pit's
  edge was taken for one under the foot's rounding, and the pit's stripe
  dropped.
- Now the ball is sought with the radius of the stripe it belongs to (a
  constant rounding's; otherwise the radius passed in, as before), and a
  point where no ball is found is not under the blend.

Result: the foot's rounding and the pit's together build, valid, with the
pit's cylinder of 5; the material is that of the two made alone within
0.004 mm3, and the faces at the pit are those of the pit's rounding made
alone.

## What it shows of Fillet1

The pit's roundings now take part in Fillet1's sets, and fail there: at
each pit corner, where the pit's edge (5), a pit side's edge with an inner
wall (1) and that wall's foot (1 on one arm, 0.8 on the other) meet,
OCCT's corner of three blends leaves the side's rounding and the foot's
crossing each other on the wall (`Failure_Intersecting`). The twelve,
thirteen and fourteen sets without the five pit edges (the pit's edge, its
two sides' edges with the walls and the two feet) build together, valid,
as before; with them they fail instead of leaving the pit's roundings out.

The stored body shows the shape that corner needs: the side's rounding of
1 runs on past the corner between the wall and the pit's rounding of 5 (a
torus about the cylinder's axis), up to the plate behind the pit; the
wall's foot (1) continues it from there to the arm's end on one arm; on
the other the foot of 0.8 is gone, the wall there taken away by the torus
and by the fin's top's rounding of 0.5, which rolls on that torus from
where its contact crosses the torus's (y 433.17) down to the plate.

## Reports and tests

- `geometry.fillet_top_chains`: the foot's rounding of 3 with the pit's
  edge rounded with 5 builds, valid, with a cylinder of radius 5 and the
  volume of the two made alone (within 0.01 mm3); the twelve, thirteen
  and fourteen sets without the pit's five edges build together, valid;
  with them, the result is never done without the pit's rounding.
- OCCT's own blend and chamfer groups (DRAW, 754 cases, against the port
  with 1725): no case changed.

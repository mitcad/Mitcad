# A walk whose spine folds round an acute corner: 1440

mitcad#1396, the follow-up of mitcad#1371
([fillet-three-face-corner-1380.md](fillet-three-face-corner-1380.md)): a
concave rounding along an acute corner whose edge turns towards the ball,
where the walk's sections, square to the edge, turned back past the ball.
Back to the [port's README](../README.md).

## Overview

- **1440** walks such a stretch between two pieces along the balls'
  centres: the cubic from the centre of the ball before it to the centre of
  the ball after it, with the spine's tangents there, is its guide
  (`ChFi3d_UnfoldGuide`).

## The case

`7dab3af4` item 26 (Fillet14, radius 1.5): a plate with two rails on it,
both sides of a mirror, run into end walls standing at 45 degrees to the
rails. Each rail is 3 wide, its foot rounded with 2 on its inner side, its
top with 1.2 on both sides (a flat of 0.6 between). The rounding runs along
each wall's foot over the plate and up round the rail's profile: the
plate's line, the foot rounding, the rail's inner side, the top roundings
and flat, the rail's outer side down to the bottom, seven edges a side,
every one concave. mitcad#1371 took the top roundings of 1.2 for concave
arcs tighter than the ball; they are convex, and 1380 is not this case.

The stored body after the item: the ball of 1.5 rolling all along. Over the
plane faces the roundings are cylinders, over the foot rounding and the top
roundings walked faces (splines), the ball's two sides mitred where the
walls meet at the mirror.

OCCT failed past the rail's inner side, where the walk from that side's
cylinder went on over the foot rounding. A walk takes the ball whose
contacts' mid-point M lies in the guide's normal plane at the guide's
parameter, and the guide is the spine, the edge on the wall. The wall meets
the foot rounding in an ellipse whose radius of curvature is 1.41 at the
rail's side, and in the corner of 45 degrees the ball lies 2.6 radii from
the edge: there M was 3.1 from the guide along its principal normal,
k (M - G).n = 2.2 (k the guide's curvature). Past 1 the normal planes turn
back faster than the ball moves: as the parameter grew the ball went back
up the rail's side, every step found its contact on the foot rounding's
cylinder beyond the face, and the walk stopped at once. At radius 0.5 the
fold is 0.73 and the walk went through; from 1 up it failed.

## 1440: the guide along the balls' centres

### 1440-chfi3d-walk-guided-along-the-balls-centres-where-the-spine-folds

`ChFi3d_UnfoldGuide` (new, in `ChFi3d_Builder_Guide.cxx`), called by
`PerformSetOfKPart` after `ChFi3d_PerformElSpine` for each guide of a
constant-radius rounding, looks at a walk between two pieces (the guide's
Previous and Next, here the cylinders along the rail's side and along the
plate). Where the spine folds over the stretch, k (M - G).n of 0.9 or more
at any of 41 points (M taken between the end balls' mid-points along the
way), the guide becomes the cubic from the centre of the ball the walk
starts on (the previous piece's last section: its contacts and its faces'
normals) to the centre of the ball it ends on (the next piece's first),
with the spine's tangents there and a circle's handles,
(2/3) tan(a/4) / sin(a/2) of the chord for a turn a. Each end ball's centre
and mid-point must lie in the spine's normal plane at its end, so that the
walk starts and ends on the same sections as before; between them the guide
runs near the balls' centres and its normal planes turn with the ball. The
cubic is taken only where it folds less than 0.5 itself. The blend is still
the rolling ball: the guide only says which ball each section is.

On the stored body before the item, the fourteen edges (both chains): the
result valid and closed; along both chains the result lies on the stored
body within 0.001 mm and the stored body on it within 0.0046 mm (the stored
foot faces' own fit: their edges' tolerance is 0.0048); the boolean
difference of the two has no piece of 1e-4 mm³. The change of volume is
144.943 mm³, the stored body's 145.227 as its faces' properties sum it.
Through the import the item is parametric and verified. Before, the fillet
failed. On the synthetic plate, rail and end wall at radii 1, 1.5 and 1.9:
valid and closed, every point of the walked face over the foot rounding
within 1.1e-5 mm of the ball (R from the analytic path of its centre);
before, each failed. Among the fillet tests of `geometry/` only these
guides are replaced.

## Tests

- 1440: mitcad#1396, `geometry.fillet_acute_turn`
  (`geometry/tests/test_fillet_acute_turn.cpp`):
  `test_rounding_round_the_foot` (the synthetic plate, rail and end wall at
  radii 1, 1.5 and 1.9: valid, closed, the walked face on the ball) and,
  with `MITCAD_GEOMETRY_CASES`, `test_reported_body` (the case
  `acute-concave-turn`: the stored body before the item rounded along both
  chains, on the stored body after it and back within 0.01 mm). Without the
  patch each rounding fails: one faulty contour at each radius, both chains
  on the stored body.

# A tangent ride's corner of three blends: 1723

mitcad#1926, after [fillet-ride-end-corners-1722.md](fillet-ride-end-corners-1722.md)
("What is left"): the corner at a tangent ride's end where the other
rounding reaches past the ride's point, so that three blends meet on the
ride's line. Back to the [port's README](../README.md).

## Overview

- **1723** makes that corner: the other rounding (the small) is cut by the
  ride stripe's blend up to the ride's line and by the ride's other blend
  from there to the face at the end; that other blend's end, at its own
  vertex, is made by this corner too.

## The cases

- Synthetic (`geometry.fillet_tangent_ride`): 1722's wall 2.5 wide, its
  top rounded with 2 and 1, an end's edge at the rounding of 2 rounded with
  1.2 or 1.5, or at the rounding of 1 with 1.7: the ride's point lies 1
  from the side of the rounding of 2, so that rounding's contact on the end
  face meets the other blend of the ride.
- GitHub #371's design (`geometry.fillet_top_chains`): the tower's top
  rounded with 2 and 1 (sets 4 and 5) and its legs' inner front edges with
  0.5 (set 12); the ride's point at the legs' fronts lies 0.285 from the
  inner side. 1722's corner did not take it; the corner went to
  `PerformMoreThreeCorner` and the blends' intersection check failed it.

## 1723: the corner of three blends

### 1723-chfi3d-tangent-ride-corner-of-three-blends

- `PerformTwoCornerAtRideEnd`: where the small's contact on the face at
  the end meets the big's blend past the ride's line (tested against the
  big's contact on the common face: its own part of its surface lies
  between that contact and the ride's line), or not at all:
  - Q, where the ride's line meets the small's blend; P*, where the small's
    contact on the face at the end meets the ride's other blend (U);
  - the small is cut by the big's blend from the crossing of their contacts
    on the common face to Q and by U's blend from Q to P*;
  - the big is cut by the small's blend up to Q, its point on the ride's
    line there;
  - U is cut by the small's blend from Q to P* and by the face at the end
    from P* to its point on its arc, its end marked in the data structure,
    so that its own vertex has no corner of its own;
  - the curves go into the data structure on their surfaces, the last on
    the face at the end too, FORWARD there where the vertex of U's edge on
    that face lies on the curve's right about the surface's own normal (that
    vertex lies outside every tangent of U's section between its contacts,
    for either kind of rounding; the middle of the face's top edge, tried
    first, lay inside for the wall).
- `Compute`: the corners of two stripes one of which is a tangent ride's
  are made before the others, so that such a corner finds U's end not made.

Result: the wall's cases build, valid, their volumes those of the two top
roundings less what the end rounding takes away below the top's arcs
(within a millionth); the tower's corner has its points where the file's
stored body has them (within a ten-thousandth); the design's twelve sets
with set 5 build together, valid (thirteen of Fillet1's fifteen).

## What is left

[1724](fillet-ride-between-crossings-1724.md) makes the ride between two
crossings (set 6's kind; a closed rider is left). The text below is as
1723 left it.

- Fillet1's set 6: the tower's outer foot of 2 (concave) beside the
  plate's back edge of 1, the plate 2 wide behind the tower's apex only:
  a part ride between two crossings of the contacts, the back edge running
  past the tower on both sides. A first construction (the rider walked on
  the support over the ride's stretch only, both stripes split at the
  crossings) computes, but the plate's top face is then built wrong (the
  tower's flat side's foot edge lands twice in its wire); not in the port.
- Fillet1's set 2: the fin's top's 0.5 along both chains, with the
  thirteen other sets: the corners at the arms' ends (with the inner
  sides' feet and the foot's rounding of 3) fail.
- A wall's top 2 wide, closed off at its end by a face at 45 degrees,
  rounded along the outer wall with 3 (the ball rolling on the inner
  wall's top edge): OCCT's fillet fails at the end (without the slope it
  builds); MitCad's fillet then returns a valid body that has lost the
  wall (a wrong result, not a failure).

## Reports and tests

- `geometry.fillet_tangent_ride`: the wall's four cases, prism and box,
  both orders.
- `geometry.fillet_top_chains`: the tower's top with the legs' edges, and
  the thirteen sets.
- OCCT's own blend and chamfer groups (DRAW, with 1723 against the port
  without it): 754 cases each, no case changed.

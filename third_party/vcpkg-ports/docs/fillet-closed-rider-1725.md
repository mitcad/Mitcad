# A tangent ride between two crossings of a closed rider: 1725

mitcad#1926, after [fillet-ride-between-crossings-1724.md](fillet-ride-between-crossings-1724.md)
("What is left"): 1724's ride between two crossings where the rider is a
closed contour. Back to the [port's README](../README.md).

## The case

A plate's top whose rim is rounded all round with 1 (one closed, tangent
contour: the plate's corners rounded in plan), past whose back edge a
tower of half a cylinder stands (its round side towards the edge, 2.3 or
2.5 from it at the apex), the tower's foot rounded with 2 (concave)
(`geometry.fillet_tangent_ride`). GitHub #371's Fillet1 set 6 is of this
kind: the tower's outer foot of 2 beside the plate's rim of 1, fourteen
edges in one contour, the plate 2 wide behind the apex. 1724 refused the
closed rider: a walk over a part of a periodic spine (its range set on the
rider's own spine) wrote past the spine's law arrays (heap corruption,
found under valgrind), so the pair failed (the blends intersecting).

## 1725: the closed rider walked on a spine of its own

### 1725-chfi3d-tangent-ride-between-two-crossings-of-a-closed-rider

- `ChFi3d_Builder::RideOnBlendBetweenCrossings`: the closed rider's ride is
  walked by a stripe of its own whose spine is the rider's edge beside the
  two crossings alone (an open spine of one edge, the rider's radius and
  concavity; its faces those of the edge, `myEdgeFirstFace`). For the walk
  that stripe is registered as the tangent ride on the larger blend (so
  that `StartSolOverflow` finds the support) and dropped after it. Its
  walked piece's spine parameters are shifted to the rider's; its lines
  keep their own parameters (the pieces are split by those, and
  `splitSurfDataAt` interpolates the spine parameters). The rider itself
  is not reset: its other SurfDatas stay as computed.
- The walk's range in the rider's spine parameters is measured on the
  spine's edge the two crossings project inside (`ChFiDS_Spine::Absc`),
  for every rider of more than one edge: the crossings' parameters are
  those of the contact line, and the line of an analytic blend starts on
  its own edge, so they are the spine's on its first edge only (set 6's
  crossings lie beside the rim's fourteenth edge, 130.6 along the spine,
  their line parameters 4.4 and 12.1). An open rider of one edge, or whose
  crossings lie beside two edges, keeps the line's parameters as before.

Result: the synthetic plates build, valid, their volumes those of the two
roundings made one after the other, the larger first (within a millionth),
in both orders, the tower at the back edge's middle or off it. Under
valgrind the design's fourteen sets run clean.

GitHub #371's Fillet1: set 6 builds with the rim (6053.027 mm3, the two one
after the other 6053.027), and fourteen of the fifteen sets build together
(all but set 2), valid.

## What is left

- Fillet1's set 2 and the strip closed off by a slope (1723's list).

## Reports and tests

- `geometry.fillet_tangent_ride`: the rounded plates, the tower 2.3 and 2.5
  from the back edge, at its middle and 3 off it, both orders (before 1725
  the fillet failed).
- OCCT's own blend and chamfer groups (DRAW, with 1725 against the port
  without it): 754 cases each, no case changed.

# A tangent ride between two crossings: 1724

mitcad#1926, after [fillet-part-ride-1721.md](fillet-part-ride-1721.md) and
[fillet-ride-three-blend-corner-1723.md](fillet-ride-three-blend-corner-1723.md)
("What is left"): the face between two roundings narrower than both in
its middle only, their contacts crossing twice. Back to the
[port's README](../README.md).

## The case

A plate's top whose back edge runs past a tower of half a cylinder (its
round side towards the edge, 2.3 or 2.5 from it at the apex), the tower's
foot rounded with 2 (concave) and the back edge with 1
(`geometry.fillet_tangent_ride`); GitHub #371's Fillet1 set 6 is of this
kind (the tower's outer foot beside the plate's back edge, the plate 2
wide behind the apex). OCCT failed: the two blends intersect.

## 1724: the ride between the crossings

### 1724-chfi3d-tangent-ride-between-two-crossings

- `ChFi3d_Builder::RideOnBlendBetweenCrossings`, from `Compute` with 1721's
  part rides (before the corners): for a pair as `RideOnBlend` takes them,
  the SurfData of each on the face between whose contacts cross twice
  inside both, the smaller one's contact nearer the other's edge between
  the crossings (measured from the edge of that SurfData). The rider's
  SurfData is split at both crossings, its pieces before and after kept;
  the rider is walked on the larger blend's surface as a face (1721's
  support) over the ride's stretch only (its spine's range set to the
  crossings and a tenth beyond, then restored), its piece between the
  crossings kept and its points there the other pieces'; its contact on
  the support goes onto the face between, as 1720's. The larger stripe's
  SurfData is split at both crossings, its middle piece holding that
  contact. `ApplyRides` makes the blends meet on it as a part ride. Either
  stripe may have several SurfDatas (the rider's others kept as they were).
- `myCrossingRides`: the stripes of such rides, whose ends are no ends of
  the ride: `PerformOneCorner`'s and `PerformTwoCornerAtRideEnd`'s tangent
  ride ends do not apply to them (they cut the tower's foot's ends wrongly,
  the plate's top then built with its edge twice).

Result: the towers build, valid, their volumes those of the two roundings
made one after the other, the larger first (within a millionth), in both
orders.

## What is left

[1725](fillet-closed-rider-1725.md) makes the ride of a closed rider. The
text below is as 1724 left it.

- A rider whose spine is closed: Fillet1's set 6 meets the plate's rim
  rounded all round (set 14, fourteen edges in one contour). Such a rider
  is refused: a walk over a part of a periodic spine failed and wrote past
  the spine's law arrays (heap corruption), so the pair fails as before. It needs the ride's stretch walked with a spine of
  its own (the rider's edge there alone) or the periodic walk restricted
  properly.
- Fillet1's set 2 and the strip closed off by a slope (1723's list).

## Reports and tests

- `geometry.fillet_tangent_ride`: the two towers, both orders.
- OCCT's own blend and chamfer groups (DRAW, with 1724 against the port
  without it): 754 cases each, no case changed.

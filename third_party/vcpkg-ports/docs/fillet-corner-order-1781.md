# Corners of three stripes before corners of two: 1781

mitcad#1987, GitHub #371's Fillet1: corner patches beside the pit with
vertices half a millimetre off their edges. Back to the
[port's README](../README.md).

## The case

The pit inside the fin has a wall on each side whose foot on the plate is
rounded (1 on one side, 0.8 on the other) along 2.5 mm. At one end that
rounding meets the plate's rounding of 5 along the pit's edge and the
rounding of 1 along the pit's 45 degree face: a corner of three stripes.
At the other end it meets the fin arm's end: a corner of two.

OCCT computes the corners in the order their vertices came in, which
follows the order of the contours. With the contours as
`geometry.fillet_top_chains` lists them (the top's 0.5 last) the corner of
three came first and the result was tight. With the top's 0.5 given right
after the foot's 3, as MitCad's fillet gives the file's sets, the corner
of two came first, and the corner of three then left the patch's vertices
0.54 and 0.66 off its edges, with 0.14 mm3 more material. Chamfer3 failed
on that result until [1730](chamfer-snap-own-tolerance-1730.md).

## 1781-chfi3d-corners-of-three-stripes-before-corners-of-two

- `Compute`, the corners' order: the corners of two stripes where one is
  a tangent ride's come first (as before), then the corners of three or
  more stripes, then the rest.

Result: Fillet1 is the same body in either order, its widest vertex
tolerance 0.028 (at the legs' fronts, the corners of the foot's 2 and the
top's 0.5), and its volume 6089.428 (the stored body's 6092.117).

Test: `geometry.fillet_top_chains` (`test_whole_fillet1`, both orders).

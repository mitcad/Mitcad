# Roundings riding on another's contact line and their corners: 1640 to 1642

mitcad#1499, the follow-up of mitcad#1494: the gaps of 0016's ride
([fillet-edge-overflow.md](fillet-edge-overflow.md#0016-chfi3d-rounding-rolls-on-anothers-contact-line))
that the roof slab of mitcad#1482 found, on its end faces and on a plate
of the same thickness. Back to the [port's README](../README.md).

## Overview

Two roundings of a face F narrower than both, where no ball touches the
faces beyond F on both sides: 0016 computes the earlier stripe A on its own
faces and lets the later one, B, roll on A's contact line on F, the two
blends meeting on that line, F vanishing between them.

- **1640** takes the ride the other way round where it fails (the shorter
  stripe rides on the longer one's line), puts the rider's side on the
  line on F itself, fills a corner of three stripes with a rider between
  the stripes cut where their contacts meet, and drops a stripe that the
  corners at both its ends cut down to nothing.
- **1641** ends a rider, at a vertex of its own edge, where the line ends
  on the arc of F that runs from that vertex, and lets it ride on a part of
  the line from that end.
- **1642** removes F where it is a strip consumed whole and both rides end
  in corners.

## The cases

A plate 25.2 by 20 by 1.3 rounded with 1 (each side 1.3 wide, narrower
than the two rims' roundings, 2): the rims of a side ride, the bottom one
on its own faces, the top one rolling on the bottom one's contact line at
the side's height 1 (its ball touching the top, its centre 0.7141 from the
side). With OCCT and 0016 alone:

- an end face's two rims build;
- with one vertical edge at their ends too, the corner of three stripes
  failed ("Failed to find edge", `ChFi3d_cherche_edge`): the rider's side
  on the line indexed the support made from the other blend, a face of no
  solid;
- with that side on F (mitcad#1494's prototype), both vertical edges left
  the end face whole between the two rides (invalid, the volume larger than
  the plate's): no edge of it was cut, the lines ending at the corners'
  points;
- and with all 12 edges, each top corner (two riders and the vertical
  edge) was a plate surface reversed or folded at two of the four corners,
  and each vertical edge's rounding, cut by the bottom corner's sphere and
  by the top corner at the same height (where the lines ridden on meet the
  vertical edges' contacts), left a face of no area.

The roof slab (a section of 7 points extruded 25.2) has its end faces'
strips along the slopes 1.303 wide square to the slopes; its end face's
two edges along one slope rounded with 1:

- the slope's edge (from the eave's tip to the ridge, 51.5 long) is the
  later stripe and rode on the underside's line (from the eave's inner
  edge to the ceiling's corner, 45.0 long): its walk ran on past the
  line's ends and threw;
- the underside's riding on the slope's line computes, but at the eave its
  contact on the underside leaves it at the section through the eave's
  inner vertex, short of the line's end on the end face's edge with the
  eave; the corner there cut an extension of its surface off the line,
  across the slope's rounding's cut at the eave's tip ("OneCorner :
  fillets have too big radiuses").

## 1640 to 1642

### 1640-chfi3d-ride-on-a-contact-line-the-other-way-its-side-on-the-face-and-its-corners

- `Compute`: where `RideOnOverlap(A, B)` fails, `RideOnOverlap(B, A)`
  (the failed ride's entry removed, and the other one's where both fail).
- `RideOnOverlap`: the rider's side on the line lies on F (its index F, its
  line's p-curve on F projected), so that the corners at its ends find
  their faces through it as for the other stripe; its transition, computed
  on the support, is turned where F lies the other way in the solid (with
  the support's orientation the plate's corners at the riders' ends came
  out reversed at two of the four corners). `ApplyRides` finds the rider's
  SurfData on F too, and the rider's points at the line's ends become the
  other stripe's also where a corner of several stripes made its own point
  there.
- `PerformThreeCorner`: a corner with a rider goes to
  `PerformMoreThreeCorner` (the pivot's walk recomputed the rider's ends
  off the line), whose plate is built with G0 constraints where a stripe
  of the corner rides (with G1 to the riders' surfaces the plate of one of
  two mirror-image corners was folded, its surface reaching 1.7 above the
  plate).
- The clash check (`ChFi3d_StripeEdgeInter`) skips two stripes that both
  end where a part ride's blends meet.
- `dropStripesCutToNothing`, after `ChFi3d_FilDS`: a stripe of one
  SurfData whose contacts are points and whose two end curves are one
  curve (within ten tolerances) leaves the data structure (its surface,
  its solid interference, its contacts), and the corner at one end takes
  the other end's curve, with p-curves of its own along it (the first
  end's curve kept, or the last end's where no p-curve along it is found
  on the other corner's surface).

### 1641-chfi3d-rider-ending-where-the-line-ends-on-the-arc-from-its-vertex

- `RideOnOverlap`: a rider's end on the line at its spine's end vertex V,
  not on an arc, where the nearest end of the line ridden on lies on an arc
  of F that runs from V, is that end's point on the arc: `PerformOneCorner`
  then cuts the rider with the face at the end between two points on arcs,
  as it cuts the other stripe.
- `ApplyRides`: a ride along the line from that end to a point inside F
  where the line runs on (the slab's ceiling corner) is a part ride (1340):
  `partOfContact` takes one end of the part that is an end of the other
  contact on the same arc (`theSharedEnd`), and the ridden SurfData is
  split once, at the rider's other end.

### 1642-chfi3d-face-consumed-between-rides-with-corners-at-both-ends

- `ApplyRides`: where F is a strip between the two stripes' edges and the
  ride is whole, F's other edges not yet cut at their vertices (where the
  lines end at corners' points, not on those edges) are cut there too, so
  that F goes with them.

## What is left

With these patches the slab rounded on all its edges still failed at 0.9
and 1.0, at the eave's end edge on the end face (a stripe 1.506 long
between the end face's two slopes' corners, consumed between its
neighbours' roundings): mitcad#1500, the eave's two long edges rounded
together and the corners at their ends. The eave, its end edges and the
whole slab build with 1650 to 1653
([fillet-eave-1650.md](fillet-eave-1650.md)), which also says what is
left.

## Reports and tests

- `geometry.fillet_ride_corners`
  (`geometry/tests/test_fillet_ride_corners.cpp`), OCCT's fillet alone, all
  results one valid closed solid with no face of no area:
  - the plate's end face's rims: no face left in the end face's plane, the
    material taken the rims' section (the square r by t less what both
    arcs keep, built as a face and measured: 0.281095) along the 20 within
    1e-5;
  - with the vertical edges at both ends of that face: as above, and two
    corner cells' 0.14997 each within 0.002 (5.922086 against 5.921845);
  - all 12 edges: no side face left; the material taken 25.638357, between
    the sharp model (the rims' sections along the four sides between the
    corner cells, and the four cells of 0.60279: 25.573410) and 0.03 a
    corner more (OCCT's fill of each top corner takes 0.016 more than the
    roundings' sharp intersections);
  - the slab's end face pair: the material taken 14.029008 against
    14.01883 (0.02), and the material both roundings take, the singles'
    sum less the pair's, 6.625046 against 6.62124 (0.01); at 0.9 too.

  The corner cells and the slab's volumes are the union of the roundings'
  removal sets (each a 90 degree edge, the rider's ball touching its face
  and passing through the line, the ends cut by the solid and the slab's
  underside's at the ceiling's plane), integrated on grids of 1/160 and
  0.004. Without the patches the end face's rims alone build as before;
  every other case fails.
- `geometry.fillet_*` and `geometry.suite.*` (the 103 others) pass with the
  patches.

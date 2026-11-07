# A boss's foot and rim rounded together: 1340, 1470

mitcad#1392: a boss rounded at its foot and at its rim in one fillet with
half its height, where the foot's contact on the boss's cylinder lies along
part of the rim's. The pairs whose contacts are one line both ways are
[0153 and 0270](fillet-results.md#0153-chfi3d-roundings-meeting-on-one-contact-line).
Back to the [port's README](../README.md).

## Overview

- **1340** lets a rounding ride on a part of another rounding's contact
  line: the open stripe whose contact lies along a part of the other's
  (which may be closed) rides on that part, the other's SurfData split at
  the part's ends; also where the face's seam lies at an end of the part
  or inside it.

## The case

A cylinder boss of radius R standing h out of a face, coaxial with a
rounding of the same radius on the part it stands on (a bar's half-round
top, a wedge's rounded tip), so that the boss's cylinder and that rounding
are one face. The boss's foot is then only the part of its foot circle
below the top (an open edge), the rim the whole circle (a closed one).
Rounded together with r = h / 2 (a `.f3d` design's boss 2 tall rounded with
1, `5ef2dbd5`'s Fillet7 of its component C2, mitcad#1385), both contacts on
the cylinder lie on the circle at the boss's middle: the foot's open
contact along part of the rim's closed one, the cylinder between them
gone. Each rounding alone builds; together `ChFi3d_StripeEdgeInter`
intersected the two contacts' curves, found one segment covering the whole
foot contact but only part of the rim's, and threw ("fillets have too big
radiuses", `Failure_Intersecting`). 0153 and 0270 record a pair as a ride
only where both contacts are one line both ways and neither stripe is
closed (or the face is a strip between the edges).

## 1340: a rounding rides on a part of another's contact

### 1340-chfi3d-rounding-rides-on-a-part-of-another-contact

- Where 0153 and 0270 record their pairs, an open stripe whose contact on
  the face (its ends inside it or on its seam) lies along a part of the
  other stripe's contact there, not the whole (`partOfContact`: points
  along the open one's contact, between its ends, project onto the other's
  line in order), is recorded as a ride on the other, the face standing
  for the support, as in 0270. Where the open stripe's contact runs over
  the face's seam it has a SurfData on each side: each must lie along the
  other's contact.
- `ApplyRides`: each SurfData of the rider on the face, its ends as the
  corners left them (the foot's contact is run out to the points where the
  face at its ends meets the cylinder tangentially), rides on its own
  piece of the other's line: the other's SurfData holding that contact is
  split at its ends (`splitForPart`, with 0271's `splitSurfDataAt`; the
  cuts in the line's direction are kept in an array of their own, as a
  braced list chosen by `?:` left them to a temporary already gone in the
  loop, garbage cuts with GCC and no ride, mitcad#1399), pieces
  on one surface that make one face, and the rider rides on its piece as
  in 0270: both blends hold its line and leave the face there. The face
  keeps its part along the rest of the other's line (the bar's top, up to
  the rim's contact).
- The piece's ends inside the other's line are not ends of the other
  stripe, whose points 0270 uses, and the rider's ends at the junctions of
  its SurfData are not ends of the rider: each stripe makes its own point
  there in `ChFi3d_FilDS`, made one afterwards (`mergePointsAt`, as for
  0271's cuts), and the shared line keeps one start and one end, the
  piece's (`keepPartLineEnds`: where the rider runs against the line, its
  own ends the other way round would make the rest of a periodic line the
  edge). Whether the rider runs against the line is told from its ends
  (its own line, an analytic blend's cut at its ends, keeps its whole
  period).
- The face's seam. Where the rider ends at the start of the other's closed
  line, on the seam (the design's boss: its cylinder's seam is the wedge's
  tangent line at an end of the foot, where the rim's contact starts), or
  crosses the seam, each stripe cut the seam there as if alone: the foot
  kept the seam's part past the rim's contact, the rim the part the foot
  consumes, and the face was rebuilt with the rim's ring of the cylinder
  left on (an invalid shape, 42 mm³ too large on the design). The seam now
  keeps the face on a side only where both keep it (`combineSeamPoints`
  where the rider's end point is made the other's start, `joinSeamPoints`
  where the points are made one afterwards).

On the synthetic bars (R 4.5, h 2 with r 1, h 2.5 with r 1.25) the
roundings build valid and closed at r = h / 2, with the cylinder's seam on
the top, at either end of the foot and below the top. Their volume
continues the smaller radii's (r - 0.002 and r - 0.001) within 3e-6 mm³,
and the cylinder's area is that of the bar's top from the rim's contact
on. On the design's body before its Fillet7 of C2 the rounding builds at
1, valid, its volume continuing that of smaller radii on the stored state
before it (3804.0742 mm³ against 3804.0743 extrapolated).

**Not covered:** a seam close to an end of the foot (on the bar, 0.13 to
0.5 rad into the foot's range) splits the foot into a short and a long
piece; there the foot's rounding alone is already invalid, and the pair
fails as before.

## 1470: a periodic contact begins at its period boundary

### 1470-chfi3d-periodic-contact-endpoints-snapped-before-splitting

The macOS release test of the same synthetic bars still failed after the
array-lifetime fix of mitcad#1399. At seam pi with h 2, and seam 2.5 with
h 2.5, a contact's first parameter was respectively about -2.74e-33 and
-7.80e-18. Trimming its circle interpreted that tiny negative start as
the previous period's end. Splitting the other stripe then made a torus
wire run beyond one turn: the face was unorientable and had negative area.

Before `splitSurfDataAt` copies and splits a SurfData, each of its two
periodic 3D contact curves now snaps a first or last parameter to the
nearest period boundary only within OCCT's existing `Precision::PConfusion()`.
Other parameters, non-periodic curves, geometric tolerances and the input
shape stay unchanged.

On the h 2 bar at seam pi, the invalid result's volume 1303.968173 mm3
became 1342.168078 mm3 (smaller-radius extrapolation 1342.168084 mm3).
On the h 2.5 bar at seam 2.5, 1320.655558 mm3 became 1372.265407 mm3
(extrapolation 1372.265412 mm3). Both are valid and closed; all seven
original seam/height configurations pass the original volume and area
checks. The tests of a ride in either direction and the periodic-seam
vertex and convex/concave crossing contacts also pass against the newly
built kernel. This is scoped native macOS validation, not a full release
or a result on another platform.

## Tests

- 1340: mitcad#1392, in `geometry/tests/test_fillet_boss_foot_rim.cpp` on
  synthetic bars with a coaxial boss (h 2 with r 1, h 2.5 with r 1.25),
  the cylinder's seam on the top, at either end of the foot, and below the
  top (the foot in two pieces): the result valid and closed, its volume
  continuing the smaller radii's within 2e-5 mm³ and the cylinder's area
  that of the bar's top from the rim's contact on (the fillet fails
  without 1340); the smaller radii still build.
- 1470: the existing OCCT-only synthetic
  `geometry/tests/test_fillet_boss_foot_rim.cpp` reproduces the two invalid
  shapes on macOS without this patch and passes all seven configurations
  with it. All original validity, closure, volume (2e-5 mm3) and area
  assertions remain unchanged. The adjacent regressions are
  `test_fillet_ride_against.cpp` and the periodic-seam vertex and
  convex/concave crossing-contact cases in `test_fillet_results.cpp`.

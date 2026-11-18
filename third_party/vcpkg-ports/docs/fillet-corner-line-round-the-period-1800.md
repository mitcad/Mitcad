# A corner's analytic line round a blend's period: 1800

mitcad#2009: a bent guide imported from a FreeCAD document, its two pads'
ends and its ribs rounded with 0.5, made smaller through its bend radius.
Back to the [port's README](../README.md).

## Overview

- **1800** passes over a line of the analytic intersection between two
  blends that goes round a periodic surface's period outside its domain:
  the corner where two roundings meet is no longer trimmed by a curve that
  runs three quarters of a turn the wrong way round, which gave a valid
  body of the wrong shape.

## The case

The guide is a channel swept through a quarter turn about the vertical
axis, a straight pad 2 long at each end. At a pad's end, the edge between
the end face and the wall's top (two planes) and the edge between the end
face and the wall's outer side (an arc, the pad's outer wall a cylinder)
are rounded with 0.5; they meet at the corner of the end face, where a
third edge, along the pad, stays sharp. `PerformTwoCornerbyInter` builds
the corner from the curve between the two roundings: a cylinder (the
straight edge's rounding, its domain a quarter turn, the contacts at
u = 0 and u = pi/2) and a torus (the arc's rounding).
`ChFi3d_ComputeCurves` takes that curve from `GeomInt_IntSS` of the two
bounded surfaces. Its ends lie on the cylinder's domain boundary.

The pad does not change with the bend radius, but its faces' last bits do.
With the radius set to 31, 32, 34, 37, 41, 42, 43 or 44 (of 31 to 45 in
steps of 1), GeomInt's line ran from u = 4 pi down to u = 5 pi / 2: between
the right ends, the other way round the cylinder, three quarters of a turn
outside its domain. The rounding's face was trimmed by that curve: valid by
the checker, but the rounding of the straight edge covered three quarters
of its cylinder, reached 3.8 beyond the pad's end and added 4.8 mm3 to the
body instead of taking 0.33 away. The same pad from a body read back from a
text BRep file (its numbers rounded to 15 digits) gave the right corner,
which is how the case hid from a replay through a file.

## 1800: the line round the period passed over

### 1800-chfi3d-analytic-line-round-the-period-outside-a-blends-domain-passed-over

`ChFi3d_LeavesPeriodicDomain` brings nine points inside a line's trimmed
pcurve into the period that starts at the domain's start; a point beyond the
domain's end by more than a hundredth of the period means the line leaves
the domain. `ChFi3d_ComputeCurves` passes over such a line of GeomInt's: the
next line is tried, then, as when no line fits, the walk between the known
ends (`IntWalk_PWalking`), which stays inside the domains. A domain of a
whole period is never left, and a line inside the domains is taken as
before. No catch is added.

With it the guide's body is right at every radius of 31 to 45: its volume
falls by 26.220 for each millimetre of radius, as the swept channel's does,
and stays within the two pads' ends.

## What it leaves

Below a bend radius of about 31, the rounding of a rib's inner and outer
edge at the rib nearest each end still fails ("fillet produced an invalid
shape" from 30.5, "the fillet would leave the body open" from 26): the rib
stands 1 degree from the channel's end, and the corner where the rib's
rounding ends on the channel's rim (`PerformMoreThreeCorner`'s plate, the
rib's sloped side going on tangentially into the channel's inside) is set
back along the rim by the rounding's width, 0.508, past the rim's end at
the pad (1 degree of the rim, 0.49 at a radius of 30). OCCT's corner of
more than three edges does not go on past that edge's far end onto the
pad's faces. The same plate at the design's radius of 45 reaches 0.02 out
of the body, which MitCad notes as a warning. Both are for a follow-up.
The first is done by 1801 and 1802 (mitcad#2018,
[fillet-corner-walk-over-vertex-1801-1802.md](fillet-corner-walk-over-vertex-1801-1802.md)).

## Reports and tests

- `cli.fcstd_bend_reported`: the reported design (from
  `MITCAD_MITCAD_REPORTED`, never committed) imported as it is and with its
  bend radius set to 50 and to 32 after the import, as `set_parameter`
  does: every feature rebuilds, and at 32 the body stays within its two
  pads' ends (before 1800: the corner at a pad's end reached 3.8 beyond
  it).
- No body built from OCCT's primitives alone reproduced the line round the
  period: it depends on the input's last bits.

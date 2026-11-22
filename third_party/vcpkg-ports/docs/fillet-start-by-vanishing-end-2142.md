# A rounding's walk not started by a vanishing end: 2142

mitcad#2140, GitHub mitcad/Mitcad#517: the crotch of a Y tube rounded with
more than 7 mm. Back to the [port's README](../README.md).

## Overview

- **2142** does not start a constant rounding's walk at a section narrower
  than 5e-4 of the radius, the width at which patch 0771 takes a walk into
  a vanishing end for that end; the next place along the spine is tried.
  Before, the walk from such a section back towards the vanishing end
  stalled at once and the stripe failed.

## The case

A circle of radius 10 swept along a line of 25 and a tangent quarter arc
of radius 30, mirrored and joined: a Y tube whose arms (two tori) meet
along a crease in the mirror plane, two edges over the crotch's top, whose
angle fades to nothing at both ends where the arms leave the trunk. The
crease was rounded with 6, 6.5 and 7, but from 8 on the stripe failed with
no piece; with the arms' seams turned away from the crotch, 6 failed and 8
to 50 built (20 only 0.1 % smaller).

The walk's trace (patch 2141) showed why. `StartSol` tries places along
the spine from 0.19 % of the first edge, 0.06 from the vanishing vertex. At
7 no section is found there; the walk starts at the next place (w = 6.27),
walks back into the vanishing end (its contacts reaching the vertex) and on
over both edges. At 8 the section there is found, its contacts 5e-5
apart and the faces' normals within 1e-5 rad of each other: the walk back
towards the vertex stopped after 0.0009 without reaching it, and
`ComputeData` gave the line up as dragging, so the stripe failed. The
seams were not the cause.

## 2142: no start by a vanishing end

### 2142-chfi3d-no-start-on-a-section-narrower-than-a-vanishing-end

- `ChFi3d_Builder::StartSol` (of a stripe), for a constant rounding: a
  first section whose contacts are less than 5e-4 of the radius apart is
  not taken; the next place is tried. The first such section is the start
  only where no place along the spine gives another (its faces set again
  where a later place was on another edge), as before.
- Elsewhere nothing changes.

## Reports and tests

- With the patch (first run): the crotch rounds with 6, 8, 20 and 50 (and,
  the seams turned, with 6 and 50), each valid, without a note or a smaller
  radius, the ball's lowest point at the crotch's middle where it should
  be. At 8 and 50 the start skips the first place (contacts 5e-5 apart) and
  takes the next (w = 6.27); the walk reaches both vanishing ends.

- `geometry.y_tube_crotch` (`geometry/tests/test_y_tube_crotch.cpp`):
  `test_crotch_rounded_6` and `test_crotch_rounded_50`, the crotch rounded
  with 6 and 50: one valid solid without a note or a smaller radius,
  ending at both vanishing ends, through the ball's lowest point at the
  crotch's middle.
- `cli.y_tube_crotch_reported`: the reporter's design at 6 and 50 mm.

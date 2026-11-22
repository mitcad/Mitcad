# A trace of a rounding's walk: 2141

mitcad#2140, GitHub mitcad/Mitcad#517: the crotch of a Y tube rounded with
more than 7 mm. Back to the [port's README](../README.md).

## Overview

- **2141** prints, while `MITCAD_TRACE_FILLET_WALK` is set, where the walk
  of a rounding starts, each piece it walks and why it fails, on the
  standard error. Nothing else changes.

## The case

A circle of radius 10 swept along a line of 25 and a tangent quarter arc
of radius 30, mirrored and joined: a Y tube whose arms (two tori) meet
along a crease in the mirror plane that fades to tangency at both ends.
Each torus's seam runs along its outer side and passes the crease's
highest point, where the crease is split into two edges. The crease is
rounded with 6, 6.5 and 7; from 8 on the walk fails with no piece
computed. With the seams turned to the arms' inner sides, away from the
crease (one edge then), 8, 20 and 50 build and 6 fails. The geometry
allows 50: the ball touches each arm inside it and its surface does not
twist.

## 2141: the walk's trace

### 2141-chfi3d-trace-of-the-walk-of-a-rounding

- `ChFi3d_Builder::StartSol` (of a stripe): each place tried for the first
  section along the spine, whether a section was found there, the states
  of its points and their places on both faces. The states start as
  `TopAbs_UNKNOWN` (they were read uninitialised where no section was
  tried).
- `ChFi3d_Builder::PerformSetOfSurfOnElSpine`: the guide's range, each
  piece walked (direction, range, start, flags), its result (spine range
  and ends, on an arc or not), and the stripe's failure.
- `ChFi3d_Builder::ComputeData` (between two faces): what the walk walked
  and each reason it fails.

## Reports and tests

- No test of its own (a trace). `geometry/tests/test_y_tube_crotch.cpp
  --probe` sets the variable round the roundings it tries.

# A rounding's end crossing a periodic edge past its period: 1460

mitcad#1395, the follow-up of mitcad#1371
([fillet-three-face-corner-1380.md](fillet-three-face-corner-1380.md)): a
rounding that builds on the stored body but not on MitCad's, whose faces
are the same within 1e-6 mm but parametrised otherwise. Back to the
[port's README](../README.md).

## Overview

- **1460** takes the crossing of a rounding's end with an edge whose curve
  is periodic in the edge's own period before the edge's p-curves are
  evaluated there (`PerformIntersectionAtEnd`).

## The case

A plate 14 thick (its sides the planes x = ±7) has an arch-shaped slot
through it; the slot's wall y = -11.6155 runs from z = 20 to 24 and meets a
sloped face below. Beside the wall a groove of radius 13.5 (a cylinder
about the z axis) is cut between z = 20 and z = 22, 0.12 mm short of the
plate's side, so that the wall keeps a 0.12 mm edge at its foot there. The
plate's side edge along the wall (x = 7, y = -11.6155, z 20 to 24) is
rounded with 0.3. At the lower end the rounding runs over the sloped face,
over the groove's cylinder (its contact on the wall at x = 6.7 lies in the
groove there) and over the groove's top z = 22: `PerformIntersectionAtEnd`
cuts it by these faces in turn, from one edge between them to the next.

On the stored body the sloped face's edge with the cylinder (an ellipse)
runs from 2.61 to 3.68 on its curve, the cylinder lies on a right-handed
axis along +z and the planes face outward. On MitCad's body (the same
faces within 1e-6 mm) the cylinder lies on a left-handed axis along -z,
the planes face inward (reversed faces) and the ellipse runs from 5.75 to
6.82, past the end of its first period (6.28).

## 1460: the crossing in the edge's period

### 1460-chfi3d-end-crossing-of-a-periodic-edge-taken-in-the-edges-period

The blend is intersected with the curve of each edge between two faces at
the end; the crossing nearest to the edge's range, judged in the edge's
period, is taken, and its parameter gives the cut's end on both faces
through the edge's p-curves. That parameter was used as the intersection
gave it, in the curve's first period: the ellipse was crossed at 6.81 and
the parameter came as 0.53. The ellipse's p-curve on the sloped face (an
ellipse itself, periodic) gave the right point; its p-curve on the
cylinder, a B-spline, was evaluated far outside its range, and the cut
along the groove started 195 mm off along the cylinder's axis. The
cylinder was rebuilt with a wire running down to z = -175: the result was
invalid, 350 mm³ short on the whole body.

The crossing's parameter is now moved into the edge's period, as its
choice already is, wherever the curve is the edge's own (not the
intersection of the faces' extended surfaces). On MitCad's body the
rounding builds valid, 182 faces as on the stored body; the piece of each
body about the end gives the same result, 7.067254 mm³ both.

## Tests

- 1460: mitcad#1395, in `geometry/tests/test_fillet_end_reversed_faces.cpp`
  on the case `fillet-end-on-reversed-faces` of `MITCAD_GEOMETRY_CASES`:
  `test_pieces_alike` (the piece of each body about the end, 9 faces,
  rounded: both closed and valid, with as many faces, the same volume and
  on each other within 1e-5 mm). Without the patch MitCad's piece fails it
  (invalid, 0.51 mm³ short); the stored piece builds either way.
  A synthetic body would need an edge curve whose range runs past its
  period's end with a non-periodic p-curve beside it, as the import's
  bodies come; the design's pieces are the case.

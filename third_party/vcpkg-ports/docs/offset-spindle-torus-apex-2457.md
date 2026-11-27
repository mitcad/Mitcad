<!-- SPDX-License-Identifier: MIT -->
# A spindle torus enlarged up to its apexes, and no shape reported: 2457

mitcad#2457, GitHub mitcad/Mitcad#678. Back to the
[port's README](../README.md).

## Overview

- **2457** stops the enlarged face of a spindle torus (its major radius
  smaller than its minor one, so that it meets itself at two apexes on its
  axis) at the apexes on the face's side, as the offset already stops a
  cone's at its apex; and it makes an offset whose unwinding keeps no shell
  an error (`BRepOffset_UnknownError`) instead of success with no shape.
- The reported body (a shelled block's top with rounded edges and a rounded
  bead ending at a corner in a spindle torus of major radius 0.6 and minor
  1.4) gave no shape for any offset of its faces, also of faces far from
  that corner; with the patch it closes up, and with patch 2418 before it
  (MitCad's port) the result is valid (see
  [Without patch 2418](#without-patch-2418)).

## The case

MitCad's Offset Face moves whole faces by the kernel's offset of the body
(`BRepOffset_MakeOffset`, Skin, the intersection join, each moved face its
own offset, every other face 0), which rebuilds every face of the body. To
intersect the faces' offsets with each other, `EnLargeFace` enlarges each
face; on a periodic surface by a tenth of the rest of the period on each
side. The spindle torus face at the corner runs in v from 3pi/2 to 2pi (from
the tube's bottom to its equator); its apex is at v = -acos(-0.6/1.4), so
the enlarged face (v from 4.24) passed it by 0.03 rad into the torus' other
part. There its offset met its neighbours' a second time (a loop of
B-spline edges round the apex), its own loop did not close, the outer
rounding beside it was left as its untrimmed enlarged face (edges 1e7 mm
long), and the unwinding (`SelectShells`, `Deboucle3D`) dropped the shell
with these free edges: `MakeOffsetShape` then reported success with an empty
shape. MitCad read that as "the offset produced no solid" on Linux, and the
reported 0.72.0 on Windows ended in an access violation at address 0x60.

With the patch the enlarged face stops at the apex (a spindle torus also
given as a `Geom_OffsetSurface` of one, its minor radius with the offset);
`MakeFace` takes the iso there for a point, a degenerated edge. The four
faces moved 2 mm give 23808.4 mm3 in DRAW on the chain without patch 2418,
the sum of their moves.

## Without patch 2418

On the chain without patch 2418 one edge of that result comes out with a
tolerance of 2.9 mm: the tangent edge between two faces whose offset is 0
(a sloped plane and a swept band, a B-spline whose parameter runs 38 times
faster than the edge's), trimmed where the moved faces' new positions meet
them. `BRepOffset_Inter2d`'s `ExtendPCurve` makes the band's straight curve
under the edge (a two-pole B-spline over [-2.72, 0]) a line of unit speed
from its first pole, which puts the curve at the other end of the band.
With patch 2418 before this one, as in MitCad's port, the result is valid:
the Offset Face of the four faces builds, 23801.198 mm3 (MitCad's measure),
as does the torus with Tangent Chain, which grows to the same four faces.

## Reports and tests

- The cli test on the reported design and the lab cases:
  [opencascade-patch-tests.md, section 2457](opencascade-patch-tests.md#2457).
- Report for OCCT: the reported body (not public) is the reproduction; a
  synthetic one is still to be found.

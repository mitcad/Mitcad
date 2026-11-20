# A walked end run on over the flat stretch beyond a curved wall: 1700

mitcad#1926, a follow-up of mitcad#1904 and mitcad#1819 (GitHub #371's
design, its Fillet1): a fin's foot rounded with 3, its chain ending where
the fin's sloping top runs down to the plate, the fin's side a curved wall
bending tangentially into a flat stretch lower than the rounding. Back to
the [port's README](../README.md).

## Overview

- **1700** runs a walked end that the face at the end cuts
  (`WalkedCutToEnd`) on past a curved wall over the wall's own surface
  continued in a span of its own (its curve's last span's polynomial, as
  the bodies stored in the reported design have it; mitcad#1987), not
  stopped at the wall's bounds; the face at the end grows up to the
  cut past the end vertex where the other contact meets its line with the
  other face only there; and a failed end try no longer leaves the stripe
  failed.

## The case

A U-shaped fin with arms 2 thick stands on a plate (z = -547). Its top is
a plane sloping at 30 degrees that runs down to the plate at the arms'
ends; the fin is extruded from its top along the top's normal, so that its
sides lean along the slope. Each arm's outer side is a flat stretch (a
plane, at most 2.676 high) that a curved side (an extrusion of a B-spline
curve) bends into tangentially. The outer foot, a chain of eight edges
from one arm's end round the fin to the other's, is rounded with 3: higher
than each flat stretch all along it, and than the curved side near its
join with the stretch.

OCCT's fillet built the chain at 2.6 (the flat stretch still reaches the
contact at its start) and failed from 2.7 on. The ball's contact on the
curved side reaches the side's edge with the top before the flat stretch
begins; the top holds the spine's end vertex, so the contact's edge is no
obstacle (it lies within four edges of that vertex) and the walk runs on
with that side unclassified, over the curved side's surface, as at any
end the face at the end cuts.

## 1700: the run-on over the flat stretch

### 1700-chfi3d-walked-end-runs-on-over-the-flat-stretch-beyond-a-curved-wall

Past the flat stretch's edge with the curved side, the curved side's
surface unbounded is the B-spline curve's polynomial extrapolation, which
bends in from the plane (1.2 mm short of the end the run-on's contact lay
0.3 mm inside the plane). The walk stopped there; `WalkedCutToEnd` walked
on from that section over a face of the curved side's surface, bounded at
its curve's ends, and failed, and its inner walk left `done` false, so the
whole stripe failed ("the geometry kernel's computation of the rounding
fails"), although the try was meant to leave the stripe as it was.

- `ChFi3d_RunOnSurface` (new): where N, the face the contact left, is an
  extrusion whose basis curve ends at an edge J of N across which a plane
  goes on tangentially to N (not the other contact's face, not the face at
  the end) and holds J's tangent line (checked within the approximation
  tolerance), N's surface with the basis curve continued past J by the
  polynomial of its span at J, in a span of its own: the span as a Bezier
  curve segmented past its end, as long as its tangent stays within 45
  degrees of J's (at most twice the spine's length), J's knot then removed
  (the two spans are one polynomial), N's own parameters unchanged.
- mitcad#1987: the patch's first form continued the curve by its tangent
  line, so that past J the surface was the flat stretch's plane. The
  bodies stored in the reported design run the rounding on over the
  polynomial instead: their foot on the plate turns in from the plane's
  by 0.01 mm at y 430.2, 0.08 at 432.8 and 0.47 at the arm's end (435.3),
  which a ball on the plate and the polynomially continued side gives
  within 0.02 mm (0.009, 0.081, 0.45), and the plane left Fillet1 0.5 mm3
  apart from the stored body at each arm's end.
- The walk on over the polynomial needs two more things: `WalkedCutToEnd`
  makes the face of the run-on surface it walks on bounded in v too (N's v
  range widened by the spine's length both ways; on an open face the walk
  reached the face's ends at edges without vertices and threw), and
  `ChFi3d_KPartCutPiece` no longer keeps the cut behind the section at an
  end given as such (the point where the cut meets the other contact):
  where the other contact turns in towards the face beyond, the cut runs
  0.045 past that point's section before it comes back to it, and no
  section bounds the piece there.
- The top's 0.5 riding on this blend to the plate (patch 1728) then ends
  at a fold of its walk; 1728's `rideFoldEnd` makes that end exactly
  ([fillet-ride-on-stripe-at-end-1728.md](fillet-ride-on-stripe-at-end-1728.md)).
- `WalkedCutToEnd`, with such a surface: the blend is walked again from the
  end piece's other end over it (as for a tight tangent face at the end,
  1540), the piece up to the crossing taking the end piece's place, also
  where the first walk ran on past the spine's end. Where the other
  contact's line meets the line of the edge Eq between its face and the
  face at the end only past the end vertex, with both faces' surfaces on
  that line up to there (both planes here), Eq is prolonged there and the
  face at the end grows up to the cut, the end in a point at a vertex of
  its own (as `ChFi3d_KPartCutToEnd` does, 0650). The flat stretch lies
  wholly under the blend: its edges are cut at its vertices, so that the
  builder drops it with them (as `ChFi3d_CutConsumedFaces` does), and its
  edges with the face at the end are passed to `ChFi3d_KPartCutPiece` as
  cut edges (the face at the end grows over them).
- `PerformSetOfSurf`: where every end try fails, `done` is restored as it
  was before them (only after a chamfer over a narrow face before).
- The history (`myEVIMap`, which `Generated()` reads): the walked piece
  that takes the end piece's place and the piece the face at the end cuts
  are generated by the spine's edges the end piece was, as the other
  patches that replace pieces keep it. Without that their faces were in
  no edge's `Generated()` and MitCad's fillet left them without a name, so
  that no later feature could refer to them or to the top's edges with
  them.

## Results

On the design's state before Fillet1, the 3 mm set alone: valid, both flat
stretches gone, the top grown out over the rounding (from x -292.931 to
-270.431), the volume 6110.5095 mm3 against 6110.5302 for the same rounding
of the fin raised along its top's normal and cut back by the top's plane
(3e-4 of the 50.1 mm3 the rounding adds: the walked surfaces'
approximation). Through MitCad's fillet the set builds at its size with no
note. (These figures are the first form's, over the flat stretch's plane;
with the polynomial run-on (mitcad#1987) the rounding lies a little under
that reference.) The whole Fillet1 (fifteen sets at once) still fails: its rounding of
0.5 along the top's edges runs along the flat stretches the rounding of 3
takes away.

## Reports and tests

- `geometry.fillet_wall_run_on`
  (`geometry/tests/test_fillet_wall_run_on.cpp`): with
  `MITCAD_GEOMETRY_CASES`, the case `fillet-wall-run-on`, the body before
  the rounding: the outer foot rounded with 3 by OCCT's fillet alone builds,
  valid, the flat stretches gone, the top grown out, the foot on the plate
  along each flat stretch that of a ball on the plate and the curved side
  continued by its polynomial within 0.01 mm (turned in by more than 0.3 mm
  at the arm's end), the volume under the raised reference's (the flat
  run-on) by at most a twentieth of the material added, and every face of the
  result an input face or generated by an edge of the foot (before the
  history part, two faces were neither). Fails without 1700 (the
  fillet fails). No synthetic body: a U of the same build whose curved
  sides are cubic B-splines went another way at its ends (OCCT built an
  invalid result with and without the patch).
- `cli.f3d_edit_reported`: the design's 3 mm set added alone before
  Fillet1 builds.
- OCCT's own blend and chamfer groups (the private suite's `run.sh`, a
  DRAW build of the port with 1700, TKFillet without and with the patch):
  754 cases each, no case changed but `blend/encoderegularity/A1`, which
  0074 broke (the case crashed) and now passes (area 52045.8 against stock
  8.0.1's 52044.6, the case's own check 52047).

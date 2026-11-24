<!-- SPDX-License-Identifier: MIT -->
# A closed guide's end taken only near the section: 2272

mitcad#2272, GitHub mitcad/Mitcad#582: a nozzle's foot on a dome that runs
on into a tube, rounded with 1.5. Back to the [port's README](../README.md).

## Overview

- **2272** lets a walk that ends on both faces' restrictions at once take
  the end parameter of a closed guide only where the guide's end lies near
  the section found, and a walk that ends on one restriction correct its
  end only to an end of that restriction near the point found on it.

## The case

A tube 40 to 50 about z, 50 high, and on it a hollow hemisphere of the
same radii: the dome's outer face runs on tangentially into the tube's
along the circle at z = 50. A nozzle of radius 15, its axis on that circle
(z = 50), extruded from a plane 80 from the axis up to the dome's face
continued, joined: its side meets the dome above z = 50 and the tube below,
so its foot is a closed tangent chain of two edges whose two vertices lie on
the circle. The nozzle's side is one face closed round its axis, its seam
through one of the two vertices. The design repeats the nozzle four times
about z.

Rounded with 1.5 (the reporter's radius), the foot built only when the
chain's first edge started at the vertex the nozzle's seam runs through: on
three of the four nozzles when picked by one of its edges, never by the
other, and on the reporter's original nozzle by neither ("the geometry
kernel's computation of the rounding along edge … (and 1 more edges of its
chain) fails", `Failure_Stripe`). The vertices lie on the exact points
(within 1e-9 mm), their tolerances below 5e-7, the two edges' tangents
within 6e-7 degrees there: the geometry is clean.

The walk's trace (patch 2141) shows where it went wrong. The walk on the
faces of the chain's first edge (the dome or the tube, and the nozzle's
side) runs from the chain's first vertex to its other one, where the
contact on the body reaches the circle (the face's restriction) and, where
the seam runs through that vertex, the contact on the nozzle reaches the
seam (the other face's restriction): both at once. Its line came out with
30 points along half the foot but the parameters 0 to 0, or 0 to the
guide's period, instead of 0 to 43.9 or 50.7.

## 2272: the guide's end taken only near the section

### 2272-brepblend-walk-ending-on-both-restrictions-takes-a-closed-guides-end-only-near-it

- `BRepBlend_Walking::InternalPerform`, a step that ends on both faces'
  restrictions (`Blend_OnRst12`): for a periodic guide the walk tests the
  guide's ends saved before its extension (`GetSavedFirstParameter`,
  `GetSavedLastParameter`) by the plane through the guide's point at the
  end and the section's two points, and takes the end's parameter where
  that plane is normal to the guide there. Round the nozzle's foot both
  vertices and the section's points lie in the plane of the circle, normal
  to the guide at both vertices, so the section at the chain's other
  vertex, half a turn from the guide's ends, took the parameter 0 or the
  period.
- Now an end is tested only where the guide's point at the end lies within
  the distance between the section's two points (at least 10 times the
  walk's 3d tolerance) of the guide's point at the parameter found. A walk
  that closes on its start keeps the end's parameter as before.
- `BRepBlend_Walking::CorrectExtremityOnOneRst` (OCC25701), a walk that
  ends on one face's restriction: it takes the end of that restriction
  nearest the guide and, where the same plane test passes, moves the walk's
  end to that end's parameter on the guide. With the nozzle across the
  dome's and the tube's seams (its foot then has four edges) and the
  chain's first edge starting at the vertex the nozzle's seam does not run
  through, the walk's first piece, going back from its start, ends at that
  vertex on the circle alone. The restriction is the circle between the
  foot's two vertices, both on the guide; the first in the order of its
  ends was the other vertex, the plane test passed (the same plane), and
  the piece started at the parameter of the vertex half a turn away
  (w -43.9 instead of 0).
- Now an end of the restriction is taken only where it lies within twice
  the distance between the point found on the restriction and the guide's
  point at the parameter found (at least 10 times the walk's 3d tolerance)
  of that point; without such an end nothing is corrected (as without a
  projection before).
- Elsewhere nothing changes.

## Reports and tests

- Before the patch (the probe of `geometry.fillet_nozzle_dome_junction`,
  with `--probe` and `MITCAD_NOZZLE_BODY` for the reporter's body): the
  kernel's fillet alone failed on the reporter's original nozzle by both
  edges and on two copies by their tube edges; the copy across the dome's
  seam (four edges, its first vertex on the nozzle's seam) built by both.
  The synthetic nozzles failed the same way: by the tube edge at every
  angle, by the dome edge at one.
- With the first change alone (bundle-dev-132's check): every pick built
  but the synthetic nozzle across the seams, picked by its tube edge (the
  end on one restriction, the second change).
- The OCCT lab case `2272-nozzle-foot-on-dome-edge.tcl` (the body in DRAW,
  the nozzle less the dome's whole sphere): without the patch the nozzles
  `n0m` and `n0u` fail by both edges and `n270` by one half of its chain
  as the dome or the tube edge; with it every pick builds, the two seeds of
  `n270` within 5e-4 mm3. The lab's other cases are unchanged (2306's
  listed difference is the reference's log older than its case: the case
  run on the slot without the patch prints the same lines), and OCCT's own
  blend and chamfer groups give no fixed, broken or changed case (37 fail
  on both chains).
- The OCCT lab case `2272-nozzle.tcl` (the reporter's body before its
  copies' roundings, exported as .brep beside it): each edge of each of the
  four feet rounded alone with 1.5 by DRAW's `blend`. Without the patch the
  original nozzle (along -y) fails by both edges and the other three build
  (by DRAW's blend also the two copies whose tube edges failed in the
  test's probe through MitCad's import of the same file); with it all
  build, valid, each foot's picks adding the same volume. The lab's other
  cases are unchanged.
- `geometry.fillet_nozzle_dome_junction`
  (`geometry/tests/test_fillet_nozzle_dome_junction.cpp`):
  `test_foot_across_the_dome_edge`, the nozzle at four angles about z, its
  foot rounded with 1.5 by the kernel's fillet alone and by MitCad's,
  picked by either edge: one valid closed solid at the exact size without a
  note, the material added the same at every angle;
  `test_foot_on_the_dome`, the control wholly on the dome.
- `cli.fillet_nozzle_dome_reported`: the reporter's design: the original
  nozzle's foot by either edge, a copy's by its tube edge, and the foot in
  the rounding the pattern repeats.

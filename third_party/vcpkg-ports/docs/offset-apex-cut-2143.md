# An offset face's apex cut only where it has a solution: 2143

mitcad#2140, GitHub mitcad/Mitcad#517: a Y tube shelled after its crotch
is rounded. Back to the [port's README](../README.md).

## Overview

- **2143** cuts the apex of an offset face with a degenerated edge (the
  inward offset of a cone's apex or a sphere's pole) only where the
  apex's construction has a solution; elsewhere that side is left as it
  is. Before, the construction read a projection that did not exist and
  the thick solid raised `Standard_OutOfRange` without a message.

## The case

A circle of radius 10 swept along a line of 25 and a tangent quarter arc
of radius 30, mirrored and joined; the crotch (a crease fading to tangency
at both ends) rounded with 6 or 50 (patch 2142), then shelled 2 mm inwards
with its three ends open. Each rounding face ends in a point where the
crease fades: a degenerated edge at its vanishing end (patch 0130).

`BRepOffset_Offset::Init`, for a face with a degenerated edge whose offset
surface's iso there is longer than 1e-5 (`TolApex`), takes that edge for an
apex: it crosses the generatrix with the apex line (through the degenerated
point, square to the plane of three points of the offset iso) and, offset
inwards, projects the crossing onto the opposite iso to cut the surface
there. At the rounding's vanishing end the offset iso is micrometres long
and nearly straight, and the crossing projects nowhere:
`GeomAPI_ProjectPointOnCurve::LowerDistanceParameter` read a solution that
does not exist and raised `Standard_OutOfRange` without a message (the
debugger's stop: TKGeomAlgo under `BRepOffset_Offset::Init` from
`MakeOffsetFaces`). MitCad said "the geometry kernel's offset raised an
error".

## 2143: the apex cut where it has a solution

### 2143-brepoffset-apex-cut-only-where-its-construction-has-a-solution

- In each of the four sides, the apex is cut only where the generatrix
  crosses the apex line (an extremum of `GeomAPI_ExtremaCurveCurve`) and,
  offset inwards, the crossing projects onto the iso (a point of
  `GeomAPI_ProjectPointOnCurve`). Elsewhere that side is not cut and does
  not count as a singularity: the degenerated edge's offset is made as for
  any other edge. Where the construction has its solutions, nothing
  changes.
- A degenerated side is taken for an apex only where its offset iso is
  longer than 2 % of the offset (and than `TolApex`, 1e-5): at an apex the
  surface's normals turn about the point, at a rounding's vanishing end
  they stay parallel. With the guard alone (the first run) the shells after
  the rounding were built and valid, but on the rounding's other face the
  projection found a meaningless point at the face's far end, the offset
  face was cut to nothing there, its edges' curves on it collapsed to a
  point, and the tolerances grew to tens of millimetres (every test point
  classified on the shell).
  The vanishing ends' offset isos measured 0.0045 (rounded with 6) and
  0.0024 (with 50) for a wall of 2, a turn of the normals of about 0.1
  degree; a first threshold of 1e-3 of the offset was below both.
- With `MITCAD_TRACE_OFFSET_CREASE` set, each degenerated side's offset iso
  length and the threshold are printed.

## Reports and tests

- `geometry.y_tube_crotch` (`test_shell_after_rounding_6`,
  `test_shell_after_rounding_50`) and `cli.y_tube_crotch_reported`: the
  shell after the rounding: one valid solid, the wall 2 mm thick by points
  in the trunk, both arms and under the rounding, the ends open, the
  largest tolerance within 0.01 mm (measured 0.0023 after 6, 0.0013 after
  50). With the guard alone the shells were built but with tolerances of
  tens of millimetres; with the threshold both pass, as does the reported
  design's Shell after its Fillet at 6 and 50.

# Booleans on faces that run many turns: 0510–0512, 0560, 0591–0593

Stored bodies can keep a thread's cylinders, flanks and helices as one face
or edge over tens of turns. These patches let booleans cut such faces and
bound their work. Back to the [port's README](../README.md).

## Overview

- **0510 to 0512** let booleans cut stored faces and edges that run many
  turns (a threaded hole's cylinders, flanks and helices): new curves are
  put into the copy of the period inside such a face, points are classified
  at every copy of them in its range, and a long B-spline's roots on a
  quadric are sampled by its knot spans (mitcad#349).
- **0560** checks a curve moved on a face spanning many periods where it is
  moved to, so that a second boolean cuts the pieces a first one left of
  such a face (mitcad#380).
- **0591 to 0593** bound the work of booleans on faces running many turns:
  a face's samples count the knot spans in its range, the face merge tests
  each surface's planarity once, and a projection's grid holds at most 1001
  parameters a direction (mitcad#235).

## 0510 to 0512: cutting faces and edges of many turns

The patches 0510 to 0512 let booleans cut stored bodies whose faces and
edges run many turns (mitcad#349): a stored body can keep a threaded hole's
minor and major cylinders as one face each between helices running tens of
turns (the face's u range spans as many periods), its flanks as one ruled
B-spline face each, and the helices as one B-spline edge each.

### 0510-boptools-pcurve-into-the-period-inside-a-face-spanning-many

`BOPTools_AlgoTools2D::AdjustPCurveOnSurf` moves a boolean's new curve on
such a face into the first period of the range and, when the classifier
finds its middle outside the face, by one period up or down; at a given
height only one copy of an angle lies inside the face, often many periods
away. A box cut across the thread got its section curves on the cylinders
outside the face; the result was invalid and its volume wrong. When the
curve's middle is outside the face, every copy by whole periods within the
range is now classified and the curve moved into the first one inside the
face; the one-period shift remains only when none is.

### 0511-inttools-fclass2d-every-copy-on-a-face-spanning-many-periods

`IntTools_FClass2d::Perform` and `TestOnRestriction` try a point's copies
upwards from where `GeomInt::AdjustPeriodic` put it, which leaves a point
already in the face's range where it is; the copies below it were never
tried. A section curve's point in the surface's first period lay in the
range of the 32-turn face, the copy inside the face 16 periods below: it was
classified outside and the parts of section curves on the face were dropped
(the box cut removed nothing). The copies now start at the first one at or
above the range's start, as `BRepTopAdaptor_FClass2d` does.

### 0512-intcurvesurface-quadric-roots-sampled-by-knot-spans

Toolkit: `TKGeomAlgo`.

The exact intersection of a curve with a plane, cylinder, cone or sphere
samples its distance to the quadric
`IntCurveSurface_TheHCurveTool::NbSamples` times, at most 50 for the whole
curve: every two thirds of a turn on a helix edge of 32 turns, and where the
helix dips below a cylinder across the hole's end and rises again within one
step neither crossing was found (`IntTools_EdgeFace` found one of three).
The section curves on the hole's faces lost their ends, and the tool's face
over the hole's opening was left whole with them inside as internal edges:
the cut kept it as an internal face. A B-spline is now sampled at least its
degree times for each knot span in the interval (at most 100000); other
curves keep their count.

## 0560: a curve checked at its own copy in the face

### 0560-boptools-pcurve-checked-at-its-own-copy-in-the-face

`AdjustPCurveOnSurf` called `GeomInt::AdjustPeriodic` with the curve's
middle u as both its input and its output, so the classifier of a face
spanning more than one period (0510's) checked the point moved twice, and
the shift was corrected from there. After a cross hole splits a 32-turn
cylinder face into pieces on the one surface, the piece below it lies 16
periods below the surface's first period: a box cut's section curves on it
were checked 32 periods down and placed a period above their first one,
outside the piece, and the cut removed nothing and was invalid. u now keeps
the curve's own value, so the point checked is where the curve goes; faces
of at most one period, and curves the shift into the range does not move,
are not affected (mitcad#380).

## 0591 to 0593: bounded work

The patches 0591 to 0593 bound the work of booleans on faces whose
B-spline surfaces run many turns (mitcad#235): a thread built as the
stored bodies keep it has one face per profile edge over all its turns,
its helices interpolated at about 140 points a turn (4000 knot spans for
25 turns), and is split into pieces of a third of a turn on the one
surface for the booleans. The cut of such a thread took 19 s (87 s of
work) and its union with the part 13 s and 7.5 GB; with the three patches
3 s and 3 s, 0.8 GB.

### 0591-adaptor3d-bspline-samples-within-the-adaptors-range

Toolkit: `TKG3d`.

`Adaptor3d_HSurfaceTool::NbSamplesU` and `NbSamplesV`, the polyhedron of the
edge-face intersections, count a B-spline's knot spans within the adaptor's
range (a face's) instead of all of the surface's; for a face of the whole
surface the counts are as before.

### 0592-shapeupgrade-unifysamedomain-planarity-once-per-surface

The face merge tests each surface for planarity once
(`GeomLib_IsPlanarSurface`, which samples the whole surface), not once for
each pair of neighbouring faces; the pieces of a thread's faces met their
tangent neighbours with that test each time. Results are the test's own.

### 0593-extrema-genextps-grid-of-very-many-knot-spans-sampled-evenly

Toolkit: `TKGeomBase`.

The grid of a point's projection onto a B-spline holds at most 1001
parameters in a direction (as many as `Adaptor3d_TopolTool::BSplSamplePnts`
samples at most), taken evenly through the list of the degree's number of
parameters in each knot span where that list would be longer, so the grid
stays dense where the knots are (spaced evenly over the range, it missed a
surface whose knots crowd into a hundredth of its range: OCCT's buc60555_3
cut a part away, mitcad#1902); every projection computes the distances to all
of the grid's nodes, edges and cells, and each face's grid was kept in every
thread's context.

## Reports and tests

- 0510 to 0512: mitcad#349 and the threaded block whose cylinders, flanks
  and helices run 32 turns in `geometry/tests/test_multi_turn_faces.cpp`
  (its box cuts fail without 0510 or 0511; its cross hole checks the
  helices' crossings that 0512 finds).
- 0560: mitcad#380 and the box cuts on the threaded block's cross-holed
  result in `geometry/tests/test_multi_turn_faces.cpp`.
- 0591 to 0593: mitcad#235 and the threaded block's 32-turn flanks in
  `geometry/tests/test_multi_turn_faces.cpp` (the samples of a face on a
  third of a turn of a flank, fail without 0591; the grid nodes of a
  projection onto a flank, fail without 0593; 0592's merge after a cut
  keeps the result, its work is not observable from outside the merge).

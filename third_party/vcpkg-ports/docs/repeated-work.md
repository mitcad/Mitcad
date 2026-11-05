# Repeated work removed: 0006–0009, 0060, 0120, 0121, 1050, 1270

These patches change no result. Each removes repeated work that made an
OCCT algorithm quadratic (or worse) in the edges or wires of a face or a
wire, which stored bodies with thousands of holes or long wires reach.
The same kind of bound for booleans on faces that run many turns is in
[multi-turn faces](multi-turn-faces.md) (0591 to 0593). Back to the
[port's README](../README.md).

## Overview

- **0006 to 0009** change no result: each removes repeated work that made
  an algorithm quadratic in the edges or wires of a face, which bodies with
  thousands of holes reach (mitcad#39, mitcad#72, mitcad#77, mitcad#79).
- **0060** changes no result either: it removes repeated work that made
  ShapeFix's repair of intersecting edges quadratic in the wires of a face
  (mitcad#60).
- **0120 and 0121** remove repeated work that made the checker quadratic in
  the wires or edges of a face (mitcad#119).
- **1050** reads a wire's edges into an array once in ShapeFix's search for
  intersecting edges of one wire instead of by number from the wire's
  sequence for every pair, which made healing a body with long wires grow
  with the cube of their edges (mitcad#642).
- **1270** keeps the open edges of a shell ShapeFix builds face by face
  instead of going through all of its edges after every face added, which
  made the face merge after a boolean quadratic in the faces of a large
  shell (mitcad#1273).

## 0006 to 0009: booleans and the checker

### 0006-shapeupgrade-unifysamedomain-seam-search-by-vertex

The face merge after booleans (`ShapeUpgrade_UnifySameDomain`) looks up the
edges it removed by vertex instead of going through all of them at each step
of a wire across a seam.

### 0007-brepcheck-analyzer-no-search-in-face

The checker (`BRepCheck_Analyzer`) no longer searches a face for each of its
own vertices and edges (`InContextOfAncestor`).

### 0008-brepcheck-wire-seam-edges-once-per-face

The checker finds the seam edges of a face once instead of once per wire for
the 2d closure check, and no longer searches the face for each of its wires
(`BRepCheck_Wire::InContextOfFace`).

### 0009-bopalgo-face-face-adaptors-once-per-face

The face/face intersections of a boolean build the surface adaptor of each
face (its UV bounds go through all its edges) once instead of once per pair
of faces (`IntTools_Context::SetSurfaceAdaptor`).

## 0060: ShapeFix's repair of intersecting edges

### 0060-shapefix-intersecting-edges-once-per-face

The repair of a wire's intersecting edges
(`ShapeFix_IntersectionTool::FixSelfIntersectWire`, once for each wire of a
face) applied the healing context to the whole face and went through all its
vertices for their largest tolerance. The face is applied with
`ShapeBuild_ReShape::ApplyAgain`, which gives the previous call's result and
status back while no replacement has been recorded since (that call recorded
none either: it rebuilt nothing), and the largest tolerance is looked up
only for a wire with edges that may cross, before anything changes. The
repair of intersecting wires (`FixIntersectingWires`) lists the edges of the
second wire of a pair only when the boxes of the two wires meet. The healing
context's class gets members (code built against the port's headers must be
rebuilt with them).

## 0120 and 0121: the checker on bodies with thousands of holes

The patches 0120 and 0121 change no result either: they remove repeated
work that made the checker (`BRepCheck_Analyzer`) quadratic in the wires
or edges of a face, on stored bodies of thousands of holes (mitcad#119).

### 0120-brepcheck-face-wire-boxes-of-conics-past-their-range

The check of how a face's wires lie inside one another
(`BRepCheck_Face::ClassifyWires`) skips pairs of wires whose UV boxes do not
meet, but gave up a wire's box when an edge's range reached past its
p-curve's own parameters, which circles and ellipses of stored bodies do by
a rounding error (an arc ending at 2π + 4 · 10⁻¹⁵): every pair of such wires
was classified (8.6 million on a face of 2 929 holes, 3.5 s). A circle's or
an ellipse's box is now the conic's over the edge's range.

### 0121-brepcheck-shell-orientation-edges-listed-once-per-face

The shell's orientation check (`BRepCheck_Shell::Orientation`) searched each
face at each edge for the edge; a face of 64 edges or more now lists the
orientations of its edges once (9.6 s → 0.1 s on a shell whose two faces
hold 23 428 edges each).

## 1050: ShapeFix's search for intersecting edges of one wire

The patch 1050 also comes from mitcad#642: after 1040 the larger part of
healing the bin's large stored bodies went to the search for intersecting
edges of one wire.

The patch 1040 is in [booleans and the face merge](booleans-and-face-merge.md#1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis).

### 1050-shapefix-self-intersecting-wire-edges-read-once

`ShapeFix_IntersectionTool::FixSelfIntersectWire` reads edge num1 and edge
num2 of the wire for every pair of numbers, by number from
`ShapeExtend_WireData`'s `NCollection_Sequence`, whose access walks node by
node from the place read last: each pair walked about twice the distance
between the two numbers, and half the stack samples of healing the bodies
stood in that walk. The edges are read into an array in one walk and taken
from it; the fixes change the wire only for a pair whose edges intersect,
and the array is read again after such a pair, so the fixes see the same
edges as before.

## 1270: ShapeFix's orientation of a large shell's faces

### 1270-shapefix-shell-closed-shell-found-as-faces-are-added

`ShapeFix_Shell::FixFaceOrientation` (its `GetShells`) builds a shell
face by face. Where more than two faces share an edge (its multi-connex
mode) it tested after every face added whether the shell had closed, with
`BRep_Tool::IsClosed`, which goes through every edge of the shell. The face
merge after a boolean (`ShapeUpgrade_UnifySameDomain::UnifyEdges`) runs it
on each shell whose faces it changed: on the union of a machine's 257
bodies, one shell of 8 800 faces, 22 of 25 stack samples of the 14 s merge
stood in those tests. The edges used an odd number of times (as `IsClosed`
toggles them) are kept in a map as the faces are added; the shell is closed
when it has an edge that counts and none is left over, the same test, and
the map is emptied with each new shell.

## Reports and tests

- 0006 to 0009: the reports mitcad#84 (0006), mitcad#90 (0007), mitcad#91
  (0008) and mitcad#92 (0009).
- 0060: mitcad#60 and `geometry/tests/test_shapefix.cpp`, whose expected
  results are OCCT's without the patch.
- 0120 and 0121: mitcad#119 and `geometry/tests/test_large_bodies.cpp`,
  whose statuses are OCCT's without them.
- 1270: mitcad#1273, the union of the reported design of GitHub #133 in
  `geometry/tests/test_join_many_bodies.cpp` (its results are the same
  without the patch; only its time changes).

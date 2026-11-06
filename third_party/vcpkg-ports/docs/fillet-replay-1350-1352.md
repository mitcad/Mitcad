# Fillets of stored designs: 1350 to 1352

mitcad#1349: a fin's top rounded across the transitions from its arcs to its
spline stretches, where the crease between the root's chamfer and the piece
of the arcs' chamfer beside it is concave by about 5 degrees, on the
import's own body instead of the file's stored state. The rounds before are
in [1330 to 1332](fillet-boss-tangent-1330-1332.md). Back to the
[port's README](../README.md).

## Overview

- **1350** cuts a rounding whose walk rolled on a slight concave crease (its
  faces' normals between a hundredth and a tenth of a radian apart, which
  OCCT takes for tangent) by the face beyond the crease instead
  (`RoundingOverSlightCreases`): the analytic blend before that face runs
  on past its end, cut by that face and by the next piece's other face up
  to where those cuts meet; the piece walked after the crease is cut by
  the face from where its contact crosses the crease up to where its other
  contact leaves that face's material; the face grows up to the other face
  between the two cuts.
- **1351** keeps all the pieces of an analytic blend split into its faces'
  domains at the spine's last end where none of them runs into a face at
  the end, as at the first end (`SplitKPart`).
- **1352** gives the piece patch 1310 adds past the spine's end the end
  piece's own surface, so the two are one face (`ChFi3d_KPartCutToEnd`).

## The case

The fin of a bracket (`5ef2dbd5` of mitcad#343, item 87; its top's chamfer
and the rounding up to the fin's end are in [1300 to 1302](fillet-replay-1300-1302.md)
and [1310 and 1311](fillet-replay-1310-1311.md)), 1 thick: its top runs as
arcs (cylinders) and spline stretches (extrusions). The arcs' edges are
chamfered by 0.263 at 45 degrees (cones); beside the stretches the fin's
sides are a chamfer of its root, B, which meets the top directly. Between
each cone and B lies a small transition piece C of the arcs' chamfer (a
ruled B-spline face of 0.4 mm², its surface the chamfer of the whole
stretch, meeting the top along x = ±0.237); three transitions a side. The
top's edges are rounded with 0.2 along the whole chain.

## 1350: the transitions cut by their pieces

### 1350-chfi3d-rounding-over-a-slight-concave-crease-cut-by-the-face-beyond

The crease between B and C is concave by 5.1 degrees, under the tenth of a
radian `ChFi3d::IsTangentFaces` takes for tangent. Coming from the arc's
torus A along the chain, the walk went on over C (one piece), rolled the
ball on the crease (a piece on the edge) and walked on over B (W): two
blend faces and C's remainder at each transition where the stored body has
one face, C itself grown round the roundings' ends (every sample of the
stored face within 1e-6 mm of C's surface). 147 faces against the stored
body's 134 on the file's own state, the stored body up to 0.013 mm off the result.

The stored body keeps the material the roundings take away outside C's
material there. The torus runs on past the arc's end, on its own surface;
the stretch's top T (an extrusion whose crease with the arc's cylinder is
1.4 degrees, convex) cuts its side on the top, and C cuts its side on the
cone; the two cuts meet at a point Q on the line of T and C. W, whose
contact on B reaches the crease at a point P, is cut by C from P up to the
point K where its contact on the top leaves C's material: before K its
section lies partly inside C's material, and from P back to the arc wholly.
C grows over the gap, up to T between Q and K (its line with T,
x = ±0.237), and the top keeps its edge with C there.

`RoundingOverSlightCreases`, after a fillet's stripe is computed
(`PerformSetOfSurf`), finds each piece that rolls on an edge E that is no
edge of the spine, whose faces are tangent for OCCT but whose outward
normals at E's middle lie more than a hundredth of a radian apart, across a
concave crease; W, the walked piece beside it on one of E's faces (B) that
ends on E; and A, the first piece on the other side whose faces are
neither E's other face C nor W's other face T: an analytic blend whose
contacts end on its faces' edges with C and with T. Then:

- Q is where the line of T and C (`GeomInt_IntSS` of their surfaces
  extended, `ChFi3d_WideSurface`) meets A's surface past its end between
  its contacts, nearest the end, refined by Newton's method on the signed
  distances to both surfaces.
- A's cuts by T and by C from its end to Q are `ChFi3d_KPartCutCurve`'s;
  T's surface, an extrusion of a profile's stretch that ends at the arc, is
  run on past that end first (`ChFi3d_RunOnExtrusionCurve`, 1311). The
  piece lies on A's surface, so A and its run on are one face, and ends in
  a point at Q (a new vertex).
- K is where W's other contact crosses C's surface; W's cut is
  `ChFi3d_NarrowCutPiece` (0531), ending in a point at P on E (E kept from
  P on, towards W's contact), and the rest of W is trimmed from K, its
  contact on T starting at K's vertex.
- The pieces between A and W are dropped. The stripe is split at each
  transition into parts ending in points, stripes of their own after the
  first (`myNarrowParts`, as 0531's parts), the last one taking over the
  stripe's end at the spine's last vertex; the parts' pieces are generated
  by the spine's edges they run along (1302).
- C's growth is the curve of T and C from Q to K (`ChFi3d_FacesCurve`),
  each face keeping its side as 0531's growth curves do.

A transition any step of which fails is left as the walk made it.
`ChFi3d_KPartCutCurve` itself now gives a cut whose projection onto a
surface fails the p-curve through its points' projections
(`ChFi3d_PCurveByPoints`, the surface's bounds widened a little): W's cut
starts at P on the boundary of W's surface, its end section, where
`GeomProjLib` gave none.

On the file's state before the rounding (with 1351 and 1352) the result
has the stored body's 134 faces; at the transitions the grown pieces and
the tori lie on the stored body's faces within 1e-4 mm, the stretches'
walked roundings within 0.0005 mm; Q and K lie within 0.008 mm of the
stored body's vertices (K moves along T and C's line, which meet at a few
degrees). On MitCad's own body the rounding (item 87) now gives the file's
10 new faces, 23 before.

## 1351: the last end of a split analytic blend

### 1351-chfi3d-splitkpart-last-end-keeps-its-pieces-as-the-first

`SplitKPart` keeps, at an end of the spine, the pieces of an edge's
analytic blend up to the one whose contact runs into a face at the end.
Where none does, it kept them all at the first end and removed them all at
the last, and the edge's blend was walked instead (0970 kept them at the
last end only where it is a wall's corner tighter than the ball). The fin's
last arc reaches the fin's end face, which goes on from the arc's top
tangentially; the arc's torus reaches that face only past the spine's end,
where 1310's `ChFi3d_KPartCutToEnd` cuts it. On one side of the fin the
spine starts there and the torus was kept; on the other it ends there, and
the torus was walked: a B-spline face where the stored body has the torus,
and 1350 could not run it on at the transition at the arc's other end (a
walked blend's surface does not run on as it is). The last end now keeps
them all, as the first.

## 1352: the piece past the end on the end piece's surface

### 1352-chfi3d-kpart-past-end-piece-one-face-with-the-end-piece

The piece 1310 adds past the spine's end (`ChFi3d_KPartCutPiece`) had a
copy of the blend's surface of its own, so it became a face of its own
beside the end piece P: a torus face of 0.07 mm² at each side of the fin's
end, where the stored body has one torus face. It now takes P's surface
where that is the blend's own (the same points at the piece's contact), and
the two are one face. The same design's item 94 (radius 0.2 round the
plate's sides) had such a split at both of its ends at z = 15 to 20 (a
cylinder of 1.602 mm² and 0.109 mm² past the end), where the stored body
has one cylinder of 1.711 mm²: now one face too.

## Tests

- 1350 to 1352: mitcad#1349, in `geometry/tests/test_fillet_fin_top_creases.cpp`
  (`test_transitions_cut_by_their_pieces`) on the case
  `fin-top-chamfer-rounding` of `MITCAD_GEOMETRY_CASES` (mitcad#1341's): the
  rounding has the stored body's faces (14 more without the patches) and
  about each of the six transitions the result and the stored body lie on
  each other within 0.001 mm (the stored body 0.013 mm off the result
  without them). A synthetic body needs the fin's spline profile with its
  chamfer's transition pieces; the design's body is the case.

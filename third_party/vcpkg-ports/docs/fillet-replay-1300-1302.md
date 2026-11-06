# Fillets and chamfers of stored designs: 1300–1302

mitcad#1341: a fin's top, chamfered over slight creases of its profile and
over side strips narrower than the chamfer (0980, 1000), rounded on the
import's own body instead of the file's stored state. The earlier rounds
are in [0990 to 1020](fillet-replay-0990-1020.md). Back to the
[port's README](../README.md).

## Overview

- **1300** judges a crease concave, for 0980's turn about it, as the two
  faces meet at the edge itself: `ChFi3d::DefineConnectType` moved the
  points it evaluates a hundredth of a face's parameter range into an
  extruded or B-spline face, where across a crease of a degree or two the
  normal lay on the other side, and a convex crease was taken for concave
  (a rounding along a fin's top past the end of an arc's analytic piece).
- **1301** ends a walk where one of its contacts reached the point where the
  analytic piece after the guide starts (before it: where the one before it
  ends), short of the guide's end only by the other contact's lag across a
  slight kink of the spine: the step on to the end made a piece of a few
  micrometres whose faces the result's build left open.
- **1302** gives OCCT's history (`Generated`) the pieces of the parts that
  0531 cuts a chamfer into where it is wider than a narrow face beside its
  edges: the parts after the first carry pieces of a surface of their own,
  and the cut pieces are new; neither was among the faces the spine's edges
  generate, so MitCad's names left them unnamed or named them after the
  wrong edge.

## The case

A fin 1 thick of a bracket (`5ef2dbd5` of mitcad#343, items 86 and 87):
its top is arcs (cylinders about the fin's normal) and spline stretches
(extrusions of the profile's splines), meeting at creases of 1.4 and 2.2
degrees. The top's edges are chamfered by 0.263 at 45 degrees (item 86;
along the spline stretches the chamfer is wider than the fin's side strip
over the root's chamfer, 0531, 1000), then the edges between the top and
the chamfers or the root's chamfer are rounded with 0.2 (item 87). Each arc's
rounding is a torus (an analytic piece between the arc's cylinder and its
chamfer's cone); the spline stretches' are walked.

On the body the import builds the chamfer gives the stored state's faces
within 3 µm, but the cone along an arc ends at the arc's meridian section,
while the file's cone ends a few micrometres short of it on the side
strip. So where the walk along a spline stretch comes to an arc's torus, or
leaves one, both contacts reach their faces' edges at one section, a few
micrometres apart in either order. On the file's state the rounding builds;
on the import's body it failed.

## 1300: a slight crease judged concave where the faces meet

### 1300-chfi3d-concave-crease-beyond-judged-at-the-edge

Where the arc's torus ends, its contact on the top lies on the crease
between the arc's cylinder and the spline's extrusion, which meet at a
convex crease of 1.4 degrees (taken for tangent: the normals within a tenth
of a radian). 0980 lets the ball turn about such a crease only where it is
concave (`ChFi3d_ConcaveCreaseBeyond`), as its contact cannot pass it; it
asked `ChFi3d::DefineConnectType` with its point correction on, which moves
a point on a B-spline or extruded face that lies within a hundredth of the
face's parameter range of its bound that far into the face. On the
extrusion of the fin's spline that is 0.17 mm along the spline, where the
normal has turned by more than the crease: the crease came out concave. The
ball then rolled on the crease's edge, which it leaves at once on a convex
crease: the walk made one point and failed on both sides of the fin. Now
the connection is judged at the edge itself (the function already checks
the normals there for its angle). On the case the walk after the torus
passes onto the spline's top as over a tangent edge.

## 1301: a walk's end at the start of the analytic piece after it

### 1301-chfi3d-walk-ends-where-a-contact-reaches-the-next-analytic-piece

Coming to the arc's torus along a spline stretch, the walk's guide leans
against the torus's sections across the spine's slight kink: the contact on
the chamfer reached the chamfer's cone where the torus starts 2.2 µm along
the spine before the guide's end, the contact on the top 4 µm short of the
top's crease. The walk stopped there (its contact crossed onto the cone),
and the next step walked the 2.2 µm to the guide's end: a piece of the
blend between the top and the cone, whose faces the build of the result
could not join to the torus; the result had the arc's edge on one face
only. A walk now ends where one of its contacts lies within ten times the
spine's tolerance of the point where the analytic piece after the guide
starts on that side (going back: where the piece before it ends),
`ChFi3d_ReachedAnalyticPiece`; the piece after it goes on from there, as
where the walk reaches the guide's end within that tolerance.

## 1302: the history of a chamfer's parts cut by the faces beyond

### 1302-chfi3d-narrow-parts-pieces-generated-by-their-spine-edges

0531 computes a chamfer wider than a narrow face again with the face
unbounded and cuts it by the faces beyond into parts: the first part is
the stripe's, the others new stripes. The pieces cut at the crossings are
new surface data, and in the parts after the first every piece has a
surface of its own; the map from the spine's edges to the surfaces they
generate (`myEVIMap`, which `Generated` reads) held only the surfaces of
the computation before the cut. So `Generated` left out half the chamfer's
faces on the fin (five of ten); MitCad named some after the nearest edge,
one of them wrongly, and left three unnamed, and an edge of the rounding
after it (the file's) was not found by its name. Now every piece of every
part is mapped to the spine's edges whose parameter range it overlaps.

## Tests

- 1300, 1301 and 1302: mitcad#1341, in
  `geometry/tests/test_fillet_fin_top_creases.cpp` on the case
  `fin-top-chamfer-rounding` of `MITCAD_GEOMETRY_CASES` (the design's
  stored body before the chamfer, and that body chamfered by the import):
  every face of OCCT's chamfer is one its history gives a contour edge
  (without 1302 five of ten are not); the rounding of the file's twelve
  edges builds, closed, and the stored body after it lies on the result
  within 0.01 mm round the fin's top (without 1300 the walk fails on both
  contours, with 1300 alone the result has an edge on one face only). A
  synthetic body with such creases on spline faces needs the profile's
  splines; the design's body is the case.

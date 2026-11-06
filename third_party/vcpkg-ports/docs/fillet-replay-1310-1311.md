# Fillets of stored designs: 1310 and 1311

mitcad#1345: a fin's top rounded up to its end face, and the fin's end
rounded along its sides up to the top, on the import's own body instead of
the file's stored state. The rounds before are in
[1300 to 1302](fillet-replay-1300-1302.md). Back to the
[port's README](../README.md).

## Overview

- **1310** runs an analytic blend on past the spine's end vertex where its
  contact reaches the face at the end exactly there, that face going on
  from the contact's face tangentially (`ChFi3d_KPartCutToEnd`): the face
  at the end cuts it from where the contact reaches it up to where the
  other contact meets that face. OCCT ended the blend at the vertex and
  closed it with a corner face.
- **1311** runs on the curve of an extruded face past its end where 0600's
  walk of an OnSame end takes that face's surface unbounded
  (`WalkedOnSameToEndFace`): a face made by extruding a profile's spline
  stretch is bounded by the stretch's ends, and the walk fell off it.

## The case

The fin of a bracket (`5ef2dbd5` of mitcad#343, items 87 and 89), 1 thick:
its top runs as arcs (cylinders about the fin's normal) and spline
stretches, chamfered by 0.263 at 45 degrees and then rounded with 0.2
(item 87, mitcad#1341). At the fin's end the last arc meets the end face,
an extrusion of a spline stretch of the fin's profile that goes on from the
arc tangentially and falls away from it towards the root. The end face's
edges with the root's chamfer and rounding are rounded with 1 (item 89).

## 1310: the top's rounding run on past the vertex at the end face

### 1310-chfi3d-kpart-blend-runs-past-the-end-where-the-end-face-goes-on-tangentially

Each side's rounding along the last arc is a torus between the arc's
cylinder N and the chamfer's cone. It ends at the vertex V where the arc's
edge meets the end face B, and its contact on N reaches the edge E between
N and B at V's section, E running into V. In the stored body the torus runs
on past V, on N's surface continued; B falls away from that surface and
cuts the torus from E on, up to where its contact on the cone meets the
cone's edge with B, 1.2 mm past V: a torus face of 1.24 mm² on each side.
`ChFi3d_KPartCutToEnd` makes such ends where a contact leaves its face into
the face at the end before the end, but not where N and B are tangent
along E; the corner then ended the torus at V, and the cone met B directly
past it, with a corner face of 0.005 mm² at V, 0.013 off the stored body.
On the fin's other side the arc's blend happened to be walked (its
analytic piece did not split into the faces' domains), and the walk past
V gave the stored body's end, within 3e-4 mm.

Where N and B are tangent along E, a fillet's end piece P lies on the end
edge and ends at V's section with its contact on E, and E runs into V, the
blend is now cut past the end: the point Q where the other contact meets
the edge between its face and B lies beyond V's section; the section
through Q is found by bisection on the spine run on beyond V (up to the end
edge's length), and the piece from Q to P's end, its side on N the blend's
intersection with B, is added before P, which keeps the end edge. The
stripe ends in a point at Q, as `ChFi3d_KPartCutToEnd`'s ends do. Chamfers
are left as before (the fin's chamfer of the same edges, item 86, is made
by 0531's cuts and lies on the stored state within 3 µm). On the case the
torus past V lies on the stored body's within 2e-6 mm.

## 1311: the end's rounding walked past its face's profile stretch

### 1311-chfi3d-onsame-end-walked-on-past-the-end-of-an-extruded-stretch

Item 89's two chains, one up each side of the end face, end at the top at
an OnSame vertex (the end face, the root's chamfer and the fin's side), and
0600's `WalkedOnSameToEndFace` walks the last piece again with the end
face's surface unbounded, past the vertex, until the contact on the root's
chamfer reaches the fin's side: the contact on the end face runs on over
the fin's top, past the top's crease. The end face is the extrusion of the
profile's spline stretch alone (a B-spline curve of four knots covering it),
while the stored state's end face is the extrusion of the whole spline
(thirteen knots, running on past both ends of the stretch). The walk fell
off the stretch's end at once, no corner was made at the top, and the two
sides' roundings were taken for intersecting; on the stored state it
built.

Where N's surface is a linear extrusion of a non-periodic B-spline curve,
the curve is now run on past the end nearer P's contact on N by four
radii: curvature continuous (`GeomLib::ExtendCurveToPoint`, third order) up
to the curve's own polynomial run on at that length (a B-spline curve
evaluated past its end runs on along its end span), its own parameters
kept. On the case the run on lies on the whole spline's extrusion within
1e-4 mm over the first 1.6 mm and 5.5e-4 mm over the four radii.

## Tests

- 1310 and 1311: mitcad#1345, in `geometry/tests/test_fillet_fin_end.cpp`
  on the cases `fin-top-chamfer-rounding` (mitcad#1341's) and
  `fin-end-rounding` of `MITCAD_GEOMETRY_CASES`: the top's rounding about
  the fin's end lies on the stored body within 0.002 mm both ways (0.011
  without 1310); the end's rounding of the body MitCad builds (its end face
  the stretch's extrusion) builds, closed, and the stored body after it lies
  on the result within 0.01 mm round the end (0.0006; it fails without
  1311). A synthetic body needs the profile's spline with its tangent end
  face; the design's bodies are the cases.

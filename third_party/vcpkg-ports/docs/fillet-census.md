# Fillets from the census and consumed faces: 0320–0331, 0350–0352, 0420, 0440

Fillets the `.f3d` import's census found failing along their edges or at a
vertex (mitcad#322, mitcad#323, mitcad#331), and roundings and chamfers
that consume a face beside their edges (mitcad#300, mitcad#334), with a
boss's rim on a cylinder whose seam runs in steps (mitcad#337). Back to
the [port's README](../README.md).

## Overview

- **0320** tells where a fillet that is not done stopped, and **0321**
  builds a closed rounding of several edges that consumes a face all round
  (mitcad#322).
- **0322** ends an open stripe whose contact runs along a whole edge at that
  edge's vertex without corrupting memory (mitcad#331).
- **0330** tells which corner a fillet failed to build at a vertex and why,
  and **0331** builds a rounding rolled on a straight edge beside a plane as
  its exact cylinder (mitcad#323).
- **0350 to 0352** put a fillet's regularity only on edges whose faces are
  tangent and build chamfers and roundings that consume both faces of a
  closed chain of edges (mitcad#300).
- **0420** rounds a boss's rim on a cylinder whose seam runs in steps
  (mitcad#337).
- **0440** builds chamfers and roundings whose contact runs along a chain of
  edges of the face they consume (mitcad#334).

## 0320 to 0322: fillets that fail along their edges

The patches 0320 and 0321 come from the split of the fillets the `.f3d`
import's census found failing along their edges (mitcad#322,
`core/import/docs/f3d-rounds-2.md`, *Fillets that fail along their edges*).

### 0320-chfi3d-where-a-fillet-stops

A fillet that is not done told its faulty contours with a status and its
faulty vertices, but not where it stopped. `ChFi3d_Builder::Failure()` (also
`BRepFilletAPI_MakeFillet::Failure()`) now gives the stage: a contour's
blend finding no start, its walk between two faces or rolling on an edge
stopping, a twisted surface or another failure (an exception in the stripe's
computation); blends overlapping or crossing on a face that fail when
computed again; the blends of two contours intersecting each other
(`ChFi3d_StripeEdgeInter`, whose stripe kept the status Ok); a corner, or a
corner built in part only. `FailedContour()` is the contour it stopped at.
The computation is unchanged; MitCad's dressup names the stage in its error.

### 0321-chfi3d-closed-rounding-consuming-its-face-ends-at-the-vertex

A slot's rim (two lines and two half circles) under a counterbore 0.5 wider,
rounded or chamfered with 0.5: the blend consumes the ledge between the two
all round, each piece's contact running along a whole edge of the ledge
(0014's blend on the edge). Those ends become the edges' vertices, but at
the closed stripe's start and end the stripe kept the point it had made
beside that vertex, and the topological build failed
(`TopoDS_Builder::Add`). The stripe's point there is now the vertex, as 0014
does for a contact along a closed edge (a round hole built before).

### 0322-chfi3d-open-stripe-end-at-the-vertex-of-the-edge-its-contact-runs-along

Ticket: mitcad#331.

The same at the ends of an open stripe, where it corrupted memory. A fin 1.5
thick on a wall, its top and bottom flush with the wall's, the edges round
one side rounded with 0.75 and then those round the other side: the second
rounding's contacts run along the whole edges the first one left on the
fin's top and bottom, up to the wall, and 0014 made their ends there the
edges' vertices while the stripe kept the indices of its own points.
`ChFi3d_FilDS` takes the kind of a stripe's end from its common point, so it
put those points' indices into the DS as vertices'; with OCCT's exceptions
off (release builds) `ChFi3d_Builder::Compute` then read and updated the
tolerance of the DS's shapes of those indices (the solid's, a face's) as
vertices', past the ends of those objects, the build failed
(`TopoDS_Builder::Add`) and the process crashed later on the corrupted heap
(an `.f3d` import, in freeing the fillet's copy of its input or in comparing
faces). Now the stripe's index at such an end is the vertex's for open
stripes too, and what the DS already has at the old point (the end's curves
and edges) is put at the vertex (`ChFi3d_StripeEndAtVertex`); the fin
builds, valid, each side's rounding taking off its exact volume.

## 0330 and 0331: fillets that cannot end or join at a vertex

The patches 0330 and 0331 come from the split of the fillets the `.f3d`
import's census found failing at a vertex (mitcad#323,
`core/import/docs/f3d-rounds-2.md`, *Fillets that cannot end or join at a vertex*).

### 0330-chfi3d-which-corner-a-fillet-fails-at

A fillet whose corner at a vertex failed told the vertex (`FaultyVertex`,
with 0320 `Failure_Corner`), but not which corner it tried there nor why.
For each faulty vertex `ChFi3d_Builder::FailedCorner(IV)` (also
`BRepFilletAPI_MakeFillet`) now gives the corner `PerformFilletOnVertex`
chose (a rounding's end, mitred, over several pieces or where more than
three sharp edges meet; two or three roundings meeting; more; roundings
ending in points), `FailedCornerBlends(IV)` and `FailedCornerEdges(IV)` the
roundings and sharp edges meeting there, and `FailedCornerMessage(IV)` the
message of the exception it failed with. The computation is unchanged;
MitCad's dressup names the corner and the cause in its error.

### 0331-chfi3d-rounding-rolled-on-a-straight-edge-exactly

A rounding wider than the face beside its edge rolls on that face's far edge
(0170's rolling ball). Where the ball touches a plane and rolls on a
straight edge parallel to it along a straight guide, its centres lie on a
line and the rounding is a cylinder; OCCT stored the walked approximation (a
B-spline surface and curves). The stored designs have the cylinder, and a
later rounding of edges of the result failed on the approximation. The
SurfData now gets the cylinder and the contacts' lines, on the same
parameters (`ChFi3d_ExactRollOnLine`, after `CompleteData` in the two
`PerformSurf` of a constant blend rolling on a restriction, only where the
walked contacts lie on the exact ones; the cylinder trimmed in U to the
section, as the walked surface is, so that 0390's ball at the blend's end is
found from its bounds). The edge on the rolled edge, where the rounding
meets the face beyond at an angle, gets no regularity with 0350.

## 0350 to 0352: consumed faces

The patches 0350 to 0352 go on with roundings and chamfers that consume a
face beside their edges (mitcad#300).

### 0350-chfi3d-regularity-only-where-the-faces-are-tangent

A rounding whose contact runs along the far edge of a face it consumes (a
boss's foot rounded as high as the boss) marked that edge, now between the
blend and the face beyond (the boss's top, at a right angle), as tangent
(`SetRegul` took the face beyond for the image of the consumed face the
blend is tangent to); so did a rounding with the walls of a hole cut through
it before. A later rounding of such an edge failed: the boss's top edge, one
closed edge, was taken for a spine ending at its vertex, and the walk
stopped at once ("Path failed", for any radius); at a hole's corner the walk
went on past the corner onto the torus. The regularity is now encoded only
where `ChFi3d::IsTangentFaces` finds the faces tangent along the edge.

`SetRegul`'s faces come from the builder's lists, each oriented as in the
data structure rather than as in the result, and `IsTangentFaces` compares
their normals as oriented: on a box rounded along two edges, half of the
blends' true tangent edges were taken for folds and lost their G1. A draft
after the rounding then could not carry the drafted face on to the blend
(OCCT's draft/angle A2 B5 C3 C4 C5 F1, mitcad#1880). The tangency is now
taken whichever way the faces face: only the angle between them along the
edge counts, so a blend's edges with its own faces keep their G1, and the
boss's top edge, at a right angle, still gets none.

### 0351-chfi3d-closed-chain-consuming-a-face-all-round

A pocket's wall foot of four lines and four arcs on a ledge as wide as its
chamfer (or rounding): the contact on the ledge is an unbounded line along
the ledge's boundary, which the hatcher found inside the ledge only beyond
the line's ends; `LineOnBoundary` (0148) now looks at such a line alongside
the spine's edge, and the line is hatched on the boundary. And the closed
stripe's start, its contacts ending at the vertices of the ledge's edges
(0148), keeps those vertices as its points
(`ChFi3d_ContactsAlongFaceEdges`); the topological build took the point made
before for a vertex (TopoDS_Builder::Add). `IsInput`, asked for those
vertices, takes a spine tangent square to the face's edges there as not
running into the face (its angle was measured from a null vector). Before,
the chamfer failed ("Path failed") or threw.

### 0352-chfi3d-contact-on-a-face-beyond-at-an-acute-angle

The transition of a contact put onto the face beyond its face's edge (0014,
`ChFi3d_ContactsAlongFaceEdges`) is taken on the side of the edge outside
that face, from the edge's direction in it; before, the side toward the
other contact, which lies under the face beyond where the blend folds back
under it (a convex lip over a wider pocket chamfered as far as its wall's
top and its underside's far edge): an invalid shape. The same wrong side
made a wall's foot on a flange chamfered as wide as the flange's top, the
flange's rim rounded on beyond it tangentially, invalid (mitcad#327).

## 0420: a closed edge on a stepped seam

The patch 0420 (mitcad#337) rounds a closed edge on a cylinder whose seam
runs in steps; `geometry/tests/test_fillet_kernel.cpp` checks it with OCCT
alone.

### 0420-chfi3d-kpart-round-a-closed-edge-on-a-face-with-a-stepped-seam

Two coaxial cylinders of one radius joined with their seams at different
angles (a boss extruded on in a second step) merge into one face whose seam
runs up the first seam, along the circle between the parts and up the second
one. The analytic rounding (a torus) of the boss's end circle has the
circle's parameters, one period round it; where the end part extends before
the first seam in the face's parameters (the second seam at a smaller
angle), the split of the blend into the face's domain (`SplitKPart`) found
that domain a period off, the piece covered half the spine and the walk for
the rest failed. The tangency line on the face is now moved by those whole
periods of the surface, and its domains with it (`DomainsIntoPeriod`). Left:
with a radius smaller than the end part's height (the contact above the
step), OCCT's rebuild of the cylinder keeps one of the step's two uses, and
the result does not check valid (it failed before; the dressup rejects it as
before).

## 0440: a contact along a chain of edges of a consumed strip

The patch 0440 goes on with 0148 and 0351 for chamfers and roundings that
consume a strip beside their edges (mitcad#334): a boss on a slab 2 wider,
the boss's foot chamfered by 2 along three sides and its corner arcs, the
slab's top going on along the fourth side.

### 0440-chfi3d-contact-along-a-chain-of-edges-of-a-consumed-strip

`ChFi3d_ContactsAlongFaceEdges` took a contact along a whole edge of its
face or along a part of one. A contact along several edges end to end (the
strip's far side split at a vertex) was along none of them, and the strip
stayed as a face of no width; such a contact's SurfData is now split at the
chain's inner vertices (`ChFi3d_SplitAtVertexOnLine`, as 0144 does at a
closed edge's vertex), each part along one edge. And a contact along a part
of an edge (starting inside it, where the chamfered chain ends) kept its own
point at its other end, the edge's vertex, where the next contact began at
the vertex: the slab's walls took two vertices there and their wires were
not closed. That end is now the vertex, as for a contact along a whole edge.
Before, the results were invalid; fillets likewise.

## Reports and tests

- 0320 and 0321: mitcad#322 and the tests of where a fillet stops and of
  the slot's rim rounded as wide as its ledge in
  `geometry/tests/test_fillet_kernel.cpp` (the first needs 0320's API to
  compile, the second fails without 0321).
- 0322: mitcad#331 and the fin whose sides are rounded one after the other
  in `geometry/tests/test_fillet_kernel.cpp`, which fails without it.
- 0330 and 0331: mitcad#323 and the tests of which corner a fillet failed
  at and of the block's top edge rounded wider than its side in
  `geometry/tests/test_fillet_kernel.cpp` (the first needs 0330's API to
  compile, the second fails without 0331).
- 0350: mitcad#300 and the boss's top rounded after its foot in
  `geometry/tests/test_fillet_overflow.cpp`; mitcad#1880 and the box
  rounded along two edges, its tangent edges kept and a draft beside the
  roundings, there too (OCCT's draft/angle A2).
- 0351 and 0352: mitcad#300 and the stepped pockets' wall feet in
  `geometry/tests/test_chamfer_overflow.cpp` (for 0352 also mitcad#327's
  flange with a rounded rim there).
- 0420: mitcad#337 and the boss's rim rounded across a stepped seam in
  `geometry/tests/test_fillet_kernel.cpp`.
- 0440: mitcad#334 and the boss on a slab in
  `geometry/tests/test_chamfer_overflow.cpp`.

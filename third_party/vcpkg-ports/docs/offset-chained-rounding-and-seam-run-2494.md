<!-- SPDX-License-Identifier: MIT -->
# A removed face beside roundings of many edges, and a cap section in pieces (2494)

mitcad#2494, GitHub mitcad/Mitcad#645. Follow-up of
[2462 and 2463](offset-removed-face-met-tangentially-2462.md). Back to the
[port's README](../README.md).

## Overview

GitHub #645's design is a loft cut to a slot with round ends, its top
rounded 2 mm; MitCad's shell splits the rounding at its crease, so that its
faces' boundaries come in many edges. Shelled inside with the top open:

- **The filleted loft at 3 mm** (wall thicker than the rounding): of the
  five rounding faces beside the top only the three of four edges were
  found vanishing (`FindVanishingFaces`, patch 0100 and 2433). The rounding
  along the slot's near round end (47 edges) and the crease's 0.4 mm2
  sliver (13 edges) were kept, their offsets turned inside out, and the
  offsets did not close. Sharp now builds (26653.3 mm3, valid).
- **The loft with its top rounding suppressed** (the reporter's second
  try): Sharp built at 1 to 3 mm; Rounded failed at each. In the arc join
  the offset of the slot's near round end, extended to the top
  (`ToContext`, `ExtentFace`), meets the top's periodic loft surface in a
  section that comes in three pieces; `SelectEdge` kept one, which stopped
  short of the plane beside, and the round end's offset got no wire.
  Rounded now builds, the same wall as Sharp (the body has no concave
  edge).

## The cases

MitCad's thick-solid input (`MITCAD_SHELL_DUMP`), the top open, OCCT's
thick solid in the OCCT lab (case `2494-chained-rounding-and-seam-run`):

| body | w | Sharp before | Sharp after | Rounded before | Rounded after |
| --- | --- | --- | --- | --- | --- |
| loft without the rounding | 1 | 9576.7 mm3 | the same | no result | 9576.7 mm3 |
| loft without the rounding | 2 | 18526.8 mm3 | the same | no result | 18526.8 mm3 |
| loft without the rounding | 3 | 26868.3 mm3 | the same | no result | 26868.3 mm3 |
| loft, top rounded 2 mm | 3 | no result | 26653.3 mm3 | no result | no result |

(all results valid). Two synthetic bodies (`geometry.port.2494`), 3 mm:
a box 40 x 30 x 20, its top edges rounded 2 mm, one rounding's edge on the
top split by a vertex (a rounding of five edges): no result before, Sharp
and Rounded, now 10010.9 mm3 both; a slot 50 x 20, 15 high (two
half-cylinders tangent to two planes), its top edges rounded 2 mm: no
result Sharp before (Rounded built), now 9327.8 mm3 both.

## What changed

### Roundings whose sides are chains of edges (FindVanishingFaces)

A face of more than four edges whose boundary turns sharply (by more than
30 degrees) at four vertices only has four sides, the chains of edges
between those corners (`FourSides`; the 47-edge rounding turns by about 90
degrees at four vertices and by less than 1e-7 degrees elsewhere). Opposite
sides are tested at three stations along each edge of both sides, each
with the nearest point of the other side (`VanishesBetweenSides`); where
the offset turns inside out at every station, the face vanishes and the
pairs of edges a station and its nearest point lie on are its pairs across.
Faces of four edges are tested as before.

### The walls across vanishing faces beside a cap (ContextIntByInt, Inter2d)

- Each kept face across the vanishing faces is intersected with the cap
  once; its section is now the image of every one of its edges to the
  vanishing faces (before, of the first only, from the first cap edge
  across).
- The walk round a kept face's wire (`BRepOffset_Inter2d::ConnexIntByInt`)
  counts consecutive edges with one image once, the vertex at the end of
  their run standing for the vertices dropped, and starts outside such a
  run.
- A smooth edge ending beside a vanishing face is cut within its offset
  (patch 0100), except now where that face lies beside a cap: there the
  image is the section with the cap, beyond the edge's offset (the slot's
  walls meet tangentially; their common edges end where the rounding
  starts, 2 mm under the top), and the edge's offset is extended to it.

### Edges of a cap shorter than the tolerance (BRepAlgo_Loop)

`StoreInMVE` merged a vertex of an edge into one stored before within the
tolerance, changing the edge in place. Two of the top's edges (0.000015
and 0.000011 mm, at the corner where the crease's sliver meets the
rounding) were closed on one vertex; they are shared with the rounding and
the sliver, whose wires were left open (`BRepCheck_NotClosed`). Two
vertices of the face's own boundary now stay apart however near.

### A cap section in pieces in the arc join (ExtentFace)

Where the section of an extended face with a cap comes in several pieces,
`SelectEdge` keeps the one covering most of the edge; now the pieces that
run on from it through their common vertices, a simple open run, are glued
into one edge (`JoinRunAcrossSeams`, with OCCT's `Glue`), its curves on
both faces. `ConcatPCurves` moves the second curve by the surface's period
where it continues the first across a seam.

## Not covered

- **#645's rounded loft at 3 mm, Rounded**: after the five roundings are
  found vanishing and `ExtentFace` extends the four walls to the top,
  `ContextIntByArc` (patch 2463's images of the top's edges across the
  vanishing faces, one per edge of a wall) raises `Standard_ConstructionError`.
  The 31 top edges and 21 wall edges across the near rounding and the
  sliver are paired edge by edge, which patch 2463's binding of one image
  per cap edge does not take. Covered since by patch 2535
  ([offset-arc-join-wall-chain-2535.md](offset-arc-join-wall-chain-2535.md)).
- **A wall exactly as thick as a circular rounding** (the box at w = r =
  2): unchanged in the kernel (a rim of no width). MitCad words the thick
  solid's failure, the rounding and the removed face its places
  (`geometry/src/faceops.cpp`, `rim_of_no_width`); its shell then builds the
  body less a cavity with the roundings left solid (a note), and the worded
  failure is its error only where that cavity fails too.

## 2494-brepoffset-rounding-of-chained-sides-vanishing-beside-a-removed-face

- `BRepOffset_MakeOffset.cxx`: `TurnBetween`, `FourSides`,
  `VanishesBetweenSides`; `FindVanishingFaces` takes faces of four sides.
- `BRepOffset_Inter3d.cxx`, `ContextIntByInt`: the section of a kept face
  with the cap, per cap, is the image of all its edges across.
- `BRepOffset_Inter2d.cxx`, `ConnexIntByInt`: `HasEdgeOfOneFace`; runs of
  edges of one image counted once; the smooth edge beside a vanishing face
  at a cap extended.
- `BRepOffset_Tool.cxx`: `JoinRunAcrossSeams` in `ExtentFace`; the period
  shift in `ConcatPCurves`.
- `BRepAlgo_Loop.cxx`, `StoreInMVE`: vertices of the face's own boundary
  are not merged.

## Reports and tests

- `geometry.port.2494`
  (`p2494-rounding-of-chained-sides-vanishing-beside-a-removed-face.cpp`),
  OCCT alone: the box with a five-edge rounding and the slot, 3 mm, Sharp
  and Rounded: one valid solid of the body's volume less the cavity.
- `cli.shell_sharp_reported` (`shell_sharp_reported645.json`: 3 mm Sharp;
  `shell_sharp_reported645_unfilleted.json`: the rounding suppressed, 1 to
  3 mm, Sharp and Rounded) on the reporter's archive.
- `cli.shell_rim_of_no_width`: the box at w = r = 2 built by the cavity
  with the roundings left solid (a note), the kernel's failure worded in
  the shell trace; 1 and 3 mm build.

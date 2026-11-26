<!-- SPDX-License-Identifier: MIT -->
# A removed face beside roundings: the arc join (2463) and the wall strips (2462)

mitcad#2463 and mitcad#2462, GitHub mitcad/Mitcad#645. Follow-ups of
[2433](offset-vanishing-beside-removed-face-2433.md). Back to the
[port's README](../README.md).

## Overview

- **2463** closes a face whose offset vanishes beside a removed face in the
  arc join (Rounded) as patch 2433 does in the intersection join (Sharp): a
  box with its top edges rounded 2 mm, shelled inside with its top open,
  Rounded, by 3 or 4 mm: before, no result; now one valid solid, the same as
  Sharp gives. (OCCT's own `offset/compshape/A3`, an arc-join thick solid
  marked as giving a wrong volume, gives the right one or the wrong one
  from run to run on one build: 3 runs in 8 wrong with both patches,
  whose code does not run there, no face vanishing in it.)
- **2462** opens a removed face that every face round it meets tangentially
  where the wall is no thicker than their rounding: the faces' offsets run
  under it, parallel to it at its edges, and never meet it. The wall now
  ends at each of its edges in a strip along the normal from the edge down
  to its offset, Sharp and Rounded. The same box by 1 mm, and GitHub #645's
  loft with its rounded top open by 1 and 2 mm (the report's shell: 2 mm
  with 2 mm roundings), give one valid solid each. On the way the
  intersection of two roundings' offsets that touch at an end of their
  common edge is cut there (a mitre's offset ran on past that point and
  across, and the offset faces got no loop): OCCT's thickening of an open
  shell with rounded edges, Sharp, gives the right wall now too.

## The cases

A box 40 x 30 x 20, its four top edges rounded r = 2 (their mitres meeting
the box's sharp vertical edges), its top removed, shelled inside by w:

| w | Sharp before | Sharp after | Rounded before | Rounded after |
| --- | --- | --- | --- | --- |
| 1 | no result | 3822.8 mm3, valid | 3822.8 mm3, invalid | 3822.8 mm3, valid |
| 2 | no result | no result | no result | no result |
| 3 | 10010.9 mm3 (2433) | the same | no result | 10010.9 mm3, valid |
| 4 | 12618.9 mm3 (2433) | the same | no result | 12618.9 mm3, valid |

GitHub #645's body (a loft cut to a slot with round ends, its top edges
rounded 2 mm, split at its fillet's crease as MitCad's shell splits it; the
lab's `2415-loft-slot-split.brep`), the loft's top removed:

| w | before, Sharp and Rounded | after, Sharp | after, Rounded |
| --- | --- | --- | --- |
| 1 | no result | 9595.7 mm3, valid | 9595.6 mm3, valid |
| 2 | no result | 18284.9 mm3, valid | 18284.9 mm3, valid |
| 3 | no result | no result | no result |

### 2463: the arc join beside a vanishing rounding

In the arc join the faces whose edges a removed face (a cap) shares are
extended to it (`ToContext`), and `ContextIntByArc` stores each cap edge's
offset as the section of the cap with its neighbour's offset. Where that
neighbour is a rounding whose offset vanishes (w > r, patch 0100), neither
did anything: the face beyond the rounding (the box's side) was not
extended, the cap got no section there, and the offsets did not close.
Now the face across the vanishing faces from the cap's edge (the pairs
patch 2433 has `FindVanishingFaces` give for a cap) is extended to the cap
instead, and its section with the cap becomes the image of the cap's
edge, which `ContextIntByArc` stores with that face. Only where the
section runs inside the cap (its middle inside the cap, not on its
boundary): at w = r it lies on the cap's edge, and the offset fails as
before.

### 2462: strips along the normal where the offsets never meet the cap

Where every edge of a cap bounds a kept face tangent to it there (G1:
the edge's continuity, or `ChFi3d::IsTangentFaces`) whose offset does not
vanish, no kept face's offset meets the cap: at a tangent edge the
offset runs parallel to the cap at the wall's distance. The cap was
intersected all the same, gave no section, and the thick solid failed
(Sharp) or came out invalid with the offset edges stored as sections they
are not (Rounded). Such a cap is now taken out of the caps before the
offset (and out of the images `SetFaces` gave it): its edges are free
edges of the offset, as in an open shell, the offset ends at their
offsets, and `MakeMissingWalls` (the thickening's walls at free edges),
restricted to those edges, builds the strip from each edge to its offset.
`MakeThickSolid` glues the strips with the shape and its offset.
`Clear` empties the walls, which an earlier run left behind.

The intersection join had one more fault at such free edges: two
roundings mitred at a corner (both tangent to the top at the corner's top
vertex) have offsets tangent to each other at that vertex's offset, and
the intersection of their enlarged offsets is two ellipses crossing there.
`CheckIntersFF` keeps all their pieces (one connected block), and with no
cap to trim them the rounding's offset got no loop (an enlarged face kept
whole, or none). `DropBranchesAtTangentEnd` (after
`DropCrossingsPastTangentEnd`, where that changed nothing) cuts the
section at that point: where the faces are tangent at an end of the
reference edge and not at its middle, of the pieces meeting at the end's
offset point only the one going back along the edge is kept, with the
pieces joined to it elsewhere. OCCT's thickening of the box's open shell
(`thickshell`) was wrong in the same way, Sharp: an untrimmed face and a
volume of -2.2e8 mm3; now 3822.8 mm3, valid.

## Not covered

- **The wall as thick as a circular rounding there** (the box at w = r =
  2): the rounding's offset is its axis, `FindVanishingFaces` takes it as
  vanishing, and the side's offset meets the top's plane on the top's edge
  itself (a rim of no width), which neither route takes. GitHub #645's
  B-spline roundings at 2 mm are not found vanishing and take the strips.
- **GitHub #645's body at 3 mm** (wall thicker than the rounding): of the
  five rounding faces beside the top only the three with four edges are
  found vanishing. The rounding along the slot's round end and both sides
  (182.3 mm2, one face of 47 edges: its boundary split where the crease
  split and the slot's faces cut it) and the crease's 0.4 mm2 sliver (13
  edges) are not, since `FindVanishingFaces` (patch 0100) takes only faces
  of exactly four edges; their offsets turn inside out and the offsets do
  not close. Closing it needs the four sides of such a face found as chains
  of edges, and the pairs across them given edge by edge to the
  intersections.
- A cap met tangentially by some neighbours and at a crease by others
  stays a cap (the strips would have to be cut by the walls of the creased
  edges).
- A failed thick solid may leave its input's edges with changed
  tolerances and curves; MitCad offsets a copy. The lab's case
  `2462-top-open-met-tangentially` takes a fresh copy per run;
  `2433-rounded-top-open` reuses its bodies, so that after its failed 3 mm
  runs the loft's Rounded runs come out invalid there.

## 2463: the arc join closes a face vanishing beside a removed face

### 2463-brepoffset-arc-join-face-vanishing-beside-a-removed-face

- `BRepOffset_MakeOffset.cxx`, `BuildOffsetByArc` and `ToContext` (a new
  optional argument, the pairs across vanishing faces): a cap edge whose
  ancestor vanishes extends the kept face across it to the cap; its
  section with the cap, where it runs inside the cap, bounds that face and
  becomes the image of the cap's edge (`myInitOffsetEdge`).
- `BRepOffset_Inter3d::ContextIntByArc` (a new optional argument, the
  pairs): for such an edge, the kept face across is the face the image
  lies on.

## 2462: wall strips at a removed face met tangentially

### 2462-brepoffset-wall-strip-at-a-removed-face-met-tangentially

- `BRepOffset_MakeOffset.cxx`, `MakeOffsetShape`: `FindCapsMetTangentially`
  finds the caps whose edges each bound one kept face, tangent to the cap
  there, whose offset does not vanish; they leave the caps (and the images),
  and after the faces are built `MakeMissingWalls` builds the walls at their
  edges (`FillContours` and `MakeMissingWalls` take an optional set of
  edges). `MakeThickSolid` glues the walls in. `Clear` empties the walls.
- `BRepOffset_Tool.cxx`, `Inter3D`: `DropBranchesAtTangentEnd`, as above.

## Reports and tests

- `geometry.port.2463` (`p2463-arc-join-face-vanishing-beside-a-removed-face.cpp`),
  OCCT alone: the box with 2 mm roundings, its top open, Rounded, 3 and 4
  mm: one valid solid of the body's volume less the cavity.
- `geometry.port.2462` (`p2462-wall-strip-at-a-removed-face-met-tangentially.cpp`),
  OCCT alone: the same box, 1 and 1.5 mm, Sharp and Rounded: one valid
  solid of the body's volume less the cavity computed from the inner
  roundings, the strip under the top's edge.
- OCCT lab case `2462-top-open-met-tangentially`: the box at 1 and 2 mm and
  #645's body at 1 to 3 mm, both joins, as in the tables above.
- MitCad's own input for #645 (its shell splits the body's C0 faces at
  their creases with `ShapeUpgrade_ShapeDivideContinuity`) held 26 edges
  flagged same-parameter but not same-range, an invalid input
  (`BRepCheck_InvalidSameParameterFlag`) that a closed offset drops but a
  thick solid keeps in the outside of its wall: MitCad's result came out
  invalid where the lab's input (another split of an exported body) did
  not. MitCad's shell now puts those edges' curves on their 3D range
  (`BRepLib::SameRange`) after the split (`geometry/src/faceops.cpp`,
  `split_at_creases`); found with `MITCAD_SHELL_DUMP`, which writes the
  kernel's thick-solid input and result.
- `cli.shell_sharp_reported` (`shell_sharp_reported645.json`): #645's design
  with the top open as reported, 2 mm Sharp and Rounded and 1 mm Sharp, the
  strips named `<shell>:offset_cap(<top>)`.

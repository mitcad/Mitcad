<!-- SPDX-License-Identifier: MIT -->
# The arc join's wall across a chain of edges to a vanishing rounding (2535)

mitcad#2535, GitHub mitcad/Mitcad#645. Follow-up of
[2494](offset-chained-rounding-and-seam-run-2494.md) and
[2463](offset-removed-face-met-tangentially-2462.md). Back to the
[port's README](../README.md).

## Overview

GitHub #645's loft with its top rounded 2 mm, shelled 3 mm inside with the
top open: the wall is thicker than the rounding, which vanishes beside the
top (patch 2494). Sharp built since 2494; Rounded (the arc join) raised
`Standard_ConstructionError` in `ContextIntByArc`.

The cause, traced in the OCCT lab: the wall along the slot's near round
end has 21 edges to the vanishing rounding (47 edges) and the crease's
sliver (13 edges); the top has 35 edges there. The pairs across
(`FindVanishingFaces`) join them edge by edge, many to many.

- `ToContext` (patch 2463) extended the wall to the top along the one
  wall edge each top edge was paired with first: 17 of the 21 edges; four
  were not extended at all.
- `ExtentFace` intersected the extended wall with the top once per
  extended edge, and each got the whole section (one curve from the
  slot's one side to the other). The vertex between two such edges is the
  intersection of two copies of one curve: it has no place on it.
- The third of the top's edges across got a section of no length as its
  image, and `OrientSection` on it raised the construction error. (Without
  the exception the top would still have got 17 copies of the section.)

The section of the wall with the top is one curve, and it bounds the wall
and the top along the whole run: the run's edges cannot each be bounded
on it, and the top's edges across have one image together.

## What changed

- `ToContext`: the kept face is extended along every edge paired across
  with the removed face's edge (`KeptFacesAcrossToCap`, all pairs, not the
  first only); those edges are handed to `ExtentFace` as edges that may be
  extended in runs. The section bounds the face once, and is the image of
  one of the removed face's edges across (the others keep their old image,
  which `ContextIntByArc` passes over: it has no curve on the cap).
- `ExtentFace`: a run of two or more consecutive edges of a wire, across
  vanishing faces and extended to the same face, is replaced while the
  face is extended by one edge from the run's first vertex to its last
  (`RunsToOneStop`; a straight segment in 3D and in the face's
  parameters, of which only the ends are used, and the run's middle edge as
  the intersection's reference). It gets one section, bounded where the
  edges beside the run meet it, and every edge of the run has that section
  as its image, oriented to its own parameter.
- `ContextIntByArc`: a section that stands for several of the cap's edges
  is stored on the cap once.

Edges extended alone (one edge across, or edges not across vanishing
faces) take the same way as before.

## The cases

MitCad's thick-solid input (`MITCAD_SHELL_DUMP`), the top open, 3 mm,
OCCT's thick solid in the OCCT lab (case
`2535-wall-chain-across-vanishing-rounding`, a fresh copy per run):

| body | Sharp before | Sharp after | Rounded before | Rounded after |
| --- | --- | --- | --- | --- |
| #645's loft, top rounded 2 mm | 26653.3 mm3 | the same | no result | 26653.3 mm3 |
| box, the wall's edge to a rounding split | 10010.9 mm3 | the same | a "valid" solid of absurd volume (about 3e98 mm3) | 10010.9 mm3 |
| box, that edge and the rounding's edge on the top split apart | 10010.9 mm3 | the same | no result | 10010.9 mm3 |

(all results valid). The boxes are 40 x 30 x 20, their top edges rounded
2 mm; the first has the edge where one long rounding meets its wall split
at x = 20, the second that edge at x = 15 and the rounding's edge on the
top at x = 25. Rounded gives the same wall as Sharp: none of these bodies
has an edge for the arc join to round once the roundings vanish.

Of the lab's shared set (55 cases with this one) only the loft's Rounded
run at 3 mm differs, in each case that has it (2433-rounded-top-open,
2462-top-open-met-tangentially, 2494-chained-rounding-and-seam-run and
this one): no result before, 26653.3 mm3 valid after. OCCT's blend,
chamfer and offset groups against the chain without the patch: 1353 same,
167 failing in both, nothing fixed, broken, changed or random.

## 2535-brepoffset-arc-join-wall-across-vanishing-faces-to-a-chain-of-cap-edges

- `BRepOffset_MakeOffset.cxx`: `KeptFacesAcrossToCap` (every pair across);
  `ToContext` extends along all of them, passes them to `ExtentFace` and
  binds a shared section once.
- `BRepOffset_Tool.cxx`/`.hxx`: `ExtentFace` takes the edges that may be
  extended in runs (`theRunEdges`, default none); `RunsToOneStop`.
- `BRepOffset_Inter3d.cxx`, `ContextIntByArc`: a section stored on the cap
  once.

## The cavity route

mitcad#2535's second item asked why MitCad's cavity route (the body less a
cavity, `geometry/src/shell_cavity.cpp`), which ran on this case after the
thick solid failed, gave an invalid result. With the thick solid now built,
the case no longer reaches that route (its Sharp and Rounded runs both end
in the thick solid). Where the cavity's result is invalid the route
refuses in words: `cavity_shell` checks it (`check_result`,
`require_valid`: "the shell produced an invalid shape", the checker's first
finding in the detail), and that refusal is the shell's error. Which check
failed on which faces was not traced further: the case cannot reach the
route any more, and the dumped cavity input of mitcad#2494's runs predates
patch 2494.

## Reports and tests

- `geometry.port.2535`
  (`p2535-arc-join-wall-chain-across-a-vanishing-rounding.cpp`), OCCT
  alone: a box 40 x 30 x 20, its top edges rounded 2 mm, the edge where one
  long rounding meets its wall split by a vertex, and the same with the
  rounding's edge on the top split elsewhere too; 3 mm, Rounded, top open:
  one valid solid of the body's volume less the cavity.
- `cli.shell_sharp_reported` (`shell_sharp_reported645.json`): 3 mm
  Rounded on the reporter's archive, the same wall and note as Sharp.

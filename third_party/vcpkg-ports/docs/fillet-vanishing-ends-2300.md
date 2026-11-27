<!-- SPDX-License-Identifier: MIT -->
# Roundings that vanish where their faces come tangent: 2300 to 2303

mitcad#2300 and mitcad#2390: the rim between a loft's side and a cylinder
pulled from its end face, rounded. Back to the
[port's README](../README.md).

## Overview

- **2300** does not take a section of no width (its two contacts within
  the walk's 3d tolerance of each other) for a twist of the contact lines.
  The last section of a walk into a vanishing end was taken for one, and
  the repair of twisted pieces then refused the whole piece.
- **2301** does not end a contact on an arc at a crossing of the contact
  with the arc that lies farther from the arc's point than the contact's
  own end: a contact that runs onto the edge it rounds tangentially, at a
  vanishing end, crosses it only at its other end, and its edge came out
  with both ends on one vertex (an invalid result).
- **2302** gives a walk's end on a restriction of a periodic surface the
  parameters of the walk's period, not the face's: a walk that crossed
  the face's seam ended a period off its points before, and the
  approximation's sections between them landed on the far side of the
  surface (mitcad#2390).
- **2303** does not cut a blend at a minimum of its section's width in a
  vanishing end, where the sections have no width and minima of noise:
  the blend cut into a piece and a sliver crashed the stripe's build, made
  a convex and a concave blend overlap at their common vertex, or left a
  sliver face (mitcad#2390).

## The case

A loft from a circle of radius 111 on XZ to a slot 520 x 80 mm 100 mm
along +Y, its circle's end pulled 230 mm straight out: a cylinder joined to
the loft. The loft's side flares out beyond the cylinder towards the slot's
ends and draws in within it towards the slot's flat sides, so the rim
between them is concave in some places and convex in others, the two faces
tangent at four points between. MitCad's fillet splits the rim at those
points (`geometry/src/vanishing_split.cpp`), each piece convex or concave
all along, its rounding vanishing in a point at both ends, and gives the
kernel all the pieces at once.

The bodies the kernel got were written by `test_geometry_fillet_f_n fillet_loft_rim --probe`
and rounded in the OCCT lab with DRAW's `blend` (each build on a freshly
restored body: the kernel's fillet gives its input's edges curves in
place). With the loft's Tangent Edges Merge the top convex piece rounded
with 5 failed, alone and with the others.

## 2300: no twist at a section of no width

### 2300-brepblend-no-twist-at-a-section-of-no-width

The walk's trace (patch 2141) of the top piece with 5: the walk runs from
one vanishing end to the other, 1054 points, both ends on the arcs; the
first section's arc is 1e-9 rad, the last one's 3.9e-7 rad, its contacts
2e-6 mm apart. Then the stripe's computation raised "adjustment by
reprocessing the non-written points". A trace in `TestArret` showed the
walk's only twist at its last point: `BlendFunc_ConstRad::TwistOnS2`, the
contact line's tangent on the second face against the guide. That tangent
comes from a system that is almost singular where the faces are tangent;
its sign at a section of no width means nothing. The walk took the whole
piece for twisted, and the repair of twisted pieces in
`ChFi3d_Builder::PerformSetOfSurf`, which needs a piece of some width at
both ends to fill between its neighbours, gave up. The contact lines do not
fold: patch 2141's fold ratio is 0.008 at most (folding above 1).

- `BRepBlend_Walking::TestArret`: where the section's two contacts are
  within `tolpoint3d` of each other, neither contact's twist is taken.
  Elsewhere nothing changes.

With the patch the top piece with 5 builds; the walk is the same.

## 2301: a contact's end on an arc not taken at a crossing farther away

### 2301-chfi3d-contact-end-on-an-arc-not-taken-at-a-crossing-farther-away

With 2300 the top piece with 5 built, but invalid (BRepCheck_NotClosed,
BRepCheck_UnorientableShape), alone and with the others; so did the bottom
piece with 10. The rounding face's second contact edge had both its ends on
the far vertex, and the cylinder's face missed that edge: the face's wire
and the cylinder's could not close. The common points were right (both
contacts at the near vertex at the start, at the far one at the end; one of
them on the neighbouring piece's arc), and both ends of the piece went the
singular corner's way (PerformSingularCorner). But the second contact's
interference had the parameters 0 to 0, not 0 to 154.37: set so in
`ChFi3d_Builder::CompleteData`, where a contact ends on an arc
(`CompParam`). There the arc's parameter of the end (408.07) and the
contact's (154.37) gave points 1.96e-6 apart on the face, more than
`Precision::PConfusion`, so the contact's curve was intersected with the
arc's. The contact runs onto that arc, the edge being rounded, tangentially
and stops a hair short of it: the only crossing was at the contact's other
end (parameter 0, on the arc 157.59), and it was taken.

- `CompParam`: a crossing whose point on the arc lies farther from the
  arc's point at the given parameter than the contact's point at its given
  parameter does is not taken; with none left, the existing projection of
  the arc's point on the contact gives the end (154.37 here). The crossing
  such a correction looks for lies at the arc's point; elsewhere nothing
  changes.

With 2300 and 2301 every piece of the Merge loft's rim, alone and all at
once, rounds valid with 2, 5 and 10, and every piece of the Keep loft's
with 5 and 10 (the lab case `2300-loft-rim`); MitCad's own fillet passes
the same (`geometry.fillet_loft_rim`, every part but Keep with 2).

Not with 2301: the Keep loft's top and bottom pieces with 2, and all the
pieces at once with 2, two more faults (2302 and 2303, mitcad#2390).

## 2302: a walk's end on a restriction in the walk's period

### 2302-brepblend-walk-end-on-a-restriction-in-the-period-of-the-walk

The Keep loft's bottom convex piece with 2 failed: "PerformSurf : Failed
approximation!". Its walk (33 points) takes one step of 8.3 from w 195.9,
where the section is 0.23 wide, onto the vanishing end at 204.17, where
both contacts reach the arcs. The approximation then solved the sections
between from those two points and landed on another branch of the
equations (contacts 100 to 200 mm off, on the far sides of the cylinder and
the loft), inserted them into the line as hard sections ("Insertion of the
point if the calculation seems long", `BRepBlend_AppFuncRoot`), and at
last found none.

The cause is not the long step but the end point's parameters. The loft's
side is a periodic surface (period 869.72 in u), and its face's seam lies
on the piece's way: the walk crossed it, the solver going on into the next
period (u 780.67 at w 195.9). Its end point came from the reframing onto
the restriction (`BRepBlend_Walking::Recadre`, state `Blend_OnRst1`):
the contact on the cylinder on its arc, the contact on the loft from the
reframing's solver at u -79.20, the same point a period below. The
approximation's start for a section between w 195.9 and the end, the
chord between the two points in the surfaces' parameters, thus lay half
a surface away, and Newton went to a ball there. With the walk's deflection 1e-4 the walk had reached the end
through both arcs at once (`Blend_OnRst12`), the arc's own curve on the
face then giving u 790.52 (the period of the walk), and the piece built:
the deflection only decided which way the end was reached.

- `BRepBlend_Walking::InternalPerform`: where a step ends on a restriction
  (either or both), the contacts' parameters on a periodic surface are
  moved by whole periods next to those of the walk's point before
  (`BRepBlend_IntoPeriodOf`, OCCT's own `RecadreIfPeriodic`), after the
  classification of the reframing on the face, which wants them as the
  reframing gave them. The point in space is the same; where no period lies
  between, nothing changes.

The bottom piece then builds valid with 2 at the walk's deflections 1e-3,
1e-4 and 1e-5.

Tried in the lab and not kept, before the cause was found: patch 2228's
re-solve of a far section from the walk's two points either side (it lands
far too, the end point being a period off), a cap on the walk's step where
the section narrows (built the bottom piece, broke it at the deflection
1e-5), and following a far section from the walk's point before in steps
(each lands on the walk's branch, but the end point still lay a period
off, and the approximation's sections near it went on landing far).

## 2303: no split at a minimum of noise in a vanishing end

### 2303-chfi3d-no-split-of-a-blend-at-a-minimum-of-noise-in-its-vanishing-end

The Keep loft's top convex piece with 2 crashed (a segmentation fault in
`ChFi3d_CutConsumedFaces`, reading the spine's first edge), and with 2302
all the pieces at once were refused (`Failure_Overlap`). Both came from
blends cut into a piece and a sliver at their vanishing ends.

`ChFi3d_FilBuilder::SplitSurf` cuts a blend into two faces at a minimum of
its section's width where its contacts meet inside it (within 100 times
the approximation's 3d tolerance, mitcad#1420): a point the blend runs
through. A walk into a vanishing end has its last sections of no width,
1e-7 to 1e-5 mm, with minima of noise among them; SplitSurf took them: the
top piece's blend was cut at w 191.26176 (width 5.4e-6), 0.0002 before its
end, the bottom piece's at 204.16916 (4.6e-6), the concave piece's after
it at 0.00012 after its start (4.5e-6), each into a piece up to the
vertex and a sliver of no width beyond it.

- With the top piece alone, `ChFi3d_FilDS`, at the sliver, took the
  meeting of the two in a point on the end vertex for the spine going on
  past the vertex with its next edge ("The edge is removed from the spine
  starting on this vertex") and read `spine->Edges(2)` of a spine of one
  edge; the read past the sequence's end left its cache at the index 2 of
  1, and the next reading of its first edge went from there to a null node.
- With all the pieces at once, the slivers' contacts of the bottom convex
  piece and the concave piece after it crossed at their common vertex
  (`convexConcaveCrossBeforeEnd`), and both were refused as overlapping.
- With 5 the bottom piece got a sliver face (7 faces, its volume
  12754318.7, where 2 gives 12751700.7 and 10 gives 12751596.3).

- `ChFi3d_FilBuilder::SplitSurf`: a minimum is taken only where the
  sections widen past that tolerance on both sides of it before the
  line's ends. A blend that narrows to a point and widens again is cut
  there as before.

With 2302 and 2303 every piece of the Keep loft's rim with 2, alone and
all at once, rounds valid, alone at the walk's deflections 1e-3, 1e-4 and
1e-5 as well; the bottom piece with 5 has 6 faces and the volume
12751679.9, between those with 2 and 10.

Not changed, though seen on the way: `ChFi3d_FilDS` reads the spine's next
edge at such a meeting of two pieces on a vertex without checking that
the spine has one; with 2303 this case no longer reaches it.

## Reports and tests

- `geometry.fillet_loft_rim.kernel_merge` and `kernel_keep`
  (`geometry/tests/test_fillet_loft_rim.cpp`): the owner's body with Merge
  and with Keep, its pieces of one surface joined and its rim split where
  the rounding vanishes, every piece rounded by the kernel's fillet alone
  with 2, 5 and 10, each build on a copy of its own: done and valid.
- `geometry.fillet_loft_rim` (its other parts): MitCad's fillet of the rim
  at 2, 5 and 10 and with pulls of 50 and 400, Merge and Keep (Keep with 2
  needs 2302 and 2303).
- The OCCT lab's case `2300-loft-rim` (the split bodies as .brep, every
  piece rounded with 2, 5 and 10, alone and all at once), and
  `2390-loft-rim-keep-deflections` (the Keep body's pieces with 2 at the
  walk's deflections 1e-4 and 1e-5, alone and all at once).
- The lab's regression set with 2300 and 2301 against the chain without
  them: 40 cases, only `2300-loft-rim` differs. OCCT's own groups blend and
  chamfer: 670 cases the same, 37 failing both ways, 47 skipped, none fixed
  or broken.
- With 2302 and 2303 against the chain without them (mitcad#2390): 41
  cases, three differ: `2300-loft-rim` (every Keep build with 2 now valid,
  alone and all at once; the Keep bottom piece with 5, and all the pieces
  with 5, one sliver face fewer and the volumes in line with 2 and 10),
  `2390-loft-rim-keep-deflections` (the same at 1e-4 and 1e-5), and
  `slot-boss-tangent-rounding` (two volumes 2e-6 mm3 apart, rounding noise).
  OCCT's groups blend and chamfer: 754 cases, 670 the same, 37 failing both
  ways, 47 skipped, none fixed, broken or changed.

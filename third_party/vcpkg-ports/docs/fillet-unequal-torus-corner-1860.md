# A torus corner with unequal sides: 1860

mitcad#2091 (from mitcad#2075). Back to the [port's README](../README.md).

## The case

A block standing on a plate, its vertical edge at one corner rounded 5 and
the two foot edges there rounded 0.8 and 1. The corner's own face (the
plate's top) is a plane normal to the rounding of 5, the layout in which
OCCT makes a torus corner. With both feet 1 the corner is the torus of
radius 1 round the rounding of 5 (ring 6, tube 1), each foot rounding
meeting it at its section.

With 0.8 and 1 OCCT walked the corner with a radius running from 0.8 to 1
along a circle (`BRepBlend_EvolRad`): a free-form face, 0.39 mm3 short of
the union of the blends' fills. `ToricCorner` takes a torus only for equal
radii, and the pivot meets each side at that side's own radius from the
plate (0.8 and 1 up the rounding of 5), so its two section parameters
differ, which the test for a torus refused as well.

`geometry/tests/test_fillet_unequal_torus_corner.cpp` checks the block on
the plate (either foot the smaller, 0.5 and 2, down to 0.9998 and 1)
against the volume of the fills' union built by booleans, the pocket that
is its complement (convex feet round a concave rounding of 5, the same
blends) against the box less that union, and a box's convex corner and
radii within the tolerance (walked as before, valid).

## 1860-chfi3d-torus-corner-with-unequal-sides-runs-on-to-where-the-smaller-blend-crosses-it

- `PerformThreeCorner`: in the layout of a torus corner (`ToricLayout`,
  `ToricCorner`'s test less the radii), the sides' radii more than
  `tolapp3d` apart, the pivot a cylinder whose points at the sides' own
  radii from the face lie there within `tolapp3d`: the corner is
  `ComputeCorner`'s torus of the larger radius, its section on the smaller
  side taken on the pivot at the larger radius as on the other side.
- `unequalTail`: past that section the torus runs on (the tail) into W,
  the face the smaller rounding shares with the pivot, which cuts it, up to
  its crossing with the smaller rounding's blend. The crossing runs from
  P3, where the torus's contact on the corner's face meets the smaller
  rounding's, to P2, where the smaller rounding's contact on W meets the
  torus's cut by W. The tail is a corner stripe of its own on a copy of the
  torus, between the crossing and the section it shares with the corner;
  the smaller rounding ends at the crossing. The material is then the union
  of the fills (for convex sides, the solid less the union of what the
  blends take away).
- The cut and the crossing are `GeomInt_IntSS` curves of windows of the
  surfaces round the corner (in a periodic direction a period less 0.05
  wide, so that no seam splits a branch), moved into the corner's periods;
  a branch is taken where it passes within ten times the intersection's
  reached tolerance (or ten `tolapp3d`) of the points found on the faces.
- Where P3 or P2 is missing (beyond the section within a quarter turn, in
  the smaller rounding's range, inside its faces) or no branch is found,
  the torus is removed from the data structure and the corner is walked as
  before. A box's three convex edges are such a case (the torus's contact
  on the top curves away from the smaller rounding's), and so is a side
  whose contact on the corner's face ends at an edge of that face short of
  P3: the reported design's 0.8 arm, whose contact on the corner's face
  runs into that face's top edge (rounded 0.5) before the torus's contact
  reaches it.

# A rounding along a knife edge: 1770

mitcad#1984 (GitHub mitcad/Mitcad#415): a fillet on an edge where two
faces meet tangentially, at 0 degrees, the material between them a wedge
that thins to nothing. Back to the [port's README](../README.md).

## Overview

- **1770** starts OCCT's walked fillet along a knife edge where the ball
  touches both faces, away from the edge: the start's solver is given the
  points where the wedge is two radii thick instead of the edge's own
  point, from which it found no ball; and the side of its section is taken
  with both normals turned towards the centre, so that the section goes
  round the ball's side facing the tip.

## The case

The reported sketch: a rectangle 130 by 210 and a circle of radius 65
centred on its bottom edge's midpoint, so that the circle touches both
long sides at the bottom corners; the rectangle's region outside the circle
extruded. At the bottom corners the flat side wall and the circle's
cylinder meet tangentially, the wall's normal opposite the cylinder's: the
material between them is a wedge of 0 degrees, a knife edge. A ball of
radius r touches both faces where the wedge is 2 r thick: for the wall and
the cylinder of 65, its centre lies at (65 - r, sqrt(260 r)) from the
circle's centre, 16.1 above the cusp for r = 1. The rounding takes the tip
beyond the two contact lines away and closes it with the ball's swept
surface, a cylinder of radius r for the straight edge.

OCCT's fillet computes a plane and a cylinder with parallel axes
analytically (ChFiKPart), which finds that ball: the sketch as the ticket
describes it (rebuilt in `cli.fillet_knife_edge`, the rectangle's region
outside the circle) rounds with that port at radii from 0.1 to 10. Every
other pair of faces is walked, and the walk's first section (`StartSol`,
`BRepBlend_Walking::PerformFirstSection`) is solved from the edge's point
on both faces. At a knife edge the ball's equations have no solution near
there (the centres a ball touching each face would have lie on opposite
sides of the edge), and the solver does not reach the one 2 r into the
wedge: "no start", for two cylinders tangent to each other (a disc less a
disc inside it touching its rim) at any radius, and for any knife edge
whose faces are not a plane and a parallel cylinder.

## 1770: the start where the ball fits

### 1770-chfi3d-start-of-a-rounding-along-a-knife-edge

- `ChFi3d_KnifeStart`, from `StartSol` for a constant rounding at each
  point it tries on the spine, before `PerformFirstSection`: where the
  centre sides of the two surfaces (their normals with the signs the
  blend's `Choix` gives them, as `BlendFunc_ConstRad::Set` has them) are
  opposite within a thousandth of a radian at the edge's point, the faces
  meet as a knife.
- It steps from the edge's point along face 1 into the face (the side a
  small step classifies inside it), square to the edge in the section
  plane, the steps growing from r / 8 to at most 2 r; at each step it takes
  the centre of the ball touching face 1 there and its foot on surface 2
  (`Extrema_GenLocateExtPS` from the last foot). Where the centre's distance
  from surface 2, on that surface's centre side, reaches r, the points
  between the last two steps are the solver's start (`SolDep`); the solver
  then finds the section as for any blend, and the walk, the ends and the
  topology are OCCT's as for any rounding.
- `ChFi3d::ConcaveSide`, in the branch where the faces turn back on each
  other at the edge (the one a knife edge takes): it sets the faces'
  orientations towards the centre from points a little inside each face,
  but took the side of the section (the parity of `Choix`, from the cross
  product of the normals along the edge) with the normals as the faces
  have them. Where one face's orientation was turned and not the other's,
  the section went round the ball's far side: the fillet gave the tip and
  the whole ball's section as the body (two cylinders whose seams lie on
  the edge, as a sketch's circles have them: 268 mm3 left of 50266). The
  normals are now turned towards the centre first, as in the other branch.
- Where face 1 ends (a step classified outside it) or the foot on face 2
  lies outside that face before the ball fits, the start stays the edge's
  point: the radius is larger than the wedge allows, and the fillet fails
  with no start ("no ball of the radius touches both faces there").
- Anywhere else (the centre sides not opposite) nothing changes.

## What is left

- A tangent chain that runs from a knife edge into an ordinary edge on the
  same line (a cylindrical bite cut part of the way along a wall): the ball
  jumps at their common vertex. Done in
  [fillet-knife-chain-1790.md](fillet-knife-chain-1790.md) (mitcad#1988):
  OCCT's corner there is refused in words, and MitCad rounds the knife edge
  first and the other edge on its result.

## Reports and tests

- `geometry.fillet_knife_edge`: the reported body rounded at its knife
  edge with 1 and 5; two cylinders tangent to each other (the disc of 50
  less the disc of 30 inside it touching its rim, above the line through
  their centres) with 1 and 5, which failed with no start before 1770;
  both knife edges with the two square corners of the wall in one fillet;
  the two cylinders with their circles' seams on the knife edge and with
  arcs through three points, each also mirrored; each a valid closed solid
  whose volume is the exact shape's within 1e-6 relative. The two
  cylinders with 25, larger than their wedge allows,
  fail.
- `cli.fillet_knife_edge`: the reported sketch extruded and rounded at its
  knife edge from the model, and the two cylinders sketched as circles.
- `geometry.suite.dressup` (`test_fillets_of_knife_edges`): a disc with a
  cove tangent to both its faces, whose knife edges OCCT's default
  tolerances found no start along (MitCad retried with tighter ones,
  mitcad#107), now rounds with the defaults.
- OCCT's own blend and chamfer groups (DRAW, 754 cases, against the port
  with 1727): no case changed.

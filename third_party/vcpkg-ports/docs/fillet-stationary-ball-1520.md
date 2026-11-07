# A ball that stays put between two tubes of its radius: 1520 to 1523

mitcad#1412, the follow-up of mitcad#1397
([fillet-walk-back-step-1490.md](fillet-walk-back-step-1490.md)): a lid's
rims rounded on through the mitres of earlier roundings of the same radius.
Back to the [port's README](../README.md).

## Overview

- **1520** walks a constant-radius blend whose ball stays put (both
  supports tubes of its radius whose axes run through its centre) with the
  section whose contacts lie in the guide's normal plane: the walk's
  points, its start and the approximation take that section and its
  derivatives, where the blend's conditions leave the section a freedom.
- **1521** walks the short edge between a plane and a tube of the ball's
  radius (a torus of no tube circle, the ball touching the plane at one
  point) with the mitre beside it, and takes a contact that is one point
  for no contact along an edge.
- **1522** (mitcad#1420) splits a walked blend only where its contacts
  meet, so that each corner is one face.
- **1523** (mitcad#1420) takes a support for a tube of the ball's radius
  within a tolerance, so that the stored body's B-spline rounding is one.

## The case

A reported design's lid: a cone (45 degrees, radius 20 at z = 25 to 23 at
z = 28), a cylinder of 23 up to its top at z = 30, cut by the plane y = 0.
An earlier rounding of 1 rounded the lid's edges with the flat side: a
vertical cylinder of 1 (axis x = 21.977, y = 1), a cylinder along x under
the top and a walked rounding between the flat side and the cone. They
meet in mitres: the vertical cylinder and the one along x from the top's
corner, the vertical cylinder and the walked rounding from the cone rim's.
A later rounding of 1 takes the rims (cone and cylinder of 23 at z = 28,
cylinder of 23 and top at z = 30). Each rim's tangent chain goes on past a
short edge (the cone's with the vertical cylinder, 0.014 mm; the top's with
it, an arc of 0.045 mm) through the mitre. The stored body has a torus of
22 / 1 along each rim and, at each corner, a sphere of 1 centred where the
rim's torus centre circle meets the vertical cylinder's axis: (21.977, 1,
28.414) at the cone rim (0.818 mm²), (21.977, 1, 29) at the top rim
(1.616 mm², pi / 2 + asin(1 / 22)).

Over the short edge and the mitre the ball does not move: its centre lies
on both tubes' axes. BlendFunc_ConstRad's conditions (the centres seen
from both contacts one, the mid-point of the contacts in the guide's
normal plane) then leave the section one freedom: the centre seen from a
contact on a tube does not depend on the contact's place round the tube's
section, and the Jacobian is singular. The solvers' roots slide along that
freedom with their starts. OCCT's fillet failed on both rims, on the
replay's body and on the stored body before the item alike.

## 1520: the section in the guide's normal plane

### 1520-blendfunc-constrad-ball-staying-put-between-tubes-walked-in-the-guides-normal-plane

Every section of such a ball is an arc of its sphere. The one whose
contacts both lie in the guide's normal plane keeps the mid-point's
condition, is the rim's own section where the walk arrives at the ball,
and vanishes where the mitre ends.

- `BlendFunc_ConstRad::StillSection` (new) detects the ball that stays put
  (along one parameter of each surface the centre seen from the contact
  does not move, within a thousandth of the contact's speed: a tube of
  the ball's radius) and moves each contact round its tube's section into
  the guide's normal plane, keeping its parameter along the tube (that of
  another section of the ball where one is given).
- `IsSolution` and both `Section` overloads with derivatives take that
  section's first and second derivatives along the guide; where the mitre
  leaves the edge whose rounding ends at the ball tangentially (1490's box
  corner), the contacts start at rest and the point has no tangent.
- `BRepBlend_Walking` takes that section for each step's point (the solver
  from it slid along the freedom, and along the tubes by its tolerance)
  and starts a walk at it, its contacts on the arcs where they lie on them,
  instead of a start the inverse function moved behind it (past the
  guide's end, a contact 0.012 rad out of its face). A contact that stays
  where it was on its face's boundary (the top rim's ball touching the
  rounding along the top at its edge with the top while the guide runs
  round the short arc) has not left the face, and the step is judged by
  the other contact: the points are not taken for confused, which ended
  the walk at every point.
- `BRepBlend_AppFuncRoot` takes that section at every parameter of the
  approximation, at the walk's own points too, with the parameters along
  the tubes of the line's middle point (a step of the walk). The solver,
  from the interpolated start or from a point of the walk, slid along the
  freedom; and the walk's ends, found otherwise, lie apart along the tubes
  by the solver's tolerance (1e-6), so the sections taken from one end or
  the other did not join smoothly: the approximation cut them up to its
  limit (1411 poles of degree 14) and still ended 0.0146 off the contact,
  which became the edge's tolerance and an invalid p-curve. Now the cone
  rim's corner piece has 72 poles, within 1e-5 of the walk.

## 1521: the short edge at the top walked with the mitre

### 1521-chfi3d-ball-staying-put-at-a-plane-walked-its-point-contact-along-no-edge

- `ChFi3d_KParticular`: the top's edge with the vertical cylinder is a
  circle where a plane meets a cylinder about its axis, a particular case
  (a torus). With the cylinder of the ball's radius, the material inside
  it and the edge convex, the torus has no tube circle: the ball's centre
  stays on the cylinder's axis and it touches the plane at one point. As a
  particular case it was a sphere piece whose contact with the top was that
  point, the walked piece after it took the top for its second face (the
  ball not seen as staying put on a plane, the walk could not leave it),
  and with that face switched the top's and the rounding along it came out
  broken. Such an edge is now no particular case: the walk takes it with
  the mitre beside it, between the vertical cylinder and the rounding
  along the top (the faces across the edges the torus's contacts end on),
  the ball staying put over both; over the arc the contact on the rounding
  along the top stays at its edge with the top (1520's walk).
- `runsAlongEdgePart`: that contact, one point over the arc, was taken for
  a contact along a part of the rounding's edge with the top
  (`ChFi3d_ContactsAlongFaceEdges`): the piece was split there and its end
  given an external transition, and the rounding along the top lost its
  edge with the top. A contact curve that is one point now runs along no
  part of an edge.

On the replay's body before the item (OCCT alone): both rims valid, the
corner pieces on the ball's sphere, every face within 2.5e-4 mm of the
stored body after the item and the stored body's within 2.5e-4 mm of the
result, the volume 0.0103 mm³ above the stored body's. On the stored body
before the item the top rim builds (valid); the cone rim does not: the
stored walked rounding there is not a tube within a thousandth (at the
walk's start, on the face's edge, the centre seen from it moves 2.4 % of
the contact's speed), so the ball is not seen as staying put (1523 takes
it for one). The design's replay rounds its own body, on which the item is
parametric and verified.

## 1522: each corner one face

### 1522-chfi3d-walked-blend-split-only-where-its-contacts-meet

With 1520 and 1521 the top rim's corners each came out as two faces (the
final body 48 faces against the stored body's 47): the walk over the short
arc and the mitre is one piece, but `ChFi3d_FilBuilder::SplitSurf`, which
splits a walked blend whose contacts came close (as at the mitre's end on
the flat side) where its iso is a point, split it at the arc's end, the
contacts 1.414 apart there. It took every minimum of the distance between
the contacts along the walk and kept it where `ChFi3d_SearchSing`'s value
was below the tolerance; that value is the derivative of half the squared
distance, nought at every minimum. A ball that stays put keeps its
contacts a constant distance apart, and the walk's noise makes minima of
it: two pieces on two surfaces (0.026460 and 1.590495 mm²) about a vertex
whose tolerance was the distance. The split is now taken only where the
contacts lie within the distance the walk took for singular (100 times the
approximation's tolerance) of each other: one face of 1.615949 mm²
(the stored body's sphere piece 1.616266 mm²).

## 1523: tubes within a tolerance

### 1523-blendfunc-constrad-ball-staying-put-between-tubes-within-a-tolerance

On the stored body before the item the cone rim failed: the earlier
rounding between the flat side and the cone is a B-spline whose sections'
curvature ripples by a few hundredths about the ball's (the centre seen
from a contact moves at up to 2.5 % of the contact's speed, and back: at
the walk's start 2.4 %, against 1.5e-5 on the replay's rounding), while
its centres lie within 1.6e-3 mm of one axis. 1520's test, a thousandth of
the contact's speed, did not take it for a tube; taken for one, the
centres seen from the two contacts lie up to 1.4e-3 mm apart, which the
walk's tolerance (1e-4) refused.

- `StillTolerance`: the centres of a ball of radius R that stays put may
  lie apart by 5e-3 (half the hundredth of a millimetre a stored body is
  taken within), at most R / 200.
- `StillDirection`: past 1520's test, where the centre moves at less than
  a tenth of the contact's speed along one parameter (and at more than 0.3
  along the other), the support is a tube of the ball's radius if the
  centres seen from the points of that parameter's iso a quarter and half
  the radius away on either side lie within the tolerance of the
  contact's. A tube of another radius, its centre moving a few hundredths
  of the contact's speed, is not one (at half the radius away its centre
  lies that fraction of half the radius off).
- `StillSection` gives a section only where the centres seen from its
  contacts lie within the tolerance, and `IsSolution` takes such a
  section of a ball that stays put; elsewhere the walk's tolerance is as
  before, and exact tubes are taken as before.

On the stored body before the item both rims round, valid, with the stored
body's faces (24), every face within 2.1e-4 mm of the stored body after it
and its faces within 2.0e-4 mm of the result, the volume 0.0083 mm³ below
the stored body's; the corner pieces 0.817959 and 1.615949 mm² (the stored
spheres' 0.818079 and 1.616266).

## Reports and tests

- 1520 to 1523: mitcad#1412 and mitcad#1420, in
  `geometry/tests/test_fillet_stationary_ball.cpp`
  (`geometry.fillet_stationary_ball`): a lid of 23 cut by a plane, the
  flat side's edges rounded with 1, its top rim rounded with 1 through both
  corners' mitres; the lid's half with its cone and post, both rims rounded:
  valid, each corner one new face, on the ball's sphere within 1e-4, the
  top corner's area pi / 2 + asin(1 / 22) within 2e-3. With
  `MITCAD_GEOMETRY_CASES`, the case `lid-rim-stationary-ball`: the
  replay's body before the item (`body.brep`) and the stored body before
  it (`stored-before.brep`), their rims rounded with 1, against the stored
  body after it (`after.brep`): valid, its number of faces, both ways
  within 0.01 mm, the volumes within 0.05 mm³. Without 1520 and 1521 each
  rounding fails; without 1522 the top corners are two faces each; without
  1523 the stored body's cone rim fails.
- 1490's box corners (`geometry.fillet_mitre_same_radius`) and the other
  fillet, chamfer and dressup geometry tests are as before.

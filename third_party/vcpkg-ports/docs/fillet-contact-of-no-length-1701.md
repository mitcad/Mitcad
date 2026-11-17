# A cube rounded within a hair of half its side: 1701

mitcad#1922 (a follow-up of mitcad#1917 and GitHub #353's third design):
a cube of 10 with its twelve edges rounded by 4.9998, where the fillet
never returned, and once its endless step was stopped gave back the cube
itself as done. Back to the [port's README](../README.md).

## Overview

- **1701** takes a contact of no length (no longer than ten times the
  join's tolerance) for a contact along no other in 0950's
  `joinEndsAlongContacts`, and makes a computation whose blends come out
  in no solid of the body not done (`Failure_NoResult`), instead of giving
  back the input as its result.

## The case

A cube of 10, its twelve edges rounded by r = 4.9998: the balls of the
corners at both ends of each edge all but meet in its middle, and each
face of the cube is left a square 10 - 2r = 4e-4 wide. The four contacts
on each face, of the roundings of its four edges, are each 4e-4 long.

0950's `joinEndsAlongContacts` (with 1655's ends inside a face whose edges
are all rounded) joins two contacts on one face that lie along each other:
where each one's points lie within the join's tolerance of the other
(ten times the approximation tolerance, 1e-3 by default). Contacts 4e-4
long lie within 1e-3 of any contact passing by, whichever two of the four:
they were all taken for contacts along each other and their ends joined.
With both ends joined, the join projected one contact onto the other
blend's surface (`GeomProjLib::Curve2d`) over a range of no length, and
the projection walked with a step of 0: the fillet never returned, and
nothing reached the cancel (mitcad#1917 gives such calls up in MitCad;
mitcad#1819 refuses this cube before the kernel).

Where that projection was stopped, the data structure had blends that the
builder put into no solid; `ChFi3d_Builder::Compute` then gives back each
solid it has no merged result for as it was, and the fillet was done with
the cube itself (volume 1000, six faces, a valid body).

## 1701: a contact of no length joined with none; blends in no solid no result

### 1701-chfi3d-contact-of-no-length-joined-with-none-and-blends-in-no-solid-no-result

- `contactLength`: a contact's length along the polyline through nine
  points of its curve, 0 for a range of no length.
- `contactAlong` (0950): a contact no longer than ten times the tolerance
  (1e-2 by default) lies along nothing, before any projection: within the
  tolerance, every point of it is near any contact passing by. The
  contacts 0950 and 1655 join (window sills, a rib's ridge) are many times
  longer.
- `ChFi3d_Builder::Compute`: where the data structure has blend surfaces
  but no solid of the body came out of the builder merged with them, the
  computation is not done, `Failure()` is the new `Failure_NoResult` and
  there is no result shape; the grown faces' continuation (1000's
  `ChFi3d_ContinueGrownFaces`) and the regularities (`SetRegul`) are left
  out, so that the input's faces and edges are not touched. A solid the
  fillet does not reach is still given back as it was beside one it does.

MitCad's fillet failure says "the geometry kernel computes the roundings
but cannot rebuild the body with them" for `Failure_NoResult`
(`geometry/src/dressup.cpp`). MitCad's own refusal of the cube before the
kernel (mitcad#1819) stays, so that its retry 0.1 % smaller builds it.

## Reports and tests

- `geometry.fillet_rib_full_round`
  (`geometry/tests/test_fillet_rib_full_round.cpp`,
  `test_cube_rounded_within_a_hair_of_half_its_side`): the cube of 10
  rounded on its twelve edges by 4.9998 with OCCT's fillet alone returns
  (before 1701 it ran into the test's time limit); it is either not done,
  throws, or one valid solid with more than six faces whose volume is the
  exact shape's (1000 - 12 (10 - 2r) r^2 (1 - pi/4) - 8 r^3 (1 - pi/6),
  within 1e-3 mm³); never the cube as done. The rib's full rounds of the
  same test (1655's contacts along each other, 38 long and more) are
  unchanged.

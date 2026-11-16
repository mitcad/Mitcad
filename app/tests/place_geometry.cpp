// SPDX-License-Identifier: MIT
// The plane geometry of a placement on a face (mitcad#656,
// commands/PlaceGeometry.hpp): projections into the plane, distances from
// edges and axes, the point at two distances, snapping and the two lines
// a click is measured from.
#include "UnitTests.hpp"
#include "commands/PlaceGeometry.hpp"

#include <cmath>
#include <cstdio>

#include <gp_Vec.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "place_geometry.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool equal(double a, double b) { return std::abs(a - b) < 1e-9; }
bool same(const gp_Pnt& a, const gp_Pnt& b) { return a.Distance(b) < 1e-9; }

} // namespace

int placeGeometryTests() {
  using namespace mitcad::cmd::place;
  const gp_Dir up(0, 0, 1);
  // The top face of a 60 x 40 block 20 high: its four edges.
  const Line front{gp_Pnt(0, 0, 20), gp_Dir(1, 0, 0), 60.0};
  const Line right{gp_Pnt(60, 0, 20), gp_Dir(0, 1, 0), 40.0};
  const Line back{gp_Pnt(60, 40, 20), gp_Dir(-1, 0, 0), 60.0};
  const Line left{gp_Pnt(0, 40, 20), gp_Dir(0, -1, 0), 40.0};
  const std::vector<Line> edges{front, right, back, left};

  // Distances: to the segment of an edge, to the whole of an axis.
  CHECK(equal(distanceTo(front, gp_Pnt(15, 12, 20)), 12.0));
  CHECK(equal(distanceTo(front, gp_Pnt(70, 0, 20)), 10.0));
  const Line axis{gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), -1.0};
  CHECK(equal(distanceTo(axis, gp_Pnt(70, 3, 0)), 3.0));

  // A click near the front left corner: the front edge, then the left.
  const auto pair = nearestPair(edges, gp_Pnt(8, 5, 20), 1.0 * 3.14159265358979323846 / 180.0);
  CHECK(pair.has_value() && pair->first == 0 && pair->second == 3);
  // Two parallel edges only: no pair.
  CHECK(!nearestPair({front, back}, gp_Pnt(8, 5, 20), 0.01).has_value());

  // The point back from its distances, with each line's side.
  const gp_Dir intoFront = across(front, up); // +Y, into the face
  const gp_Dir intoLeft(1, 0, 0);
  CHECK(equal(signedDistance(front, intoFront, gp_Pnt(8, 5, 20)), 5.0));
  CHECK(equal(signedDistance(left, intoLeft, gp_Pnt(8, 5, 20)), 8.0));
  CHECK(equal(signedDistance(left, intoLeft, gp_Pnt(-2, 5, 20)), -2.0));
  const auto point = pointAt(left, intoLeft, 15.0, front, intoFront, 20.0, up);
  CHECK(point.has_value() && same(*point, gp_Pnt(15, 20, 20)));
  CHECK(!pointAt(front, intoFront, 1.0, back, intoFront, 2.0, up).has_value());

  // An axis above the face put into its plane; one along the normal is
  // no line in it.
  const auto projected = inPlane(Line{gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0), -1.0}, gp_Pnt(5, 5, 20), up);
  CHECK(projected.has_value() && equal(projected->point.Z(), 20.0) && projected->length < 0.0);
  CHECK(!inPlane(Line{gp_Pnt(0, 0, 0), up, -1.0}, gp_Pnt(5, 5, 20), up).has_value());

  // Snapping: the nearest candidate within the tolerance.
  const std::vector<gp_Pnt> corners{gp_Pnt(0, 0, 20), gp_Pnt(60, 0, 20), gp_Pnt(30, 0, 20)};
  const auto snapped = snap(gp_Pnt(29, 1, 20), corners, 2.0);
  CHECK(snapped.has_value() && same(*snapped, gp_Pnt(30, 0, 20)));
  CHECK(!snap(gp_Pnt(20, 10, 20), corners, 2.0).has_value());

  // A new line takes the place of the reference it runs most nearly with.
  CHECK(replaced(front, left, back) == 0);
  CHECK(replaced(front, left, right) == 1);

  // A box on XY measured from the plane's own axes turns from the X axis.
  // Reference 1 picked again (a line along Y at x = 10): without the
  // plane's x pinned as Align it would turn from that line, 90 degrees;
  // pinned, its x axis stays where it was (mitcad#656).
  const gp_Dir planeX(1, 0, 0);
  const Line yAxis{gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0), -1.0};
  const gp_Dir intoX(1, 0, 0);
  const gp_Dir before = zeroAxis(std::nullopt, std::nullopt, true, planeX, yAxis, intoX, up);
  CHECK(before.IsEqual(planeX, 1e-9));
  const Line line{gp_Pnt(10, 0, 0), gp_Dir(0, 1, 0), 50.0};
  CHECK(keepsPlaneX(true, false, true));
  const std::optional<gp_Dir> pinned =
      keepsPlaneX(true, false, true) ? std::optional<gp_Dir>(planeX) : std::optional<gp_Dir>();
  CHECK(zeroAxis(pinned, std::nullopt, false, planeX, line, intoX, up).IsEqual(before, 1e-9));
  CHECK(!zeroAxis(std::nullopt, std::nullopt, false, planeX, line, intoX, up).IsEqual(before, 1e-6));
  // Align picked by the user, or the references no plane's axes before:
  // nothing pinned.
  CHECK(!keepsPlaneX(true, false, false));
  CHECK(!keepsPlaneX(false, false, true));

  // A reference on the face's far side: the part grows into the face
  // whichever way the edge runs.
  const gp_Dir intoBack(0, -1, 0);
  CHECK(intoFace(back.direction, intoBack, up).IsEqual(gp_Dir(-1, 0, 0), 1e-9));
  CHECK(intoFace(gp_Dir(1, 0, 0), intoBack, up).IsEqual(gp_Dir(-1, 0, 0), 1e-9));
  CHECK(intoFace(front.direction, intoFront, up).IsEqual(gp_Dir(1, 0, 0), 1e-9));
  return failures;
}
MITCAD_UNIT_TEST(placeGeometryTests);
